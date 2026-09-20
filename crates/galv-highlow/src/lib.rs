//! Galvanize-native, one-way Low -> High air-gap replication primitives.
//!
//! This crate deliberately owns the wire format and transport boundary only.
//! Corrosion integrates it with CR-SQLite capture, the agent lifecycle, and
//! high-side sparse merge state.

pub mod apply;
pub mod transport;

use std::{
    io::{Cursor, Read},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use ed25519_dalek::{Signer, Verifier};
use getrandom::fill;
use rsa::{
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    Oaep, RsaPrivateKey, RsaPublicKey,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const BUNDLE_FORMAT: &str = "galvanize-highlow/1";
pub const SEALED_FORMAT: &str = "galvanize-highlow-sealed/1";
pub const MANIFEST_FORMAT: &str = "galvanize-highlow-manifest/1";
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_EVENTS_PER_BUNDLE: usize = 100_000;
pub const MAX_VALUE_BYTES: usize = 4 * 1024 * 1024;
pub const ZSTD_LEVEL: i32 = 15;

/// Creates the internal, non-replicated high/low journal tables. These tables
/// are intentionally ordinary SQLite tables: callers must exclude the
/// `__galv_highlow_` prefix from CR-SQLite replication.
pub fn initialize_store(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(
            "BEGIN;
         CREATE TABLE IF NOT EXISTS __galv_highlow_events (
           stream_id TEXT NOT NULL,
           sequence INTEGER NOT NULL,
           event_json BLOB NOT NULL,
           exported_at_ms INTEGER,
           PRIMARY KEY (stream_id, sequence)
         ) WITHOUT ROWID;
         CREATE TABLE IF NOT EXISTS __galv_highlow_outbox (
           bundle_id TEXT PRIMARY KEY,
           stream_id TEXT NOT NULL,
           sequence_first INTEGER NOT NULL,
           sequence_last INTEGER NOT NULL,
           payload_filename TEXT NOT NULL UNIQUE,
           manifest_filename TEXT NOT NULL UNIQUE,
           created_at_ms INTEGER NOT NULL,
           uploaded_at_ms INTEGER
         );
         CREATE TABLE IF NOT EXISTS __galv_highlow_inbox (
           bundle_id TEXT PRIMARY KEY,
           stream_id TEXT NOT NULL,
           sequence_first INTEGER NOT NULL,
           sequence_last INTEGER NOT NULL,
           received_at_ms INTEGER NOT NULL,
           status TEXT NOT NULL,
           detail TEXT
         );
         CREATE TABLE IF NOT EXISTS __galv_highlow_streams (
           stream_id TEXT PRIMARY KEY,
           highest_seen_sequence INTEGER NOT NULL DEFAULT 0,
           highest_contiguous_sequence INTEGER NOT NULL DEFAULT 0,
           updated_at_ms INTEGER NOT NULL
         );
         COMMIT;",
        )
        .map_err(sql_error)
}

/// Appends already committed Low-side events. Sequence allocation remains at
/// the CR-SQLite capture boundary so a failed caller transaction never leaks
/// a high/low event.
pub fn append_events(transaction: &Transaction<'_>, events: &[Event]) -> Result<()> {
    validate_events(events)?;
    let mut statement = transaction.prepare_cached(
        "INSERT INTO __galv_highlow_events (stream_id, sequence, event_json) VALUES (?1, ?2, ?3)",
    ).map_err(sql_error)?;
    for event in events {
        statement
            .execute(params![
                event.stream_id,
                event.sequence,
                serde_json::to_vec(event).map_err(json_error)?
            ])
            .map_err(sql_error)?;
    }
    Ok(())
}

/// Returns the next contiguous, unexported Low-side events, capped before
/// memory is allocated for serialization.
pub fn pending_events(
    connection: &Connection,
    stream_id: &str,
    maximum: usize,
) -> Result<Vec<Event>> {
    if maximum == 0 || maximum > MAX_EVENTS_PER_BUNDLE {
        return Err(Error::Configuration("invalid pending-event limit".into()));
    }
    let mut statement = connection.prepare(
        "SELECT event_json FROM __galv_highlow_events WHERE stream_id = ?1 AND exported_at_ms IS NULL ORDER BY sequence LIMIT ?2",
    ).map_err(sql_error)?;
    let rows = statement
        .query_map(params![stream_id, maximum as i64], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .map_err(sql_error)?;
    let mut events = Vec::new();
    for row in rows {
        let bytes = row.map_err(sql_error)?;
        events.push(serde_json::from_slice(&bytes).map_err(json_error)?);
    }
    if !events.is_empty() {
        validate_events(&events)?;
    }
    Ok(events)
}

pub fn record_outbox(
    transaction: &Transaction<'_>,
    artifacts: &Artifacts,
    payload_filename: &str,
    manifest_filename: &str,
) -> Result<()> {
    validate_filename(payload_filename)?;
    validate_filename(manifest_filename)?;
    let manifest = &artifacts.manifest;
    transaction.execute(
        "INSERT INTO __galv_highlow_outbox (bundle_id, stream_id, sequence_first, sequence_last, payload_filename, manifest_filename, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![manifest.bundle_id, manifest.stream_id, manifest.sequence_first, manifest.sequence_last, payload_filename, manifest_filename, now_ms()?],
    ).map_err(sql_error)?;
    Ok(())
}

pub fn mark_uploaded(transaction: &Transaction<'_>, bundle_id: &str) -> Result<()> {
    let changed = transaction
        .execute(
            "UPDATE __galv_highlow_outbox SET uploaded_at_ms = ?2 WHERE bundle_id = ?1",
            params![bundle_id, now_ms()?],
        )
        .map_err(sql_error)?;
    if changed != 1 {
        return Err(Error::Configuration("unknown outbox bundle".into()));
    }
    transaction.execute("UPDATE __galv_highlow_events SET exported_at_ms = ?2 WHERE stream_id = (SELECT stream_id FROM __galv_highlow_outbox WHERE bundle_id = ?1) AND sequence BETWEEN (SELECT sequence_first FROM __galv_highlow_outbox WHERE bundle_id = ?1) AND (SELECT sequence_last FROM __galv_highlow_outbox WHERE bundle_id = ?1)", params![bundle_id, now_ms()?]).map_err(sql_error)?;
    Ok(())
}

/// Idempotently registers receipt and tracks gaps without treating them as a
/// permanent configuration failure. A later bundle can close the gap.
pub fn register_received(transaction: &Transaction<'_>, bundle: &Bundle) -> Result<bool> {
    bundle.validate()?;
    let exists = transaction
        .query_row(
            "SELECT 1 FROM __galv_highlow_inbox WHERE bundle_id = ?1",
            [bundle.bundle_id.as_str()],
            |_| Ok(()),
        )
        .optional()
        .map_err(sql_error)?
        .is_some();
    if exists {
        return Ok(false);
    }
    let now = now_ms()?;
    transaction.execute("INSERT INTO __galv_highlow_inbox (bundle_id, stream_id, sequence_first, sequence_last, received_at_ms, status) VALUES (?1, ?2, ?3, ?4, ?5, 'received')", params![bundle.bundle_id, bundle.stream_id, bundle.sequence_first, bundle.sequence_last, now]).map_err(sql_error)?;
    transaction.execute("INSERT INTO __galv_highlow_streams (stream_id, highest_seen_sequence, highest_contiguous_sequence, updated_at_ms) VALUES (?1, ?2, 0, ?3) ON CONFLICT(stream_id) DO UPDATE SET highest_seen_sequence = MAX(highest_seen_sequence, excluded.highest_seen_sequence), updated_at_ms = excluded.updated_at_ms", params![bundle.stream_id, bundle.sequence_last, now]).map_err(sql_error)?;
    Ok(true)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("high/low configuration error: {0}")]
    Configuration(String),
    #[error("high/low integrity error: {0}")]
    Integrity(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(String),
}

impl Value {
    pub fn blob(bytes: &[u8]) -> Self {
        Self::Blob(STANDARD_NO_PAD.encode(bytes))
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Real(value) if !value.is_finite() => Err(Error::Configuration(
                "non-finite real values are unsupported".into(),
            )),
            Self::Text(value) if value.len() > MAX_VALUE_BYTES => {
                Err(Error::Configuration("text value exceeds size limit".into()))
            }
            Self::Blob(value) => {
                let decoded = STANDARD_NO_PAD
                    .decode(value)
                    .map_err(|_| Error::Configuration("blob is not base64".into()))?;
                if decoded.len() > MAX_VALUE_BYTES {
                    return Err(Error::Configuration("blob value exceeds size limit".into()));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Upsert,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub stream_id: String,
    pub sequence: i64,
    pub transaction_id: String,
    pub table: String,
    /// Canonical object mapping primary-key column names to typed values.
    pub primary_key: serde_json::Map<String, serde_json::Value>,
    pub operation: Operation,
    pub columns: Vec<Column>,
    pub source_actor: String,
    pub committed_at_ms: i64,
}

impl Event {
    pub fn validate(&self) -> Result<()> {
        if self.stream_id.is_empty()
            || self.sequence <= 0
            || self.transaction_id.is_empty()
            || self.table.is_empty()
            || self.source_actor.is_empty()
        {
            return Err(Error::Configuration(
                "event has required empty fields".into(),
            ));
        }
        if self.primary_key.is_empty() {
            return Err(Error::Configuration("event primary key is empty".into()));
        }
        if matches!(self.operation, Operation::Delete) && !self.columns.is_empty() {
            return Err(Error::Configuration("delete event has columns".into()));
        }
        for column in &self.columns {
            if !identifier(&column.name) {
                return Err(Error::Configuration("invalid event column name".into()));
            }
            column.value.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub bundle_id: String,
    pub stream_id: String,
    pub sequence_first: i64,
    pub sequence_last: i64,
    pub created_at_ms: i64,
    pub schema_hash: String,
    #[serde(default)]
    pub replay: bool,
    pub events: Vec<Event>,
}

impl Bundle {
    pub fn new(events: Vec<Event>, schema_hash: String, replay: bool) -> Result<Self> {
        validate_events(&events)?;
        if schema_hash.len() != 64 || !schema_hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Configuration(
                "schema_hash must be a SHA-256 hex digest".into(),
            ));
        }
        let now = now_ms()?;
        let stream_id = events[0].stream_id.clone();
        let first = events[0].sequence;
        let last = events.last().expect("nonempty").sequence;
        let mut id = Sha256::new();
        id.update(stream_id.as_bytes());
        id.update(first.to_be_bytes());
        id.update(last.to_be_bytes());
        id.update(now.to_be_bytes());
        Ok(Self {
            format: BUNDLE_FORMAT.into(),
            bundle_id: hex::encode(id.finalize()),
            stream_id,
            sequence_first: first,
            sequence_last: last,
            created_at_ms: now,
            schema_hash,
            replay,
            events,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != BUNDLE_FORMAT || self.bundle_id.is_empty() {
            return Err(Error::Configuration("unsupported bundle format".into()));
        }
        validate_events(&self.events)?;
        if self.stream_id != self.events[0].stream_id
            || self.sequence_first != self.events[0].sequence
            || self.sequence_last != self.events.last().expect("nonempty").sequence
        {
            return Err(Error::Integrity(
                "bundle envelope does not match events".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SealedBundle {
    pub format: String,
    pub bundle_id: String,
    pub recipient_key_id: String,
    pub content_sha256: String,
    pub compressed_sha256: String,
    pub nonce: String,
    pub wrapped_key: String,
    pub ciphertext: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub bundle_id: String,
    pub stream_id: String,
    pub sequence_first: i64,
    pub sequence_last: i64,
    pub schema_hash: String,
    pub payload_filename: String,
    pub payload_size: usize,
    pub payload_sha256: String,
    pub content_sha256: String,
    pub compressed_sha256: String,
    pub recipient_key_id: String,
    pub sender_key_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Artifacts {
    pub payload: Vec<u8>,
    pub manifest: Manifest,
}

pub fn seal(
    bundle: Bundle,
    recipient_key_id: &str,
    recipient_public_key_pem: &str,
    sender_key_hex: &str,
    payload_filename: &str,
) -> Result<Artifacts> {
    bundle.validate()?;
    validate_filename(payload_filename)?;
    let plain = serde_json::to_vec(&bundle).map_err(json_error)?;
    if plain.len() > MAX_DECOMPRESSED_BYTES {
        return Err(Error::Configuration(
            "bundle exceeds decompressed size limit".into(),
        ));
    }
    let compressed = zstd::stream::encode_all(Cursor::new(&plain), ZSTD_LEVEL)?;
    if compressed.len() > MAX_PAYLOAD_BYTES {
        return Err(Error::Configuration(
            "bundle exceeds compressed size limit".into(),
        ));
    }
    let content_sha256 = digest(&plain);
    let compressed_sha256 = digest(&compressed);
    let mut key = [0_u8; 32];
    let mut nonce = [0_u8; 24];
    fill(&mut key).map_err(|_| Error::Configuration("secure random source failed".into()))?;
    fill(&mut nonce).map_err(|_| Error::Configuration("secure random source failed".into()))?;
    let aad = aad(
        &bundle.bundle_id,
        recipient_key_id,
        &content_sha256,
        &compressed_sha256,
    );
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| Error::Configuration("invalid encryption key".into()))?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &compressed,
                aad: &aad,
            },
        )
        .map_err(|_| Error::Integrity("could not encrypt bundle".into()))?;
    let recipient = RsaPublicKey::from_public_key_pem(recipient_public_key_pem)
        .map_err(|e| Error::Configuration(format!("invalid recipient RSA public key: {e}")))?;
    let wrapped_key = recipient
        .encrypt(&mut rsa::rand_core::OsRng, Oaep::new::<Sha256>(), &key)
        .map_err(|e| Error::Configuration(format!("could not wrap content key: {e}")))?;
    let sealed = SealedBundle {
        format: SEALED_FORMAT.into(),
        bundle_id: bundle.bundle_id.clone(),
        recipient_key_id: recipient_key_id.into(),
        content_sha256: content_sha256.clone(),
        compressed_sha256: compressed_sha256.clone(),
        nonce: STANDARD_NO_PAD.encode(nonce),
        wrapped_key: STANDARD_NO_PAD.encode(wrapped_key),
        ciphertext: STANDARD_NO_PAD.encode(ciphertext),
    };
    let payload = serde_json::to_vec(&sealed).map_err(json_error)?;
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Error::Configuration(
            "sealed payload exceeds size limit".into(),
        ));
    }
    let mut manifest = Manifest {
        format: MANIFEST_FORMAT.into(),
        bundle_id: bundle.bundle_id,
        stream_id: bundle.stream_id,
        sequence_first: bundle.sequence_first,
        sequence_last: bundle.sequence_last,
        schema_hash: bundle.schema_hash,
        payload_filename: payload_filename.into(),
        payload_size: payload.len(),
        payload_sha256: digest(&payload),
        content_sha256,
        compressed_sha256,
        recipient_key_id: recipient_key_id.into(),
        sender_key_id: sender_public_key_hex(sender_key_hex)?,
        signature: String::new(),
    };
    let signer = signing_key(sender_key_hex)?;
    manifest.signature = STANDARD_NO_PAD.encode(signer.sign(&manifest.signing_bytes()?).to_bytes());
    Ok(Artifacts { payload, manifest })
}

pub fn open(
    manifest_bytes: &[u8],
    payload: &[u8],
    recipient_private_key_pem: &str,
    permitted_sender_key_hex: &[String],
    expected_schema_hash: &str,
) -> Result<Bundle> {
    if manifest_bytes.len() > MAX_MANIFEST_BYTES || payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Error::Configuration("artifact exceeds size limit".into()));
    }
    let manifest: Manifest = serde_json::from_slice(manifest_bytes).map_err(json_error)?;
    manifest.validate()?;
    if manifest.schema_hash != expected_schema_hash {
        return Err(Error::Configuration(
            "waiting-schema: application schema hash differs".into(),
        ));
    }
    if !permitted_sender_key_hex
        .iter()
        .any(|key| key == &manifest.sender_key_id)
    {
        return Err(Error::Integrity("sender key is not permitted".into()));
    }
    let public = verifying_key(&manifest.sender_key_id)?;
    let signature: [u8; 64] = STANDARD_NO_PAD
        .decode(&manifest.signature)
        .map_err(|_| Error::Integrity("invalid manifest signature".into()))?
        .try_into()
        .map_err(|_| Error::Integrity("invalid manifest signature length".into()))?;
    public
        .verify(
            &manifest.signing_bytes()?,
            &ed25519_dalek::Signature::from_bytes(&signature),
        )
        .map_err(|_| Error::Integrity("invalid manifest signature".into()))?;
    if manifest.payload_size != payload.len() || manifest.payload_sha256 != digest(payload) {
        return Err(Error::Integrity(
            "payload size or SHA-256 differs from manifest".into(),
        ));
    }
    let sealed: SealedBundle = serde_json::from_slice(payload).map_err(json_error)?;
    if sealed.format != SEALED_FORMAT
        || sealed.bundle_id != manifest.bundle_id
        || sealed.recipient_key_id != manifest.recipient_key_id
        || sealed.content_sha256 != manifest.content_sha256
        || sealed.compressed_sha256 != manifest.compressed_sha256
    {
        return Err(Error::Integrity(
            "sealed envelope differs from manifest".into(),
        ));
    }
    let private = RsaPrivateKey::from_pkcs8_pem(recipient_private_key_pem)
        .map_err(|e| Error::Configuration(format!("invalid recipient RSA private key: {e}")))?;
    let nonce: [u8; 24] = STANDARD_NO_PAD
        .decode(&sealed.nonce)
        .map_err(|_| Error::Integrity("invalid nonce".into()))?
        .try_into()
        .map_err(|_| Error::Integrity("invalid nonce length".into()))?;
    let wrapped = STANDARD_NO_PAD
        .decode(&sealed.wrapped_key)
        .map_err(|_| Error::Integrity("invalid wrapped key".into()))?;
    let ciphertext = STANDARD_NO_PAD
        .decode(&sealed.ciphertext)
        .map_err(|_| Error::Integrity("invalid ciphertext".into()))?;
    let key = private
        .decrypt(Oaep::new::<Sha256>(), &wrapped)
        .map_err(|_| Error::Integrity("could not unwrap content key".into()))?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| Error::Integrity("invalid unwrapped content key".into()))?;
    let compressed = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad(
                    &sealed.bundle_id,
                    &sealed.recipient_key_id,
                    &sealed.content_sha256,
                    &sealed.compressed_sha256,
                ),
            },
        )
        .map_err(|_| Error::Integrity("payload authentication failed".into()))?;
    if digest(&compressed) != manifest.compressed_sha256 {
        return Err(Error::Integrity(
            "compressed payload SHA-256 differs".into(),
        ));
    }
    let plain = read_limited(
        zstd::stream::read::Decoder::new(Cursor::new(compressed))?,
        MAX_DECOMPRESSED_BYTES,
    )?;
    if digest(&plain) != manifest.content_sha256 {
        return Err(Error::Integrity("plaintext SHA-256 differs".into()));
    }
    let bundle: Bundle = serde_json::from_slice(&plain).map_err(json_error)?;
    bundle.validate()?;
    if bundle.bundle_id != manifest.bundle_id
        || bundle.stream_id != manifest.stream_id
        || bundle.sequence_first != manifest.sequence_first
        || bundle.sequence_last != manifest.sequence_last
        || bundle.schema_hash != manifest.schema_hash
    {
        return Err(Error::Integrity("bundle differs from manifest".into()));
    }
    Ok(bundle)
}

impl Manifest {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        serde_json::to_vec(self).map_err(json_error)
    }

    fn validate(&self) -> Result<()> {
        if self.format != MANIFEST_FORMAT
            || self.bundle_id.is_empty()
            || self.stream_id.is_empty()
            || self.sequence_first <= 0
            || self.sequence_last < self.sequence_first
            || self.payload_size > MAX_PAYLOAD_BYTES
            || self.payload_filename.is_empty()
            || self.sender_key_id.len() != 64
        {
            return Err(Error::Configuration("invalid manifest".into()));
        }
        validate_filename(&self.payload_filename)
    }

    fn signing_bytes(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct Signable<'a> {
            format: &'a str,
            bundle_id: &'a str,
            stream_id: &'a str,
            sequence_first: i64,
            sequence_last: i64,
            schema_hash: &'a str,
            payload_filename: &'a str,
            payload_size: usize,
            payload_sha256: &'a str,
            content_sha256: &'a str,
            compressed_sha256: &'a str,
            recipient_key_id: &'a str,
            sender_key_id: &'a str,
        }
        serde_json::to_vec(&Signable {
            format: &self.format,
            bundle_id: &self.bundle_id,
            stream_id: &self.stream_id,
            sequence_first: self.sequence_first,
            sequence_last: self.sequence_last,
            schema_hash: &self.schema_hash,
            payload_filename: &self.payload_filename,
            payload_size: self.payload_size,
            payload_sha256: &self.payload_sha256,
            content_sha256: &self.content_sha256,
            compressed_sha256: &self.compressed_sha256,
            recipient_key_id: &self.recipient_key_id,
            sender_key_id: &self.sender_key_id,
        })
        .map_err(json_error)
    }
}

pub fn manifest_filename(payload: &str) -> Result<String> {
    let base = payload
        .strip_suffix(".zstd.galvh")
        .ok_or_else(|| Error::Configuration("payload filename must end in .zstd.galvh".into()))?;
    let filename = format!("{base}.json.galv");
    validate_filename(&filename)?;
    Ok(filename)
}

pub fn validate_filename(filename: &str) -> Result<()> {
    if filename.is_empty()
        || filename.len() > 255
        || filename.starts_with('.')
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || !filename
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
    {
        return Err(Error::Configuration("unsafe artifact filename".into()));
    }
    Ok(())
}

fn validate_events(events: &[Event]) -> Result<()> {
    let first = events
        .first()
        .ok_or_else(|| Error::Configuration("cannot create an empty bundle".into()))?;
    if events.len() > MAX_EVENTS_PER_BUNDLE {
        return Err(Error::Configuration("bundle has too many events".into()));
    }
    for (offset, event) in events.iter().enumerate() {
        event.validate()?;
        if event.stream_id != first.stream_id || event.sequence != first.sequence + offset as i64 {
            return Err(Error::Configuration(
                "bundle events must form one contiguous stream".into(),
            ));
        }
    }
    Ok(())
}

fn signing_key(hex_key: &str) -> Result<ed25519_dalek::SigningKey> {
    let bytes: [u8; 32] = hex::decode(hex_key)
        .map_err(|_| Error::Configuration("Ed25519 signing key must be hexadecimal".into()))?
        .try_into()
        .map_err(|_| Error::Configuration("Ed25519 signing key must be 32 bytes".into()))?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&bytes))
}
fn sender_public_key_hex(hex_key: &str) -> Result<String> {
    Ok(hex::encode(
        signing_key(hex_key)?.verifying_key().to_bytes(),
    ))
}
fn verifying_key(hex_key: &str) -> Result<ed25519_dalek::VerifyingKey> {
    let bytes: [u8; 32] = hex::decode(hex_key)
        .map_err(|_| Error::Configuration("Ed25519 public key must be hexadecimal".into()))?
        .try_into()
        .map_err(|_| Error::Configuration("Ed25519 public key must be 32 bytes".into()))?;
    ed25519_dalek::VerifyingKey::from_bytes(&bytes)
        .map_err(|_| Error::Configuration("invalid Ed25519 public key".into()))
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn aad(bundle: &str, recipient: &str, content: &str, compressed: &str) -> Vec<u8> {
    format!("{SEALED_FORMAT}\n{bundle}\n{recipient}\n{content}\n{compressed}").into_bytes()
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(i, c)| {
            c == b'_' || (c.is_ascii_alphanumeric() && (i > 0 || !c.is_ascii_digit()))
        })
}
fn now_ms() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| Error::Configuration(e.to_string()))?
        .as_millis()
        .try_into()
        .map_err(|_| Error::Configuration("time does not fit in i64".into()))?)
}
fn read_limited(mut reader: impl Read, maximum: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(output);
        }
        if output
            .len()
            .checked_add(count)
            .is_none_or(|len| len > maximum)
        {
            return Err(Error::Configuration(
                "decompressed bundle exceeds size limit".into(),
            ));
        }
        output.extend_from_slice(&buffer[..count]);
    }
}
fn json_error(error: serde_json::Error) -> Error {
    Error::Configuration(format!("JSON error: {error}"))
}
fn sql_error(error: rusqlite::Error) -> Error {
    Error::Configuration(format!("SQLite journal error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    fn event() -> Event {
        Event {
            stream_id: "low-a".into(),
            sequence: 1,
            transaction_id: "actor:1".into(),
            table: "records".into(),
            primary_key: serde_json::Map::from_iter([(
                String::from("id"),
                serde_json::json!({"type":"integer","value":1}),
            )]),
            operation: Operation::Upsert,
            columns: vec![Column {
                name: "value".into(),
                value: Value::Text("ok".into()),
            }],
            source_actor: "actor".into(),
            committed_at_ms: 1,
        }
    }

    #[test]
    fn seals_and_opens_a_bundle() {
        let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let public = RsaPublicKey::from(&private);
        let schema = "a".repeat(64);
        let artifacts = seal(
            Bundle::new(vec![event()], schema.clone(), false).unwrap(),
            "high-1",
            &public.to_public_key_pem(LineEnding::LF).unwrap(),
            &"11".repeat(32),
            "low-a-0001.zstd.galvh",
        )
        .unwrap();
        let manifest = artifacts.manifest.to_bytes().unwrap();
        let bundle = open(
            &manifest,
            &artifacts.payload,
            &private.to_pkcs8_pem(LineEnding::LF).unwrap(),
            &[artifacts.manifest.sender_key_id.clone()],
            &schema,
        )
        .unwrap();
        assert_eq!(bundle.events.len(), 1);
    }

    #[test]
    fn rejects_unsafe_names() {
        assert!(validate_filename("../escape").is_err());
        assert_eq!(manifest_filename("a.zstd.galvh").unwrap(), "a.json.galv");
    }

    #[test]
    fn journal_is_idempotent_and_preserves_gaps() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_store(&connection).unwrap();
        let tx = connection.unchecked_transaction().unwrap();
        append_events(&tx, &[event()]).unwrap();
        tx.commit().unwrap();
        assert_eq!(pending_events(&connection, "low-a", 10).unwrap().len(), 1);

        let bundle = Bundle::new(vec![event()], "a".repeat(64), false).unwrap();
        let tx = connection.unchecked_transaction().unwrap();
        assert!(register_received(&tx, &bundle).unwrap());
        assert!(!register_received(&tx, &bundle).unwrap());
        tx.commit().unwrap();
    }
}

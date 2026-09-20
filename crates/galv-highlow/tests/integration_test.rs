use galv_highlow::{
    apply::apply_bundle,
    manifest_filename, open, pending_events, record_outbox, register_received, seal,
    transport::{fetch_pair, list_manifests, publish, TransportConfig, TransportKind},
    Bundle, Column, Event, Operation, Value,
};
use rsa::{
    pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding},
    RsaPrivateKey, RsaPublicKey,
};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn test_end_to_end_highlow_replication() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup keys
    let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)?;
    let public = RsaPublicKey::from(&private);
    let rsa_pub_pem = public.to_public_key_pem(LineEnding::LF)?;
    let rsa_priv_pem = private.to_pkcs8_pem(LineEnding::LF)?;

    let sender_signing_key_hex = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
    let sender_pub = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&hex::decode(sender_signing_key_hex)?.try_into().unwrap())
            .verifying_key()
            .to_bytes(),
    );

    // 2. Setup shared transport staging directory
    let transport_dir = tempdir()?;
    let transport_config = TransportConfig {
        kind: TransportKind::Directory,
        endpoint: transport_dir.path().display().to_string(),
        username: None,
        password: None,
        bearer_token: None,
    };

    // 3. Setup Low DB
    let low_conn = Connection::open_in_memory()?;
    galv_highlow::initialize_store(&low_conn)?;

    let event1 = Event {
        stream_id: "plant-1".into(),
        sequence: 1,
        transaction_id: "actor1:1".into(),
        table: "telemetry".into(),
        primary_key: serde_json::Map::from_iter([(
            "sensor_id".to_string(),
            serde_json::json!({"type": "text", "value": "temp-01"}),
        )]),
        operation: Operation::Upsert,
        columns: vec![
            Column {
                name: "reading".into(),
                value: Value::Real(23.5),
            },
            Column {
                name: "status".into(),
                value: Value::Text("nominal".into()),
            },
        ],
        source_actor: "actor-1".into(),
        committed_at_ms: 1700000000,
    };

    let event2 = Event {
        stream_id: "plant-1".into(),
        sequence: 2,
        transaction_id: "actor1:2".into(),
        table: "telemetry".into(),
        primary_key: serde_json::Map::from_iter([(
            "sensor_id".to_string(),
            serde_json::json!({"type": "text", "value": "temp-02"}),
        )]),
        operation: Operation::Upsert,
        columns: vec![
            Column {
                name: "reading".into(),
                value: Value::Real(45.2),
            },
            Column {
                name: "status".into(),
                value: Value::Text("alert".into()),
            },
        ],
        source_actor: "actor-1".into(),
        committed_at_ms: 1700000010,
    };

    let tx = low_conn.unchecked_transaction()?;
    galv_highlow::append_events(&tx, &[event1.clone(), event2.clone()])?;
    tx.commit()?;

    // 4. Low Exporter packaging
    let events = pending_events(&low_conn, "plant-1", 1000)?;
    assert_eq!(events.len(), 2);

    let schema_hash = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let bundle = Bundle::new(events, schema_hash.into(), false)?;
    let payload_filename = format!("plant-1-{:016}-{}.zstd.galvh", bundle.sequence_last, &bundle.bundle_id[..8]);
    let manifest_name = manifest_filename(&payload_filename)?;

    let artifacts = seal(
        bundle.clone(),
        "high-rsa-key",
        &rsa_pub_pem,
        sender_signing_key_hex,
        &payload_filename,
    )?;

    let tx = low_conn.unchecked_transaction()?;
    record_outbox(&tx, &artifacts, &payload_filename, &manifest_name)?;
    tx.commit()?;

    publish(&transport_config, &artifacts)?;

    let tx = low_conn.unchecked_transaction()?;
    galv_highlow::mark_uploaded(&tx, &artifacts.manifest.bundle_id)?;
    tx.commit()?;

    // Verify Low pending is now empty
    let remaining = pending_events(&low_conn, "plant-1", 1000)?;
    assert_eq!(remaining.len(), 0);

    // 5. High Receiver Ingestion
    let manifests = list_manifests(&transport_config)?;
    assert_eq!(manifests, vec![manifest_name.clone()]);

    let (fetched_manifest_bytes, fetched_payload_bytes) = fetch_pair(&transport_config, &manifest_name)?;
    let received_bundle = open(
        &fetched_manifest_bytes,
        &fetched_payload_bytes,
        &rsa_priv_pem,
        &[sender_pub],
        schema_hash,
    )?;

    assert_eq!(received_bundle.bundle_id, bundle.bundle_id);
    assert_eq!(received_bundle.events.len(), 2);

    // 6. High DB apply
    let high_conn = Connection::open_in_memory()?;
    galv_highlow::initialize_store(&high_conn)?;
    high_conn.execute_batch(
        "CREATE TABLE telemetry (
            sensor_id TEXT PRIMARY KEY,
            reading REAL,
            status TEXT
        );",
    )?;

    let tx = high_conn.unchecked_transaction()?;
    let is_new = register_received(&tx, &received_bundle)?;
    assert!(is_new);
    let stats = apply_bundle(&tx, &received_bundle)?;
    tx.commit()?;

    assert_eq!(stats.upserts, 2);
    assert_eq!(stats.deletes, 0);

    // Verify row contents on High side
    let reading1: f64 = high_conn.query_row(
        "SELECT reading FROM telemetry WHERE sensor_id = 'temp-01'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(reading1, 23.5);

    let status2: String = high_conn.query_row(
        "SELECT status FROM telemetry WHERE sensor_id = 'temp-02'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(status2, "alert");

    Ok(())
}

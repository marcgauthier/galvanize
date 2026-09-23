pub mod airgap;
pub mod sealed_file;

use std::{
    fs,
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use getrandom::fill;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::debug;

pub const FILE_STORAGE_SALT: &[u8] = b"GALVANIZE_FILE_STORAGE_SALT_V1";

#[derive(Debug, Error)]
pub enum FileError {
    #[error("File not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Decryption error: authentication tag or key mismatch")]
    DecryptionFailed,

    #[error("Encryption error: {0}")]
    EncryptionFailed(String),

    #[error("Checksum mismatch for file {uuid}: expected {expected}, actual {actual}")]
    ChecksumMismatch {
        uuid: String,
        expected: String,
        actual: String,
    },

    #[error("File size limit exceeded: {actual} > {limit}")]
    SizeLimitExceeded { actual: u64, limit: u64 },

    #[error("Invalid UUID or filename: {0}")]
    InvalidIdentifier(String),

    #[error("Uploads disabled on this node")]
    UploadsDisabled,

    #[error("Peer file fetch failed: {0}")]
    PeerFetchFailed(String),

    #[error("Air-gap transport error: {0}")]
    Transport(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Metadata record representing a stored file in the Galvanize cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileRecord {
    pub uuid: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub content_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Option<serde_json::Value>,
}

/// Local file availability status in node-local storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalFileStatus {
    pub uuid: String,
    pub status: String, // "available", "downloading", "failed"
    pub downloaded_at: String,
    pub error: Option<String>,
}

/// Request payload for file search.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileSearchQuery {
    pub name: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Summary stats of the file storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStorageStats {
    pub total_files: usize,
    pub total_bytes: u64,
    pub local_cached_files: usize,
    pub local_cached_bytes: u64,
    pub missing_local_files: usize,
}

/// Derives a 32-byte XChaCha20-Poly1305 key from an active key passphrase.
pub fn derive_file_key(secret: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(FILE_STORAGE_SALT);
    hasher.update(secret.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypts plaintext bytes using XChaCha20-Poly1305 with a random 24-byte nonce.
pub fn encrypt_payload(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, FileError> {
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| FileError::EncryptionFailed(e.to_string()))?;
    let mut nonce_bytes = [0u8; 24];
    fill(&mut nonce_bytes)
        .map_err(|e| FileError::EncryptionFailed(format!("failed to generate random nonce: {e}")))?;
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| FileError::EncryptionFailed(e.to_string()))?;

    let mut out = Vec::with_capacity(24 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypts ciphertext bytes encrypted with `encrypt_payload`.
pub fn decrypt_payload(encrypted: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, FileError> {
    if encrypted.len() < 24 {
        return Err(FileError::DecryptionFailed);
    }
    let (nonce_bytes, ciphertext) = encrypted.split_at(24);
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| FileError::DecryptionFailed)?;
    let nonce = XNonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| FileError::DecryptionFailed)
}

/// Calculates the SHA-256 hex digest of a byte slice.
pub fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Validates that a UUID/file identifier string contains only safe alphanumeric and hyphen characters.
pub fn validate_file_uuid(uuid_str: &str) -> Result<(), FileError> {
    if uuid_str.is_empty()
        || uuid_str.len() > 64
        || !uuid_str
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(FileError::InvalidIdentifier(format!(
            "invalid file uuid: '{uuid_str}'"
        )));
    }
    Ok(())
}

/// Manager for local encrypted-at-rest file storage.
#[derive(Clone, Debug)]
pub struct EncryptedFileStore {
    base_path: PathBuf,
}

impl EncryptedFileStore {
    pub fn new<P: AsRef<Path>>(base_path: P) -> std::io::Result<Self> {
        let p = base_path.as_ref().to_path_buf();
        fs::create_dir_all(&p)?;
        Ok(Self { base_path: p })
    }

    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    fn file_path(&self, uuid_str: &str) -> PathBuf {
        self.base_path.join(format!("{uuid_str}.enc"))
    }

    pub fn has_file(&self, uuid_str: &str) -> bool {
        if validate_file_uuid(uuid_str).is_err() {
            return false;
        }
        self.file_path(uuid_str).is_file()
    }

    pub fn file_size_on_disk(&self, uuid_str: &str) -> Option<u64> {
        if validate_file_uuid(uuid_str).is_err() {
            return None;
        }
        fs::metadata(self.file_path(uuid_str)).ok().map(|m| m.len())
    }

    pub fn save_file(
        &self,
        uuid_str: &str,
        plaintext: &[u8],
        key: &[u8; 32],
    ) -> Result<u64, FileError> {
        validate_file_uuid(uuid_str)?;
        let encrypted = encrypt_payload(plaintext, key)?;
        let final_path = self.file_path(uuid_str);
        let temp_path = self.base_path.join(format!(".{uuid_str}.tmp"));

        fs::write(&temp_path, &encrypted)?;
        fs::rename(&temp_path, &final_path)?;

        debug!(uuid = %uuid_str, size = plaintext.len(), "saved encrypted file to disk");
        Ok(plaintext.len() as u64)
    }

    pub fn read_file(&self, uuid_str: &str, key: &[u8; 32]) -> Result<Vec<u8>, FileError> {
        validate_file_uuid(uuid_str)?;
        let path = self.file_path(uuid_str);
        if !path.exists() {
            return Err(FileError::NotFound(uuid_str.to_string()));
        }
        let encrypted = fs::read(path)?;
        decrypt_payload(&encrypted, key)
    }

    pub fn delete_file(&self, uuid_str: &str) -> Result<bool, FileError> {
        validate_file_uuid(uuid_str)?;
        let path = self.file_path(uuid_str);
        if path.exists() {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_encryption_roundtrip() {
        let key = derive_file_key("test_passphrase_123");
        let payload = b"Hello, Galvanize Encrypted File Storage!";
        let encrypted = encrypt_payload(payload, &key).expect("encryption failed");
        assert_ne!(&encrypted[24..], payload);

        let decrypted = decrypt_payload(&encrypted, &key).expect("decryption failed");
        assert_eq!(&decrypted, payload);

        let wrong_key = derive_file_key("wrong_passphrase");
        assert!(decrypt_payload(&encrypted, &wrong_key).is_err());
    }

    #[test]
    fn test_file_store_operations() {
        let dir = tempdir().unwrap();
        let store = EncryptedFileStore::new(dir.path()).unwrap();
        let key = derive_file_key("node_secret_key");
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let content = b"Sample configuration and state binary data.";

        assert!(!store.has_file(uuid));
        let size = store.save_file(uuid, content, &key).unwrap();
        assert_eq!(size, content.len() as u64);
        assert!(store.has_file(uuid));

        let loaded = store.read_file(uuid, &key).unwrap();
        assert_eq!(loaded, content);

        assert!(store.delete_file(uuid).unwrap());
        assert!(!store.has_file(uuid));
        assert!(matches!(store.read_file(uuid, &key), Err(FileError::NotFound(_))));
    }
}

use std::path::Path;
use rusqlite::Connection;
use thiserror::Error;
use tracing::info;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Key payload containing the encryption key and optional cipher configuration.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct KeyPayload {
    pub key: String,
    #[zeroize(skip)]
    pub cipher: Option<String>,
    #[zeroize(skip)]
    pub cipher_params: Option<String>,
}

impl std::fmt::Debug for KeyPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyPayload")
            .field("key", &"[REDACTED]")
            .field("cipher", &self.cipher)
            .field("cipher_params", &self.cipher_params)
            .finish()
    }
}

/// Rekey error types
#[derive(Debug, Error)]
pub enum RekeyError {
    #[error("Database file not found: {0}")]
    FileNotFound(String),

    #[error("Invalid current encryption key or cipher: {0}")]
    InvalidCurrentKey(rusqlite::Error),

    #[error("Failed to execute rekey operation: {0}")]
    RekeyFailed(rusqlite::Error),

    #[error("Failed to verify database after rekey: {0}")]
    VerificationFailed(rusqlite::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Reads encryption configuration from environment variables:
/// - `GALVANIZE_DB_KEY` (or `GALVANIZE_DB_PASSPHRASE`)
/// - `GALVANIZE_DB_CIPHER` (optional)
/// - `GALVANIZE_DB_CIPHER_PARAMS` (optional)
pub fn get_env_key() -> Option<KeyPayload> {
    let key = std::env::var("GALVANIZE_DB_KEY")
        .or_else(|_| std::env::var("GALVANIZE_DB_PASSPHRASE"))
        .ok()?;

    if key.is_empty() {
        return None;
    }

    let cipher = std::env::var("GALVANIZE_DB_CIPHER").ok().filter(|s| !s.is_empty());
    let cipher_params = std::env::var("GALVANIZE_DB_CIPHER_PARAMS").ok().filter(|s| !s.is_empty());

    Some(KeyPayload {
        key,
        cipher,
        cipher_params,
    })
}

/// Applies SQLite3MC cipher settings and encryption key to an open connection.
pub fn apply_encryption_pragma(conn: &mut Connection, key: &KeyPayload) -> Result<(), rusqlite::Error> {
    if let Some(cipher) = &key.cipher {
        conn.pragma_update(None, "cipher", cipher)?;
    }
    if let Some(params) = &key.cipher_params {
        conn.pragma_update(None, "cipher_params", params)?;
    }
    conn.pragma_update(None, "key", &key.key)?;
    Ok(())
}

/// Rekey an existing SQLite database file offline.
pub fn rekey_database(
    db_path: &Path,
    current_key: Option<&KeyPayload>,
    new_key: &KeyPayload,
) -> Result<(), RekeyError> {
    if !db_path.exists() {
        return Err(RekeyError::FileNotFound(db_path.display().to_string()));
    }

    info!("Opening database at {} for rekeying...", db_path.display());

    // 1. Open with current key
    {
        let mut conn = Connection::open(db_path)?;

        if let Some(cur_key) = current_key {
            apply_encryption_pragma(&mut conn, cur_key)?;
        }

        // Verify that current key works
        let res: Result<i64, _> = conn.query_row("SELECT count(*) FROM sqlite_master;", [], |row| row.get(0));
        if let Err(e) = res {
            return Err(RekeyError::InvalidCurrentKey(e));
        }

        // Configure new cipher if specified
        if let Some(new_cipher) = &new_key.cipher {
            conn.pragma_update(None, "cipher", new_cipher)?;
        }
        if let Some(new_params) = &new_key.cipher_params {
            conn.pragma_update(None, "cipher_params", new_params)?;
        }

        // Execute PRAGMA rekey
        conn.pragma_update(None, "rekey", &new_key.key)
            .map_err(RekeyError::RekeyFailed)?;

        // Connection closes here when dropped
    }

    // 2. Verification: Open with new key and verify
    {
        let mut conn = Connection::open(db_path)?;
        apply_encryption_pragma(&mut conn, new_key)?;

        let count: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master;", [], |row| row.get(0))
            .map_err(RekeyError::VerificationFailed)?;

        info!(
            "Rekey verification successful! Master table entries count: {}",
            count
        );
    }

    Ok(())
}

/// Interactive CLI prompt for offline rekeying.
pub fn interactive_rekey(
    db_path: &Path,
    current_cipher: Option<&str>,
    new_cipher: Option<&str>,
) -> eyre::Result<()> {
    if !db_path.exists() {
        eyre::bail!("Database file not found at: {}", db_path.display());
    }

    println!("\n=== Corrosion Database Offline Rekey ===");
    println!("Target Database: {}\n", db_path.display());

    let current_pass = rpassword::prompt_password("Enter current encryption key (leave empty if unencrypted): ")?;
    let new_pass = rpassword::prompt_password("Enter NEW encryption key: ")?;

    if new_pass.trim().is_empty() {
        eyre::bail!("New encryption key cannot be empty.");
    }

    let confirm_pass = rpassword::prompt_password("Confirm NEW encryption key: ")?;

    if new_pass != confirm_pass {
        eyre::bail!("New encryption keys do not match. Rekey aborted.");
    }

    let current_key = if !current_pass.is_empty() {
        Some(KeyPayload {
            key: current_pass,
            cipher: current_cipher.map(String::from),
            cipher_params: None,
        })
    } else {
        None
    };

    let new_key = KeyPayload {
        key: new_pass,
        cipher: new_cipher.map(String::from),
        cipher_params: None,
    };

    println!("\nRekeying database...");
    rekey_database(db_path, current_key.as_ref(), &new_key)?;

    println!("SUCCESS: Database at '{}' successfully rekeyed!\n", db_path.display());
    Ok(())
}

/// Rekey without a terminal prompt. The arguments name environment variables,
/// rather than carrying key material, so this is safe for service automation
/// and the live-node test harness.
pub fn rekey_from_environment(
    db_path: &Path,
    current_key_env: Option<&str>,
    new_key_env: &str,
    current_cipher: Option<&str>,
    new_cipher: Option<&str>,
) -> eyre::Result<()> {
    let current_key = match current_key_env {
        Some(name) => Some(KeyPayload {
            key: std::env::var(name)
                .map_err(|_| eyre::eyre!("current key environment variable {name} is not set"))?,
            cipher: current_cipher.map(str::to_owned),
            cipher_params: None,
        }),
        None => None,
    };
    let new_key = KeyPayload {
        key: std::env::var(new_key_env)
            .map_err(|_| eyre::eyre!("new key environment variable {new_key_env} is not set"))?,
        cipher: new_cipher.map(str::to_owned),
        cipher_params: None,
    };
    if new_key.key.is_empty() {
        eyre::bail!("new key environment variable {new_key_env} is empty");
    }
    rekey_database(db_path, current_key.as_ref(), &new_key)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_rekey_requires_a_new_key() {
        let result = rekey_from_environment(
            Path::new("/definitely/not/a/database"),
            None,
            "GALV_REKEY_TEST_MISSING_KEY",
            None,
            None,
        );
        assert!(result.is_err());
    }
}

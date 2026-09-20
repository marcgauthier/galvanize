use galv_rekey_cli::{apply_encryption_pragma, get_env_key, rekey_database, KeyPayload};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn test_rekey_unencrypted_to_encrypted() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db_path = dir.path().join("plain.db");

    // 1. Create plain database with some data
    {
        let conn = Connection::open(&db_path)?;
        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);", [])?;
        conn.execute("INSERT INTO users (name) VALUES ('alice');", [])?;
    }

    // 2. Rekey to encrypted with chacha20
    let new_key = KeyPayload {
        key: "secret123".into(),
        cipher: Some("chacha20".into()),
        cipher_params: None,
    };

    rekey_database(&db_path, None, &new_key)?;

    // 3. Opening plain without key should fail to read data
    {
        let plain_conn = Connection::open(&db_path)?;
        let res: Result<String, _> = plain_conn.query_row("SELECT name FROM users WHERE id = 1;", [], |row| row.get(0));
        assert!(res.is_err(), "Opening encrypted DB without key must fail");
    }

    // 4. Opening with new key should succeed
    {
        let mut enc_conn = Connection::open(&db_path)?;
        apply_encryption_pragma(&mut enc_conn, &new_key)?;
        let name: String = enc_conn.query_row("SELECT name FROM users WHERE id = 1;", [], |row| row.get(0))?;
        assert_eq!(name, "alice");
    }

    Ok(())
}

#[test]
fn test_rekey_encrypted_to_new_key_and_cipher() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db_path = dir.path().join("enc.db");

    let initial_key = KeyPayload {
        key: "pass-one".into(),
        cipher: Some("chacha20".into()),
        cipher_params: None,
    };

    // 1. Create encrypted database
    {
        let mut conn = Connection::open(&db_path)?;
        apply_encryption_pragma(&mut conn, &initial_key)?;
        conn.execute("CREATE TABLE records (id INTEGER PRIMARY KEY, val TEXT);", [])?;
        conn.execute("INSERT INTO records (val) VALUES ('confidential');", [])?;
    }

    // 2. Rekey to new key and aegis cipher
    let new_key = KeyPayload {
        key: "pass-two-rotated".into(),
        cipher: Some("aegis".into()),
        cipher_params: None,
    };

    rekey_database(&db_path, Some(&initial_key), &new_key)?;

    // 3. Old key must fail
    {
        let mut old_conn = Connection::open(&db_path)?;
        apply_encryption_pragma(&mut old_conn, &initial_key)?;
        let res: Result<String, _> = old_conn.query_row("SELECT val FROM records WHERE id = 1;", [], |row| row.get(0));
        assert!(res.is_err(), "Old key must fail after rekeying");
    }

    // 4. New key must succeed
    {
        let mut new_conn = Connection::open(&db_path)?;
        apply_encryption_pragma(&mut new_conn, &new_key)?;
        let val: String = new_conn.query_row("SELECT val FROM records WHERE id = 1;", [], |row| row.get(0))?;
        assert_eq!(val, "confidential");
    }

    Ok(())
}

#[test]
fn test_get_env_key() {
    std::env::set_var("GALVANIZE_DB_KEY", "env-secret-999");
    std::env::set_var("GALVANIZE_DB_CIPHER", "chacha20");
    std::env::set_var("GALVANIZE_DB_CIPHER_PARAMS", "kdf_iter=32000");

    let payload = get_env_key().expect("env key should be parsed");
    assert_eq!(payload.key, "env-secret-999");
    assert_eq!(payload.cipher.as_deref(), Some("chacha20"));
    assert_eq!(payload.cipher_params.as_deref(), Some("kdf_iter=32000"));

    std::env::remove_var("GALVANIZE_DB_KEY");
    std::env::remove_var("GALVANIZE_DB_CIPHER");
    std::env::remove_var("GALVANIZE_DB_CIPHER_PARAMS");
}

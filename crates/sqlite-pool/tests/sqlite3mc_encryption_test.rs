use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn test_sqlite3mc_encryption_and_decryption() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db_path = dir.path().join("encrypted.db");

    // 1. Create encrypted database
    {
        let conn = Connection::open(&db_path)?;

        // Check that SQLite3MC is active
        let mc_version: String = conn.query_row("SELECT sqlite3mc_version();", [], |row| row.get(0))?;
        println!("SQLite3MC version: {mc_version}");
        assert!(mc_version.contains("SQLite3 Multiple Ciphers"), "Must report SQLite3MC version string");

        // Set key
        conn.execute_batch("PRAGMA key = 'my_super_secret_key_123';")?;

        // Create table and insert sensitive data
        conn.execute(
            "CREATE TABLE vault (id INTEGER PRIMARY KEY, payload TEXT NOT NULL);",
            [],
        )?;
        conn.execute(
            "INSERT INTO vault (id, payload) VALUES (1, 'classified-galvanize-encryption-data');",
            [],
        )?;

        // Close connection cleanly
    }

    // 2. Verify raw file on disk is encrypted (no plain SQLite magic header or plaintext string)
    let raw_bytes = std::fs::read(&db_path)?;
    assert!(
        !raw_bytes.is_empty(),
        "Database file should not be empty"
    );
    let raw_string = String::from_utf8_lossy(&raw_bytes);
    assert!(
        !raw_string.contains("classified-galvanize-encryption-data"),
        "Raw database file must not contain plaintext payload!"
    );

    // 3. Opening with wrong password MUST fail to query table
    {
        let wrong_conn = Connection::open(&db_path)?;
        wrong_conn.execute_batch("PRAGMA key = 'wrong_password';")?;
        let res: Result<String, rusqlite::Error> =
            wrong_conn.query_row("SELECT payload FROM vault WHERE id = 1;", [], |row| row.get(0));
        assert!(
            res.is_err(),
            "Querying with wrong key must return an error, got: {:?}",
            res
        );
    }

    // 4. Opening with NO password MUST fail to query table
    {
        let no_key_conn = Connection::open(&db_path)?;
        let res: Result<String, rusqlite::Error> =
            no_key_conn.query_row("SELECT payload FROM vault WHERE id = 1;", [], |row| row.get(0));
        assert!(
            res.is_err(),
            "Querying with no key must return an error, got: {:?}",
            res
        );
    }

    // 5. Opening with CORRECT password MUST succeed
    {
        let correct_conn = Connection::open(&db_path)?;
        correct_conn.execute_batch("PRAGMA key = 'my_super_secret_key_123';")?;
        let payload: String =
            correct_conn.query_row("SELECT payload FROM vault WHERE id = 1;", [], |row| row.get(0))?;
        assert_eq!(payload, "classified-galvanize-encryption-data");
    }

    // 6. Test re-keying to a new password
    {
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA key = 'my_super_secret_key_123';")?;
        conn.execute_batch("PRAGMA rekey = 'new_updated_password_456';")?;
    }

    // 7. Verify old password no longer works and new password works
    {
        let old_conn = Connection::open(&db_path)?;
        old_conn.execute_batch("PRAGMA key = 'my_super_secret_key_123';")?;
        let res: Result<String, rusqlite::Error> =
            old_conn.query_row("SELECT payload FROM vault WHERE id = 1;", [], |row| row.get(0));
        assert!(res.is_err(), "Old password should no longer work after rekey");

        let new_conn = Connection::open(&db_path)?;
        new_conn.execute_batch("PRAGMA key = 'new_updated_password_456';")?;
        let payload: String =
            new_conn.query_row("SELECT payload FROM vault WHERE id = 1;", [], |row| row.get(0))?;
        assert_eq!(payload, "classified-galvanize-encryption-data");
    }

    Ok(())
}

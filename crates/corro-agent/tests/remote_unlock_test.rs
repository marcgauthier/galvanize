use std::time::Duration;
use corro_agent::agent::start_with_config;
use corro_types::{
    api::{ExecResponse, ExecResult, HealthResponse, Statement, UnlockRequest, UnlockResponse},
    config::Config,
};
use galv_rekey_cli::{apply_encryption_pragma, KeyPayload};
use rusqlite::Connection;
use tripwire::Tripwire;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_remote_unlock_lifecycle() -> eyre::Result<()> {
    _ = tracing_subscriber::fmt::try_init();

    // Ensure clean state before test
    galv_rekey_cli::clear_active_key();

    let tmpdir = tempfile::tempdir()?;
    let db_path = tmpdir.path().join("corrosion.db");
    let schema_dir = tmpdir.path().join("schema");
    std::fs::create_dir_all(&schema_dir)?;
    std::fs::write(
        schema_dir.join("01_init.sql"),
        "CREATE TABLE items (id INTEGER NOT NULL PRIMARY KEY, name TEXT NOT NULL DEFAULT '') WITHOUT ROWID;",
    )?;

    let valid_key = "remote-unlock-test-key-12345";
    let cipher = "aegis";
    let cipher_params = "tcost=2,mcost=19456,pcost=1";

    // 1. Create an encrypted SQLite3MC database offline
    {
        let mut conn = Connection::open(&db_path)?;
        let key_payload = KeyPayload {
            key: valid_key.to_string(),
            cipher: Some(cipher.to_string()),
            cipher_params: Some(cipher_params.to_string()),
        };
        apply_encryption_pragma(&mut conn, &key_payload)?;
        conn.execute_batch("CREATE TABLE seed (id INTEGER PRIMARY KEY);")?;
    }

    // 2. Start agent with config (no env vars set)
    let (tripwire, _tripwire_worker, _tripwire_tx) = Tripwire::new_simple();
    let conf = Config::builder()
        .db_path(db_path.display().to_string())
        .api_addr("127.0.0.1:0".parse()?)
        .gossip_addr("127.0.0.1:0".parse()?)
        .admin_path(tmpdir.path().join("admin.sock").display().to_string())
        .add_schema_path(schema_dir.display().to_string())
        .build()?;

    let (agent, _bookie, _transport, _handles) =
        start_with_config(conf.clone(), tripwire.clone()).await?;

    let api_addr = agent.api_addr();
    let client = reqwest::Client::new();
    let base_url = format!("http://{api_addr}");

    // Wait a brief moment for the HTTP listener to accept connections
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Health check should report "awaiting_unlock"
    let res = client
        .get(format!("{base_url}/v1/health"))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let bytes = res.bytes().await?;
    let health: HealthResponse = serde_json::from_slice(&bytes)?;
    match health {
        HealthResponse::AwaitingUnlock { status, .. } => {
            assert_eq!(status, "awaiting_unlock");
        }
        other => panic!("Expected AwaitingUnlock health response, got: {other:?}"),
    }

    // 4. Queries should return 503 Service Unavailable while locked
    let res = client
        .post(format!("{base_url}/v1/queries"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&Statement::Simple("SELECT 1;".into()))?)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);

    // 5. Transactions should return 503 Service Unavailable while locked
    let res = client
        .post(format!("{base_url}/v1/transactions"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&vec![Statement::Simple("INSERT INTO items (id, name) VALUES (1, 'test');".into())])?)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);

    // 6. Unlock attempt with wrong key should return 401 Unauthorized
    let wrong_unlock_req = UnlockRequest {
        key: "wrong-passphrase".to_string(),
        cipher: Some(cipher.to_string()),
        cipher_params: Some(cipher_params.to_string()),
    };
    let res = client
        .post(format!("{base_url}/v1/admin/unlock"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&wrong_unlock_req)?)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::UNAUTHORIZED);
    let bytes = res.bytes().await?;
    let unlock_res: UnlockResponse = serde_json::from_slice(&bytes)?;
    assert!(matches!(unlock_res, UnlockResponse::Error { .. }));

    // 7. Unlock with valid 3-field JSON payload
    let valid_unlock_req = UnlockRequest {
        key: valid_key.to_string(),
        cipher: Some(cipher.to_string()),
        cipher_params: Some(cipher_params.to_string()),
    };
    let res = client
        .post(format!("{base_url}/v1/admin/unlock"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&valid_unlock_req)?)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let bytes = res.bytes().await?;
    let unlock_res: UnlockResponse = serde_json::from_slice(&bytes)?;
    assert_eq!(
        unlock_res,
        UnlockResponse::Ok {
            status: "unlocked".into()
        }
    );

    // Wait a brief moment for background services to wake up
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 8. Subsequent unlock should return 409 Conflict
    let res = client
        .post(format!("{base_url}/v1/admin/unlock"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&valid_unlock_req)?)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::CONFLICT);

    // 9. Health check should now report "ok" / normal metrics
    let res = client
        .get(format!("{base_url}/v1/health"))
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let bytes = res.bytes().await?;
    let health: HealthResponse = serde_json::from_slice(&bytes)?;
    match health {
        HealthResponse::Response { .. } => {}
        other => panic!("Expected healthy node response, got: {other:?}"),
    }

    // 10. Transactions should now succeed
    let res = client
        .post(format!("{base_url}/v1/transactions"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&vec![Statement::Simple("INSERT INTO items (id, name) VALUES (100, 'widget');".into())])?)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let bytes = res.bytes().await?;
    let exec_res: ExecResponse = serde_json::from_slice(&bytes)?;
    assert_eq!(exec_res.results.len(), 1);
    match &exec_res.results[0] {
        ExecResult::Execute { rows_affected, .. } => {
            assert_eq!(*rows_affected, 1);
        }
        ExecResult::Error { error } => panic!("Transaction failed: {error}"),
    }

    // 11. Queries should now succeed
    let res = client
        .post(format!("{base_url}/v1/queries"))
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&Statement::Simple("SELECT name FROM items WHERE id = 100;".into()))?)
        .send()
        .await?;
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let body_text = res.text().await?;
    assert!(body_text.contains("widget"));

    // Cleanup active key
    galv_rekey_cli::clear_active_key();

    Ok(())
}

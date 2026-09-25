# GALVANIZE Rust client

Async Rust SDK for GALVANIZE's public HTTP(S), dedicated mTLS control APIs, and
PostgreSQL wire listener. SQL goes through `tokio-postgres`, never the node's
SQLite file. This crate is standalone under `clients/rust`.

```rust,no_run
use galvanize_client::{connect_postgres, Client};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder("https://db.example")
        .admin_url("https://db.example:8444")
        .highlow_url("https://high.example:8443")
        .bearer_token(std::env::var("GALVANIZE_API_TOKEN")?)
        .ca_file("ca.pem")?
        .identity_file("client-identity.pem")?
        .build()?;

    let dsn = std::env::var("GALVANIZE_PG_DSN")?;
    let ca = std::fs::read("ca.pem")?;
    let (pg, connection) = connect_postgres(&dsn, &ca, None).await?;
    tokio::spawn(async move { let _ = connection.await; });
    pg.execute("INSERT INTO notes(id, body) VALUES($1, $2)", &[&1_i64, &"hello"]).await?;

    let mut events = client.query(&json!("SELECT id, body FROM notes"), None).await?;
    while let Some(event) = events.next().await? { println!("{event}"); }
    Ok(())
}
```

`connect_postgres` enforces TLS and takes a PEM CA plus an optional combined
client certificate/private-key PEM. `connect_postgres_from_env` reads the DSN
from its named argument and requires `GALVANIZE_PG_CA_FILE`; optionally set
`GALVANIZE_PG_IDENTITY_PEM_FILE` to the client identity PEM. HTTPS
`ClientBuilder` supports a CA PEM and combined mTLS identity PEM. The SDK
provides query and NDJSON event streams, transactions, subscriptions/resume,
table updates/stats, health, unlock-from-environment, file operations, High/Low
status/replay/provenance, and remote admin commands. Schema reload reads the
node's configured schema paths; schema upload and local backup/restore are not
remote API operations.

Run `cargo test --manifest-path Cargo.toml` from this directory.

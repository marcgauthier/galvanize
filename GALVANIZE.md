# GALVANIZE

GALVANIZE is a fork of `superfly/corrosion` that adds encryption at rest using SQLite3 Multiple Ciphers (SQLite3MC).

## Upstream Tracking & Changes Log

### 1. Vendored SQLite3MC Crate
- **Crate:** [`crates/galv-libsqlite3-sys`](file:///home/marc/GALVANIZE/crates/galv-libsqlite3-sys)
- **Version:** `0.36.0` (matching `rusqlite 0.38.0` dependency)
- **SQLite3MC Version:** `2.5.1` (bundled with SQLite `3.53.4`)
- **Features:** Static compilation of SQLite3MC amalgamation (`-DSQLITE3MC_STATIC=1`) with support for ChaCha20-Poly1305, AES-256-CBC, SQLCipher, wxSQLite3, and Ascon-128.

### 2. Environment Variable Key Management & Offline CLI Rekey
- **Crate:** [`crates/galv-rekey-cli`](file:///home/marc/GALVANIZE/crates/galv-rekey-cli)
- **Environment Variables**:
  - `GALVANIZE_DB_KEY` (or `GALVANIZE_DB_PASSPHRASE`): Encryption passphrase.
  - `GALVANIZE_DB_CIPHER` (optional): `chacha20` (default), `aegis`, `aes256cbc`, `sqlcipher`, `ascon128`, etc.
  - `GALVANIZE_DB_CIPHER_PARAMS` (optional): Extra cipher parameters.
- **Offline CLI Command**: `corrosion rekey`
  - Interactive hidden prompts for current key, new key, and confirmation.
  - Verifies corrosion daemon is not running before rekeying.
  - Safely rekeys SQLite database using SQLite3MC and verifies data integrity.
  - Memory security: Passwords are automatically zeroized with `zeroize::ZeroizeOnDrop`.
  - Automation: `--current-key-env NAME --new-key-env NAME` reads keys from named environment variables without exposing them in process arguments. The original prompt flow remains the default.

### 3. Root Workspace Configuration
- **File:** [`Cargo.toml`](file:///home/marc/GALVANIZE/Cargo.toml)
- Added `[patch.crates-io]` section:
  ```toml
  [patch.crates-io]
  libsqlite3-sys = { path = "crates/galv-libsqlite3-sys" }
  ```

### 4. Tests & Verification
- **SQLite3MC Static Encryption Test:** [`crates/sqlite-pool/tests/sqlite3mc_encryption_test.rs`](file:///home/marc/GALVANIZE/crates/sqlite-pool/tests/sqlite3mc_encryption_test.rs)
- **Offline CLI Rekey Tests:** [`crates/galv-rekey-cli/tests/rekey_test.rs`](file:///home/marc/GALVANIZE/crates/galv-rekey-cli/tests/rekey_test.rs)

### 5. Galvanize High/Low Air-Gap Replication
- **Crate:** `crates/galv-highlow` (Galvanize-specific; no Vaultmesh wire compatibility).
- **Format:** `galvanize-highlow/1` bundles are zstd-compressed (level 15), encrypted with a random XChaCha20-Poly1305 content key, RSA-OAEP (SHA-256) wrapped for the High receiver, and accompanied by an Ed25519-signed manifest. Payloads use `.zstd.galvh`; manifests use `.json.galv`.
- **Transport Adapters:** Supports `directory` (local filesystem/USB staging), `http`/`https` (POST artifacts / GET manifests), `ftp`/`ftps`, and `sftp`.
- **Low-side Change Capture:** Intercepts committing local transactions in `crates/corro-types/src/change.rs` (`insert_local_changes`) and replicated mesh transactions in `crates/corro-agent/src/agent/util.rs` (`handle_changes`, `process_fully_buffered_changes`), appending ordered row delta events across the entire Low cluster to `__galv_highlow_events`.
- **Low-side Exporter Background Task:** `spawn_highlow_exporter` in `crates/corro-agent/src/agent/highlow.rs` packages pending events, seals bundles, publishes them on `upload_interval_seconds`, and marks outbox uploaded.
- **High-side Ingestion & Merge:** `spawn_highlow_receiver` in `crates/corro-agent/src/agent/highlow.rs` polls for manifests on `download_interval_seconds`, downloads payloads, authenticates signatures and digests, decrypts bundles, records to `__galv_highlow_inbox`, applies row upserts/deletes via `galv_highlow::apply::apply_bundle`, and broadcasts local changes to High cluster peers.
- **Safety boundaries:** artifact names cannot escape the selected transport directory; manifest/payload/decompressed sizes are bounded; hashes and signatures are verified before ingestion; schema hash mismatch is held as `waiting-schema`; SFTP configuration requires a SHA-256 host-key pin; endpoints cannot contain credentials.
- **Durability:** internal `__galv_highlow_*` SQLite tables hold the Low event journal/outbox and High inbox/stream state. Receipt is idempotent and a missing sequence is recoverable rather than permanently quarantined.
- **Agent lifecycle change:** when enabled, `corro-agent` creates that local journal before CR-SQLite initialization so bookkeeping tables are not enrolled as mesh-replicated user tables.
- **Configuration:** Corrosion has a disabled-by-default `[highlow]` block. Enabling it requires exactly one role: `[highlow.low]`, `[highlow.high]`, or `[highlow.high-replica]`. Low and High require `[highlow.transport]`; secrets are environment-variable names, never inline credentials.

### 6. Gossip Allow-List Replication Control
- **Configuration:** `[gossip.allow-list]` (or `allow_list`).
- **Default:** `["*"]` (wildcard, allowing all nodes to communicate without restriction).
- **Functionality:** Restricts all cluster peer communication (SWIM gossip, broadcast replication, sync bi-streams, datagrams, and discovery bootstrap) to explicitly listed IPv4/IPv6 addresses, CIDR subnets (e.g. `10.0.0.0/8`, `192.168.1.0/24`), or wildcard `*`.
- **Implementation:**
  - `AllowRule` and `AllowList` in [`crates/corro-types/src/config.rs`](file:///home/marc/GALVANIZE/crates/corro-types/src/config.rs) using `ipnet`.
  - Outbound QUIC transport gating in [`crates/corro-agent/src/transport.rs`](file:///home/marc/GALVANIZE/crates/corro-agent/src/transport.rs) with `TransportError::NotAllowed`.
  - Inbound connection refusal in [`crates/corro-agent/src/agent/handlers.rs`](file:///home/marc/GALVANIZE/crates/corro-agent/src/agent/handlers.rs) via `quinn::Incoming::refuse()`.
  - Discovery filtering in `spawn_swim_announcer`.

### 7. Live PostgreSQL-Wire Node Tests
- **Runner:** `bash tests-live/run.sh {encryption|rekey|allow-nodes|highlow|partition|all}` after `cargo build -p corrosion`.
- **Behavior:** each scenario creates isolated `node-*` folders, starts real Galvanize agents, and makes all application SQL requests through PostgreSQL wire listeners using `psql`.
- **Retention:** successful runtime directories are removed. Failed runs are moved to `tests-live/failures/` with encrypted databases and agent logs for diagnosis.
- **High/Low:** the runner executes a full 5-minute ($300\text{s}$) 5-node scenario (3 Low nodes mesh with 1 gateway exporter $\rightarrow$ air-gap staging $\rightarrow$ 2 High nodes mesh with 1 gateway receiver). It validates continuous concurrent PostgreSQL writes from all nodes, one-way air-gap transfer, mesh convergence, High-side data non-overwriting/preservation, and mutation propagation.
- **Partition:** 4-node cluster split-brain test ({A, B} vs {C, D}) testing concurrent partitioned writes, intra-partition isolation and replication, network partition healing, and automatic anti-entropy bi-stream reconciliation to identical SHA-256 state across all 4 nodes.

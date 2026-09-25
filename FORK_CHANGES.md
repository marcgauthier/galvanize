# GALVANIZE

GALVANIZE is a fork of `superfly/corrosion` that adds encryption at rest using SQLite3 Multiple Ciphers (SQLite3MC).

## Upstream Tracking & Changes Log

### 1. Managed SQLite View Support
- **Files:** `crates/corro-types/src/schema.rs`, `crates/corro-agent/src/agent/util.rs`, `crates/corro-agent/src/agent/tests.rs`, and `doc/schema.md`.
- **Behavior:** Managed schema files now accept persistent `CREATE VIEW` (including `IF NOT EXISTS`) and `DROP VIEW IF EXISTS` statements. Views are persisted in `__corro_schema`, restored on startup, and updated transactionally after table/index work.
- **Replication boundary:** Views remain ordinary local SQLite objects. They are never supplied to `crsql_as_crr`, do not create CR-SQLite metadata, and do not independently replicate data.
- **Live coverage:** `tests-live/views` starts three encrypted nodes, writes through the PostgreSQL listener, verifies converged view results and replicated-table SHA-256 snapshots, and confirms the view is absent from `crsql_changes`.

### 2. Vendored SQLite3MC Crate
- **Crate:** [`crates/galv-libsqlite3-sys`](file:///home/marc/GALVANIZE/crates/galv-libsqlite3-sys)
- **Version:** `0.36.0` (matching `rusqlite 0.38.0` dependency)
- **SQLite3MC Version:** `2.5.1` (bundled with SQLite `3.53.4`)
- **Features:** Static compilation of SQLite3MC amalgamation (`-DSQLITE3MC_STATIC=1`) with support for ChaCha20-Poly1305, AES-256-CBC, SQLCipher, wxSQLite3, and Ascon-128.

### 3. Remote Database Unlock API & Offline CLI Rekey
- **Zero Runtime Environment Variable Secret Invariant**: The daemon runtime does not inspect `GALVANIZE_DB_KEY` or environment variables for database keys. All encryption parameters must be supplied dynamically via the Remote Unlock API.
- **Remote Unlock Endpoint**: `POST /v1/admin/unlock` (and `POST /v1/unlock`)
  - **Request Body (JSON)**:
    ```json
    {
      "key": "db_passphrase_here",
      "cipher": "chacha20",
      "cipher_params": ""
    }
    ```
  - **Behavior**:
    - If a database is locked or configured with `await_unlock = true`, the agent boots into `AgentLockState::AwaitingUnlock`.
    - When locked, `GET /v1/health` returns `200 OK` with `{"status": "awaiting_unlock", "db_path": "..."}`.
    - All queries, transactions, subscriptions, and updates return `503 Service Unavailable` with `database is locked / awaiting unlock`.
    - PostgreSQL wire listeners (`corro-pg`), SWIM gossip, QUIC transport, and sync loops are deferred and only launch once unlocked.
    - Supplying invalid keys returns `401 Unauthorized` without crashing the daemon.
    - On valid unlock, the active key is stored in zeroized memory, SQLite pool is created, migrations run, state transitions to `Unlocked`, and background services start.
- **Offline CLI Command**: `galvanize rekey`
  - Interactive hidden prompts for current key, new key, and confirmation.
  - Verifies corrosion daemon is not running before rekeying.
  - Safely rekeys SQLite database using SQLite3MC and verifies data integrity.
  - Memory security: Passwords are automatically zeroized with `zeroize::ZeroizeOnDrop`.
  - Automation: `--current-key-env NAME --new-key-env NAME` reads keys from named environment variables without exposing them in process arguments for offline maintenance scripts.

### 4. Root Workspace Configuration
- **File:** [`Cargo.toml`](file:///home/marc/GALVANIZE/Cargo.toml)
- Added `[patch.crates-io]` section:
  ```toml
  [patch.crates-io]
  libsqlite3-sys = { path = "crates/galv-libsqlite3-sys" }
  ```

### 5. Tests & Verification
- **SQLite3MC Static Encryption Test:** [`crates/sqlite-pool/tests/sqlite3mc_encryption_test.rs`](file:///home/marc/GALVANIZE/crates/sqlite-pool/tests/sqlite3mc_encryption_test.rs)
- **Offline CLI Rekey Tests:** [`crates/galv-rekey-cli/tests/rekey_test.rs`](file:///home/marc/GALVANIZE/crates/galv-rekey-cli/tests/rekey_test.rs)
- **Remote Unlock Integration Test:** [`crates/corro-agent/tests/remote_unlock_test.rs`](file:///home/marc/GALVANIZE/crates/corro-agent/tests/remote_unlock_test.rs)

### 6. Galvanize High/Low Air-Gap Replication
- **Crate:** `crates/galv-highlow` (Galvanize-specific; no Vaultmesh wire compatibility).
- **Format:** `galvanize-highlow/1` bundles are zstd-compressed (level 15), encrypted with a random XChaCha20-Poly1305 content key, RSA-OAEP (SHA-256) wrapped for the High receiver, and accompanied by an Ed25519-signed manifest. Payloads use `.zstd.galvh`; manifests use `.json.galv`.
- **Transport Adapters:** Supports `directory` (local filesystem/USB staging), `http`/`https` (POST artifacts / GET manifests), `ftp`/`ftps`, and `sftp`.
- **Low-side Change Capture:** Automatically intercepts committing local user transactions in `crates/corro-types/src/change.rs` (`insert_local_changes`) and appends ordered row delta events to `__galv_highlow_events`.
- **Low-side Exporter Background Task:** `spawn_highlow_exporter` in `crates/corro-agent/src/agent/highlow.rs` packages pending events, seals bundles, publishes them on `upload_interval_seconds`, and marks outbox uploaded.
- **High-side Ingestion & Merge:** `spawn_highlow_receiver` in `crates/corro-agent/src/agent/highlow.rs` polls for manifests on `download_interval_seconds`, downloads payloads, authenticates signatures and digests, decrypts bundles, records to `__galv_highlow_inbox`, and applies row upserts/deletes via `galv_highlow::apply::apply_bundle`.
- **Safety boundaries:** artifact names cannot escape the selected transport directory; manifest/payload/decompressed sizes are bounded; hashes and signatures are verified before ingestion; schema hash mismatch is held as `waiting-schema`; SFTP configuration requires a SHA-256 host-key pin; endpoints cannot contain credentials.
- **Durability:** internal `__galv_highlow_*` SQLite tables hold the Low event journal/outbox and High inbox/stream state. Receipt is idempotent and a missing sequence is recoverable rather than permanently quarantined.
- **Agent lifecycle change:** when enabled, `corro-agent` creates that local journal before CR-SQLite initialization so bookkeeping tables are not enrolled as mesh-replicated user tables.
- **Configuration:** Corrosion has a disabled-by-default `[highlow]` block. Enabling it requires exactly one role: `[highlow.low]`, `[highlow.high]`, or `[highlow.high-replica]`. Low and High require `[highlow.transport]`; secrets are environment-variable names, never inline credentials.

### 7. Gossip Allow-List Replication Control
- **Configuration:** `[gossip.allow-list]` (or `allow_list`).
- **Default:** `["*"]` (wildcard, allowing all nodes to communicate without restriction).
- **Functionality:** Restricts all cluster peer communication (SWIM gossip, broadcast replication, sync bi-streams, datagrams, and discovery bootstrap) to explicitly listed IPv4/IPv6 addresses, CIDR subnets (e.g. `10.0.0.0/8`, `192.168.1.0/24`), or wildcard `*`.
- **Implementation:**
  - `AllowRule` and `AllowList` in [`crates/corro-types/src/config.rs`](file:///home/marc/GALVANIZE/crates/corro-types/src/config.rs) using `ipnet`.
  - Outbound QUIC transport gating in [`crates/corro-agent/src/transport.rs`](file:///home/marc/GALVANIZE/crates/corro-agent/src/transport.rs) with `TransportError::NotAllowed`.
  - Inbound connection refusal in [`crates/corro-agent/src/agent/handlers.rs`](file:///home/marc/GALVANIZE/crates/corro-agent/src/agent/handlers.rs) via `quinn::Incoming::refuse()`.
  - Discovery filtering in `spawn_swim_announcer`.

### 8. Live PostgreSQL-Wire Node Tests
- **Runner:** `bash tests-live/run.sh {encryption|rekey|allow-nodes|highlow|partition|crash-recovery|crdt-contention|highlow-faults|highlow-schema|large-payload|all}` (or individual `tests-live/<scenario>/run.sh`) after `cargo build -p corrosion`.
- **Behavior:** each scenario creates isolated `node-*` folders, starts real Galvanize agents, and makes all application SQL requests through PostgreSQL wire listeners using `psql`.
- **Retention:** successful runtime directories are removed. Failed runs are moved to `tests-live/failures/` with encrypted databases and agent logs for diagnosis.
- **High/Low:** the runner executes the full end-to-end `highlow` scenario verifying Low node PostgreSQL writes, air-gap artifact packaging, RSA-OAEP/Ed25519 signing/encryption, directory staging transport, and High node background ingestion/updates/deletes.
- **Partition:** 4-node split-brain test ({A, B} vs {C, D}) verifying strict intra-partition isolation, concurrent partitioned writes, network healing, and anti-entropy bi-stream reconciliation to identical SHA-256 state across all nodes.
- **Crash Recovery:** 3-node cluster crash-recovery test executing ungraceful `kill -9` mid-transaction bursts, rolling crashes of Nodes B and C, SQLite3MC encrypted WAL recovery, and anti-entropy reconciliation to identical SHA-256 database state across all nodes.
- **CRDT Contention:** 3-node concurrent mutation test executing parallel disjoint column updates on identical rows, same-column Last-Write-Wins (LWW) contention, and concurrent delete interleaving, validating CR-SQLite conflict-free convergence to identical SHA-256 database state across all nodes.
- **High/Low Faults:** 3-node air-gap resiliency test verifying deterministic rejection of corrupted payloads (MAC/digest failures) and forged Ed25519 manifest signatures, replay attack idempotency, and out-of-order sequence gap holding and healing across the High cluster mesh.
- **High/Low Schema Evolution:** 3-node air-gap schema drift test validating safe `waiting-schema` hold on the High receiver when Low evolves schema first, followed by zero-downtime hot schema reload (`galvanize reload`) and automatic backlog ingestion across all mesh peers.
- **Large Payload & Bulk Batches:** 3-node cluster stress test validating 1,000-row atomic bulk batch transactions, multi-megabyte binary blob payloads (1.5 MB), Zstd level-15 compression/decompression, and 8 KiB chunked QUIC stream replication to bit-identical state across all mesh peers.
- **Two-Node Benchmark:** Manual `tests-live/benchmark` scenario drives fully acknowledged 100-mutation PostgreSQL-wire transactions on one encrypted node for a configurable duration, measures convergence to a SHA-256-identical peer, and reports workload, replication-drain, and total logical protocol byte rates from Prometheus counters.

### 9. SQLite Memory Mapping and WAL Journal Size Limit Configuration
- **Configuration:** Exposed under `[db]` in `config.toml`:
  - `mmap_size_bytes`: Maximum memory-mapped I/O size in bytes (`PRAGMA mmap_size`). Defaults to 8 GiB (`8589934592`). Setting to `0` disables memory mapping.
  - `journal_size_limit_bytes`: Maximum WAL journal file size limit in bytes (`PRAGMA journal_size_limit`). Truncates WAL files upon checkpoint. Defaults to 1 GiB (`1073741824`). Setting to `-1` allows unlimited WAL growth.
  - `cache_size_kib`: SQLite page cache size for writes in KiB (`PRAGMA cache_size`). Defaults to -1 GiB (`-1048576`).
- **Implementation:**
  - `DbConfig` and `ConfigBuilder` in [`crates/corro-types/src/config.rs`](file:///home/marc/GALVANIZE/crates/corro-types/src/config.rs).
  - Applied on all connection pools and dedicated write connections in [`crates/corro-types/src/sqlite.rs`](file:///home/marc/GALVANIZE/crates/corro-types/src/sqlite.rs) and [`crates/corro-types/src/agent.rs`](file:///home/marc/GALVANIZE/crates/corro-types/src/agent.rs).

### 10. High/Low Control API
- **Files:** `crates/corro-types/src/config.rs` and `crates/corro-agent` High/Low integration.
- **Behavior:** Adds an optional dedicated mTLS HTTPS control listener for Galvanize High/Low status, replay, and provenance operations. TLS material is supplied by named environment variables; High/Low provenance and replay state remain outside CR-SQLite gossip replication.

### 11. File Uploads, Encrypted-at-Rest Storage, and Air-Gap Replication
- **New Crate:** [`crates/galv-files`](crates/galv-files) providing authenticated symmetric encryption (`XChaCha20-Poly1305`) for file payloads, SHA-256 integrity verification, storage path management, and multi-transport airgap transfers (Directory, HTTPS, FTP/FTPS, SFTP).
- **Configuration:** Exposed under `[files]` in `config.toml`:
  - `enabled`: Enables file service routes and capabilities.
  - `accept-uploads`: Gating for direct client uploads (`POST /v1/files/upload`).
  - `accept-from-peers`: Gating for servicing peer downloads (`GET /v1/files/{uuid}`).
  - `push-to-high`: Automatically stages/publishes uploaded files to configured High/Low air-gap transport.
  - `sync-airgap-files`: Enables background polling worker on High nodes to download missing payloads from airgap transport.
  - `storage-path`: Storage folder for encrypted file blobs (`<storage-path>/{uuid}.galvf`). Defaults to `<data-dir>/files`.
  - `max-file-size-bytes`: Maximum file upload size limit in bytes (defaults to 1 GiB).
  - `airgap-poll-interval-seconds`: Frequency of background air-gap sync scans.
- **CR-SQLite `files` Table:**
  - Distributed metadata table initialized on node unlock: `files (uuid TEXT PRIMARY KEY NOT NULL, filename TEXT NOT NULL DEFAULT '', size INTEGER NOT NULL DEFAULT 0, sha256 TEXT NOT NULL DEFAULT '', content_type TEXT, created_at TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL DEFAULT '', metadata JSON)`.
  - Converted to CRR via `crsql_as_crr('files')` and registered in `__corro_schema` so metadata automatically replicates across all mesh peers in the cluster domain.
  - Local state tracking via non-CRR `__galv_files_local` table (`uuid`, `status`, `downloaded_at`, `error`).
- **Cryptographic Key Derivation:**
  - Files are encrypted at rest using `XChaCha20-Poly1305` AEAD with 24-byte unique random nonces.
  - The 256-bit encryption key is derived via `HKDF-SHA256` from the active in-memory master unlock key (`galv_rekey_cli::get_active_key()`), preserving zero plaintext secrets on disk.
- **HTTP REST APIs:**
  - `POST /v1/files/upload?uuid=<uuid>&filename=<name>&content_type=<mime>`: Upload and encrypt a file payload. Returns JSON metadata.
  - `GET /v1/files/{uuid}`: Decrypt and stream file payload. If not locally cached, transparently fetches and caches payload from cluster peers.
  - `GET /v1/files/{uuid}/metadata`: Retrieve file metadata without downloading payload.
  - `GET /v1/files/search?name=<query>`: Search file records by filename.
  - `GET /v1/files/stats`: Total file counts, byte sizes, and local cached storage metrics.
  - `DELETE /v1/files/{uuid}`: Delete file from CR-SQLite database and wipe encrypted payload from disk.
  - `POST /v1/files/sync`: Trigger immediate on-demand synchronization of missing air-gap payloads.
  - `GET /v1/files/{uuid}/peer_fetch`: Peer-to-peer payload fetch endpoint.
- **Live Test Scenario:** `tests-live/files` verifying 4-node cluster (Low-1, Low-2, High-1, High-2) upload, gossip metadata mesh convergence, peer-fetch replication, airgap bundle delivery, background transport payload download, search, stats, and delete cascade.

### 12. Recipient-Encrypted File Air-Gap Artifacts
- **Upstream integration files:** `crates/corro-agent/src/api/public/files.rs` and `crates/corro-agent/src/agent/files_sync.rs` seal uploaded file bytes for the High RSA recipient before transport and decrypt them on High before re-encrypting with the local file-store key. Unsealed artifacts are rejected.
- **Galvanize crate:** `crates/galv-files/src/sealed_file.rs` owns the versioned hybrid-encryption format and tests; `tests-live/files` checks staged ciphertext and High-side retrieval.

### 13. Galvanize Executable Name
- **File:** `crates/corrosion/Cargo.toml`.
- **Behavior:** The upstream `corrosion` Cargo package now declares a single `galvanize` binary target from `src/main.rs`. The package and internal crate names remain unchanged for upstream compatibility.
- **Call sites:** README and usage examples, the upstream CLI/deployment docs, the Fly image, live-test runners, and CLI integration tests launch `galvanize`.

### 14. Quoted Identifier & Case-Insensitive Schema Parsing
- **File:** `crates/corro-types/src/schema.rs`.
- **Behavior:**
  - Unquotes column names in `prepare_table` before evaluating primary key membership against the unquoted PK set, preventing false positive `NotNullableColumnNeedsDefault` errors on quoted primary key columns.
  - Adds case-insensitive table lookup fallback when attaching indexes to parsed tables, ensuring quoted/unquoted mixed-case table identifiers in `CREATE INDEX` match their corresponding `CREATE TABLE` definitions.

### 15. Go Client and mTLS Admin Control API
- **Files:** `Cargo.toml`, `crates/corro-admin/Cargo.toml`, `crates/corro-types/src/config.rs`, `crates/corro-admin/src/lib.rs`, `crates/corrosion/src/command/agent.rs`, and `crates/corrosion/src/command/reload.rs`.
- **Behavior:** Adds an optional dedicated mTLS HTTPS listener for forwarding remote admin commands through the existing Unix-socket protocol. The listener requires server cert/key and client CA PEM values supplied by named environment variables.
- **Client:** `clients/go` adds the `database/sql` PostgreSQL-wire client and Go APIs for public HTTP, High/Low mTLS, and remote admin operations.

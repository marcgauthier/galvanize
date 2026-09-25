# GALVANIZE usage and endpoints

GALVANIZE runs as the `galvanize` executable. This fork retains the Cargo package name `corrosion`, internal `corro-*` crate names, and some upstream documentation that mentions `corrosion`; those names identify its ancestry, not the executable to launch. Build with `cargo build --release -p corrosion --bin galvanize` and run `./target/release/galvanize`. Each node has a PostgreSQL wire listener for application SQL, a public HTTP API, and a QUIC peer listener. Optional interfaces are the High/Low mTLS control API, the Prometheus listener, and a local admin Unix socket. Addresses and paths below are examples; use the addresses from your own TOML file.

## Start and unlock a node

Put application DDL in a schema file listed under `schema_paths`. For example, `schema/app.sql` can contain:

```sql
CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY NOT NULL,
    body TEXT NOT NULL DEFAULT ''
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS notes_body_idx ON notes (body);

CREATE VIEW nonempty_notes AS
SELECT id, body FROM notes WHERE body != '';
```

This example `config.toml` binds client interfaces to localhost:

```toml
[db]
path = "./data/corrosion.db"
schema_paths = ["./schema"]
await_unlock = true

[api]
addr = "127.0.0.1:8080"

[[api.pg]]
addr = "127.0.0.1:5433"
readonly = false

[gossip]
addr = "127.0.0.1:8787"
bootstrap = []
plaintext = true

[admin]
path = "./data/admin.sock"
```

Start with `./target/release/galvanize --config ./config.toml agent`. The HTTP listener starts before the database is unlocked. Check `curl http://127.0.0.1:8080/v1/health`; while locked it returns `{"status":"awaiting_unlock","db_path":"..."}`. The PostgreSQL listener, gossip, and replication workers start after a successful unlock. `await_unlock = true` is appropriate for encrypted nodes; an existing encrypted database also requires its correct key.

`POST /v1/admin/unlock` accepts JSON fields `key` (alias `passphrase`), optional `cipher`, and optional `cipher_params`. `/v1/unlock` is an alias. Supply the key through a named environment variable and stdin so it does not appear in TOML or command arguments:

```bash
python3 -c 'import json, os; print(json.dumps({"key": os.environ["GALVANIZE_DB_KEY"], "cipher": "chacha20"}))' |
  curl -fsS http://127.0.0.1:8080/v1/admin/unlock \
    -H 'Content-Type: application/json' --data-binary @-
```

A successful unlock returns `{"status":"unlocked"}`; an invalid key returns HTTP 401, and an already unlocked node returns 409. The runtime does not read `GALVANIZE_DB_KEY` itself: your unlock client reads that variable and sends the key to the HTTP API. Restrict access to this listener and use a protected network path for remote unlocks. The relevant TOML settings are `[db].path`, `[db].await_unlock`, and `[api].addr` (alias `bind_addr`).

## Managed schema files

`[db].schema_paths` points to SQL files or directories of `.sql` files. GALVANIZE loads them when the database initializes (at startup or after remote unlock) and when you run `./target/release/galvanize --config ./config.toml reload`. Files within a directory are loaded in filename order. Define replicated tables there. The schema loader understands SQLite-flavored `CREATE TABLE`, non-unique `CREATE INDEX`, and `CREATE VIEW` statements, plus `DROP VIEW IF EXISTS` and `DROP INDEX IF EXISTS`. `IF NOT EXISTS` is accepted on creation. Declare a table before its indexes in the loaded schema. The `notes` example above is valid; `WITHOUT ROWID` is optional.

For every replicated table, declare a primary key and mark **each primary-key column `NOT NULL`**. This also applies to composite keys. Every other `NOT NULL` column needs a `DEFAULT` value, including when the table is first created, so that adding columns remains compatible with earlier rows. The primary-key index created by SQLite needs no separate `CREATE INDEX` statement.

These examples are invalid as managed replicated schemas:

```sql
-- No declared primary key:
CREATE TABLE no_key (body TEXT);

-- A nullable primary key (SQLite permits NULL in some rowid-table keys):
CREATE TABLE nullable_key (id TEXT PRIMARY KEY, body TEXT);

-- Unique indexes are rejected; the primary key supplies uniqueness:
CREATE UNIQUE INDEX notes_body_unique ON notes (body);

-- A non-primary-key NOT NULL column needs a default:
CREATE TABLE no_default (id INTEGER PRIMARY KEY NOT NULL, body TEXT NOT NULL);

-- Column foreign keys are unsupported:
CREATE TABLE with_reference (
    id INTEGER PRIMARY KEY NOT NULL,
    parent_id INTEGER REFERENCES notes(id)
);
```

Temporary tables/views, `CREATE TABLE ... AS SELECT`, and other statements such as `ALTER TABLE`, `INSERT`, and `PRAGMA` are not accepted in managed schema files. A `CREATE INDEX` must refer to a table defined earlier in the loaded schema. Do not put `CREATE` and `DROP` statements for the same index or view in one schema load. Views are local SQL objects evaluated over replicated tables; view definitions are not replicated independently.

For schema changes, edit the files and run `./target/release/galvanize --config ./config.toml reload` on each affected node, or restart it. You can add nullable columns, add `NOT NULL` columns with defaults, and add or remove non-unique indexes. To remove an index, omit its `CREATE INDEX` definition while retaining the table definition; `DROP INDEX IF EXISTS` is accepted by the parser but does not by itself remove a previously managed index. To remove a managed view, use `DROP VIEW IF EXISTS view_name`. Removing tables or columns, changing existing columns or primary keys, and adding a new primary-key column are rejected by the schema reconciliation path. Keep Low and High schema files aligned: High/Low transfer does not carry DDL, and a High receiver holds bundles with a mismatched `schema_hash` in `waiting-schema`. Use the PostgreSQL listener for application SQL against a running node; manage schema changes through these files.

## PostgreSQL wire protocol

Use `[[api.pg]]` for one or more PostgreSQL-compatible listeners. This is the preferred application SQL interface, including reads and writes against running nodes. SQL uses SQLite syntax; PostgreSQL-only syntax is not supported. Do not open the database file directly for application SQL, because that bypasses GALVANIZE change capture.

```bash
psql 'postgresql://postgres@127.0.0.1:5433/postgres?sslmode=disable' \
  -v ON_ERROR_STOP=1 -c "INSERT INTO notes (id, body) VALUES (1, 'hello')"
psql 'postgresql://postgres@127.0.0.1:5433/postgres?sslmode=disable' \
  -Atqc 'SELECT id, body FROM notes ORDER BY id'
```

The PostgreSQL listener starts only after unlock. `[[api.pg]].addr`/`bind_addr` sets its address; `readonly = true` rejects writes. Optional `[api.pg.tls]` fields are `cert_file`, `key_file`, `ca_file`, and `verify_client`; configure client TLS accordingly. These TLS file paths are distinct from the High/Low control API's environment-supplied mTLS material. The `[db].schema_paths` setting controls managed DDL.

## Public HTTP API

All routes in this section use `[api].addr`/`bind_addr`. The optional `[api].authorization` (alias `authz`) bearer-token setting applies to **every** public route, including health and unlock; clients then send `Authorization: Bearer <token>`. `[api].endpoint_name` can identify a listener: a mismatched `x-corrosion-requested-endpoint-name` request header is rejected. Protect bearer tokens as secrets and keep the listener on a trusted interface unless you provide appropriate network protection. SQL routes require an unlocked database.

| Method and path | Purpose and example | Response |
| --- | --- | --- |
| `POST /v1/queries` | Read with one JSON SQL statement: `curl -sS localhost:8080/v1/queries -H 'Content-Type: application/json' -d '"SELECT id, body FROM notes ORDER BY id"'` | NDJSON `columns`, `row`, and `eoq` events. |
| `POST /v1/transactions` | Commit one or more write statements atomically: `curl -sS localhost:8080/v1/transactions -H 'Content-Type: application/json' -d '[["INSERT INTO notes (id, body) VALUES (2, ?)", ["from HTTP"]]]'` | JSON `results`, elapsed `time`, `version`, and `actor_id`. |
| `POST /v1/subscriptions` | Stream an initial query and subsequent changes: `curl -N localhost:8080/v1/subscriptions -H 'Content-Type: application/json' -d '"SELECT id, body FROM notes"'` | NDJSON snapshot and `change` events; save the `corro-query-id` response header. |
| `GET /v1/subscriptions/{id}` | Resume a saved subscription: `curl -N 'localhost:8080/v1/subscriptions/UUID?from=4'` | NDJSON changes after change ID 4; omit `from` for a fresh snapshot. |
| `POST /v1/updates/{table}` | Watch primary-key updates/deletes: `curl -N -X POST localhost:8080/v1/updates/notes` | NDJSON `notify` events; no initial snapshot. |
| `POST /v1/table_stats` | Count rows across tables: `curl -sS localhost:8080/v1/table_stats -H 'Content-Type: application/json' -d '{"tables":["notes"]}'` | JSON `total_row_count` and `invalid_tables`. |
| `GET /v1/health` | Check node state: `curl -sS localhost:8080/v1/health` | JSON health data, or `awaiting_unlock` while locked. |
| `POST /v1/admin/unlock`, `POST /v1/unlock` | Supply the database key; see the startup example. | JSON unlock status or error. |

For parameterized SQL, a statement may be `"SELECT ..."`, `["SELECT ... WHERE id = ?", [1]]`, or `{"query":"SELECT ... WHERE id = ?","params":[1]}`. A transaction body is an array of these statements. `/v1/queries` and `/v1/transactions` accept `?timeout=<seconds>`; health accepts threshold query parameters such as `?gaps=0&max_queue=100` and returns HTTP 503 by default when a threshold fails. `/v1/updates/{table}` requires an existing CR-SQLite table with a primary key. See [the upstream API reference](doc/api/README.md) for detailed streaming event formats.

## File endpoints

The `files` metadata table replicates through the local cluster; each node stores payloads separately as encrypted `.enc` files. The key for local payload encryption derives from the active database unlock key, so file operations require that key to be active. An upload body is raw bytes, **not** multipart form data. Set the file flags on the nodes that perform each role:

```toml
[files]
enabled = true
accept-uploads = true
accept-from-peers = true
push-to-high = false
sync-airgap-files = false
storage-path = "./data/files"
max-file-size-bytes = 104857600
airgap-poll-interval-seconds = 10
```

`storage-path` defaults to a `files` directory beside `[db].path`; `airgap-poll-interval-seconds` defaults to 10. The HTTP router also imposes a 1 GiB request-body limit. In the current code, `enabled` is a configuration field but does not gate these routes; `accept-uploads` gates uploads, `accept-from-peers` allows a missing download to try other mesh peers, and `sync-airgap-files` starts the background air-gap fetch worker. File routes use the public `[api]` listener and its authorization setting.

| Method and path | Description and example | Related settings |
| --- | --- | --- |
| `POST /v1/files/upload` | Upload bytes and generate a UUID: `curl -sS localhost:8080/v1/files/upload?filename=report.pdf -H 'Content-Type: application/pdf' --data-binary @./report.pdf` | `[files].accept-uploads`, `storage-path`, `max-file-size-bytes` |
| `POST /v1/files/{uuid}` | Upload using that UUID: `curl -sS localhost:8080/v1/files/11111111-1111-4111-8111-111111111111?filename=report.pdf --data-binary @./report.pdf` | Same upload settings; `POST /v1/files` is another upload alias. |
| `GET /v1/files/{uuid}` | Download decrypted bytes: `curl -fS localhost:8080/v1/files/11111111-1111-4111-8111-111111111111 -o report.pdf` | `[files].accept-from-peers` permits a peer fetch when the payload is missing locally. |
| `GET /v1/files/{uuid}/metadata` | Read metadata and `is_local`: `curl -sS localhost:8080/v1/files/11111111-1111-4111-8111-111111111111/metadata` | `[files].storage-path` |
| `GET /v1/files/search` | Search filenames: `curl -sS 'localhost:8080/v1/files/search?name=report&limit=20&offset=0'` | `GET /v1/files` is an alias; default limit 100, maximum 1000. |
| `GET /v1/files/stats` | Get total and locally cached counts/bytes: `curl -sS localhost:8080/v1/files/stats` | `[files].storage-path` |
| `DELETE /v1/files/{uuid}` | Delete replicated metadata and this node's local payload: `curl -sS -X DELETE localhost:8080/v1/files/11111111-1111-4111-8111-111111111111` | Public API authorization; allow cluster propagation time. |
| `POST /v1/files/sync` | Run one missing-file air-gap fetch cycle: `curl -sS -X POST localhost:8080/v1/files/sync` | `[highlow.transport]`; the background worker additionally needs `[files].sync-airgap-files = true`. |
| `GET /v1/files/{uuid}/peer_fetch` | Return locally stored payload bytes to a mesh peer: `curl -fS localhost:8080/v1/files/11111111-1111-4111-8111-111111111111/peer_fetch -o report.pdf` | Public API authorization; intended for peer retrieval. |

Upload can also take `uuid` and `content_type` query parameters, or `x-file-uuid`, `x-filename`, and JSON `x-metadata` headers. The response includes `status`, `uuid`, filename, size, SHA-256, content type, and timestamps. A missing file may have metadata on a node before its bytes arrive. The peer-fetch route returns decrypted bytes and is protected only by the public API's normal access controls; restrict access accordingly.

For Low-to-High file transfer, set `push-to-high = true` on the Low uploader and `sync-airgap-files = true` on the High receiver, with a shared `[highlow.transport]`. The Low node must have `[highlow.low].recipient-rsa-public-key-env` set to the High recipient's RSA public key; the High receiver needs `[highlow.high].recipient-rsa-private-key-env` set to the matching private key. High/Low row replication must also carry the file metadata before the High sync worker can identify a missing payload. A successful local upload can still report success if transport publishing fails; check logs and High-side availability. File artifacts `{uuid}.file` are sealed with a fresh XChaCha20-Poly1305 content key wrapped using the High recipient's RSA-OAEP/SHA-256 public key, the same encryption construction used for database-change bundles. The High receiver rejects unsealed or tampered artifacts, verifies the decrypted bytes against the replicated SHA-256, then encrypts them with its own local file-store key. File artifacts do not use the database bundle's signed manifest format; the artifact UUID is authenticated as associated data, while integrity against the replicated metadata is checked by SHA-256. Pre-upgrade plaintext artifacts are rejected and must be republished by an upgraded Low node. Even though staged file contents are encrypted, protect the staging area because filenames and other transport metadata remain visible.

## High/Low air-gap replication

High/Low sends row changes from Low to High in signed, encrypted bundles; it does not send DDL or High changes back to Low. Maintain matching schemas on both sides using `[db].schema_paths`. A High receiver holds a bundle with a mismatched `schema_hash` in `waiting-schema` until its schema is aligned. Set `enabled = true` and exactly one role per participating node. A Low or High role needs a transport; a `high-replica` role receives changes through its High mesh and has no transport requirement.

Low exporter example:

```toml
[highlow]
enabled = true

[highlow.transport]
kind = "directory"
endpoint = "/media/airgap/galvanize"

[highlow.low]
stream-id = "plant-a"
network-name = "plant-a-low"
upload-interval-seconds = 60
recipient-key-id = "high-key-1"
recipient-rsa-public-key-env = "GALV_HIGH_RSA_PUBLIC_KEY"
sender-signing-key-env = "GALV_LOW_ED25519_PRIVATE_KEY"
```

High receiver example, with the same transport location visible after physical transfer:

```toml
[highlow]
enabled = true

[highlow.transport]
kind = "directory"
endpoint = "/media/airgap/galvanize"

[highlow.high]
accepted-streams = ["plant-a"]
download-interval-seconds = 60
recipient-key-id = "high-key-1"
recipient-rsa-private-key-env = "GALV_HIGH_RSA_PRIVATE_KEY"
permitted-sender-key-envs = ["GALV_LOW_ED25519_PUBLIC_KEY"]
```

The named environment variables must contain the appropriate PEM/key material in each node's environment; TOML contains only their names. Match the `recipient-key-id`, stream ID, and signing-key pair across the two sides. For a High mesh peer that only serves replicated High state, use `[highlow.high-replica]` with `accepted-streams = ["plant-a"]` instead of `[highlow.high]`.

Working `[highlow.transport].kind` adapters are `directory`, `http`, `https`, `ftp`, and `ftps`. Configuration also accepts `smb` (currently mapped to the directory adapter) and `sftp`, but SFTP publish/fetch methods currently return an unimplemented-transport error. The transport's `endpoint` is the artifact staging location, **not** a Galvanize node endpoint. An HTTP(S) artifact service must support `POST`/`GET {endpoint}/artifacts/{filename}` and `GET {endpoint}/manifests` (a JSON array of names). File payload transfer additionally uses `POST`/`GET {endpoint}/{uuid}.file`. `username`, `password-env`, and `bearer-token-env` are used by relevant adapters; use named environment variables for credentials. Configuration also accepts `private-key-env`, `private-key-passphrase-env`, `host-key-sha256`, `ca-file`, and `domain`, but the current transport conversion does not pass these to the adapters. SFTP configuration requires `host-key-sha256` even though the adapter is not operational. Credentials must not be embedded in `endpoint`.

### High/Low mTLS control API

When configured, this is a **separate HTTPS listener** requiring a client certificate. Add `[highlow.control-api]` on a High/Low node:

```toml
[highlow.control-api]
addr = "127.0.0.1:8443"
server-cert-env = "GALV_CONTROL_SERVER_CERT"
server-key-env = "GALV_CONTROL_SERVER_KEY"
client-ca-cert-env = "GALV_CONTROL_CLIENT_CA"
```

Supply PEM contents through those named environment variables. A `high-replica` control API also requires `authoritative-high-url = "https://high.example:8443"` so provenance requests can redirect to its authoritative High node. Set client certificate, key, and CA **file paths** in `GALV_CLIENT_CERT_FILE`, `GALV_CLIENT_KEY_FILE`, and `GALV_CONTROL_CA_FILE` for these command examples:

```bash
curl --cacert "$GALV_CONTROL_CA_FILE" --cert "$GALV_CLIENT_CERT_FILE" --key "$GALV_CLIENT_KEY_FILE" \
  https://127.0.0.1:8443/v1/highlow/status
```

| Method and path | Role and use | Example request/response |
| --- | --- | --- |
| `GET /v1/highlow/status` | Any configured role; worker status, pending exports, latest import/export result, and active replay job. | The `curl` command above returns JSON. |
| `POST /v1/highlow/replay` | Low only; enqueue replay of retained events. An active job causes HTTP 409. | `curl --cacert "$GALV_CONTROL_CA_FILE" --cert "$GALV_CLIENT_CERT_FILE" --key "$GALV_CLIENT_KEY_FILE" https://127.0.0.1:8443/v1/highlow/replay -H 'Content-Type: application/json' -d '{"scope":"all"}'` returns HTTP 202 and a job. Use `{"scope":"since","since_utc":"2026-01-01T00:00:00Z"}` for a UTC RFC3339 cutoff. |
| `GET /v1/highlow/replay/status` | Low only; latest job and recent logs. | `curl --cacert "$GALV_CONTROL_CA_FILE" --cert "$GALV_CLIENT_CERT_FILE" --key "$GALV_CLIENT_KEY_FILE" https://127.0.0.1:8443/v1/highlow/replay/status` |
| `POST /v1/highlow/provenance` | High only; inspect whether records originated on Low and which fields High owns. A High replica redirects with HTTP 307. | `curl --cacert "$GALV_CONTROL_CA_FILE" --cert "$GALV_CLIENT_CERT_FILE" --key "$GALV_CLIENT_KEY_FILE" https://127.0.0.1:8443/v1/highlow/provenance -H 'Content-Type: application/json' -d '{"records":[{"table":"notes","primary_key":{"id":1}}]}'` returns a `records` array. |

## Operator interfaces

The admin interface is a length-delimited JSON protocol on a Unix socket, not an HTTP endpoint. Use the `galvanize` CLI for it. The `[admin].path` setting (alias `uds_path`) chooses the socket:

```bash
./target/release/galvanize --config ./config.toml cluster members
./target/release/galvanize --config ./config.toml sync generate
./target/release/galvanize --config ./config.toml reload
```

For remote administration, GALVANIZE can also expose the separate mTLS HTTPS listener configured under `[admin.control-api]`; it accepts all existing admin commands at `POST /v1/admin/commands`. Server certificate, private key, and client CA PEM contents are read from the named environment variables. See [the API configuration reference](doc/config/api.md#remote-admin-control-api).

Prometheus metrics are available on a separate HTTP listener when configured:

```toml
[telemetry.prometheus]
addr = "127.0.0.1:9090"
```

Scrape with `curl http://127.0.0.1:9090/metrics`; the exporter responds to GET requests on that listener. `[gossip].addr`, `bootstrap`, and optional `allow-list` configure internal peer discovery and QUIC replication, not SQL or file client endpoints. For example, `allow-list = ["10.0.0.0/24"]` limits which peers may communicate with the node. Keep Low and High gossip meshes separate.

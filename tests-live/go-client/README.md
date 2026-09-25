# Go client live test

Run `./tests-live/go-client/run.sh` after building `galvanize`. The scenario
starts isolated encrypted Low and High nodes with unique database keys and temporary
mTLS credentials, then exercises the Go SDK over PostgreSQL wire, the public
HTTP API, remote admin HTTPS, and the High/Low mTLS API. It simulates a small
application workflow over `database/sql` and HTTP: one-shot queries,
transactions, live subscriptions and resume, table notifications, health and
table stats, encrypted file upload/download/peer-fetch/search/delete (including
caller-chosen UUIDs), file sync, admin success/error responses, and High/Low
status/replay/provenance authorization, typed response contracts, upload
metadata, successful Low-to-High import and provenance, High file sync, and local CLI cluster inspection. It also checks that the SDK refuses
to send an unlock key over the fixture's cleartext public API and that Low
rejects High-only file synchronization. The two nodes' application rows are
compared through PostgreSQL wire with SHA-256; their internal databases differ
because Low exports and High imports.

On success, temporary databases, keys, certificates, and logs are removed. On
failure, the runtime tree is retained in `tests-live/failures/`. Override
`GALVANIZE_LIVE_RUNTIME`, `GALVANIZE_BIN`, or
`GALVANIZE_GO_CLIENT_BASE_PORT` when the default path or ports are unavailable.

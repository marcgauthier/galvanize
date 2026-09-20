# Galvanize live-node tests

Run `cargo build -p corrosion`, then use `bash tests-live/run.sh all` or an
individual feature runner:

- `tests-live/encryption/run.sh`
- `tests-live/rekey/run.sh`
- `tests-live/allow-nodes/run.sh`
- `tests-live/highlow/run.sh`
- `tests-live/partition/run.sh`
- `tests-live/crash-recovery/run.sh`
- `tests-live/crdt-contention/run.sh`
- `tests-live/highlow-faults/run.sh`
- `tests-live/highlow-schema/run.sh`
- `tests-live/large-payload/run.sh`
- `tests-live/benchmark/run.sh` (manual-only two-node replication benchmark)

Each feature owns a `runtime/node-*` tree and uses `psql` against each node's
PostgreSQL wire listener for application SQL. Successful runtime directories
are removed. Failures are retained under `tests-live/failures/` with encrypted
databases and agent logs.

The benchmark requires `curl` to read each node's Prometheus counters and is
intentionally excluded from `bash tests-live/run.sh all`.

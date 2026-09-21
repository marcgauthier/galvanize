# Managed Views Live Test

Run `bash tests-live/views/run.sh` after `cargo build -p corrosion`.

The test starts three encrypted Galvanize nodes with identical managed schema:
`view_devices` is a CR-SQLite replicated table and `enabled_devices` is a
normal SQLite view over that table. It writes one row through each node's
PostgreSQL listener, waits for three-node convergence, then verifies through
`psql` that:

- every node has the view and obtains the same enabled rows from it;
- the underlying table has converged to the same canonical SHA-256 snapshot;
- `enabled_devices` never appears in `crsql_changes`.

Successful runs remove their runtime directory. Failures are retained under
`tests-live/failures/`.

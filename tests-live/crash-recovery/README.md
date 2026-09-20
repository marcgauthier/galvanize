# Crash Recovery & Encrypted WAL Live Test

Run `bash tests-live/crash-recovery/run.sh`.

It creates a 3-node cluster (A, B, C) on separate loopback IPs (`127.0.0.61`..`127.0.0.63`) with distinct SQLite3MC encryption keys on each node.

### Flow
1. **Baseline Startup:** Confirms initial 3-node baseline convergence.
2. **Active Bursts & Abrupt Kill -9:** Executes concurrent write bursts via `psql` while ungracefully terminating Node B mid-transaction using `kill -9` (no clean shutdown or SQLite checkpointing). Surviving nodes A and C continue taking transactions.
3. **Encrypted WAL Recovery & Rolling Crash:** Re-starts Node B with its encryption key, verifying SQLite3MC encrypted WAL recovery and readiness, then ungracefully terminates Node C mid-burst with `kill -9`.
4. **Cluster Healing:** Re-starts Node C with its encryption key.
5. **Anti-Entropy Reconciliation:** Adaptive bi-stream anti-entropy sync reconciles all historical and partitioned transactions to a bit-identical SHA-256 database state across all 3 nodes with zero data loss.
6. **Post-Recovery Verification:** Confirms subsequent live writes propagate across all nodes.

Override test duration with `GALVANIZE_LIVE_CRASH_WRITE_SECONDS` and `GALVANIZE_LIVE_CRASH_SETTLE_SECONDS`.

# Partition Live Test

Run `bash tests-live/partition/run.sh`.

It creates a 4-node cluster (A, B, C, D) on separate loopback IPs (`127.0.0.51`..`127.0.0.54`) with SQLite3MC encryption enabled.

### Flow
1. **Baseline Full Mesh:** Confirms initial 4-node convergence across all nodes.
2. **Partition Injection:** Splits the cluster into two isolated sub-clusters `{A, B}` and `{C, D}` using allow-lists. Concurrent writes commit into both partitions simultaneously.
3. **Partition Isolation Verification:** Confirms data is strictly isolated across partitions while intra-partition replication functions correctly.
4. **Partition Healing:** Re-connects all 4 nodes into a unified mesh.
5. **Anti-Entropy Reconciliation:** Validates bi-stream anti-entropy sync reconciles all partitioned mutations to an identical SHA-256 database state across all 4 nodes.
6. **Post-Heal Operational Verification:** Validates post-heal live writes propagate across all nodes.

Override test duration with `GALVANIZE_LIVE_PARTITION_WRITE_SECONDS` and `GALVANIZE_LIVE_PARTITION_SETTLE_SECONDS`.

# CRDT Contention Live Test

Run `bash tests-live/crdt-contention/run.sh`.

It creates a 3-node cluster (A, B, C) on separate loopback IPs (`127.0.0.71`..`127.0.0.73`) with SQLite3MC encryption enabled.

### Flow
1. **Baseline Hot Rows:** Seeds row `100` (disjoint column target), rows `201..220` (shared column LWW contention target), and rows `301..310` (concurrent delete target).
2. **Concurrent Contention Wave:**
   - Node A continuously updates `col_a` on row `100` and `val_shared` on hot rows.
   - Node B continuously updates `col_b` on row `100` and `val_shared` on hot rows.
   - Node C continuously updates `col_c` on row `100` and `val_shared` on hot rows, and concurrently deletes rows `301..310`.
3. **Adaptive Reconciliation:** Validates anti-entropy sync convergence.
4. **CRDT Invariant Assertions:**
   - **Disjoint Column Merge:** Verifies row `100` retained `col_a` from A, `col_b` from B, and `col_c` from C simultaneously without lost updates.
   - **LWW Convergence:** Verifies all 3 nodes reached 100% identical row values for shared contention rows.
   - **Delete Pruning:** Verifies rows `301..310` are deleted across all nodes.
   - **SHA-256 State Equality:** Asserts bit-identical database state across all 3 nodes.
5. **Post-Contention Verification:** Confirms ongoing transaction processing and replication.

Override test duration with `GALVANIZE_LIVE_CONTENTION_WRITE_SECONDS` and `GALVANIZE_LIVE_CONTENTION_SETTLE_SECONDS`.

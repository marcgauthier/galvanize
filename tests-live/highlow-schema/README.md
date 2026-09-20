# Scenario 5: High/Low Schema Drift & `waiting-schema` Hold / Replay

This live test validates Galvanize's air-gap schema boundary defenses and zero-downtime schema evolution safety.

## Overview & Architecture

Galvanize does not replicate DDL or schema migrations across air-gaps; each domain manages its database schema independently. To prevent data corruption, column mismatches, or silent ingestion failures:

1. **Low Exporter:** Every bundle sealed by Low contains a `schema_hash` calculated from the SHA-256 digest of all active user table definitions in SQLite.
2. **High Receiver:** Before opening or applying any bundle, High verifies `manifest.schema_hash == local_schema_hash`.
3. **`waiting-schema` Hold:** If Low evolves its schema ahead of High, High logs `highlow bundle held: waiting-schema` and leaves the bundle intact in the staging transport without partial writes or crashing.
4. **Hot Schema Reload:** When High's schema is updated to match (using `corrosion reload`), High automatically discovers the matching schema on its next polling cycle, unblocks the held bundles, and ingests all pending rows.
5. **High Mesh Propagation:** All applied changes replicate across the High QUIC mesh so peers converge to bit-identical state.

## Cluster Topology

- **Low Domain:** 1 Node (`node-low-1`, Air-Gap Exporter)
- **High Domain:** 2 Nodes (`node-high-1` Air-Gap Receiver, `node-high-2` High mesh peer)
- **Air-Gap Transport:** Staging directory (`staging/`) with RSA-OAEP / Ed25519 cryptography
- **Encryption at Rest:** SQLite3MC ChaCha20-Poly1305 on all nodes

## Test Phases

1. **Phase 1: Baseline Ingestion (Schema V1)**
   - Both Low and High start with Schema V1 (`live_records`).
   - Low writes `id=1`; High receives and replicates to High-2 (both nodes verify 1 row).

2. **Phase 2: Schema Drift on Low (Schema V1 -> V2)**
   - Low adds `schema/v2.sql` containing `metrics_records` and triggers `corrosion reload`.
   - Low writes into both `live_records` (`id=2`) and `metrics_records` (`id=101`).
   - Low packages Bundle 2 with the Schema V2 hash.

3. **Phase 3: `waiting-schema` Hold Invariant Verification**
   - High receiver detects `schema_hash` mismatch.
   - High logs `waiting-schema` and holds Bundle 2 in staging.
   - Verified: High cluster remains on Schema V1 without applying partial rows.

4. **Phase 4: Schema Alignment on High & Automatic Catch-Up**
   - High-1 and High-2 receive `schema/v2.sql` and trigger `corrosion reload`.
   - On the next poll cycle, High-1 detects matching schema hash, decrypts, and applies Bundle 2.
   - High-1 replicates changes to High-2 via QUIC mesh.

5. **Phase 5: Post-Migration Continuous Sync & Invariant Verification**
   - Low writes additional rows (`id=3` in `live_records`, `id=102` in `metrics_records`).
   - High receiver ingests Bundle 3 seamlessly.
   - Verified: High-1 and High-2 converge to 100% bit-identical SHA-256 database state across both tables.

## Running the Test

```bash
# Direct runner
bash tests-live/highlow-schema/run.sh

# Or via parent runner
bash tests-live/run.sh highlow-schema
```

# Scenario 6: Large Payload & High-Throughput Bulk Batch Synchronization

This live test validates Galvanize's data path boundaries under heavy transactional load, multi-megabyte payloads, Zstd compression limits, and QUIC change streaming chunking.

## Overview & Architecture

Galvanize handles data propagation through two distinct channels:
1. **Air-Gap High/Low Ingestion:** Packaging local database transactions into Zstd-compressed (level 15), RSA-OAEP/Ed25519-signed bundles with strict validation against `MAX_PAYLOAD_BYTES` (64 MiB), `MAX_DECOMPRESSED_BYTES` (64 MiB), and `MAX_VALUE_BYTES` (4 MiB).
2. **Cluster QUIC Mesh Synchronization:** Replicating changes across nodes in 8 KiB chunked streams (`MAX_CHANGES_BYTE_SIZE`) through QUIC transport and anti-entropy bi-streams.

This scenario validates that both systems operate reliably under stress without buffer overflow, packet drops, or data corruption.

## Cluster Topology

- **Low Domain:** 1 Node (`node-low-1`, Air-Gap Exporter)
- **High Domain:** 2 Nodes (`node-high-1` Air-Gap Receiver, `node-high-2` High Cluster Peer)
- **Air-Gap Transport:** Staging directory (`staging/`) with RSA-OAEP / Ed25519 cryptography
- **Encryption at Rest:** SQLite3MC ChaCha20-Poly1305 on all nodes

## Test Phases

1. **Phase 1: High-Volume Bulk Transaction (1,000 Rows)**
   - Low node commits 1,000 distinct records in a single atomic SQL transaction block via `psql`.
   - Low exporter captures the 1,000-event delta, compresses, and seals the bundle.
   - High receiver ingests all 1,000 rows.
   - High-1 propagates the batch to High-2 via chunked QUIC replication.
   - Verified: Both High nodes have exactly 1,000 rows.

2. **Phase 2: Multi-Megabyte Large Binary/Text Blob**
   - Low node inserts a 1.5 MB structured payload record into `live_records`.
   - Validates Zstd level-15 compression efficiency, encrypted bundle packaging, and high-side decompression.
   - Validates QUIC multi-chunk stream transmission to High-2.
   - Verified: Exact byte-level SHA-256 matching of the large blob across all nodes.

3. **Phase 3: Direct High-Mesh Bulk Transaction**
   - High-1 directly executes a 500-row batch commit into `high_records`.
   - Validates direct cluster broadcast and anti-entropy synchronization to High-2.
   - Verified: Both High nodes have exactly 500 rows in `high_records`.

4. **Phase 4: Full High Cluster Bit-Identical Reconciliation**
   - Computes SHA-256 digests of all tables (`live_records` and `high_records`) on High-1 and High-2.
   - Verified: 100% bit-for-bit SHA-256 database equality and complete zero data loss across the High cluster.

## Running the Test

```bash
# Direct runner
bash tests-live/large-payload/run.sh

# Or via parent runner
bash tests-live/run.sh large-payload
```

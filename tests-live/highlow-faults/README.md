# High/Low Air-Gap Fault Injection & Resiliency Live Test

Run `bash tests-live/highlow-faults/run.sh`.

It creates a cross-domain architecture with 1 Low node (`127.0.0.81`), an air-gap directory transport staging area, and a 2-node High cluster (`127.0.0.82` and `127.0.0.83`) with SQLite3MC encryption.

### Flow
1. **Payload Corruption:** Inverts payload bytes in `.zstd.galvh` file $\rightarrow$ verifies High receiver rejects bundle due to digest/MAC error and blocks database insertion (0 rows applied).
2. **Signature Tampering:** Modifies Ed25519 signature in `.json.galv` manifest $\rightarrow$ verifies High receiver fails cryptographic signature verification and blocks ingestion (0 rows applied).
3. **Replay Idempotency:** Stages valid bundle 103, verifies successful ingestion and High mesh replication, then re-delivers the exact same bundle to test replay protection $\rightarrow$ verifies High receiver idempotently skips already processed bundles.
4. **Sequence Gap & Healing:** Stages sequence 5 while withholding sequence 4, then delivers sequence 4 $\rightarrow$ verifies gap detection and automatic stream healing.
5. **High Mesh Convergence:** Asserts valid rows (103, 104, 105) are present, corrupted rows (101, 102) are absent, and High-1 and High-2 have bit-identical SHA-256 state.

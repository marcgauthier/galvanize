# Galvanize Live Test: File Upload & High/Low Air-Gap Replication

This scenario verifies file upload support, encrypted-at-rest storage, HTTP search/download APIs, peer-to-peer file retrieval, and High/Low air-gap file replication.

## Topology
- **Low Domain**:
  - `node-low-1` (Low Air-Gap Exporter, `accept_uploads = true`, `push_to_high = true`)
  - `node-low-2` (Low Mesh Peer, `accept_from_peers = true`)
- **High Domain**:
  - `node-high-1` (High Air-Gap Receiver, `sync_airgap_files = true`)
  - `node-high-2` (High Mesh Peer, `accept_from_peers = true`)
- **Air-Gap Transport**:
  - Directory staging endpoint with encrypted sealed delta bundles and recipient-encrypted file artifacts.

## Invariants Verified
1. Node unlock and key derivation (ChaCha20-Poly1305 encrypted file storage at rest).
2. File upload via `POST /v1/files/upload` with custom UUID and validation.
3. Metadata persistence in SQLite `files` CRR table.
4. Peer file retrieval over HTTP (`GET /v1/files/{uuid}`).
5. Low node publishing RSA-wrapped, XChaCha20-Poly1305-encrypted file artifacts to air-gap transport; staged bytes contain no document plaintext.
6. High node background worker detecting missing `files` table records, downloading and decrypting artifacts with its RSA private key.
7. High node re-encrypting payload at rest with its local store key.
8. Bit-identical SHA-256 validation across all Low and High nodes.
9. File search via `GET /v1/files/search?name=...`.
10. File deletion cascade via `DELETE /v1/files/{uuid}`.

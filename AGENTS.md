# GALVANIZE Development Guidelines

GALVANIZE is a fork of [superfly/corrosion](https://github.com/superfly/corrosion).
It adds encryption at rest (SQLite3MC) and cross-domain (high/low) air-gap replication to Corrosion.

## Keep the Fork Maintainable

- Keep changes to upstream Corrosion code minimal, so upstream updates and fixes can
  be integrated easily.
- Record every change to upstream Corrosion code in `FORK_CHANGES.md`. This record
  supports future upgrades from Corrosion.
- New GALVANIZE-specific crates must be created under `crates/` and use the
  `galv-` prefix (for example, `crates/galv-encryption`, `crates/galv-highlow`).

## Correctness and Security Invariants

- Prioritize code correctness over performance optimization. Optimize only after correctness is established and the optimization is justified.
- **PostgreSQL Wire Protocol:** Application SQL queries against running nodes must always travel over the PostgreSQL wire listener (`psql`), never via direct SQLite file access, so CR-SQLite and local change journals operate correctly.
- **Zero Plaintext Secrets:** Passwords and cryptographic keys (`GALVANIZE_DB_KEY`, RSA PEMs, Ed25519 keys) must always be supplied via named environment variables, never hardcoded in configuration files or CLI flags.
- **Air-Gap Schema Boundary:** Galvanize does not replicate DDL across air-gaps. High receivers verify `schema_hash` in incoming manifests and hold mismatched bundles in `waiting-schema` until the High cluster schema is aligned.

## Testing Guidelines

- Add tests that cover new or changed code. Test coverage is a core requirement,
  not an optional follow-up.
- When adding an important feature, add a live test under `./tests-live` in a
  dedicated folder.
- Follow the existing `tests-live` layout and conventions:
  - Each scenario has its own executable `tests-live/<scenario>/run.sh` and `README.md`.
  - Scenarios must allocate isolated `runtime/node-*` environments, dedicated ports, and unique encryption keys.
  - Multi-node tests should assert bit-identical convergence across all nodes via SHA-256 database comparisons.
  - Passing runs clean up runtime directories; failures are retained under `tests-live/failures/` for debugging.

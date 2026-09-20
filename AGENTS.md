# GALVANIZE Development Guidelines

GALVANIZE is a fork of [superfly/corrosion](https://github.com/superfly/corrosion).
It adds encryption at rest and cross-domain (high/low) replication to Corrosion.

## Keep the Fork Maintainable

- Keep changes to upstream Corrosion code minimal, so upstream updates and fixes can
  be integrated easily.
- Record every change to upstream Corrosion code in `FORK_CHANGES.md`. This record
  supports future upgrades from Corrosion.
- New GALVANIZE-specific crates must be created under `crates/` and use the
  `galv-` prefix (for example, `crates/galv-encryption`).

## Correctness and Testing

- Prioritize code correctness over performance optimization. Optimize only after correctness is established and the optimization is justified.
- Add tests that cover new or changed code. Test coverage is a core requirement,
  not an optional follow-up.
- When adding an important feature, add a live test under `./tests-live` in a
  dedicated folder.
- Follow the existing `tests-live` layout and conventions. Live tests should use
  subfolders and multi-node replication so they resemble the way the database is
  deployed in practice.

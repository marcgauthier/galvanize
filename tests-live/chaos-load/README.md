# chaos-load: faults during continuous write load

Live stability scenario. A 3-node encrypted cluster takes continuous
application writes while faults are injected mid-write, then must recover
fully.

## What it proves

While a continuous writer runs:

1. **Kill -9 mid-write.** Node B is terminated with `SIGKILL` in the middle
   of a write burst (A and C keep committing), then restarted with its
   encryption key. The restart must cleanly re-open the SQLite3MC database
   (WAL recovery) and serve the PostgreSQL wire listener again.
2. **Allow-list partition mid-write.** The cluster is split into side `{A,B}`
   vs side `{C}` via gossip `allow-list` while both sides keep committing.
   The sides must stay isolated (divergent SHA-256), then heal.

After healing it asserts:

- All nodes reconverge to bit-identical SHA-256 application state.
- Health endpoints (`/v1/health`) are OK on all nodes.
- Post-heal writes on all nodes propagate.

## Invariants

- All application SQL goes through `psql` over the PostgreSQL wire
  listener, never via direct SQLite file access.
- Encryption keys are generated per run (`openssl rand`) and held only in
  named environment variables (`GALV_CHAOS_KEY_A/B/C`); they never appear in
  config files, CLI flags, or logs.

## Layout

- Isolated `runtime/node-*` dirs, dedicated ports derived from
  `GALVANIZE_CHAOS_BASE_PORT` (pg `base+11..13`, api `pg-11000`, gossip
  `base+1091..1093` on `127.0.0.91..93`).
- Passing runs remove `runtime/`; failures are retained under
  `tests-live/failures/<timestamp>-chaos-load/` via the EXIT trap.

## Knobs

| Variable | Default | Meaning |
|---|---|---|
| `GALVANIZE_BIN` | `target/debug/galvanize` | Node binary under test |
| `GALVANIZE_LIVE_RUNTIME` | `<scenario>/runtime` | Runtime dir override |
| `GALVANIZE_CHAOS_BASE_PORT` | `46300` | Base for pg/api/gossip ports |
| `GALVANIZE_CHAOS_WRITE_SECONDS` | `12` | Continuous-write seconds per fault phase |
| `GALVANIZE_CHAOS_SETTLE_SECONDS` | `20` | Max reconvergence wait after healing |
| `GALVANIZE_LIVE_TIMEOUT_SECONDS` | `30` | Readiness/unlock timeouts |
| `GALVANIZE_LIVE_RUST_LOG` | `info,...` | Agent log filter |

Defaults keep the scenario to a few minutes.

## Dispatch (for `tests-live/run.sh`, not applied)

```sh
chaos-load) exec "$root/tests-live/chaos-load/run.sh" ;;
```

plus a `chaos-load` entry in the `all)` list and usage string.

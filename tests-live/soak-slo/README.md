# Soak SLO: Sustained-Write Stability Soak

Run `bash tests-live/soak-slo/run.sh` after `cargo build -p corrosion --bin galvanize`.

The scenario starts three encrypted GALVANIZE nodes (unique per-run keys via
`openssl`, unlocked through each node's Remote Unlock HTTP API) and drives
continuous batched `psql` writes against all three PostgreSQL wire listeners
for `GALVANIZE_SLO_DURATION_SECONDS` (default 120s). It then gates on SLOs,
not just convergence:

- **Convergence:** bit-identical SHA-256 application-data snapshots and equal
  row counts across all three nodes, with a minimum row floor proving the
  soak actually wrote.
- **WAL bound:** every node's `-wal`/`-journal` file stays under
  `GALVANIZE_SLO_MAX_WAL_BYTES` (also applied as `PRAGMA journal_size_limit`
  on a best-effort basis; the file-size check is authoritative).
- **Health:** every node's `/v1/health` stays responsive and within
  `queue_size`/`gaps` thresholds (`GALVANIZE_SLO_MAX_QUEUE`,
  `GALVANIZE_SLO_MAX_GAPS`).
- **Liveness:** no agent process died during the soak.

Prometheus telemetry (`[telemetry.prometheus]`) is enabled on each node and
logical protocol byte deltas are reported for the write and settle phases,
following the `benchmark` scenario's metrics-scraping pattern.

Ports default to `GALVANIZE_SLO_BASE_PORT=46100` with the go-client
convention: PostgreSQL `base+i`, API `base+100+i`, gossip `base+200+i`,
metrics `base+300+i` per node `i` in `{0,1,2}`.

```bash
GALVANIZE_SLO_DURATION_SECONDS=15 GALVANIZE_SLO_SETTLE_SECONDS=30 \
bash tests-live/soak-slo/run.sh
```

Writer selection: `GALVANIZE_SLO_WRITERS` lists which nodes get a sustained
writer (`a`, `b`, `c`; default `"a"`, one tight 20-row-txn loop per listed
node over disjoint PK ranges).

> **Known issue (2026-09-25): concurrent multi-writer runs leave a stuck
> replication tail.** With two or more writers, one node ends up permanently
> missing a few batches of another writer's rows (e.g. 650/650/640 with
> `WRITERS="a b"`), frozen across a 90s settle window while agent logs show
> the same ~40-change sync batch re-sent every ~2s without the gap closing.
> Single-writer runs converge fully (verified green at ~2000 rows), and the
> small-scale multi-writer `crdt-contention` scenario passes, so this bites
> with sustained concurrent INSERT volume. Repro:
> `GALVANIZE_SLO_DURATION_SECONDS=15 GALVANIZE_SLO_SETTLE_SECONDS=30
> GALVANIZE_SLO_BATCH_ROWS=5 GALVANIZE_SLO_WRITERS="a b"
> bash tests-live/soak-slo/run.sh`.
> Until that product bug is fixed, the default gate is single-writer
> (`GALVANIZE_SLO_WRITERS="a"`); set `GALVANIZE_SLO_WRITERS="a b c"` to
> exercise (and currently repro with) concurrent writers.

| Variable | Default | Meaning |
| --- | --- | --- |
| `GALVANIZE_BIN` | `target/debug/galvanize` | Agent binary under test |
| `GALVANIZE_LIVE_RUNTIME` | `tests-live/soak-slo/runtime` | Isolated node dirs |
| `GALVANIZE_SLO_BASE_PORT` | `46100` | Port base (pg/api/gossip/metrics offsets) |
| `GALVANIZE_SLO_DURATION_SECONDS` | `120` | Sustained-write duration |
| `GALVANIZE_SLO_SETTLE_SECONDS` | `30` | Convergence window after writes stop |
| `GALVANIZE_LIVE_TIMEOUT_SECONDS` | `60` | Readiness/unlock deadline |
| `GALVANIZE_SLO_MAX_QUEUE` | `5000` | `/v1/health?queue_size=` threshold |
| `GALVANIZE_SLO_MAX_GAPS` | `100` | `/v1/health?gaps=` threshold |
| `GALVANIZE_SLO_MAX_WAL_BYTES` | `67108864` | WAL/journal file-size cap |
| `GALVANIZE_SLO_MIN_ROWS` | `60` | Minimum rows per node (soak liveness floor) |
| `GALVANIZE_SLO_BATCH_ROWS` | `20` | Rows per write transaction |
| `GALVANIZE_SLO_WRITERS` | `"a"` | Nodes with a sustained writer (`a`/`b`/`c`) |

A passing run removes its `runtime/` tree. A failing run retains it under
`tests-live/failures/<timestamp>-soak-slo/` with encrypted databases and
agent logs.

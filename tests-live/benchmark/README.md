# Two-Node Replication Benchmark

Run `bash tests-live/benchmark/run.sh` after `cargo build -p corrosion`.

The benchmark starts two encrypted GALVANIZE nodes. Node A writes through its
PostgreSQL wire listener for 30 seconds using fully acknowledged transactions.
Every transaction has 100 mutations: 50 new rows and 50 updates to seeded hot
rows. Node B must converge to the same row count and SHA-256 application-data
hash within 120 seconds.

The console report includes transaction and mutation rates, total inserts and
updates, convergence time, and GALVANIZE logical protocol byte rates. Protocol
bytes aggregate outbound sync, broadcast, and gossip payloads from both nodes;
they exclude PostgreSQL client traffic and QUIC/IP wire overhead.

Override durations when needed:

```bash
GALVANIZE_LIVE_BENCHMARK_WRITE_SECONDS=60 \
GALVANIZE_LIVE_BENCHMARK_SYNC_TIMEOUT_SECONDS=180 \
bash tests-live/benchmark/run.sh
```

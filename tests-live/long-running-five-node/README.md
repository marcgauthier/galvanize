# Five-Node Long-Running Replication Test

This manual-only test runs five encrypted Galvanize nodes until their combined
allocated SQLite database storage reaches 25 GiB. It is intentionally excluded
from `bash tests-live/run.sh all`.

The test **deletes and recreates** these exact directories at every startup:

- `/media/marc/2TB/DATA_GALVANIZE/node1`
- `/media/marc/2TB/DATA_GALVANIZE/node2`
- `/media/marc/2TB/DATA_GALVANIZE/node3`
- `/media/marc/2TB/DATA_GALVANIZE/node4`
- `/media/marc/2TB/DATA_GALVANIZE/node5`

The mount must be read-write. The five directories are retained after a
capacity completion, interruption, or failure for inspection; the next launch
resets them.

Build first, then run the scenario. Its wrapper sets five distinct test-only
defaults as named environment variables (`12345-a` through `12345-e`); callers
may override any of them. The runner posts each value only to that node's
Remote Unlock HTTP API; no key is put in a config file or daemon command line.

```bash
cargo build -p corrosion
bash tests-live/long-running-five-node/run.sh
```

To replace a test default, set the corresponding named environment variable
before launching the wrapper.

Nodes 1–3 remain online. Node 4 is down 30% of each 330-second cycle and Node
5 is down 50%, using real process stops and restarts while keeping their
external databases. Every five minutes the workload pauses for 30 seconds,
forces both intermittent nodes online, then reports each node's row count,
allocated DB storage, and canonical PostgreSQL-query SHA-256 snapshot. A
mismatched checkpoint fails the test.

The workload averages roughly one 256 KiB insert per active second across the
cluster, periodically waits 20 seconds then commits a 20-row burst, and deletes
one random row for every 100 successful inserts. Capacity includes `corrosion.db`,
its WAL/SHM files, and subscriptions, but not logs or configuration files.

The Python runner owns process groups and verifies that every API, PostgreSQL,
and gossip listener is released before restarting a node. Unexpected process
exits, unlock failures, PostgreSQL errors, listener conflicts, and convergence
mismatches fail immediately with a bounded agent-log tail. Checkpoints compare
the complete canonical row metadata and verify deterministic payload samples
against their expected SHA-256 values. Checkpoint comparison queries have no
client-side time limit because their runtime grows with the databases; the
console reports the start and elapsed time of every node scan. Routine writes,
readiness checks, and process operations retain bounded timeouts.

Run the harness regression suite without starting Galvanize nodes:

```bash
python3 tests-live/long-running-five-node/test_runner.py
```

For a short validation run only, these environment variables may override the
defaults: `GALVANIZE_LIVE_LONG_RUNNING_STORAGE_ROOT`,
`GALVANIZE_LIVE_LONG_RUNNING_CAPACITY_BYTES`,
`GALVANIZE_LIVE_LONG_RUNNING_CHECKPOINT_SECONDS`,
`GALVANIZE_LIVE_LONG_RUNNING_CHECKPOINT_PAUSE_SECONDS`, and
`GALVANIZE_LIVE_LONG_RUNNING_PAYLOAD_BYTES`. Accelerated harness validation may
also override `GALVANIZE_LIVE_LONG_RUNNING_BURST_INTERVAL_SECONDS`,
`GALVANIZE_LIVE_LONG_RUNNING_BURST_QUIET_SECONDS`,
`GALVANIZE_LIVE_LONG_RUNNING_BURST_INSERT_COUNT`, and
`GALVANIZE_LIVE_LONG_RUNNING_SAMPLE_ROWS`.

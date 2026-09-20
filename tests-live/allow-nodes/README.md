# Allow-nodes live test

Run `bash tests-live/allow-nodes/run.sh`. It creates `runtime/allow-nodes/node-a`,
`runtime/allow-nodes/node-b`, and `runtime/allow-nodes/node-c` on separate loopback IPs.
Nodes A and C allow all peers; B allows only A. For three minutes, every node
writes one row per second through its own PostgreSQL listener. The test pauses
for 30 seconds, verifies B logged rejection of C's direct connection attempt,
and compares every node's ordered row digest and row count. Override the long
timings for development with `GALVANIZE_LIVE_WRITE_SECONDS` and
`GALVANIZE_LIVE_SETTLE_SECONDS`.

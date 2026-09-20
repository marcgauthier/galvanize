# High/Low live test

This scenario owns `runtime/highlow/node-low`, `runtime/highlow/node-high`, and
`runtime/highlow/node-high-replica`. Its runner is reserved for the daemon High/Low
worker and currently exits 77 until PostgreSQL commit capture, HTTP artifact
transfer, High application, and relay are available.

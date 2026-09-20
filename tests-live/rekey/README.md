# Rekey live test

Run `bash tests-live/rekey/run.sh`. It creates `runtime/rekey/node-a`, writes data
through PostgreSQL wire, stops the agent, performs offline rekeying via named
key environment variables, then restarts and verifies the data with the new key.

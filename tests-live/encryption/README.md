# Encryption live test

Run `bash tests-live/encryption/run.sh`. It creates encrypted node A and
unencrypted node B in `runtime/encryption/`. Both nodes write one PostgreSQL
row per second for one minute, then pause for 10 seconds. The test compares the
complete ordered row digest and count on both nodes, stops them, and directly
checks that A's database/WAL contains no application plaintext while B's does.
It also verifies that a wrong key cannot start A and that the correct key can
restart it. For a shorter development run, set
`GALVANIZE_LIVE_ENCRYPTION_WRITE_SECONDS` and
`GALVANIZE_LIVE_ENCRYPTION_SETTLE_SECONDS`.

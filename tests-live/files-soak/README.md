# files-soak — Long-Running File Upload / Download / Search Soak Test

## Overview

This scenario exercises the Galvanize file subsystem continuously over a configurable
duration (default **10 minutes**). It uploads one file per minute, downloads every
previously uploaded file from a second node via peer-fetch, and runs a filename search
after every upload round.

## Topology

```
  ┌─────────────────────────────────┐
  │  Low Cluster (plain gossip)     │
  │                                 │
  │  LOW-1  api=47051  pg=54951     │  ← accepts uploads, pushes to peer
  │  LOW-2  api=47052  pg=54952     │  ← peer-fetch node
  └─────────────────────────────────┘
```

Both nodes share the same encryption key, gossip together, and accept files from peers.
Only LOW-1 has `accept-uploads = true`.

## What Is Tested

| Step              | Description                                                                 |
|-------------------|-----------------------------------------------------------------------------|
| **UPLOAD**        | POST a uniquely named file to LOW-1 every 60 s                              |
| **DB-CHECK**      | Verify the new row is present in `files` via PostgreSQL wire on LOW-1       |
| **GOSSIP-CHECK**  | Wait for the metadata row to appear on LOW-2 via mesh replication           |
| **PEER-FETCH**    | Download every uploaded file from LOW-2; assert bit-identical content       |
| **SEARCH**        | Search `soak_` on LOW-2; assert expected file count                         |
| **STATS**         | Assert running file count and total bytes on both nodes                     |

## Running

```bash
# Quick run with overridden duration (e.g. 2 minutes for CI):
GALVANIZE_LIVE_FILES_SOAK_DURATION_SECONDS=120 bash tests-live/files-soak/run.sh

# Default 10-minute soak:
bash tests-live/files-soak/run.sh

# Via top-level dispatcher:
bash tests-live/run.sh files-soak
```

## Failure Artifacts

On failure the runtime directory is moved to `tests-live/failures/` with full agent
logs and the encrypted databases preserved.

## Environment Variables

| Variable                                        | Default | Description                           |
|-------------------------------------------------|---------|---------------------------------------|
| `GALVANIZE_LIVE_FILES_SOAK_DURATION_SECONDS`    | `600`   | Total soak duration in seconds        |
| `GALVANIZE_LIVE_FILES_SOAK_INTERVAL_SECONDS`    | `60`    | Seconds between upload rounds         |
| `GALVANIZE_LIVE_TIMEOUT_SECONDS`                | `30`    | Per-operation wait timeout (seconds)  |
| `GALVANIZE_BIN`                                 | (auto)  | Path to the `corrosion` binary        |

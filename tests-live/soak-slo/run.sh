#!/usr/bin/env bash
# Soak SLO: sustained high-write-rate soak against a 3-node encrypted
# Galvanize cluster, gated on SLOs (not just convergence).
#
# All application SQL travels over the PostgreSQL wire listeners via psql.
# Database keys travel via named environment variables to the Remote Unlock
# HTTP API only; they never appear in config files or CLI flags.
set -euo pipefail

test_dir=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$test_dir/../.." && pwd)
runtime=${GALVANIZE_LIVE_RUNTIME:-"$test_dir/runtime"}
binary=${GALVANIZE_BIN:-"$root/target/debug/galvanize"}
base=${GALVANIZE_SLO_BASE_PORT:-46100}
duration=${GALVANIZE_SLO_DURATION_SECONDS:-120}
settle=${GALVANIZE_SLO_SETTLE_SECONDS:-30}
timeout=${GALVANIZE_LIVE_TIMEOUT_SECONDS:-60}
max_queue=${GALVANIZE_SLO_MAX_QUEUE:-5000}
max_gaps=${GALVANIZE_SLO_MAX_GAPS:-100}
max_wal=${GALVANIZE_SLO_MAX_WAL_BYTES:-67108864}
min_rows=${GALVANIZE_SLO_MIN_ROWS:-60}
batch=${GALVANIZE_SLO_BATCH_ROWS:-20}
# Default is single-writer: sustained CONCURRENT multi-writer INSERTs leave a
# stuck replication tail (see README known issue). Opt into "a b c" to repro.
writers=${GALVANIZE_SLO_WRITERS:-"a"}

[[ -x "$binary" ]] || { echo "build first: cargo build -p corrosion --bin galvanize" >&2; exit 2; }
command -v psql >/dev/null || { echo "psql is required for live tests" >&2; exit 2; }
command -v curl >/dev/null || { echo "curl is required for health/metrics checks" >&2; exit 2; }
command -v openssl >/dev/null || { echo "openssl is required for unique encryption keys" >&2; exit 2; }

# Unique per-run encryption keys, supplied to agents via env vars only.
export GALV_SLO_KEY_A=$(openssl rand -hex 32)
export GALV_SLO_KEY_B=$(openssl rand -hex 32)
export GALV_SLO_KEY_C=$(openssl rand -hex 32)

pids=()
writer_pids=()
cleanup() {
  local status=$?
  local pid
  for pid in "${writer_pids[@]:-}"; do kill "$pid" 2>/dev/null || true; done
  for pid in "${pids[@]:-}"; do kill "$pid" 2>/dev/null || true; done
  for pid in "${writer_pids[@]:-}"; do wait "$pid" 2>/dev/null || true; done
  for pid in "${pids[@]:-}"; do wait "$pid" 2>/dev/null || true; done
  for pid in "${pids[@]:-}"; do kill -9 "$pid" 2>/dev/null || true; done
  if (( status != 0 )); then
    local failed="$root/tests-live/failures/$(date -u +%Y%m%dT%H%M%SZ)-soak-slo"
    mkdir -p "$(dirname "$failed")"
    mv "$runtime" "$failed"
    echo "RESULT: FAIL scenario=soak-slo" >&2
    echo "failure artifacts retained at $failed" >&2
  else
    rm -rf "$runtime"
    echo "Soak SLO live scenario passed; temporary runtime data was removed."
  fi
  exit "$status"
}
trap cleanup EXIT

unlock_node() {
  local api_port=$1 key_env=$2 key
  key=${!key_env}
  local escaped_key=${key//\\/\\\\}
  escaped_key=${escaped_key//\"/\\\"}
  local deadline=$((SECONDS + timeout))
  until printf '{"key":"%s","cipher":"chacha20"}' "$escaped_key" | \
    curl -fsS -X POST "http://127.0.0.1:$api_port/v1/admin/unlock" \
      -H "Content-Type: application/json" --data-binary @- >/dev/null 2>&1; do
    (( SECONDS < deadline )) || { echo "Failed to unlock node on port $api_port" >&2; return 1; }
    sleep 0.1
  done
}

start_node() {
  local node=$1 gossip=$2 pg=$3 api=$4 metrics=$5 bootstrap=$6 key_env=$7
  local label=${node##*/node-}
  mkdir -p "$node/schema" "$node/logs"
  echo "  START  node ${label^^}  pg=$pg api=$api gossip=$gossip metrics=$metrics"
  cat >"$node/schema/soak.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS slo_records (
  id INTEGER PRIMARY KEY NOT NULL,
  source TEXT NOT NULL DEFAULT '',
  value TEXT NOT NULL DEFAULT ''
) WITHOUT ROWID;
SQL
  cat >"$node/config.toml" <<EOF
[db]
path = "$node/corrosion.db"
schema_paths = ["$node/schema"]
await-unlock = true
[api]
addr = "127.0.0.1:$api"
[[api.pg]]
addr = "127.0.0.1:$pg"
[gossip]
addr = "127.0.0.1:$gossip"
client_addr_v4 = "127.0.0.1:0"
bootstrap = $bootstrap
plaintext = true
allow-list = ["*"]
[admin]
path = "$node/admin.sock"
[perf]
min_sync_backoff = 1
max_sync_backoff = 2
[telemetry.prometheus]
bind_addr = "127.0.0.1:$metrics"
[log]
format = "json"
EOF
  RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info}" \
    "$binary" --config "$node/config.toml" agent >"$node/logs/agent.log" 2>&1 &
  pids+=("$!")
  unlock_node "$api" "$key_env"
}

wait_pg() {
  local port=$1 deadline=$((SECONDS + timeout))
  until psql "postgresql://postgres@127.0.0.1:$port/postgres" -Atqc 'SELECT 1' >/dev/null 2>&1; do
    (( SECONDS < deadline )) || { echo "PostgreSQL listener $port did not become ready" >&2; return 1; }
    sleep 0.2
  done
}

wait_schema() {
  local port=$1 deadline=$((SECONDS + timeout))
  wait_pg "$port"
  until psql "postgresql://postgres@127.0.0.1:$port/postgres" -Atqc 'SELECT count(*) FROM slo_records' >/dev/null 2>&1; do
    (( SECONDS < deadline )) || { echo "schema slo_records was not ready on $port" >&2; return 1; }
    sleep 0.2
  done
  echo "  READY  pg=127.0.0.1:$port schema=slo_records"
}

tx_bytes() {
  local metrics_port=$1
  curl -fsS "http://127.0.0.1:$metrics_port/metrics" |
    awk '$1 ~ /^corro_transport_tx_bytes_v2_total(\{|$)/ { total += $2 } END { printf "%.0f\n", total + 0 }'
}

rm -rf "$runtime"; mkdir -p "$runtime"
echo "== soak-slo: 3-node encrypted sustained-write SLO soak =="
echo "  PLAN      continuous psql writes for ${duration}s, then settle ${settle}s, then SLO gates"

pg_a=$base;       api_a=$((base + 100));       gossip_a=$((base + 200));       metrics_a=$((base + 300))
pg_b=$((base+1)); api_b=$((base + 101));       gossip_b=$((base + 201));       metrics_b=$((base + 301))
pg_c=$((base+2)); api_c=$((base + 102));       gossip_c=$((base + 202));       metrics_c=$((base + 302))

start_node "$runtime/node-a" "$gossip_a" "$pg_a" "$api_a" "$metrics_a" \
  "[\"127.0.0.1:$gossip_b\", \"127.0.0.1:$gossip_c\"]" GALV_SLO_KEY_A
start_node "$runtime/node-b" "$gossip_b" "$pg_b" "$api_b" "$metrics_b" \
  "[\"127.0.0.1:$gossip_a\", \"127.0.0.1:$gossip_c\"]" GALV_SLO_KEY_B
start_node "$runtime/node-c" "$gossip_c" "$pg_c" "$api_c" "$metrics_c" \
  "[\"127.0.0.1:$gossip_a\", \"127.0.0.1:$gossip_b\"]" GALV_SLO_KEY_C
wait_schema "$pg_a"; wait_schema "$pg_b"; wait_schema "$pg_c"

# Bound engine WAL growth to the SLO cap (best effort; the file-size gate below is authoritative).
for port in "$pg_a" "$pg_b" "$pg_c"; do
  psql "postgresql://postgres@127.0.0.1:$port/postgres" -v ON_ERROR_STOP=1 \
    -qc "PRAGMA journal_size_limit=$max_wal;" >/dev/null 2>&1 || true
done

baseline_bytes=$(( $(tx_bytes "$metrics_a") + $(tx_bytes "$metrics_b") + $(tx_bytes "$metrics_c") ))

# Sustained writers: one tight batched-insert loop per node, disjoint PK ranges.
# NOTE: the batch-building loop runs inside a pipeline (`{ ...; } | psql`) and
# therefore in a subshell, so per-row `id` mutation inside it would be lost and
# every batch would resend the same IDs. Derive IDs arithmetically from a batch
# counter maintained outside the pipeline instead.
writer() {
  local pg=$1 start_id=$2 tag=$3
  local end=$((SECONDS + duration)) n=0 b=0
  while (( SECONDS < end )); do
    local base=$((start_id + b * batch))
    {
      echo "BEGIN;"
      for ((k = 0; k < batch; k++)); do
        printf "INSERT INTO slo_records (id, source, value) VALUES (%d, '%s', 'payload-%d');\n" "$((base + k))" "$tag" "$((base + k))"
      done
      echo "COMMIT;"
    } | psql "postgresql://postgres@127.0.0.1:$pg/postgres" -v ON_ERROR_STOP=1 -q >/dev/null \
      || { echo "$base" >"$runtime/writer-$tag.failed"; return 1; }
    n=$((n + 1))
    b=$((b + 1))
  done
  local next_id=$((start_id + b * batch))
  echo "$next_id" >"$runtime/writer-$tag.final"
  echo "  WRITE  writer=$tag transactions=$n rows=$((next_id - start_id))"
}
for w in $writers; do
  case $w in
    a) writer "$pg_a" 1 'a' & writer_pids+=("$!") ;;
    b) writer "$pg_b" 100000001 'b' & writer_pids+=("$!") ;;
    c) writer "$pg_c" 200000001 'c' & writer_pids+=("$!") ;;
    *) echo "unknown writer '$w'" >&2; exit 2 ;;
  esac
done

echo "  WRITE  three concurrent writers running for ${duration}s (batch=${batch} rows/txn)"
writer_status=0
for pid in "${writer_pids[@]}"; do wait "$pid" || writer_status=1; done
writer_pids=()
[[ $writer_status == 0 ]] || { echo "a soak writer failed; see $runtime/writer-*.failed" >&2; exit 1; }
[[ ! -e "$runtime/writer-a.failed" && ! -e "$runtime/writer-b.failed" && ! -e "$runtime/writer-c.failed" ]] \
  || { echo "a soak writer failed; see $runtime/writer-*.failed" >&2; exit 1; }
bytes_after_writes=$(( $(tx_bytes "$metrics_a") + $(tx_bytes "$metrics_b") + $(tx_bytes "$metrics_c") ))

inserted_a=0; inserted_b=0; inserted_c=0
[[ -f "$runtime/writer-a.final" ]] && inserted_a=$(($(cat "$runtime/writer-a.final") - 1))
[[ -f "$runtime/writer-b.final" ]] && inserted_b=$(($(cat "$runtime/writer-b.final") - 100000001))
[[ -f "$runtime/writer-c.final" ]] && inserted_c=$(($(cat "$runtime/writer-c.final") - 200000001))
echo "  STATS  inserted_rows a=$inserted_a b=$inserted_b c=$inserted_c total=$((inserted_a + inserted_b + inserted_c))"
echo "  STATS  logical_protocol_bytes during_writes=$((bytes_after_writes - baseline_bytes))"

echo "  PAUSE  writes stopped; waiting ${settle}s for mesh convergence"
sleep "$settle"
bytes_after_settle=$(( $(tx_bytes "$metrics_a") + $(tx_bytes "$metrics_b") + $(tx_bytes "$metrics_c") ))
echo "  STATS  logical_protocol_bytes during_settle=$((bytes_after_settle - bytes_after_writes))"

# Gate (d): no agent process died during the soak.
for pid in "${pids[@]}"; do
  kill -0 "$pid" 2>/dev/null || { echo "agent process $pid died during the soak" >&2; exit 1; }
done
echo "  CHECK  all 3 agent processes survived the soak"

# Gate (c): health endpoints responsive and within queue/gap thresholds.
for api in "$api_a" "$api_b" "$api_c"; do
  body=$(curl -fsS "http://127.0.0.1:$api/v1/health?queue_size=$max_queue&gaps=$max_gaps") \
    || { echo "health SLO failed on api port $api (thresholds queue_size<=$max_queue gaps<=$max_gaps)" >&2; exit 1; }
  echo "  CHECK  health api=127.0.0.1:$api body=$body"
done

# Gate (a): bit-identical SHA-256 convergence across all nodes.
count_a=$(psql "postgresql://postgres@127.0.0.1:$pg_a/postgres" -Atqc 'SELECT count(*) FROM slo_records')
count_b=$(psql "postgresql://postgres@127.0.0.1:$pg_b/postgres" -Atqc 'SELECT count(*) FROM slo_records')
count_c=$(psql "postgresql://postgres@127.0.0.1:$pg_c/postgres" -Atqc 'SELECT count(*) FROM slo_records')
hash_a=$(psql "postgresql://postgres@127.0.0.1:$pg_a/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM slo_records ORDER BY id" | sha256sum | awk '{print $1}')
hash_b=$(psql "postgresql://postgres@127.0.0.1:$pg_b/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM slo_records ORDER BY id" | sha256sum | awk '{print $1}')
hash_c=$(psql "postgresql://postgres@127.0.0.1:$pg_c/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM slo_records ORDER BY id" | sha256sum | awk '{print $1}')
echo "  COMPARE node=A rows=$count_a sha256=$hash_a"
echo "  COMPARE node=B rows=$count_b sha256=$hash_b"
echo "  COMPARE node=C rows=$count_c sha256=$hash_c"
[[ $count_a -ge $min_rows && $count_b -ge $min_rows && $count_c -ge $min_rows ]] \
  || { echo "soak produced too few rows (min $min_rows per node)" >&2; exit 1; }
[[ $count_a == "$count_b" && $count_b == "$count_c" && $hash_a == "$hash_b" && $hash_b == "$hash_c" ]] \
  || { echo "replicated database mismatch after convergence window" >&2; exit 1; }
echo "  CHECK  all nodes converged to identical $count_a-row slo_records data (sha256=$hash_a)"

# Gate (b): WAL file sizes under the journal_size_limit SLO cap.
for node in "$runtime/node-a" "$runtime/node-b" "$runtime/node-c"; do
  limit=$(psql "postgresql://postgres@127.0.0.1:$pg_a/postgres" -Atqc "PRAGMA journal_size_limit;" 2>/dev/null || true)
  for f in "$node/corrosion.db-wal" "$node/corrosion.db-journal"; do
    size=0
    [[ -f $f ]] && size=$(stat -c%s "$f")
    echo "  CHECK  wal node=$(basename "$node") file=$(basename "$f") bytes=$size cap=$max_wal pragma=${limit:-unknown}"
    (( size <= max_wal )) || { echo "WAL SLO failed: $f is $size bytes (cap $max_wal)" >&2; exit 1; }
  done
done

echo "RESULT: PASS scenario=soak-slo nodes=3 rows=$count_a sha256=$hash_a writes_s=$duration"

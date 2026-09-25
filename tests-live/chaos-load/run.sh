#!/usr/bin/env bash
# chaos-load: fault injection DURING continuous write load, then recovery proof.
#
# 3-node encrypted cluster. All application SQL travels over the PostgreSQL
# wire listener via psql (never direct SQLite access). Secrets (encryption
# keys) travel only via named environment variables, never via config files
# or CLI flags.
#
# Phases:
#   1. Baseline full-mesh convergence.
#   2. Continuous writes while node B is hit with kill -9 mid-write, then
#      restarted (encrypted WAL recovery).
#   3. Continuous writes while the cluster is partitioned via a gossip
#      allow-list split ({A,B} vs {C}), then healed back to a full mesh.
#   4. Reconvergence: bit-identical SHA-256 state on all nodes, health
#      endpoints OK on all nodes, post-heal writes propagate.
set -euo pipefail

test_dir=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$test_dir/../.." && pwd)
runtime=${GALVANIZE_LIVE_RUNTIME:-"$test_dir/runtime"}
binary=${GALVANIZE_BIN:-"$root/target/debug/galvanize"}
base=${GALVANIZE_CHAOS_BASE_PORT:-46300}
timeout_seconds=${GALVANIZE_LIVE_TIMEOUT_SECONDS:-30}
write_seconds=${GALVANIZE_CHAOS_WRITE_SECONDS:-12}
settle_seconds=${GALVANIZE_CHAOS_SETTLE_SECONDS:-20}

[[ -x "$binary" ]] || { echo "build first: cargo build -p corrosion --bin galvanize" >&2; exit 2; }
command -v psql >/dev/null || { echo "psql is required for live tests" >&2; exit 2; }
command -v curl >/dev/null || { echo "curl is required for health checks" >&2; exit 2; }
command -v openssl >/dev/null || { echo "openssl is required for unique encryption keys" >&2; exit 2; }

pg_a=$((base + 11)); pg_b=$((base + 12)); pg_c=$((base + 13))
api_a=$((pg_a - 11000)); api_b=$((pg_b - 11000)); api_c=$((pg_c - 11000))
gossip_a="127.0.0.91:$((base + 1091))"
gossip_b="127.0.0.92:$((base + 1092))"
gossip_c="127.0.0.93:$((base + 1093))"
bootstrap_all="[\"$gossip_a\", \"$gossip_b\", \"$gossip_c\"]"
bootstrap_ab="[\"$gossip_a\", \"$gossip_b\"]"

# Unique per-run encryption keys, held only in named environment variables.
export GALV_CHAOS_KEY_A="galv-chaos-a-$(openssl rand -hex 8)"
export GALV_CHAOS_KEY_B="galv-chaos-b-$(openssl rand -hex 8)"
export GALV_CHAOS_KEY_C="galv-chaos-c-$(openssl rand -hex 8)"

cleanup_pids=()
declare -A named_pids=()
started_at=$SECONDS

cleanup() {
  local status=$?
  local pid
  for pid in "${cleanup_pids[@]:-}"; do kill "$pid" 2>/dev/null || true; done
  for pid in "${cleanup_pids[@]:-}"; do wait "$pid" 2>/dev/null || true; done
  for pid in "${cleanup_pids[@]:-}"; do kill -9 "$pid" 2>/dev/null || true; done
  if (( status != 0 )); then
    if [[ -d "$runtime" ]]; then
      local failed="$root/tests-live/failures/$(date -u +%Y%m%dT%H%M%SZ)-chaos-load"
      mkdir -p "$(dirname "$failed")"
      mv "$runtime" "$failed"
      echo "chaos-load failure artifacts retained at $failed" >&2
    fi
  else
    rm -rf "$runtime"
    echo "chaos-load live scenario passed; temporary runtime data was removed."
  fi
  exit "$status"
}
trap cleanup EXIT

unlock_node() {
  local api_port=$1 key=$2
  local escaped_key
  escaped_key=${key//\\/\\\\}
  escaped_key=${escaped_key//\"/\\\"}
  local deadline=$((SECONDS + timeout_seconds))
  until printf '{"key":"%s","cipher":"chacha20"}' "$escaped_key" | \
    curl -fsS -X POST "http://127.0.0.1:$api_port/v1/admin/unlock" \
      -H "Content-Type: application/json" --data-binary @- >/dev/null 2>&1; do
    (( SECONDS < deadline )) || { echo "Failed to unlock node on port $api_port" >&2; return 1; }
    sleep 0.1
  done
}

start_chaos_node() {
  local name=$1 node=$2 gossip=$3 pg=$4 bootstrap=$5 key=$6 allow=$7
  local pg_port=${pg##*:}
  local api_port=$(( pg_port - 11000 ))
  mkdir -p "$node/schema" "$node/logs"
  echo "  START  node ${name^^}  pg=$pg gossip=$gossip allow=$allow"
  cat >"$node/schema/live.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS live_records (
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
addr = "127.0.0.1:$api_port"
[[api.pg]]
addr = "$pg"
[gossip]
addr = "$gossip"
client_addr_v4 = "${gossip%:*}:0"
bootstrap = $bootstrap
plaintext = true
allow-list = $allow
[admin]
path = "$node/admin.sock"
[perf]
min_sync_backoff = 1
max_sync_backoff = 2
[log]
format = "json"
EOF
  RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info,corro_agent::agent::handlers=debug}" \
    "$binary" --config "$node/config.toml" agent >"$node/logs/agent.log" 2>&1 &
  local pid=$!
  cleanup_pids+=("$pid")
  named_pids["$name"]="$pid"
  unlock_node "$api_port" "$key"
}

kill_named_node_hard() {
  local name=$1
  local pid="${named_pids[$name]:-}"
  if [[ -n "$pid" ]]; then
    kill -9 "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    unset "named_pids[$name]"
  fi
}

stop_all_nodes() {
  local pid
  for pid in "${cleanup_pids[@]:-}"; do
    kill "$pid" 2>/dev/null || true
  done
  for pid in "${cleanup_pids[@]:-}"; do
    wait "$pid" 2>/dev/null || true
  done
  for pid in "${cleanup_pids[@]:-}"; do
    kill -9 "$pid" 2>/dev/null || true
  done
  cleanup_pids=()
  named_pids=()
}

wait_pg() {
  local port=$1 deadline=$((SECONDS + timeout_seconds))
  until psql "postgresql://postgres@127.0.0.1:$port/postgres" -Atqc 'SELECT 1' >/dev/null 2>&1; do
    (( SECONDS < deadline )) || { echo "PostgreSQL listener $port did not become ready" >&2; return 1; }
    sleep 0.2
  done
}

wait_node() {
  local port=$1 deadline=$((SECONDS + timeout_seconds))
  wait_pg "$port"
  until psql "postgresql://postgres@127.0.0.1:$port/postgres" -Atqc 'SELECT count(*) FROM live_records' >/dev/null 2>&1; do
    (( SECONDS < deadline )) || { echo "schema live_records was not ready on $port" >&2; return 1; }
    sleep 0.2
  done
  echo "  READY  pg=127.0.0.1:$port schema=live_records"
}

check_health() {
  local label=$1 api_port=$2
  curl -fsS "http://127.0.0.1:$api_port/v1/health" >/dev/null \
    || { echo "health endpoint not OK on node $label (api $api_port)" >&2; return 1; }
  echo "  CHECK  node ${label^^} health endpoint OK (api $api_port)"
}

row_count() {
  psql "postgresql://postgres@127.0.0.1:$1/postgres" -Atqc 'SELECT count(*) FROM live_records' 2>/dev/null || echo 0
}

state_hash() {
  psql "postgresql://postgres@127.0.0.1:$1/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}'
}

chaos_load() {
  local node_a="$runtime/node-a" node_b="$runtime/node-b" node_c="$runtime/node-c"
  rm -rf "$runtime"; mkdir -p "$runtime"
  echo "== chaos-load: kill -9 + partition faults during continuous write load =="
  echo "  TOPOLOGY  3 encrypted nodes A/B/C; pg=$pg_a/$pg_b/$pg_c gossip=$gossip_a $gossip_b $gossip_c"
  echo "  PLAN      baseline -> kill -9 B mid-write (${write_seconds}s) -> allow-list partition {A,B}|{C} (${write_seconds}s) -> heal -> reconverge (${settle_seconds}s)"

  # Phase 1: baseline full mesh.
  echo "  PHASE 1: baseline full mesh"
  start_chaos_node a "$node_a" "$gossip_a" "127.0.0.1:$pg_a" "$bootstrap_all" "$GALV_CHAOS_KEY_A" '["*"]'
  start_chaos_node b "$node_b" "$gossip_b" "127.0.0.1:$pg_b" "$bootstrap_all" "$GALV_CHAOS_KEY_B" '["*"]'
  start_chaos_node c "$node_c" "$gossip_c" "127.0.0.1:$pg_c" "$bootstrap_all" "$GALV_CHAOS_KEY_C" '["*"]'
  wait_node "$pg_a"; wait_node "$pg_b"; wait_node "$pg_c"

  psql "postgresql://postgres@127.0.0.1:$pg_a/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (1, 'base-a', 'val-base-a-1'), (2, 'base-a', 'val-base-a-2');" >/dev/null
  psql "postgresql://postgres@127.0.0.1:$pg_b/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (3, 'base-b', 'val-base-b-1'), (4, 'base-b', 'val-base-b-2');" >/dev/null
  psql "postgresql://postgres@127.0.0.1:$pg_c/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (5, 'base-c', 'val-base-c-1'), (6, 'base-c', 'val-base-c-2');" >/dev/null

  local deadline=$((SECONDS + timeout_seconds))
  while [[ $(row_count "$pg_a") != 6 || $(row_count "$pg_b") != 6 || $(row_count "$pg_c") != 6 ]]; do
    (( SECONDS < deadline )) || { echo "baseline convergence failed; expected 6 rows on all nodes" >&2; return 1; }
    sleep 0.5
  done
  echo "  CHECK  Phase 1 baseline converged (6 rows on all 3 nodes)"

  # Phase 2: continuous writes with a mid-write kill -9 of node B.
  echo "  PHASE 2: continuous writes with kill -9 of node B mid-write"
  local second tick_started remaining crash_point=$((write_seconds / 2))
  (( crash_point < 1 )) && crash_point=1
  for ((second = 1; second <= write_seconds; second++)); do
    tick_started=$SECONDS
    psql "postgresql://postgres@127.0.0.1:$pg_a/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((1000 + second * 10 + 1)), 'burst-a', 'val-burst-a-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:$pg_c/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((1000 + second * 10 + 3)), 'burst-c', 'val-burst-c-$second');" >/dev/null
    if (( second <= crash_point )); then
      psql "postgresql://postgres@127.0.0.1:$pg_b/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((1000 + second * 10 + 2)), 'burst-b', 'val-burst-b-$second');" >/dev/null
    fi
    if (( second == crash_point )); then
      kill_named_node_hard b
      echo "  CRASH  node B terminated with SIGKILL mid-write while A and C keep committing"
    fi
    if (( second == 1 || second % 5 == 0 || second == write_seconds )); then
      echo "  WRITE  Phase 2 second=$second/${write_seconds} commits: A=$second C=$second B=$(( second <= crash_point ? second : crash_point ))"
    fi
    remaining=$((1 - (SECONDS - tick_started)))
    (( remaining > 0 )) && sleep "$remaining"
  done

  echo "  RESTART  node B with encryption key (WAL recovery)"
  start_chaos_node b "$node_b" "$gossip_b" "127.0.0.1:$pg_b" "$bootstrap_all" "$GALV_CHAOS_KEY_B" '["*"]'
  wait_node "$pg_b"
  sleep 2
  echo "  RECOVER  node B re-opened encrypted database and recovered WAL successfully"

  local base_total=$((6 + 2 * write_seconds + crash_point))

  # Let the restarted node catch up before partitioning, so the partition
  # phase starts from identical state on all nodes.
  echo "  PAUSE  waiting up to ${timeout_seconds}s for node B to catch up after WAL recovery"
  deadline=$((SECONDS + timeout_seconds))
  local catch_a catch_b catch_c catch_ha catch_hb catch_hc
  while true; do
    catch_a=$(row_count "$pg_a"); catch_b=$(row_count "$pg_b"); catch_c=$(row_count "$pg_c")
    if [[ $catch_a == "$base_total" && $catch_b == "$base_total" && $catch_c == "$base_total" ]]; then
      catch_ha=$(state_hash "$pg_a"); catch_hb=$(state_hash "$pg_b"); catch_hc=$(state_hash "$pg_c")
      if [[ $catch_ha == "$catch_hb" && $catch_hb == "$catch_hc" ]]; then
        echo "  SYNC  node B caught up after restart (all nodes $base_total rows, sha256=$catch_ha)"
        break
      fi
    fi
    (( SECONDS < deadline )) || { echo "node B did not catch up after restart" >&2; return 1; }
    sleep 1
  done

  # Phase 3: allow-list partition {A,B} vs {C} during continuous writes.
  echo "  PHASE 3: allow-list partition {A,B} vs {C} during continuous writes"
  stop_all_nodes
  start_chaos_node a "$node_a" "$gossip_a" "127.0.0.1:$pg_a" "$bootstrap_ab" "$GALV_CHAOS_KEY_A" '["127.0.0.91", "127.0.0.92"]'
  start_chaos_node b "$node_b" "$gossip_b" "127.0.0.1:$pg_b" "$bootstrap_ab" "$GALV_CHAOS_KEY_B" '["127.0.0.91", "127.0.0.92"]'
  start_chaos_node c "$node_c" "$gossip_c" "127.0.0.1:$pg_c" '[]' "$GALV_CHAOS_KEY_C" '["127.0.0.93"]'
  wait_node "$pg_a"; wait_node "$pg_b"; wait_node "$pg_c"

  for ((second = 1; second <= write_seconds; second++)); do
    tick_started=$SECONDS
    psql "postgresql://postgres@127.0.0.1:$pg_a/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((10000 + second * 10 + 1)), 'part-ab-a', 'val-part-a-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:$pg_b/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((10000 + second * 10 + 2)), 'part-ab-b', 'val-part-b-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:$pg_c/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((20000 + second * 10 + 3)), 'part-c', 'val-part-c-$second');" >/dev/null
    if (( second == 1 || second % 5 == 0 || second == write_seconds )); then
      echo "  WRITE  Phase 3 second=$second/${write_seconds} partitioned commits: side-AB=$((second * 2)) side-C=$second"
    fi
    remaining=$((1 - (SECONDS - tick_started)))
    (( remaining > 0 )) && sleep "$remaining"
  done

  echo "  PAUSE  partition writes stopped; waiting for intra-side sync"
  sleep 5
  local expected_ab=$((base_total + 2 * write_seconds))
  local expected_c=$((base_total + write_seconds))
  local count_a count_b count_c hash_ab_a hash_ab_b hash_c
  count_a=$(row_count "$pg_a"); count_b=$(row_count "$pg_b"); count_c=$(row_count "$pg_c")
  hash_ab_a=$(state_hash "$pg_a"); hash_ab_b=$(state_hash "$pg_b"); hash_c=$(state_hash "$pg_c")
  echo "  COMPARE side-AB rows: A=$count_a B=$count_b (expected $expected_ab, sha256=$hash_ab_a)"
  echo "  COMPARE side-C rows: C=$count_c (expected $expected_c, sha256=$hash_c)"
  [[ $count_a == "$expected_ab" && $count_b == "$expected_ab" && $hash_ab_a == "$hash_ab_b" ]] || { echo "partition side {A,B} intra-sync mismatch" >&2; return 1; }
  [[ $count_c == "$expected_c" ]] || { echo "partition side {C} row-count mismatch" >&2; return 1; }
  [[ $hash_ab_a != "$hash_c" ]] || { echo "partition sides were not isolated" >&2; return 1; }
  echo "  CHECK  partition stayed isolated with independent intra-side replication"

  # Phase 4: heal and reconverge.
  echo "  PHASE 4: heal partition to full mesh and reconverge"
  stop_all_nodes
  start_chaos_node a "$node_a" "$gossip_a" "127.0.0.1:$pg_a" "$bootstrap_all" "$GALV_CHAOS_KEY_A" '["*"]'
  start_chaos_node b "$node_b" "$gossip_b" "127.0.0.1:$pg_b" "$bootstrap_all" "$GALV_CHAOS_KEY_B" '["*"]'
  start_chaos_node c "$node_c" "$gossip_c" "127.0.0.1:$pg_c" "$bootstrap_all" "$GALV_CHAOS_KEY_C" '["*"]'
  wait_node "$pg_a"; wait_node "$pg_b"; wait_node "$pg_c"

  local total_expected=$((base_total + 3 * write_seconds))
  echo "  PAUSE  waiting up to ${settle_seconds}s for anti-entropy sync (expected $total_expected rows)"
  deadline=$((SECONDS + settle_seconds))
  count_a=0; count_b=0; count_c=0
  hash_ab_a=''; hash_ab_b=''; hash_c=''
  while (( SECONDS < deadline )); do
    count_a=$(row_count "$pg_a"); count_b=$(row_count "$pg_b"); count_c=$(row_count "$pg_c")
    if [[ $count_a == "$total_expected" && $count_b == "$total_expected" && $count_c == "$total_expected" ]]; then
      hash_ab_a=$(state_hash "$pg_a"); hash_ab_b=$(state_hash "$pg_b"); hash_c=$(state_hash "$pg_c")
      if [[ -n "$hash_ab_a" && $hash_ab_a == "$hash_ab_b" && $hash_ab_b == "$hash_c" ]]; then
        echo "  SYNC  all 3 nodes converged (A=$count_a B=$count_b C=$count_c, sha256=$hash_ab_a)"
        break
      fi
    fi
    sleep 2
  done
  echo "  COMPARE reconciled counts: A=$count_a B=$count_b C=$count_c (expected $total_expected)"
  echo "  COMPARE reconciled sha256 A=$hash_ab_a"
  echo "  COMPARE reconciled sha256 B=$hash_ab_b"
  echo "  COMPARE reconciled sha256 C=$hash_c"
  [[ $count_a == "$total_expected" && $count_b == "$total_expected" && $count_c == "$total_expected" ]] || { echo "healed row-count mismatch" >&2; return 1; }
  [[ -n "$hash_ab_a" && $hash_ab_a == "$hash_ab_b" && $hash_ab_b == "$hash_c" ]] || { echo "healed database contents differ across nodes" >&2; return 1; }
  echo "  CHECK  all nodes reconverged to bit-identical SHA-256 state after kill -9 and partition"

  check_health a "$api_a"
  check_health b "$api_b"
  check_health c "$api_c"

  echo "  WRITE  post-heal verification writes on all 3 nodes"
  psql "postgresql://postgres@127.0.0.1:$pg_a/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (99901, 'post-heal-a', 'val-post-heal-a');" >/dev/null
  psql "postgresql://postgres@127.0.0.1:$pg_b/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (99902, 'post-heal-b', 'val-post-heal-b');" >/dev/null
  psql "postgresql://postgres@127.0.0.1:$pg_c/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (99903, 'post-heal-c', 'val-post-heal-c');" >/dev/null
  sleep 3
  local final_count
  final_count=$(row_count "$pg_c")
  [[ $final_count == "$((total_expected + 3))" ]] || { echo "post-heal live write propagation failed" >&2; return 1; }
  echo "  CHECK  post-heal cluster operation verified ($((total_expected + 3)) total rows)"

  stop_all_nodes
  echo "RESULT: PASS scenario=chaos-load nodes=3 elapsed=$((SECONDS - started_at))s"
}

chaos_load

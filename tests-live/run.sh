#!/usr/bin/env bash
# Live Galvanize checks. Application SQL always travels through PostgreSQL.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
scenario=${1:-all}
binary=${GALVANIZE_BIN:-"$root/target/debug/corrosion"}
runtime_root=${GALVANIZE_LIVE_RUNTIME:-"$root/tests-live/runtime"}
timeout_seconds=${GALVANIZE_LIVE_TIMEOUT_SECONDS:-30}
write_seconds=${GALVANIZE_LIVE_WRITE_SECONDS:-180}
settle_seconds=${GALVANIZE_LIVE_SETTLE_SECONDS:-30}
encryption_write_seconds=${GALVANIZE_LIVE_ENCRYPTION_WRITE_SECONDS:-60}
encryption_settle_seconds=${GALVANIZE_LIVE_ENCRYPTION_SETTLE_SECONDS:-10}
highlow_write_seconds=${GALVANIZE_LIVE_HIGHLOW_WRITE_SECONDS:-300}
highlow_settle_seconds=${GALVANIZE_LIVE_HIGHLOW_SETTLE_SECONDS:-15}
partition_write_seconds=${GALVANIZE_LIVE_PARTITION_WRITE_SECONDS:-30}
partition_settle_seconds=${GALVANIZE_LIVE_PARTITION_SETTLE_SECONDS:-15}
started_at=$SECONDS

[[ -x "$binary" ]] || { echo "build first: cargo build -p corrosion" >&2; exit 2; }
command -v psql >/dev/null || { echo "psql is required for live tests" >&2; exit 2; }

cleanup_pids=()
active_runtime=''
cleanup() {
  local status=$?
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
  if (( status != 0 )) && [[ -n $active_runtime && -d $active_runtime ]]; then
    local failed="$root/tests-live/failures/$(date -u +%Y%m%dT%H%M%SZ)-$(basename "$active_runtime")"
    mkdir -p "$(dirname "$failed")"
    mv "$active_runtime" "$failed"
    echo "RESULT: FAIL scenario=$(basename "$active_runtime") elapsed=$((SECONDS - started_at))s" >&2
    echo "failure artifacts retained at $failed" >&2
  fi
  return "$status"
}
trap cleanup EXIT

write_node() {
  local node=$1 gossip=$2 pg=$3 bootstrap=$4 key=$5 allow=$6
  local label=${node##*/node-}
  mkdir -p "$node/schema" "$node/logs"
  echo "  START  node ${label^^}  pg=$pg gossip=$gossip allow=$allow"
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
[api]
addr = "127.0.0.1:0"
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
[log]
format = "json"
EOF
  GALVANIZE_DB_KEY="$key" \
    RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info,corro_agent::agent::handlers=debug}" \
    "$binary" --config "$node/config.toml" agent >"$node/logs/agent.log" 2>&1 &
  cleanup_pids+=("$!")
}

start_node_instance() {
  local node=$1 gossip=$2 pg=$3 bootstrap=$4 key=$5 allow=$6
  local label=${node##*/node-}
  mkdir -p "$node/schema" "$node/logs"
  echo "  START  node ${label^^}  pg=$pg gossip=$gossip allow=$allow"
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
[api]
addr = "127.0.0.1:0"
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
[log]
format = "json"
EOF
  GALVANIZE_DB_KEY="$key" \
    RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info,corro_agent::agent::handlers=debug}" \
    "$binary" --config "$node/config.toml" agent >"$node/logs/agent.log" 2>&1 &
  cleanup_pids+=("$!")
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

stop_last_node() {
  local i=$((${#cleanup_pids[@]} - 1))
  kill "${cleanup_pids[$i]}" 2>/dev/null || true
  wait "${cleanup_pids[$i]}" 2>/dev/null || true
  kill -9 "${cleanup_pids[$i]}" 2>/dev/null || true
  unset 'cleanup_pids[$i]'
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
}

finish() {
  local runtime=$1 status=$2
  if (( status == 0 )); then
    local nodes
    nodes=$(find "$runtime" -mindepth 1 -maxdepth 1 -type d -name 'node-*' | wc -l)
    rm -rf "$runtime"
    active_runtime=''
    echo "RESULT: PASS scenario=$(basename "$runtime") nodes=$nodes elapsed=$((SECONDS - started_at))s"
  else
    local failed="$root/tests-live/failures/$(date -u +%Y%m%dT%H%M%SZ)-$(basename "$runtime")"
    mkdir -p "$(dirname "$failed")"; mv "$runtime" "$failed"
    echo "failure artifacts retained at $failed" >&2
    active_runtime=''
  fi
  return "$status"
}

encryption() {
  local runtime="$runtime_root/encryption" key='galv-live-encryption-key'
  rm -rf "$runtime"; mkdir -p "$runtime"; active_runtime=$runtime
  echo "== encryption: encrypted A and unencrypted B replication comparison =="
  echo "  PLAN      A/B each write once/second for ${encryption_write_seconds}s; then settle ${encryption_settle_seconds}s"
  write_node "$runtime/node-a" 127.0.0.11:48011 127.0.0.1:54911 '["127.0.0.12:48012"]' "$key" '["*"]'
  write_node "$runtime/node-b" 127.0.0.12:48012 127.0.0.1:54912 '["127.0.0.11:48011"]' '' '["*"]'
  wait_node 54911; wait_node 54912
  local second tick_started expected_rows count_a count_b hash_a hash_b
  for ((second = 1; second <= encryption_write_seconds; second++)); do
    tick_started=$SECONDS
    psql "postgresql://postgres@127.0.0.1:54911/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((second * 10 + 1)), 'from-a-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54912/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((second * 10 + 2)), 'from-b-$second');" >/dev/null
    if (( second == 1 || second % 10 == 0 || second == encryption_write_seconds )); then
      echo "  WRITE  second=$second/${encryption_write_seconds} commits=A:$second B:$second"
    fi
    local remaining=$((1 - (SECONDS - tick_started)))
    (( remaining > 0 )) && sleep "$remaining"
  done
  echo "  PAUSE  writes stopped; waiting ${encryption_settle_seconds}s for mesh convergence"
  sleep "$encryption_settle_seconds"
  expected_rows=$((encryption_write_seconds * 2))
  count_a=$(psql "postgresql://postgres@127.0.0.1:54911/postgres" -Atqc 'SELECT count(*) FROM live_records')
  count_b=$(psql "postgresql://postgres@127.0.0.1:54912/postgres" -Atqc 'SELECT count(*) FROM live_records')
  hash_a=$(psql "postgresql://postgres@127.0.0.1:54911/postgres" -Atqc "SELECT id || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  hash_b=$(psql "postgresql://postgres@127.0.0.1:54912/postgres" -Atqc "SELECT id || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  echo "  COMPARE node=A encrypted rows=$count_a sha256=$hash_a"
  echo "  COMPARE node=B plaintext rows=$count_b sha256=$hash_b"
  [[ $count_a == "$expected_rows" && $count_b == "$expected_rows" && $hash_a == "$hash_b" ]] || { echo "replicated database mismatch; expected $expected_rows rows" >&2; return 1; }
  echo "  CHECK  A and B have identical replicated application data"
  stop_last_node; stop_last_node
  ! grep -a -q 'from-a-' "$runtime/node-a/corrosion.db"
  ! grep -a -q 'from-b-' "$runtime/node-a/corrosion.db"
  [[ ! -f "$runtime/node-a/corrosion.db-wal" ]] || ! grep -a -q 'from-' "$runtime/node-a/corrosion.db-wal"
  grep -a -q 'from-b-' "$runtime/node-b/corrosion.db" || grep -a -q 'from-b-' "$runtime/node-b/corrosion.db-wal"
  echo "  CHECK  direct encrypted-node database/WAL inspection found no application plaintext"
  echo "  CHECK  unencrypted node B retains application plaintext on disk"
  GALVANIZE_DB_KEY=wrong "$binary" --config "$runtime/node-a/config.toml" agent >"$runtime/node-a/logs/wrong-key.log" 2>&1 &
  local wrong_pid=$!; sleep 1
  ! psql "postgresql://postgres@127.0.0.1:54911/postgres" -Atqc 'SELECT 1' >/dev/null 2>&1
  echo "  CHECK  wrong key cannot serve PostgreSQL"
  kill "$wrong_pid" 2>/dev/null || true; wait "$wrong_pid" 2>/dev/null || true
  write_node "$runtime/node-a" 127.0.0.11:48011 127.0.0.1:54911 '["127.0.0.12:48012"]' "$key" '["*"]'
  wait_node 54911
  [[ $(psql "postgresql://postgres@127.0.0.1:54911/postgres" -Atqc 'SELECT count(*) FROM live_records') == "$expected_rows" ]]
  finish "$runtime" 0
}

rekey() {
  local runtime="$runtime_root/rekey" old='galv-live-old-key' new='galv-live-new-key'
  rm -rf "$runtime"; mkdir -p "$runtime"; active_runtime=$runtime
  echo "== rekey: PostgreSQL write, offline rekey, restart with replacement key =="
  write_node "$runtime/node-a" 127.0.0.12:48012 127.0.0.1:54912 '[]' "$old" '["*"]'
  wait_node 54912
  psql "postgresql://postgres@127.0.0.1:54912/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (1, 'rekey-survives');" >/dev/null
  stop_last_node
  GALV_LIVE_OLD_KEY="$old" GALV_LIVE_NEW_KEY="$new" "$binary" --config "$runtime/node-a/config.toml" rekey --path "$runtime/node-a/corrosion.db" --current-key-env GALV_LIVE_OLD_KEY --new-key-env GALV_LIVE_NEW_KEY
  echo "  CHECK  offline rekey completed without exposing keys as arguments"
  write_node "$runtime/node-a" 127.0.0.12:48012 127.0.0.1:54912 '[]' "$new" '["*"]'
  wait_node 54912
  [[ $(psql "postgresql://postgres@127.0.0.1:54912/postgres" -Atqc 'SELECT value FROM live_records WHERE id = 1') == rekey-survives ]]
  finish "$runtime" 0
}

allow_nodes() {
  local runtime="$runtime_root/allow-nodes"
  rm -rf "$runtime"; mkdir -p "$runtime"; active_runtime=$runtime
  echo "== allow-nodes: three-writer convergence with B rejecting direct C traffic =="
  echo "  TOPOLOGY  A allow=*; C allow=*; B allow=127.0.0.21 (A only)"
  echo "  PLAN      each node writes once/second for ${write_seconds}s; then settle ${settle_seconds}s"
  write_node "$runtime/node-a" 127.0.0.21:48021 127.0.0.1:54921 '["127.0.0.22:48022", "127.0.0.23:48023"]' key-a '["*"]'
  write_node "$runtime/node-b" 127.0.0.22:48022 127.0.0.1:54922 '["127.0.0.21:48021"]' key-b '["127.0.0.21"]'
  # C explicitly bootstraps B as well as A so the test proves that B rejects
  # a direct C connection before C's update reaches B through A.
  write_node "$runtime/node-c" 127.0.0.23:48023 127.0.0.1:54923 '["127.0.0.21:48021", "127.0.0.22:48022"]' key-c '["*"]'
  wait_node 54921; wait_node 54922; wait_node 54923
  echo "  WRITE  starting A/B/C PostgreSQL writers"
  local second tick_started expected_rows deadline
  for ((second = 1; second <= write_seconds; second++)); do
    tick_started=$SECONDS
    psql "postgresql://postgres@127.0.0.1:54921/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((second * 10 + 1)), 'from-a-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54922/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((second * 10 + 2)), 'from-b-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54923/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((second * 10 + 3)), 'from-c-$second');" >/dev/null
    if (( second == 1 || second % 10 == 0 || second == write_seconds )); then
      echo "  WRITE  second=$second/${write_seconds} commits=A:$second B:$second C:$second"
    fi
    local remaining=$((1 - (SECONDS - tick_started)))
    (( remaining > 0 )) && sleep "$remaining"
  done
  expected_rows=$((write_seconds * 3))
  echo "  PAUSE  writes stopped; waiting ${settle_seconds}s for mesh convergence"
  for ((second = settle_seconds; second > 0; second--)); do
    if (( second == settle_seconds || second % 5 == 0 || second == 1 )); then
      echo "  PAUSE  ${second}s remaining"
    fi
    sleep 1
  done
  deadline=$((SECONDS + timeout_seconds))
  until grep -Eq 'remote_addr.*127\.0\.0\.23|127\.0\.0\.23.*refusing incoming connection' "$runtime/node-b/logs/agent.log" && grep -q 'refusing incoming connection: remote address not in gossip allow-list' "$runtime/node-b/logs/agent.log"; do
    (( SECONDS < deadline )) || { echo 'node B did not log rejection of direct node C traffic' >&2; return 1; }; sleep .2
  done
  echo "  CHECK  node B rejected direct traffic from node C (allow-list)"
  local count_a count_b count_c hash_a hash_b hash_c
  count_a=$(psql "postgresql://postgres@127.0.0.1:54921/postgres" -Atqc 'SELECT count(*) FROM live_records')
  count_b=$(psql "postgresql://postgres@127.0.0.1:54922/postgres" -Atqc 'SELECT count(*) FROM live_records')
  count_c=$(psql "postgresql://postgres@127.0.0.1:54923/postgres" -Atqc 'SELECT count(*) FROM live_records')
  hash_a=$(psql "postgresql://postgres@127.0.0.1:54921/postgres" -Atqc "SELECT id || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  hash_b=$(psql "postgresql://postgres@127.0.0.1:54922/postgres" -Atqc "SELECT id || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  hash_c=$(psql "postgresql://postgres@127.0.0.1:54923/postgres" -Atqc "SELECT id || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  echo "  COMPARE node=A rows=$count_a sha256=$hash_a"
  echo "  COMPARE node=B rows=$count_b sha256=$hash_b"
  echo "  COMPARE node=C rows=$count_c sha256=$hash_c"
  [[ $count_a == "$expected_rows" && $count_b == "$expected_rows" && $count_c == "$expected_rows" ]] || { echo "row-count mismatch; expected $expected_rows" >&2; return 1; }
  [[ $hash_a == "$hash_b" && $hash_b == "$hash_c" ]] || { echo 'database contents differ after convergence window' >&2; return 1; }
  echo "  CHECK  all nodes have identical $expected_rows-row live_records data"
  finish "$runtime" 0
}

highlow() {
  local runtime="$runtime_root/highlow"
  rm -rf "$runtime"; mkdir -p "$runtime/staging"; active_runtime=$runtime
  echo "== highlow: Low (3 nodes mesh) -> Air-Gap Staging -> High (2 nodes mesh) =="
  echo "  TOPOLOGY  Low Domain: 3 nodes (Low-1 is Air-Gap Exporter)"
  echo "            High Domain: 2 nodes (High-1 is Air-Gap Receiver)"
  echo "            Air-Gap: Directory staging with XChaCha20-Poly1305 / RSA-OAEP / Ed25519"
  echo "  PLAN      Continuous concurrent writes for ${highlow_write_seconds}s; then settle ${highlow_settle_seconds}s"

  # 1. Generate RSA keypair for High node
  openssl genpkey -algorithm RSA -out "$runtime/high_rsa_priv.pem" -pkeyopt rsa_keygen_bits:2048 2>/dev/null
  openssl rsa -in "$runtime/high_rsa_priv.pem" -pubout -out "$runtime/high_rsa_pub.pem" 2>/dev/null

  # 2. Generate Ed25519 signing key for Low node
  local keys_out
  keys_out=$(python3 -c "
from cryptography.hazmat.primitives.asymmetric import ed25519
priv = ed25519.Ed25519PrivateKey.generate()
print(priv.private_bytes_raw().hex())
print(priv.public_key().public_bytes_raw().hex())
")
  local low_priv_hex low_pub_hex
  low_priv_hex=$(echo "$keys_out" | head -n1)
  low_pub_hex=$(echo "$keys_out" | tail -n1)

  local staging_endpoint="$runtime/staging"
  local rsa_pub_content rsa_priv_content
  rsa_pub_content=$(cat "$runtime/high_rsa_pub.pem")
  rsa_priv_content=$(cat "$runtime/high_rsa_priv.pem")

  local common_schema="CREATE TABLE IF NOT EXISTS live_records (
  id INTEGER PRIMARY KEY NOT NULL,
  source TEXT NOT NULL DEFAULT '',
  value TEXT NOT NULL DEFAULT ''
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS high_records (
  id INTEGER PRIMARY KEY NOT NULL,
  secret_data TEXT NOT NULL DEFAULT ''
) WITHOUT ROWID;"

  # Write Low Node 1 (Airgap Exporter)
  local node_low_1="$runtime/node-low-1"
  mkdir -p "$node_low_1/schema" "$node_low_1/logs"
  echo "$common_schema" >"$node_low_1/schema/live.sql"
  cat >"$node_low_1/config.toml" <<EOF
[db]
path = "$node_low_1/corrosion.db"
schema_paths = ["$node_low_1/schema"]
[api]
addr = "127.0.0.1:0"
[[api.pg]]
addr = "127.0.0.1:54931"
[gossip]
addr = "127.0.0.1:48031"
client_addr_v4 = "127.0.0.1:0"
bootstrap = ["127.0.0.1:48032", "127.0.0.1:48033"]
plaintext = true
allow-list = ["*"]
[admin]
path = "$node_low_1/admin.sock"
[log]
format = "json"
[highlow]
enabled = true
[highlow.transport]
kind = "directory"
endpoint = "$staging_endpoint"
[highlow.low]
stream-id = "stream-live-1"
network-name = "net-live"
upload-interval-seconds = 1
recipient-key-id = "high-rsa-key"
recipient-rsa-public-key-env = "GALV_TEST_RSA_PUB"
sender-signing-key-env = "GALV_TEST_ED25519_KEY"
EOF

  # Write Low Node 2 (Mesh Peer)
  local node_low_2="$runtime/node-low-2"
  mkdir -p "$node_low_2/schema" "$node_low_2/logs"
  echo "$common_schema" >"$node_low_2/schema/live.sql"
  cat >"$node_low_2/config.toml" <<EOF
[db]
path = "$node_low_2/corrosion.db"
schema_paths = ["$node_low_2/schema"]
[api]
addr = "127.0.0.1:0"
[[api.pg]]
addr = "127.0.0.1:54932"
[gossip]
addr = "127.0.0.1:48032"
client_addr_v4 = "127.0.0.1:0"
bootstrap = ["127.0.0.1:48031", "127.0.0.1:48033"]
plaintext = true
allow-list = ["*"]
[admin]
path = "$node_low_2/admin.sock"
[log]
format = "json"
EOF

  # Write Low Node 3 (Mesh Peer)
  local node_low_3="$runtime/node-low-3"
  mkdir -p "$node_low_3/schema" "$node_low_3/logs"
  echo "$common_schema" >"$node_low_3/schema/live.sql"
  cat >"$node_low_3/config.toml" <<EOF
[db]
path = "$node_low_3/corrosion.db"
schema_paths = ["$node_low_3/schema"]
[api]
addr = "127.0.0.1:0"
[[api.pg]]
addr = "127.0.0.1:54933"
[gossip]
addr = "127.0.0.1:48033"
client_addr_v4 = "127.0.0.1:0"
bootstrap = ["127.0.0.1:48031", "127.0.0.1:48032"]
plaintext = true
allow-list = ["*"]
[admin]
path = "$node_low_3/admin.sock"
[log]
format = "json"
EOF

  # Write High Node 1 (Airgap Receiver)
  local node_high_1="$runtime/node-high-1"
  mkdir -p "$node_high_1/schema" "$node_high_1/logs"
  echo "$common_schema" >"$node_high_1/schema/live.sql"
  cat >"$node_high_1/config.toml" <<EOF
[db]
path = "$node_high_1/corrosion.db"
schema_paths = ["$node_high_1/schema"]
[api]
addr = "127.0.0.1:0"
[[api.pg]]
addr = "127.0.0.1:54941"
[gossip]
addr = "127.0.0.1:48041"
client_addr_v4 = "127.0.0.1:0"
bootstrap = ["127.0.0.1:48042"]
plaintext = true
allow-list = ["*"]
[admin]
path = "$node_high_1/admin.sock"
[log]
format = "json"
[highlow]
enabled = true
[highlow.transport]
kind = "directory"
endpoint = "$staging_endpoint"
[highlow.high]
accepted-streams = ["stream-live-1"]
download-interval-seconds = 1
recipient-key-id = "high-rsa-key"
recipient-rsa-private-key-env = "GALV_TEST_RSA_PRIV"
permitted-sender-key-envs = ["GALV_TEST_PERMITTED_SENDER"]
EOF

  # Write High Node 2 (Mesh Peer)
  local node_high_2="$runtime/node-high-2"
  mkdir -p "$node_high_2/schema" "$node_high_2/logs"
  echo "$common_schema" >"$node_high_2/schema/live.sql"
  cat >"$node_high_2/config.toml" <<EOF
[db]
path = "$node_high_2/corrosion.db"
schema_paths = ["$node_high_2/schema"]
[api]
addr = "127.0.0.1:0"
[[api.pg]]
addr = "127.0.0.1:54942"
[gossip]
addr = "127.0.0.1:48042"
client_addr_v4 = "127.0.0.1:0"
bootstrap = ["127.0.0.1:48041"]
plaintext = true
allow-list = ["*"]
[admin]
path = "$node_high_2/admin.sock"
[log]
format = "json"
EOF

  # Start Low Nodes
  echo "  START  node LOW-1 (Exporter) pg=127.0.0.1:54931 gossip=127.0.0.1:48031"
  GALV_TEST_RSA_PUB="$rsa_pub_content" \
  GALV_TEST_ED25519_KEY="$low_priv_hex" \
  RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info,corro_agent::agent::highlow=debug}" \
    "$binary" --config "$node_low_1/config.toml" agent >"$node_low_1/logs/agent.log" 2>&1 &
  cleanup_pids+=("$!")

  echo "  START  node LOW-2 pg=127.0.0.1:54932 gossip=127.0.0.1:48032"
  RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info}" \
    "$binary" --config "$node_low_2/config.toml" agent >"$node_low_2/logs/agent.log" 2>&1 &
  cleanup_pids+=("$!")

  echo "  START  node LOW-3 pg=127.0.0.1:54933 gossip=127.0.0.1:48033"
  RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info}" \
    "$binary" --config "$node_low_3/config.toml" agent >"$node_low_3/logs/agent.log" 2>&1 &
  cleanup_pids+=("$!")

  # Start High Nodes
  echo "  START  node HIGH-1 (Receiver) pg=127.0.0.1:54941 gossip=127.0.0.1:48041"
  GALV_TEST_RSA_PRIV="$rsa_priv_content" \
  GALV_TEST_PERMITTED_SENDER="$low_pub_hex" \
  RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info,corro_agent::agent::highlow=debug}" \
    "$binary" --config "$node_high_1/config.toml" agent >"$node_high_1/logs/agent.log" 2>&1 &
  cleanup_pids+=("$!")

  echo "  START  node HIGH-2 pg=127.0.0.1:54942 gossip=127.0.0.1:48042"
  RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info}" \
    "$binary" --config "$node_high_2/config.toml" agent >"$node_high_2/logs/agent.log" 2>&1 &
  cleanup_pids+=("$!")

  wait_node 54931; wait_node 54932; wait_node 54933
  wait_node 54941; wait_node 54942

  echo "  WRITE  starting concurrent writers across Low (3 nodes) and High (2 nodes) for ${highlow_write_seconds}s"
  local second tick_started
  for ((second = 1; second <= highlow_write_seconds; second++)); do
    tick_started=$SECONDS
    # Low-side writes (IDs: second * 10 + {1, 2, 3})
    psql "postgresql://postgres@127.0.0.1:54931/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((second * 10 + 1)), 'low-1', 'val-low-1-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54932/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((second * 10 + 2)), 'low-2', 'val-low-2-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54933/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((second * 10 + 3)), 'low-3', 'val-low-3-$second');" >/dev/null

    # High-side writes (live_records IDs: 1000000 + second * 10 + {1, 2}; high_records IDs: second * 10 + {1, 2})
    psql "postgresql://postgres@127.0.0.1:54941/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((1000000 + second * 10 + 1)), 'high-1', 'val-high-1-$second'); INSERT INTO high_records VALUES ($((second * 10 + 1)), 'secret-high-1-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54942/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((1000000 + second * 10 + 2)), 'high-2', 'val-high-2-$second'); INSERT INTO high_records VALUES ($((second * 10 + 2)), 'secret-high-2-$second');" >/dev/null

    if (( second == 1 || second % 10 == 0 || second == highlow_write_seconds )); then
      echo "  WRITE  second=$second/${highlow_write_seconds} writes: Low(1,2,3)=$((second * 3)) High(1,2)=$((second * 2))"
    fi
    local remaining=$((1 - (SECONDS - tick_started)))
    (( remaining > 0 )) && sleep "$remaining"
  done

  echo "  PAUSE  writes stopped; waiting ${highlow_settle_seconds}s for mesh & air-gap convergence"
  for ((second = highlow_settle_seconds; second > 0; second--)); do
    if (( second == highlow_settle_seconds || second % 5 == 0 || second == 1 )); then
      echo "  PAUSE  ${second}s remaining"
    fi
    sleep 1
  done

  # Validation:
  local expected_low_live=$((highlow_write_seconds * 3))
  local expected_high_records=$((highlow_write_seconds * 2))
  local expected_high_live=$((expected_low_live + expected_high_records))

  # 1. Check Low cluster
  local low1_live low2_live low3_live low1_high low2_high low3_high
  low1_live=$(psql "postgresql://postgres@127.0.0.1:54931/postgres" -Atqc 'SELECT count(*) FROM live_records')
  low2_live=$(psql "postgresql://postgres@127.0.0.1:54932/postgres" -Atqc 'SELECT count(*) FROM live_records')
  low3_live=$(psql "postgresql://postgres@127.0.0.1:54933/postgres" -Atqc 'SELECT count(*) FROM live_records')
  low1_high=$(psql "postgresql://postgres@127.0.0.1:54931/postgres" -Atqc 'SELECT count(*) FROM high_records')
  low2_high=$(psql "postgresql://postgres@127.0.0.1:54932/postgres" -Atqc 'SELECT count(*) FROM high_records')
  low3_high=$(psql "postgresql://postgres@127.0.0.1:54933/postgres" -Atqc 'SELECT count(*) FROM high_records')

  echo "  COMPARE Low cluster live_records counts: Low1=$low1_live Low2=$low2_live Low3=$low3_live (expected $expected_low_live)"
  echo "  COMPARE Low cluster high_records counts: Low1=$low1_high Low2=$low2_high Low3=$low3_high (expected 0)"

  [[ $low1_live == "$expected_low_live" && $low2_live == "$expected_low_live" && $low3_live == "$expected_low_live" ]] || { echo "Low cluster live_records count mismatch" >&2; return 1; }
  [[ $low1_high == "0" && $low2_high == "0" && $low3_high == "0" ]] || { echo "Low cluster has leaked High records" >&2; return 1; }

  local low1_hash low2_hash low3_hash
  low1_hash=$(psql "postgresql://postgres@127.0.0.1:54931/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  low2_hash=$(psql "postgresql://postgres@127.0.0.1:54932/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  low3_hash=$(psql "postgresql://postgres@127.0.0.1:54933/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')

  [[ $low1_hash == "$low2_hash" && $low2_hash == "$low3_hash" ]] || { echo "Low cluster data hash mismatch" >&2; return 1; }
  echo "  CHECK  Low cluster (3 nodes) converged with identical data (sha256=$low1_hash)"

  # 2. Check High cluster
  local high1_live high2_live high1_high high2_high
  high1_live=$(psql "postgresql://postgres@127.0.0.1:54941/postgres" -Atqc 'SELECT count(*) FROM live_records')
  high2_live=$(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc 'SELECT count(*) FROM live_records')
  high1_high=$(psql "postgresql://postgres@127.0.0.1:54941/postgres" -Atqc 'SELECT count(*) FROM high_records')
  high2_high=$(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc 'SELECT count(*) FROM high_records')

  echo "  COMPARE High cluster live_records counts: High1=$high1_live High2=$high2_live (expected $expected_high_live)"
  echo "  COMPARE High cluster high_records counts: High1=$high1_high High2=$high2_high (expected $expected_high_records)"

  [[ $high1_live == "$expected_high_live" && $high2_live == "$expected_high_live" ]] || { echo "High cluster live_records count mismatch" >&2; return 1; }
  [[ $high1_high == "$expected_high_records" && $high2_high == "$expected_high_records" ]] || { echo "High cluster high_records count mismatch" >&2; return 1; }

  local high1_live_hash high2_live_hash high1_sec_hash high2_sec_hash
  high1_live_hash=$(psql "postgresql://postgres@127.0.0.1:54941/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  high2_live_hash=$(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  high1_sec_hash=$(psql "postgresql://postgres@127.0.0.1:54941/postgres" -Atqc "SELECT id || ':' || secret_data FROM high_records ORDER BY id" | sha256sum | awk '{print $1}')
  high2_sec_hash=$(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc "SELECT id || ':' || secret_data FROM high_records ORDER BY id" | sha256sum | awk '{print $1}')

  [[ $high1_live_hash == "$high2_live_hash" ]] || { echo "High cluster live_records hash mismatch between High1 and High2" >&2; return 1; }
  [[ $high1_sec_hash == "$high2_sec_hash" ]] || { echo "High cluster high_records hash mismatch between High1 and High2" >&2; return 1; }

  echo "  CHECK  High cluster (2 nodes) converged with identical live_records (sha256=$high1_live_hash)"
  echo "  CHECK  High cluster high_records remained intact and uncorrupted (sha256=$high1_sec_hash)"

  # 3. Check Low vs High (cross-domain comparison for Low-originated values only)
  local high1_low_hash high2_low_hash
  high1_low_hash=$(psql "postgresql://postgres@127.0.0.1:54941/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records WHERE source LIKE 'low-%' ORDER BY id" | sha256sum | awk '{print $1}')
  high2_low_hash=$(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records WHERE source LIKE 'low-%' ORDER BY id" | sha256sum | awk '{print $1}')

  [[ $low1_hash == "$high1_low_hash" && $high1_low_hash == "$high2_low_hash" ]] || { echo "Low data on High does not match Low cluster data" >&2; return 1; }
  echo "  CHECK  Low data replicated across air-gap to High matches Low cluster data exactly (sha256=$low1_hash)"

  # 4. Test update and delete propagation from Low across Air-Gap while High data remains unaffected
  echo "  WRITE  updating id=11 (Low-1) and deleting id=12 (Low-2) on node LOW-1 and LOW-2"
  psql "postgresql://postgres@127.0.0.1:54931/postgres" -v ON_ERROR_STOP=1 -qc "UPDATE live_records SET value = 'val-low-1-updated' WHERE id = 11;"
  psql "postgresql://postgres@127.0.0.1:54932/postgres" -v ON_ERROR_STOP=1 -qc "DELETE FROM live_records WHERE id = 12;"

  local deadline=$((SECONDS + 20))
  until [[ $(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc "SELECT value FROM live_records WHERE id = 11" 2>/dev/null) == "val-low-1-updated" && $(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc "SELECT count(*) FROM live_records WHERE id = 12" 2>/dev/null) == "0" ]]; do
    (( SECONDS < deadline )) || { echo "High node did not receive update/delete in time" >&2; return 1; }
    sleep 0.5
  done

  # Verify High-side data is still 100% intact after the Low-side mutation
  local final_high_sec_count final_high1_val
  final_high_sec_count=$(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc 'SELECT count(*) FROM high_records')
  final_high1_val=$(psql "postgresql://postgres@127.0.0.1:54942/postgres" -Atqc 'SELECT value FROM live_records WHERE id = 1000011')
  [[ $final_high_sec_count == "$expected_high_records" ]] || { echo "High records count altered by Low mutation" >&2; return 1; }
  [[ $final_high1_val == "val-high-1-1" ]] || { echo "High live_record value overwritten by Low mutation" >&2; return 1; }

  echo "  CHECK  High cluster correctly applied Low update/delete without altering High-side data"

  stop_all_nodes
  finish "$runtime" 0
}

partition() {
  local runtime="$runtime_root/partition"
  rm -rf "$runtime"; mkdir -p "$runtime"; active_runtime=$runtime
  echo "== partition: 4-node cluster network partition & split-brain reconciliation =="
  echo "  TOPOLOGY  4 Nodes: A (127.0.0.51), B (127.0.0.52), C (127.0.0.53), D (127.0.0.54)"
  echo "  PLAN      Phase 1: Baseline mesh -> Phase 2: Partition {A,B} vs {C,D} (${partition_write_seconds}s) -> Phase 3: Heal & Reconcile"

  local key_a="galv-part-key-a" key_b="galv-part-key-b" key_c="galv-part-key-c" key_d="galv-part-key-d"
  local node_a="$runtime/node-a" node_b="$runtime/node-b" node_c="$runtime/node-c" node_d="$runtime/node-d"

  # Phase 1: Baseline Mesh
  echo "  PHASE 1: Starting all 4 nodes in full mesh"
  start_node_instance "$node_a" 127.0.0.51:48051 127.0.0.1:54951 '["127.0.0.52:48052", "127.0.0.53:48053", "127.0.0.54:48054"]' "$key_a" '["*"]'
  start_node_instance "$node_b" 127.0.0.52:48052 127.0.0.1:54952 '["127.0.0.51:48051", "127.0.0.53:48053", "127.0.0.54:48054"]' "$key_b" '["*"]'
  start_node_instance "$node_c" 127.0.0.53:48053 127.0.0.1:54953 '["127.0.0.51:48051", "127.0.0.52:48052", "127.0.0.54:48054"]' "$key_c" '["*"]'
  start_node_instance "$node_d" 127.0.0.54:48054 127.0.0.1:54954 '["127.0.0.51:48051", "127.0.0.52:48052", "127.0.0.53:48053"]' "$key_d" '["*"]'

  wait_node 54951; wait_node 54952; wait_node 54953; wait_node 54954

  echo "  WRITE  writing 5 baseline rows per node (20 total rows)"
  local i
  for ((i = 1; i <= 5; i++)); do
    psql "postgresql://postgres@127.0.0.1:54951/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((i * 10 + 1)), 'base-a', 'val-base-a-$i');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54952/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((i * 10 + 2)), 'base-b', 'val-base-b-$i');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54953/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((i * 10 + 3)), 'base-c', 'val-base-c-$i');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54954/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((i * 10 + 4)), 'base-d', 'val-base-d-$i');" >/dev/null
  done

  sleep 3
  local base_count_a base_count_b base_count_c base_count_d
  base_count_a=$(psql "postgresql://postgres@127.0.0.1:54951/postgres" -Atqc 'SELECT count(*) FROM live_records')
  base_count_b=$(psql "postgresql://postgres@127.0.0.1:54952/postgres" -Atqc 'SELECT count(*) FROM live_records')
  base_count_c=$(psql "postgresql://postgres@127.0.0.1:54953/postgres" -Atqc 'SELECT count(*) FROM live_records')
  base_count_d=$(psql "postgresql://postgres@127.0.0.1:54954/postgres" -Atqc 'SELECT count(*) FROM live_records')

  [[ $base_count_a == "20" && $base_count_b == "20" && $base_count_c == "20" && $base_count_d == "20" ]] || { echo "Baseline convergence failed; expected 20 rows on all nodes" >&2; return 1; }
  echo "  CHECK  Phase 1 baseline converged (20 rows on all 4 nodes)"

  # Phase 2: Inject Network Partition {A, B} vs {C, D}
  echo "  PHASE 2: Injecting partition: Partition 1 {A, B} isolated from Partition 2 {C, D}"
  stop_all_nodes

  start_node_instance "$node_a" 127.0.0.51:48051 127.0.0.1:54951 '["127.0.0.52:48052"]' "$key_a" '["127.0.0.51", "127.0.0.52"]'
  start_node_instance "$node_b" 127.0.0.52:48052 127.0.0.1:54952 '["127.0.0.51:48051"]' "$key_b" '["127.0.0.51", "127.0.0.52"]'
  start_node_instance "$node_c" 127.0.0.53:48053 127.0.0.1:54953 '["127.0.0.54:48054"]' "$key_c" '["127.0.0.53", "127.0.0.54"]'
  start_node_instance "$node_d" 127.0.0.54:48054 127.0.0.1:54954 '["127.0.0.53:48053"]' "$key_d" '["127.0.0.53", "127.0.0.54"]'

  wait_node 54951; wait_node 54952; wait_node 54953; wait_node 54954

  echo "  WRITE  writing concurrent partitioned records for ${partition_write_seconds}s"
  local second tick_started
  for ((second = 1; second <= partition_write_seconds; second++)); do
    tick_started=$SECONDS
    # Partition 1 writes (A & B)
    psql "postgresql://postgres@127.0.0.1:54951/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((1000 + second * 10 + 1)), 'part-1-a', 'val-1-a-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54952/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((1000 + second * 10 + 2)), 'part-1-b', 'val-1-b-$second');" >/dev/null

    # Partition 2 writes (C & D)
    psql "postgresql://postgres@127.0.0.1:54953/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((2000 + second * 10 + 1)), 'part-2-c', 'val-2-c-$second');" >/dev/null
    psql "postgresql://postgres@127.0.0.1:54954/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES ($((2000 + second * 10 + 2)), 'part-2-d', 'val-2-d-$second');" >/dev/null

    if (( second == 1 || second % 10 == 0 || second == partition_write_seconds )); then
      echo "  WRITE  second=$second/${partition_write_seconds} partitioned commits: P1(A,B)=$((second * 2)) P2(C,D)=$((second * 2))"
    fi
    local remaining=$((1 - (SECONDS - tick_started)))
    (( remaining > 0 )) && sleep "$remaining"
  done

  echo "  PAUSE  partition writes stopped; waiting 5s for intra-partition sync"
  sleep 5

  local p1_expected=$((20 + partition_write_seconds * 2))
  local p2_expected=$((20 + partition_write_seconds * 2))

  local count_p1_a count_p1_b count_p2_c count_p2_d
  count_p1_a=$(psql "postgresql://postgres@127.0.0.1:54951/postgres" -Atqc 'SELECT count(*) FROM live_records')
  count_p1_b=$(psql "postgresql://postgres@127.0.0.1:54952/postgres" -Atqc 'SELECT count(*) FROM live_records')
  count_p2_c=$(psql "postgresql://postgres@127.0.0.1:54953/postgres" -Atqc 'SELECT count(*) FROM live_records')
  count_p2_d=$(psql "postgresql://postgres@127.0.0.1:54954/postgres" -Atqc 'SELECT count(*) FROM live_records')

  local hash_p1_a hash_p1_b hash_p2_c hash_p2_d
  hash_p1_a=$(psql "postgresql://postgres@127.0.0.1:54951/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  hash_p1_b=$(psql "postgresql://postgres@127.0.0.1:54952/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  hash_p2_c=$(psql "postgresql://postgres@127.0.0.1:54953/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  hash_p2_d=$(psql "postgresql://postgres@127.0.0.1:54954/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')

  echo "  COMPARE Partition 1 rows: A=$count_p1_a B=$count_p1_b (expected $p1_expected, sha256=$hash_p1_a)"
  echo "  COMPARE Partition 2 rows: C=$count_p2_c D=$count_p2_d (expected $p2_expected, sha256=$hash_p2_c)"

  [[ $count_p1_a == "$p1_expected" && $count_p1_b == "$p1_expected" && $hash_p1_a == "$hash_p1_b" ]] || { echo "Partition 1 intra-sync mismatch" >&2; return 1; }
  [[ $count_p2_c == "$p2_expected" && $count_p2_d == "$p2_expected" && $hash_p2_c == "$hash_p2_d" ]] || { echo "Partition 2 intra-sync mismatch" >&2; return 1; }
  [[ $hash_p1_a != "$hash_p2_c" ]] || { echo "Partitions were not isolated" >&2; return 1; }

  echo "  CHECK  Partitions remained strictly isolated with independent intra-partition replication"

  # Phase 3: Heal Network Partition
  echo "  PHASE 3: Healing partition: Reconnecting all 4 nodes into full unified mesh"
  stop_all_nodes

  start_node_instance "$node_a" 127.0.0.51:48051 127.0.0.1:54951 '["127.0.0.52:48052", "127.0.0.53:48053", "127.0.0.54:48054"]' "$key_a" '["*"]'
  start_node_instance "$node_b" 127.0.0.52:48052 127.0.0.1:54952 '["127.0.0.51:48051", "127.0.0.53:48053", "127.0.0.54:48054"]' "$key_b" '["*"]'
  start_node_instance "$node_c" 127.0.0.53:48053 127.0.0.1:54953 '["127.0.0.51:48051", "127.0.0.52:48052", "127.0.0.54:48054"]' "$key_c" '["*"]'
  start_node_instance "$node_d" 127.0.0.54:48054 127.0.0.1:54954 '["127.0.0.51:48051", "127.0.0.52:48052", "127.0.0.53:48053"]' "$key_d" '["*"]'

  wait_node 54951; wait_node 54952; wait_node 54953; wait_node 54954

  # Phase 4: Settle & Reconcile
  echo "  PAUSE  waiting ${partition_settle_seconds}s for anti-entropy bi-stream sync across former partition boundary"
  for ((second = partition_settle_seconds; second > 0; second--)); do
    if (( second == partition_settle_seconds || second % 5 == 0 || second == 1 )); then
      echo "  PAUSE  ${second}s remaining"
    fi
    sleep 1
  done

  # Phase 5: Verification of Full Reconciliation
  local total_expected=$((20 + partition_write_seconds * 4))
  local healed_count_a healed_count_b healed_count_c healed_count_d
  healed_count_a=$(psql "postgresql://postgres@127.0.0.1:54951/postgres" -Atqc 'SELECT count(*) FROM live_records')
  healed_count_b=$(psql "postgresql://postgres@127.0.0.1:54952/postgres" -Atqc 'SELECT count(*) FROM live_records')
  healed_count_c=$(psql "postgresql://postgres@127.0.0.1:54953/postgres" -Atqc 'SELECT count(*) FROM live_records')
  healed_count_d=$(psql "postgresql://postgres@127.0.0.1:54954/postgres" -Atqc 'SELECT count(*) FROM live_records')

  local healed_hash_a healed_hash_b healed_hash_c healed_hash_d
  healed_hash_a=$(psql "postgresql://postgres@127.0.0.1:54951/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  healed_hash_b=$(psql "postgresql://postgres@127.0.0.1:54952/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  healed_hash_c=$(psql "postgresql://postgres@127.0.0.1:54953/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')
  healed_hash_d=$(psql "postgresql://postgres@127.0.0.1:54954/postgres" -Atqc "SELECT id || ':' || source || ':' || value FROM live_records ORDER BY id" | sha256sum | awk '{print $1}')

  echo "  COMPARE Reconciled counts: A=$healed_count_a B=$healed_count_b C=$healed_count_c D=$healed_count_d (expected $total_expected)"
  echo "  COMPARE Reconciled sha256 A=$healed_hash_a"
  echo "  COMPARE Reconciled sha256 B=$healed_hash_b"
  echo "  COMPARE Reconciled sha256 C=$healed_hash_c"
  echo "  COMPARE Reconciled sha256 D=$healed_hash_d"

  [[ $healed_count_a == "$total_expected" && $healed_count_b == "$total_expected" && $healed_count_c == "$total_expected" && $healed_count_d == "$total_expected" ]] || { echo "Healed row-count mismatch" >&2; return 1; }
  [[ $healed_hash_a == "$healed_hash_b" && $healed_hash_b == "$healed_hash_c" && $healed_hash_c == "$healed_hash_d" ]] || { echo "Healed database contents differ across nodes" >&2; return 1; }

  echo "  CHECK  All 4 nodes successfully reconciled all partitioned data with identical SHA-256 state"

  # Post-heal live writes to confirm normal ongoing operation
  echo "  WRITE  post-heal verification writes on all 4 nodes"
  psql "postgresql://postgres@127.0.0.1:54951/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (99901, 'post-a', 'val-post-a');" >/dev/null
  psql "postgresql://postgres@127.0.0.1:54952/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (99902, 'post-b', 'val-post-b');" >/dev/null
  psql "postgresql://postgres@127.0.0.1:54953/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (99903, 'post-c', 'val-post-c');" >/dev/null
  psql "postgresql://postgres@127.0.0.1:54954/postgres" -v ON_ERROR_STOP=1 -qc "INSERT INTO live_records VALUES (99904, 'post-d', 'val-post-d');" >/dev/null

  sleep 3
  local final_count_d
  final_count_d=$(psql "postgresql://postgres@127.0.0.1:54954/postgres" -Atqc 'SELECT count(*) FROM live_records')
  [[ $final_count_d == "$((total_expected + 4))" ]] || { echo "Post-heal live write propagation failed" >&2; return 1; }
  echo "  CHECK  Post-heal cluster operation verified ($((total_expected + 4)) total rows)"

  stop_all_nodes
  finish "$runtime" 0
}

case "$scenario" in
  encryption) encryption ;;
  rekey) rekey ;;
  allow-nodes) allow_nodes ;;
  highlow) highlow ;;
  partition) partition ;;
  all) encryption; rekey; allow_nodes; highlow; partition ;;
  *) echo "usage: $0 {encryption|rekey|allow-nodes|highlow|partition|all}" >&2; exit 2 ;;
esac

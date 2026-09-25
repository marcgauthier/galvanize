#!/usr/bin/env bash
set -euo pipefail
test_dir=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$test_dir/../.." && pwd)
runtime=${GALVANIZE_LIVE_RUNTIME:-"$test_dir/runtime"}
binary=${GALVANIZE_BIN:-"$root/target/debug/galvanize"}
base=${GALVANIZE_GO_CLIENT_BASE_PORT:-45430}
timeout=${GALVANIZE_LIVE_TIMEOUT_SECONDS:-30}
[[ -x "$binary" ]] || { echo "build first: cargo build -p corrosion --bin galvanize" >&2; exit 2; }
command -v openssl >/dev/null && command -v go >/dev/null && command -v curl >/dev/null
mkdir -p "$runtime/node-a/schema" "$runtime/node-a/logs" "$runtime/staging"
node="$runtime/node-a"
pid=''
cleanup() {
  local status=$?
  if [[ -n "$pid" ]]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; kill -9 "$pid" 2>/dev/null || true; fi
  if (( status != 0 )); then
    local failed="$root/tests-live/failures/$(date -u +%Y%m%dT%H%M%SZ)-go-client"
    mkdir -p "$(dirname "$failed")"
    mv "$runtime" "$failed"
    echo "Go client failure artifacts retained at $failed" >&2
  else
    rm -rf "$runtime"
    echo "Go client live scenario passed; temporary runtime data was removed."
  fi
  exit "$status"
}
trap cleanup EXIT
cat >"$node/schema/live.sql" <<'SQL'
CREATE TABLE IF NOT EXISTS notes (
  id INTEGER PRIMARY KEY NOT NULL,
  body TEXT NOT NULL DEFAULT ''
) WITHOUT ROWID;
SQL
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$runtime/ca.key" -out "$runtime/ca.pem" -subj "/CN=GALVANIZE Go client test CA" -days 2 >/dev/null 2>&1
for identity in server client; do
  openssl req -newkey rsa:2048 -nodes -keyout "$runtime/$identity.key" -out "$runtime/$identity.csr" -subj "/CN=127.0.0.1" >/dev/null 2>&1
  if [[ "$identity" == server ]]; then
    openssl x509 -req -in "$runtime/server.csr" -CA "$runtime/ca.pem" -CAkey "$runtime/ca.key" -CAcreateserial -out "$runtime/server.pem" -days 2 -extfile <(printf 'subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n') >/dev/null 2>&1
  else
    openssl x509 -req -in "$runtime/client.csr" -CA "$runtime/ca.pem" -CAkey "$runtime/ca.key" -CAcreateserial -out "$runtime/client.pem" -days 2 -extfile <(printf 'extendedKeyUsage=clientAuth\n') >/dev/null 2>&1
  fi
done
export GALV_TEST_DB_KEY=$(openssl rand -hex 32)
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$runtime/recipient.key" 2>/dev/null
export GALV_TEST_RSA_PUB=$(openssl pkey -in "$runtime/recipient.key" -pubout 2>/dev/null)
export GALV_TEST_ED25519_KEY=$(openssl rand -hex 32)
export GALV_ADMIN_SERVER_CERT=$(<"$runtime/server.pem")
export GALV_ADMIN_SERVER_KEY=$(<"$runtime/server.key")
export GALV_ADMIN_CLIENT_CA=$(<"$runtime/ca.pem")
export GALV_CONTROL_SERVER_CERT="$GALV_ADMIN_SERVER_CERT"
export GALV_CONTROL_SERVER_KEY="$GALV_ADMIN_SERVER_KEY"
export GALV_CONTROL_CLIENT_CA="$GALV_ADMIN_CLIENT_CA"
cat >"$node/config.toml" <<EOF
[db]
path = "$node/galvanize.db"
schema_paths = ["$node/schema"]
await-unlock = true
[api]
addr = "127.0.0.1:$((base + 100))"
[[api.pg]]
addr = "127.0.0.1:$base"
[gossip]
addr = "127.0.0.1:$((base + 200))"
client_addr_v4 = "127.0.0.1:0"
bootstrap = []
plaintext = true
allow-list = ["*"]
[admin]
uds-path = "$node/admin.sock"
[admin.control-api]
addr = "127.0.0.1:$((base + 300))"
server-cert-env = "GALV_ADMIN_SERVER_CERT"
server-key-env = "GALV_ADMIN_SERVER_KEY"
client-ca-cert-env = "GALV_ADMIN_CLIENT_CA"
[files]
accept-uploads = true
accept-from-peers = true
[highlow]
enabled = true
[highlow.transport]
kind = "directory"
endpoint = "$runtime/staging"
[highlow.low]
stream-id = "go-client-test"
network-name = "go-client-test"
upload-interval-seconds = 3600
recipient-key-id = "go-client-test-key"
recipient-rsa-public-key-env = "GALV_TEST_RSA_PUB"
sender-signing-key-env = "GALV_TEST_ED25519_KEY"
[highlow.control-api]
addr = "127.0.0.1:$((base + 301))"
server-cert-env = "GALV_CONTROL_SERVER_CERT"
server-key-env = "GALV_CONTROL_SERVER_KEY"
client-ca-cert-env = "GALV_CONTROL_CLIENT_CA"
EOF
RUST_LOG="${GALVANIZE_LIVE_RUST_LOG:-info}" "$binary" --config "$node/config.toml" agent >"$node/logs/agent.log" 2>&1 &
pid=$!
deadline=$((SECONDS + timeout))
until curl -fsS "http://127.0.0.1:$((base + 100))/v1/health" >/dev/null 2>&1; do
  (( SECONDS < deadline )) || { echo "public API did not become ready" >&2; exit 1; }
  sleep 0.2
done
export GALVANIZE_TEST_API_URL="http://127.0.0.1:$((base + 100))"
export GALVANIZE_TEST_ADMIN_URL="https://127.0.0.1:$((base + 300))"
export GALVANIZE_TEST_HIGHLOW_URL="https://127.0.0.1:$((base + 301))"
export GALVANIZE_TEST_PG_DSN="postgres://postgres@127.0.0.1:$base/postgres?sslmode=disable"
export GALVANIZE_TEST_TLS_CA="$runtime/ca.pem"
export GALVANIZE_TEST_TLS_CERT="$runtime/client.pem"
export GALVANIZE_TEST_TLS_KEY="$runtime/client.key"
(cd "$root/clients/go" && GOCACHE="${GOCACHE:-/tmp/galvanize-go-cache}" GOTMPDIR="${GOTMPDIR:-/tmp}" go test -count=1 -run '^TestLiveNodeSurface$' -v ./...)

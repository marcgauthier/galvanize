#!/usr/bin/env bash
set -euo pipefail
test_dir=$(cd "$(dirname "$0")" && pwd)
export GALVANIZE_LIVE_RUNTIME="${GALVANIZE_LIVE_RUNTIME:-$test_dir/runtime}"
exec "$test_dir/../run.sh" files-soak

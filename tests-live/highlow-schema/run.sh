#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

exec "$(dirname "$0")/../run.sh" highlow-schema

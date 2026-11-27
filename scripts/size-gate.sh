#!/usr/bin/env bash
# Fail if a board's app image exceeds its budget in budget.toml.
# Usage: scripts/size-gate.sh <board> <app-image.bin>
set -euo pipefail
board=$1 image=$2
budget_file=$(dirname "$0")/../budget.toml

max=$(awk -v section="[$board]" '
  /^\[/ { in_section = ($1 == section) }
  in_section && $1 == "app_image_max_bytes" { print $3; exit }
' "$budget_file")
if [ -z "$max" ]; then
  echo "size-gate: no app_image_max_bytes for [$board] in budget.toml" >&2
  exit 2
fi

size=$(stat -c %s "$image")
mib() { awk -v b="$1" 'BEGIN { printf "%.2f MiB", b / 1048576 }'; }
if [ "$size" -gt "$max" ]; then
  echo "size-gate: $board app image $(mib "$size") ($size bytes) exceeds budget $(mib "$max") ($max bytes)" >&2
  exit 1
fi
echo "size-gate: $board app image $(mib "$size") ($size bytes) within budget $(mib "$max") ($max bytes)"

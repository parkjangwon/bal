#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT_DIR"

cargo build --quiet

pass=0
fail=0

run_case() {
  local name="$1"
  if "$ROOT_DIR/scripts/e2e/$name"; then
    echo "[PASS] $name"
    pass=$((pass+1))
  else
    echo "[FAIL] $name"
    fail=$((fail+1))
  fi
}

run_case test_release_gate.sh

echo "Gate summary: pass=$pass fail=$fail"
[[ "$fail" -eq 0 ]]

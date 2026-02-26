#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT_DIR/scripts/e2e/lib.sh"
require_tools

TMP_DIR="$(mktemp -d)"
HOME_DIR="$TMP_DIR/home"
mkdir -p "$HOME_DIR"
trap 'jobs -pr | xargs -r kill 2>/dev/null || true; rm -rf "$TMP_DIR"' EXIT

PORT_LB="$(rand_port)"
PORT_B1="$(rand_port)"
PORT_B2="$(rand_port)"

BACKEND1_PID="$(start_backend "$PORT_B1" "b1" "$TMP_DIR/backend1.log")"
BACKEND2_PID="$(start_backend "$PORT_B2" "b2" "$TMP_DIR/backend2.log")"

cat > "$TMP_DIR/config.yaml" <<YAML
port: $PORT_LB
bind_address: "127.0.0.1"
method: "round_robin"
backends:
  - host: "127.0.0.1"
    port: $PORT_B1
  - host: "127.0.0.1"
    port: $PORT_B2
YAML

# check --json contract
HOME="$HOME_DIR" "$BAL_BIN" check --config "$TMP_DIR/config.yaml" --json > "$TMP_DIR/check.json"
python3 - "$TMP_DIR/check.json" <<'PY'
import json,sys
obj=json.load(open(sys.argv[1]))
required=["config_path","errors","warnings","backend_count"]
missing=[k for k in required if k not in obj]
assert not missing, f"missing keys: {missing}"
assert isinstance(obj["errors"], list)
assert isinstance(obj["warnings"], list)
PY

# doctor --json contract
HOME="$HOME_DIR" "$BAL_BIN" doctor --config "$TMP_DIR/config.yaml" --json > "$TMP_DIR/doctor.json"
python3 - "$TMP_DIR/doctor.json" <<'PY'
import json,sys
obj=json.load(open(sys.argv[1]))
assert "checks" in obj and isinstance(obj["checks"], list)
assert "protection_mode" in obj and isinstance(obj["protection_mode"], dict)
for c in obj["checks"]:
    for k in ["name","level","summary","hint"]:
        assert k in c, f"doctor check missing {k}"
PY

# start daemon and verify status --json contract
HOME="$HOME_DIR" "$BAL_BIN" start --config "$TMP_DIR/config.yaml" -d >/dev/null 2>"$TMP_DIR/start.stderr" &
BAL_DAEMON_CLI_PID=$!
wait_for_pid_file "$HOME_DIR"
HOME="$HOME_DIR" "$BAL_BIN" status --config "$TMP_DIR/config.yaml" --json > "$TMP_DIR/status.json"
python3 - "$TMP_DIR/status.json" <<'PY'
import json,sys
obj=json.load(open(sys.argv[1]))
required=["running","pid","config_path","bind_address","port","method","backend_total","backend_reachable","backends","active_connections","last_check_time","protection_mode"]
missing=[k for k in required if k not in obj]
assert not missing, f"missing status keys: {missing}"
assert obj["running"] is True
assert isinstance(obj["backends"], list)
PY

# ndjson log validation: ignore non-JSON lines and validate JSON lines only
LOG_FILE="$TMP_DIR/start.stderr"
{
  echo "noise line"
  cat "$LOG_FILE"
  echo "not-json-trailer"
} > "$TMP_DIR/mixed.log"
python3 - "$TMP_DIR/mixed.log" <<'PY'
import json,sys
req={"timestamp","level","message","module","event","fields"}
count=0
for raw in open(sys.argv[1], encoding='utf-8'):
    line=raw.strip()
    if not line:
        continue
    try:
        obj=json.loads(line)
    except json.JSONDecodeError:
        continue
    count += 1
    missing=req-set(obj.keys())
    assert not missing, f"missing ndjson keys: {sorted(missing)}"
assert count>0, "no JSON lines validated"
PY

# deterministic stop checks: first stop success, then expected-not-running failure
HOME="$HOME_DIR" "$BAL_BIN" stop >/dev/null
wait_until_not_running "$HOME_DIR"
if HOME="$HOME_DIR" "$BAL_BIN" stop >"$TMP_DIR/stop2.out" 2>"$TMP_DIR/stop2.err"; then
  echo "second stop unexpectedly succeeded" >&2
  exit 1
fi
if ! grep -Eq "not running|Cannot find running bal process" "$TMP_DIR/stop2.err"; then
  echo "second stop did not return expected not-running message" >&2
  cat "$TMP_DIR/stop2.err" >&2
  exit 1
fi

# graceful when not running should fail deterministically
if HOME="$HOME_DIR" "$BAL_BIN" graceful >"$TMP_DIR/graceful.out" 2>"$TMP_DIR/graceful.err"; then
  echo "graceful unexpectedly succeeded while daemon not running" >&2
  exit 1
fi

kill "$BACKEND1_PID" "$BACKEND2_PID" 2>/dev/null || true
echo "PASS: release gate integration scenarios"

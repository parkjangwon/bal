#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BAL_BIN="${BAL_BIN:-$ROOT_DIR/target/debug/bal}"

require_tools() {
  command -v python3 >/dev/null
}

rand_port() {
  python3 - <<'PY'
import socket
s=socket.socket()
s.bind(("127.0.0.1",0))
print(s.getsockname()[1])
s.close()
PY
}

start_backend() {
  local port="$1"
  local label="$2"
  local logfile="$3"
  python3 -u - "$port" "$label" >"$logfile" 2>&1 <<'PY' &
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
port = int(sys.argv[1]); label = sys.argv[2]
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        body = json.dumps({"backend": label, "path": self.path}).encode()
        self.send_response(200)
        self.send_header("Content-Type","application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, fmt, *args):
        return
HTTPServer(("127.0.0.1", port), H).serve_forever()
PY
  echo $!
}

wait_for_pid_file() {
  local home_dir="$1"
  local tries="${2:-80}"
  local pid_file="$home_dir/.bal/bal.pid"
  for _ in $(seq 1 "$tries"); do
    [[ -s "$pid_file" ]] && return 0
    sleep 0.05
  done
  echo "pid file not created: $pid_file" >&2
  return 1
}

wait_until_not_running() {
  local home_dir="$1"
  local tries="${2:-120}"
  for _ in $(seq 1 "$tries"); do
    if HOME="$home_dir" "$BAL_BIN" status --json >/tmp/bal-status.json 2>/dev/null; then
      if python3 - <<'PY'
import json,sys
with open('/tmp/bal-status.json') as f:
    sys.exit(0 if not json.load(f).get('running', False) else 1)
PY
      then
        return 0
      fi
    fi
    sleep 0.05
  done
  return 1
}

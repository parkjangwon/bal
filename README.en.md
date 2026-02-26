# bal

Ultra-lightweight L4 TCP load balancer for practical on-prem/local operations.
Philosophy: **Simple / Convenient / Fast / Stable**

## 5-minute start

1) Install
```bash
cargo install --path .
```

2) Prepare minimum config
```bash
mkdir -p ~/.bal
cp sample/config.yaml ~/.bal/config.yaml
```

3) Core ops flow (fixed)
```bash
bal check
bal doctor
bal status
```

4) Run/stop
```bash
bal start -d
bal status
bal stop
```

## Minimum config (Simple, recommended)

Same as `sample/config.yaml`:

```yaml
port: 9295
backends:
  - host: "127.0.0.1"
    port: 9000
```

> If `mode`/`runtime` is omitted, conservative safe defaults (auto-tuned) are applied.

## Advanced config (optional)

Use only when explicit tuning is needed:

```yaml
mode: "advanced"
bind_address: "0.0.0.0"
method: "round_robin"
log_level: "info"
runtime:
  health_check_interval_ms: 700
  health_check_timeout_ms: 1000
  health_check_fail_threshold: 2
  health_check_success_threshold: 2
  backend_connect_timeout_ms: 500
  failover_backoff_initial_ms: 300
  failover_backoff_max_ms: 3000
  backend_cooldown_ms: 1500
  protection_trigger_threshold: 8
  protection_window_ms: 10000
  protection_stable_success_threshold: 6
  max_concurrent_connections: 20000
  connection_idle_timeout_ms: 30000
  overload_policy: "reject"
  tcp_backlog: 1024
backends:
  - host: "127.0.0.1"
    port: 9000
```

## Core commands

### 1) `bal check` — static config validation
- Purpose: validate YAML/required fields/ranges before runtime
```bash
bal check
bal check --verbose
bal check --json
bal check --strict   # [advanced]
```

### 2) `bal doctor` — runtime diagnostics
- Purpose: process/PID/bind/backend reachability diagnostics
```bash
bal doctor
bal doctor --verbose
bal doctor --json
bal doctor --brief   # [advanced]
```

### 3) `bal status` — state observation
- Purpose: inspect current daemon/backend state
```bash
bal status
bal status --verbose
bal status --json
bal status --brief   # [advanced]
```

### Service control
```bash
bal start            # foreground
bal start -d         # daemon
bal graceful         # zero-downtime reload
bal stop
```

## Protection mode

When failure storms are detected (e.g., timeout/refused spikes or effective backend unavailability), protection mode is enabled automatically.

- ON: retry aggressiveness is reduced (stronger backoff/cooldown)
- OFF: automatically recovers after stable successes (hysteresis)
- Visible in: `bal status`, `bal doctor`, and JSON outputs

## Log format (ELK/Loki)

Logs are emitted as **one-line JSON (NDJSON)** by default.

```bash
bal start -d
```

JSON schema keys:
- `timestamp` (RFC3339 UTC)
- `level`
- `message`
- `module`
- `event`
- `fields`

Example:
```json
{"timestamp":"2026-02-26T00:00:00Z","level":"INFO","message":"bal v1.2.0 starting","module":"bal::main","event":"log","fields":{}}
```

## Troubleshooting

- `check` failed
  - what_happened: config is invalid
  - why_likely: YAML syntax or missing required fields
  - do_this_now: verify `--config` path and run `bal check --verbose`

- `doctor` reports CRITICAL
  - what_happened: runtime environment issue (PID/port/network)
  - why_likely: stale PID, port conflict, firewall/VPN issues
  - do_this_now: run `bal doctor --verbose` and fix failing checks

- `status` shows reachable `0/N`
  - what_happened: all backends are unreachable
  - why_likely: remote backend down or network path broken
  - do_this_now: run `bal doctor --verbose`, then verify backend/firewall/VPN

## Safety notes

Before production actions, always run in this order:
1. `bal check`
2. `bal doctor`
3. `bal status`

Prefer non-root execution. If using `bind_address: 0.0.0.0`, verify firewall/security-group policy.

# bal

Ultra-lightweight L4 TCP load balancer focused on **simple / convenient / stable** operations.

## 5-minute start

1. Install
```bash
cargo install --path .
```

2. Prepare config (simple mode, minimum required fields)
```bash
mkdir -p ~/.bal
cp sample/config.yaml ~/.bal/config.yaml
```

3. Core health flow
```bash
bal check
bal doctor
bal status
```

4. Run/stop
```bash
bal start -d
bal status
bal stop
```

## Config profiles

### Simple mode (recommended, minimum)
```yaml
port: 9295
backends:
  - host: "127.0.0.1"
    port: 9000
```

### Advanced mode (optional)
```yaml
mode: "advanced"
port: 9295
method: "round_robin"
log_level: "info"
bind_address: "0.0.0.0"
runtime:
  health_check_interval_ms: 700
  health_check_timeout_ms: 1000
backends:
  - host: "127.0.0.1"
    port: 9000
```

## Core commands

### `bal check` (static validation, concise by default)
```bash
bal check
bal check --verbose          # detailed report
bal check --json             # keep existing JSON behavior
bal check --strict           # [advanced] non-zero on warnings
```

### `bal doctor` (runtime diagnostics, concise by default)
```bash
bal doctor
bal doctor --verbose         # detailed diagnostics + hints
bal doctor --json            # keep existing JSON behavior
bal doctor --brief           # [advanced] force concise mode (compat)
```

### `bal status` (state observation, concise by default)
```bash
bal status
bal status --verbose         # backend details + hints
bal status --json            # keep existing JSON behavior
bal status --brief           # [advanced] force concise mode (compat)
```

### Service control
```bash
bal start -d
bal graceful
bal stop
```

## Troubleshooting

- `check` fails:
  - verify config path (`--config <FILE>`)
  - fix YAML syntax / required fields
- `doctor` reports CRITICAL:
  - clean stale PID file
  - resolve bind-port conflicts
  - verify backend host/port/firewall
- `status` shows `0/N reachable`:
  - run `bal doctor --verbose` for root cause

## Structured logs (ELK/Loki friendly)

Default logs remain plain text (backward compatible).

```bash
# one-line JSON logs
BAL_LOG_FORMAT=json bal start -d
```

JSON schema keys per line:
- `timestamp` (RFC3339 UTC)
- `level`
- `message`
- `module`
- `event` (currently `log`)
- `fields` (JSON object, currently `{}` unless custom payload is added)

Example:
```json
{"timestamp":"2026-02-26T00:00:00Z","level":"INFO","message":"bal v1.2.0 starting","module":"bal::main","event":"log","fields":{}}
```

Shipping tip:
- Filebeat/Fluent Bit: parse as NDJSON and map `timestamp` to event time.
- Loki: keep `level`/`module` as labels, `message` as log body.

## Safety notes

- Before production actions, always run in order:
  1. `bal check`
  2. `bal doctor`
  3. `bal status`
- Prefer non-root execution.
- If using `bind_address: 0.0.0.0`, verify firewall/security-group policies.

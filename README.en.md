# bal

Ultra-lightweight L4 TCP load balancer focused on **simple / convenient / stable** operations.

## 5-minute start

1. Install
```bash
cargo install --path .
```

2. Prepare config
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

## Safety notes

- Before production actions, always run in order:
  1. `bal check`
  2. `bal doctor`
  3. `bal status`
- Prefer non-root execution.
- If using `bind_address: 0.0.0.0`, verify firewall/security-group policies.

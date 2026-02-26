# bal

<img width="1024" height="1024" alt="1772038664631" src="https://github.com/user-attachments/assets/2ea1ea47-50bd-4c41-b4ab-e71ac90a5f85" />

온프레미스/로컬 운영에 맞춘 초경량 L4 TCP 로드밸런서.

**쉽고(Simple) / 단순하고(Convenient) / 빠르고(Fast) / 안정적(Stable)**

## 빠른 설치/업데이트/삭제

```bash
# 설치/업데이트
curl -sSL https://raw.githubusercontent.com/parkjangwon/bal/main/install.sh | bash

# 삭제
curl -sSL https://raw.githubusercontent.com/parkjangwon/bal/main/install.sh | bash -s -- --uninstall
```

## 5분 시작

1) 설치
```bash
cargo install --path .
```

2) 최소 설정 준비
```bash
mkdir -p ~/.bal
cp sample/config.yaml ~/.bal/config.yaml
```

3) 핵심 운영 흐름 (고정)
```bash
bal check
bal doctor
bal status
```

4) 실행/중지
```bash
bal start -d
bal status
bal stop
```

## 최소 설정 (권장)

`sample/config.yaml`과 동일한 최소 필드:

```yaml
port: 9295
backends:
  - host: "127.0.0.1"
    port: 9000
```

> `runtime`을 생략하면 백엔드 수 기준 보수적 auto-tuning 기본값이 적용됩니다.

## 명시적 오버라이드 설정 (선택)

필요한 키만 명시해서 기본값을 덮어쓰세요 (`sample/config.advanced.yaml` 참고):

```yaml
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

## 핵심 명령어

### 1) `bal check` — 정적 설정 검증
- 목적: YAML/필수값/범위 검증 (실행 전 검증)
```bash
bal check
bal check --verbose
bal check --json
bal check --strict   # [advanced]
```

> 하위 호환: 구버전 설정의 `mode` 필드는 파싱 시 무시됩니다.

### 2) `bal doctor` — 런타임 진단
- 목적: 프로세스/PID/바인딩/백엔드 도달성 점검
```bash
bal doctor
bal doctor --verbose
bal doctor --json
bal doctor --brief   # [advanced]
```

### 3) `bal status` — 상태 관찰
- 목적: 현재 daemon/backend 상태 조회
```bash
bal status
bal status --verbose
bal status --json
bal status --brief   # [advanced]
```

### 서비스 제어
```bash
bal start            # foreground
bal start -d         # daemon
bal graceful         # 무중단 리로드
bal stop
```

## 자동 보호 모드 (Protection Mode)

장애 폭주(예: timeout/refused 급증, 백엔드 실질 불가용) 감지 시 자동으로 보호 모드가 켜집니다.

- ON 시: 재시도 공격성 완화(백오프/쿨다운 강화)
- OFF 시: 안정 성공 누적 후 자동 복귀(히스테리시스)
- 노출 위치: `bal status`, `bal doctor`, JSON 출력

## 로그 포맷 (ELK/Loki)

로그는 기본적으로 **one-line JSON (NDJSON)** 입니다.

```bash
bal start -d
```

JSON 키 스키마:
- `timestamp` (RFC3339 UTC)
- `level`
- `message`
- `module`
- `event`
- `fields`

예시:
```json
{"timestamp":"2026-02-26T00:00:00Z","level":"INFO","message":"bal v1.2.0 starting","module":"bal::main","event":"log","fields":{}}
```

## 릴리즈 게이트 통합 테스트

릴리즈 태깅 전에 아래 게이트 스크립트를 실행하세요.

```bash
scripts/e2e/run_gate.sh
```

검증 범위:
- check/doctor/status JSON 계약 키 검증
- NDJSON 로그 키 검증 (비JSON 노이즈 라인은 안전하게 무시)
- stop 라이프사이클 결정론 검증 (정상 중지 vs 이미 중지 분리)

## 트러블슈팅

- `check` 실패
  - what_happened: 설정이 유효하지 않음
  - why_likely: YAML 문법/필수 필드 누락
  - do_this_now: `--config` 경로 확인 후 `bal check --verbose`

- `doctor` CRITICAL
  - what_happened: 런타임 환경 문제(PID/포트/네트워크)
  - why_likely: stale PID, 포트 충돌, 방화벽/VPN 이슈
  - do_this_now: `bal doctor --verbose`로 항목별 원인 확인

- `status`에서 reachable 0/N
  - what_happened: 모든 백엔드 도달 실패
  - why_likely: 원격 백엔드 다운 또는 경로 단절
  - do_this_now: `bal doctor --verbose` → backend/firewall/VPN 순서로 점검

## 안전 수칙

운영 반영 전 항상 아래 순서를 지키세요.
1. `bal check`
2. `bal doctor`
3. `bal status`

가능하면 non-root로 실행하고, `bind_address: 0.0.0.0` 사용 시 방화벽 정책을 반드시 함께 점검하세요.

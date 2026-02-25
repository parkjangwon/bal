# bal

초경량 L4 TCP 로드밸런서입니다. 기본 철학은 **simple / convenient / stable** 입니다.

## 5분 시작

1. 설치
```bash
cargo install --path .
```

2. 샘플 설정 복사
```bash
mkdir -p ~/.bal
cp sample/config.yaml ~/.bal/config.yaml
```

3. 핵심 점검 (권장 순서)
```bash
bal check
bal doctor
bal status
```

4. 실행/중지
```bash
bal start -d
bal status
bal stop
```

## Core Commands

### `bal check` (정적 검증, 기본은 간결 출력)
```bash
bal check
bal check --verbose          # 상세 리포트
bal check --json             # JSON 출력(기존 동작 유지)
bal check --strict           # [advanced] warning도 실패 처리
```

### `bal doctor` (런타임 진단, 기본은 간결 출력)
```bash
bal doctor
bal doctor --verbose         # 상세 진단 + 힌트
bal doctor --json            # JSON 출력(기존 동작 유지)
bal doctor --brief           # [advanced] 기본 간결 출력 강제(하위호환)
```

### `bal status` (상태 관찰, 기본은 간결 출력)
```bash
bal status
bal status --verbose         # backend_details + 힌트
bal status --json            # JSON 출력(기존 동작 유지)
bal status --brief           # [advanced] 기본 간결 출력 강제(하위호환)
```

### 서비스 제어
```bash
bal start -d
bal graceful
bal stop
```

## Troubleshooting

- `check` 실패:
  - 설정 파일 경로 확인: `--config <FILE>`
  - YAML 문법/필수 필드 확인
- `doctor` 에서 CRITICAL:
  - stale PID 제거 후 재시도
  - 포트 충돌 프로세스 정리
  - backend host/port/firewall 확인
- `status` 에서 reachable 0/N:
  - `bal doctor --verbose`로 상세 원인 확인

## Safety Notes (필수)

- 운영 반영 전 반드시 아래 3개를 순서대로 실행하세요.
  1. `bal check`
  2. `bal doctor`
  3. `bal status`
- 가능하면 비루트 사용자로 실행하세요.
- `bind_address: 0.0.0.0` 사용 시 방화벽/보안그룹 정책을 함께 점검하세요.

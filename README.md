# bal - Ultra-lightweight TCP Load Balancer

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

<img width="1024" height="1024" alt="1772038664631" src="https://github.com/user-attachments/assets/c104a22a-d3fe-4d85-9ceb-5854e4ad4780" />

bal은 고성능 L4(TCP) 로드밸런서로, SSL Passthrough와 무중단 설정 교체를 지원합니다.

## 5분 시작 (권장: simple mode)

1. `bal start` 실행 (없으면 `~/.bal/config.yaml` 자동 생성)
2. config에 `mode: "simple"` 유지 + `backends`만 채우기
3. `bal check`로 검증
4. `bal start -d`로 데몬 실행
5. `bal status`와 `bal doctor`로 상태 점검

`simple` 모드는 운영에 필요한 핵심 필드만 노출해 실수를 줄입니다. 세부 런타임 튜닝이 필요할 때만 `mode: "advanced"`로 전환하세요.

## 주요 기능

- **SSL Passthrough**: L4 레벨에서 패킷을 투명하게 전달하여 백엔드에서 SSL 인증서 처리
- **무중단 설정 교체**: arc-swap 기반 핫 리로드 (SIGHUP 신호)
- **비동기 헬스체크**: 5초 간격으로 백엔드 상태 모니터링
- **비루트 실행**: 홈 디렉토리 기반 작업 (`~/.bal/`)
- **Graceful Shutdown**: SIGINT/SIGTERM 시 기존 연결 유지
- **단일 바이너리**: 외부 의존성 없이 단일 파일로 배포 가능

## 설치

### 빠른 설치

```bash
curl -sSL https://raw.githubusercontent.com/parkjangwon/bal/main/install.sh | bash
```

지원 환경:
- macOS (Apple Silicon)
- Linux (x86_64, i386)

### 업데이트

이미 설치된 경우, 동일한 명령어로 최신 버전으로 업데이트할 수 있습니다:

```bash
curl -sSL https://raw.githubusercontent.com/parkjangwon/bal/main/install.sh | bash
```

### 삭제

```bash
curl -sSL https://raw.githubusercontent.com/parkjangwon/bal/main/install.sh | bash -s -- --uninstall
```
### 소스에서 빌드

```bash
# 소스에서 빌드
git clone https://github.com/parkjangwon/bal
cd bal
cargo build --release

# 바이너리 복사
sudo cp target/release/bal /usr/local/bin/
```

## 빠른 시작 (테스트)

### 1. 샘플 백엔드 서버 실행

```bash
cd sample

# 터미널 1: 첫 번째 백엔드 실행 (포트 9000)
node backend.js 9000

# 터미널 2: 두 번째 백엔드 실행 (포트 9100)
node backend.js 9100
```

또는 npm 스크립트 사용:
```bash
npm run start:all  # 두 백엔드 동시 실행
```

### 2. bal 로드밸런서 실행

```bash
# 설정 검증
bal check

# 데몬 시작
bal start
```

### 3. 테스트

```bash
# 여러 번 요청하여 로드밸런싱 확인
curl http://localhost:9295/
curl http://localhost:9295/
curl http://localhost:9295/
```

각 요청이 다른 백엔드(9000, 9100)로 전달되는 것을 백엔드 콘솔 로그에서 확인할 수 있습니다.

### 4. 종료

```bash
# bal 종료
bal stop

# 백엔드 서버 종료 (샘플 폴터에서)
npm run stop
```

---

## 사용법

### 1. 설정 파일 생성

```bash
# 기본 설정 파일 자동 생성 (없는 경우)
bal start
```

또는 수동으로 `~/.bal/config.yaml` 생성:

```yaml
mode: "simple"

# bal 서비스 포트 (9295: 설계자 지정 유니크 포트)
port: 9295

# 로드밸런싱 방법 (기본값: round_robin)
method: "round_robin"

# 백엔드 서버 목록
backends:
  - host: "127.0.0.1"
    port: 9000
  - host: "127.0.0.1"
    port: 9100
```

### 2. 데몬 시작

```bash
bal start                    # 기본 설정으로 시작
bal start -c /path/to/config.yaml  # 지정된 설정 파일로 시작
```

### 3. 설정 검증 (정적 검사)

```bash
bal check                              # 정적 설정 검사(기본)
bal check --strict                     # 경고도 실패(비정상 종료 코드)
bal check --json                       # JSON 출력
bal check -c /path/to/config.yaml
```

### 4. 설정 무중단 재로드

```bash
bal graceful                 # 실행 중인 데몬에 SIGHUP 신호 전송
```

### 5. 데몬 종료

```bash
bal stop                     # SIGTERM 신호로 안전하게 종료
```

### CLI 도움말

```bash
bal --help                   # 전체 도움말
bal start --help            # start 명령어 도움말
```

## 아키텍처

```
클이언트
    │
    ▼
┌─────────────────────────────────────┐
│  bal 로드밸런서 (포트 9295)         │
│  ┌─────────────┐  ┌─────────────┐  │
│  │   Proxy     │  │   Health    │  │
│  │   Server    │  │   Checker   │  │
│  └──────┬──────┘  └─────────────┘  │
│         │                           │
│  ┌──────▼──────┐  ┌─────────────┐  │
│  │  Load       │  │   Config    │  │
│  │  Balancer   │  │   Store     │  │
│  │ (RoundRobin)│  │ (arc-swap)  │  │
│  └──────┬──────┘  └─────────────┘  │
└─────────┼───────────────────────────┘
          │     │
          ▼     ▼
    ┌─────────┐ ┌─────────┐
    │백엔드 1 │ │백엔드 2 │
    │:443     │ │:443     │
    └─────────┘ └─────────┘
```

## 파일 구조

```
src/
├── main.rs           # 애플리케이션 진입점
├── cli.rs            # CLI 인자 파싱 (clap)
├── config.rs         # 설정 파일 관리 (YAML)
├── config_store.rs   # arc-swap 기반 핫스왑
├── constants.rs      # 상수 정의
├── error.rs          # 에러 처리
├── backend_pool.rs   # 백엔드 상태 관리
├── load_balancer.rs  # 로드밸런싱 알고리즘
├── proxy.rs          # TCP 프록시 (copy_bidirectional)
├── health.rs         # 헬스체크
├── supervisor.rs     # 태스크 오케스트레이션
├── process.rs        # PID 파일, 프로세스 제어
├── state.rs          # 앱 상태 관리
└── logging.rs        # 로깅 설정
```

## 기술 스택

- **언어**: Rust (Latest Stable)
- **비동기 런타임**: Tokio
- **CLI**: clap v4
- **설정**: serde + serde_yaml
- **핫스왑**: arc-swap
- **시그널**: nix

## 성능 특성

- **커널 수준 zero-copy**: `tokio::io::copy_bidirectional` 사용
- **락 프리 설정 교체**: `arc-swap`으로 원자적 설정 교체
- **효율적인 메모리 사용**: 단일 바이너리 약 2MB
- **빠른 시작**: 밀리초 단위 초기화

## 운영 배포 템플릿 (On-prem + VPN + hosts)

예시 `/etc/hosts`:

```txt
10.10.0.11 app-a.internal
10.10.0.12 app-b.internal
```

예시 `~/.bal/config.yaml`:

```yaml
mode: "advanced"
port: 9295
bind_address: "0.0.0.0"
method: "round_robin"
log_level: "info"

runtime:
  health_check_interval_ms: 1000
  health_check_timeout_ms: 500
  health_check_fail_threshold: 2
  health_check_success_threshold: 2
  backend_connect_timeout_ms: 400
  failover_backoff_initial_ms: 100
  failover_backoff_max_ms: 5000
  backend_cooldown_ms: 500
  max_concurrent_connections: 20000
  connection_idle_timeout_ms: 120000
  overload_policy: "reject"
  tcp_backlog: 2048

backends:
  - host: "app-a.internal"
    port: 443
  - host: "app-b.internal"
    port: 443
```

## 서비스 등록 예시

### systemd (Linux)

`/etc/systemd/system/bal.service`

```ini
[Unit]
Description=bal TCP Load Balancer
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=bal
Group=bal
ExecStart=/usr/local/bin/bal start -c /home/bal/.bal/config.yaml
ExecReload=/usr/bin/kill -HUP $MAINPID
Restart=always
RestartSec=2
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now bal
sudo systemctl status bal
```

### launchd (macOS)

`~/Library/LaunchAgents/com.bal.daemon.plist`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.bal.daemon</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/bal</string>
    <string>start</string>
    <string>-c</string>
    <string>/Users/your-user/.bal/config.yaml</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/tmp/bal.out.log</string>
  <key>StandardErrorPath</key><string>/tmp/bal.err.log</string>
</dict>
</plist>
```

```bash
launchctl load ~/Library/LaunchAgents/com.bal.daemon.plist
launchctl list | grep bal
```

## 명령 경계 (check / doctor / status)

- `bal check`: **정적 설정 검증만** 수행합니다. (기본적으로 네트워크 접속 테스트 안 함)
- `bal doctor`: 런타임 진단/환경 점검(바인딩 가능 여부, PID/백엔드 도달성 등)
- `bal status`: 현재 상태 관찰(실행 여부, 설정/백엔드 요약, 보호 모드 상태)

간단 매트릭스:

| 명령어 | 목적 | 기본 출력 | 주요 옵션 |
|---|---|---|---|
| `bal check` | 정적 설정 유효성 | 체크 리포트 | `--strict`, `--json` |
| `bal doctor` | 런타임/환경 진단 | 진단 리포트 | `--brief`, `--json` |
| `bal status` | 상태 관찰 | 상태 요약 | `--brief`, `--json` |

## 자동 보호 모드 (Protection Mode)

- 트리거: 짧은 시간 내 timeout/refused 오류 폭증 또는 사실상 모든 백엔드 불가용
- 동작: failover 재시도 공격성을 낮추기 위해 backoff/cooldown을 자동 상향
- 복구: 안정 성공이 충분히 누적되면 자동 해제(히스테리시스)
- 노출: `bal status`, `bal doctor`, JSON 출력에 `protection_mode`와 `reason` 표시

## 운영 점검/트러블슈팅 런북

1. 설정 사전 검증

```bash
bal check -c ~/.bal/config.yaml
```

2. 런타임 상태 확인

```bash
bal status
bal status --json
```

3. 무중단 리로드

```bash
bal graceful
```

4. 증상별 대응
- `All backends failed`: VPN/hosts/DNS 확인, 백엔드 방화벽/보안그룹 점검
- 연결이 튄다(Flapping): `backend_cooldown_ms`, `failover_backoff_*` 상향
- 과부하 시 거절 증가: `max_concurrent_connections`/`tcp_backlog` 조정
- 리로드 실패: 에러 로그 확인 후 설정 수정, 기존 런타임 설정은 유지됨

## 라이선스

MIT License

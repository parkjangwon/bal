# bal - Ultra-lightweight TCP Load Balancer

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

bal은 고성능 L4(TCP) 로드밸런서로, SSL Passthrough와 무중단 설정 교체를 지원합니다.

## 주요 기능

- **SSL Passthrough**: L4 레벨에서 패킷을 투명하게 전달하여 백엔드에서 SSL 인증서 처리
- **무중단 설정 교체**: arc-swap 기반 핫 리로드 (SIGHUP 신호)
- **비동기 헬스체크**: 5초 간격으로 백엔드 상태 모니터링
- **비루트 실행**: 홈 디렉토리 기반 작업 (`~/.bal/`)
- **Graceful Shutdown**: SIGINT/SIGTERM 시 기존 연결 유지
- **단일 바이너리**: 외부 의존성 없이 단일 파일로 배포 가능

## 설치

### 빠른 설치 (원라이너)

```bash
curl -sSL https://raw.githubusercontent.com/parkjangwon/bal/main/install.sh | bash
```

지원 환경:
- macOS (Apple Silicon)
- Linux (x86_64, i386)

### 소스에서 빌드

```bash
# 소스에서 빌드
git clone https://github.com/parkjangwon/bal
cd bal
cargo build --release

# 바이너리 복사
cp target/release/bal /usr/local/bin/
```

```bash
# 소스에서 빌드
git clone https://github.com/parkjangwon/bal
cd bal
cargo build --release

# 바이너리 복사
cp target/release/bal /usr/local/bin/
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

### 3. 설정 검증 (Dry-run)

```bash
bal check                    # 설정 파일 검증
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

## 라이선스

MIT License

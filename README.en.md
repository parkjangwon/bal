# bal - Ultra-lightweight TCP Load Balancer

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

bal is a high-performance L4 (TCP) load balancer supporting SSL Passthrough and zero-downtime configuration reload.

## Key Features

- **SSL Passthrough**: Transparently relays packets at L4 level, letting backends handle SSL certificates
- **Zero-downtime Config Reload**: Hot reload via arc-swap (SIGHUP signal)
- **Async Health Checks**: Backend status monitoring every 5 seconds
- **Non-root Execution**: Home directory based operations (`~/.bal/`)
- **Graceful Shutdown**: Preserves existing connections on SIGINT/SIGTERM
- **Single Binary**: Deploy as a single file without external dependencies

## Installation

```bash
# Build from source
git clone https://github.com/bal/bal
cd bal
cargo build --release

# Copy binary
cp target/release/bal /usr/local/bin/
```

## Usage

### 1. Create Configuration File

```bash
# Auto-create default config if not exists
bal start
```

Or manually create `~/.bal/config.yaml`:

```yaml
# bal service port (9295: designer-assigned unique port)
port: 9295

# Load balancing method (default: round_robin)
method: "round_robin"

# Backend server list
backends:
  - host: "210.22.11.33"
    port: 443
  - host: "210.22.11.34"
    port: 443
```

### 2. Start Daemon

```bash
bal start                    # Start with default config
bal start -c /path/to/config.yaml  # Start with specified config
```

### 3. Validate Configuration (Dry-run)

```bash
bal check                    # Validate config file
bal check -c /path/to/config.yaml
```

### 4. Reload Configuration Without Downtime

```bash
bal graceful                 # Send SIGHUP signal to running daemon
```

### 5. Stop Daemon

```bash
bal stop                     # Safely terminate with SIGTERM
```

### CLI Help

```bash
bal --help                   # Full help
bal start --help            # Help for start command
```

## Architecture

```
Client
    │
    ▼
┌─────────────────────────────────────┐
│  bal Load Balancer (Port 9295)      │
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
    │Backend 1│ │Backend 2│
    │:443     │ │:443     │
    └─────────┘ └─────────┘
```

## Project Structure

```
src/
├── main.rs           # Application entry point
├── cli.rs            # CLI argument parsing (clap)
├── config.rs         # Configuration file management (YAML)
├── config_store.rs   # arc-swap based hot-swapping
├── constants.rs      # Constants definition
├── error.rs          # Error handling
├── backend_pool.rs   # Backend state management
├── load_balancer.rs  # Load balancing algorithms
├── proxy.rs          # TCP proxy (copy_bidirectional)
├── health.rs         # Health checks
├── supervisor.rs     # Task orchestration
├── process.rs        # PID file, process control
├── state.rs          # App state management
└── logging.rs        # Logging configuration
```

## Tech Stack

- **Language**: Rust (Latest Stable)
- **Async Runtime**: Tokio
- **CLI**: clap v4
- **Configuration**: serde + serde_yaml
- **Hot-swap**: arc-swap
- **Signals**: nix

## Performance Characteristics

- **Kernel-level zero-copy**: Uses `tokio::io::copy_bidirectional`
- **Lock-free config reload**: Atomic config replacement via `arc-swap`
- **Efficient memory usage**: Single binary ~2MB
- **Fast startup**: Millisecond-level initialization

## License

MIT License

//! CLI argument parsing module
//!
//! Uses clap derive macros to declaratively define commands and arguments.
//! This approach ensures type safety and automatically generates --help and --version.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// bal - Ultra-lightweight TCP Load Balancer
///
/// A high-performance L4 TCP load balancer supporting SSL Passthrough,
/// zero-downtime config reload, and async health checks.
#[derive(Parser, Debug)]
#[command(
    name = "bal",
    about = "Ultra-lightweight TCP Load Balancer",
    long_about = r#"
bal is a high-performance L4 TCP load balancer.

Key Features:
  - SSL Passthrough: Transparent packet relay at L4 level
  - Zero-downtime config reload: arc-swap based hot reload
  - Async health checks: Backend status monitoring every 5 seconds
  - Non-root execution: Home directory based operations
  - Graceful Shutdown: Existing connections preserved on SIGINT/SIGTERM

Usage Examples:
  bal start                    # Start in foreground mode
  bal start -d                 # Start as background daemon
  bal start -c /path/config.yaml  # Start with specified config file
  bal stop                     # Stop running daemon
  bal graceful                 # Reload config without downtime
  bal check                    # Validate configuration file
  bal status                   # Show local process/backend summary
"#,
    version = env!("CARGO_PKG_VERSION"),
    author = "bal Team"
)]
pub struct Cli {
    /// Subcommand (start, stop, graceful, check)
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true, help = "Enable verbose logging output")]
    pub verbose: bool,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the load balancer
    ///
    /// Starts the load balancer with the specified configuration file.
    /// If no config file is specified, searches default paths.
    /// Use -d flag to run as background daemon.
    #[command(name = "start", about = "Start the load balancer")]
    Start {
        /// Configuration file path (optional)
        ///
        /// If not specified, searches in this order:
        /// 1. $HOME/.bal/config.yaml
        /// 2. /etc/bal/config.yaml
        #[arg(short, long, value_name = "FILE", help = "Configuration file path")]
        config: Option<PathBuf>,

        /// Run as daemon in background
        ///
        /// Detaches from terminal and runs in background.
        /// Logs are written to file instead of console.
        #[arg(short, long, help = "Run as daemon in background")]
        daemon: bool,
    },

    /// Stop running daemon
    ///
    /// Reads the PID file and sends SIGTERM signal to gracefully
    /// terminate the running bal process.
    #[command(name = "stop", about = "Stop running daemon")]
    Stop,

    /// Reload configuration without downtime (graceful reload)
    ///
    /// Sends SIGHUP signal to the running daemon to reload configuration.
    /// Existing connections are preserved, new connections use new config.
    #[command(name = "graceful", about = "Reload configuration without downtime")]
    Graceful,

    /// Validate configuration file (Dry-run)
    ///
    /// Validates configuration file syntax and backend connectivity.
    /// Does not start the actual service, only checks for problems.
    #[command(name = "check", about = "Validate configuration file")]
    Check {
        /// Configuration file path to validate
        ///
        /// If not specified, uses default config file search paths.
        #[arg(
            short,
            long,
            value_name = "FILE",
            help = "Configuration file path to validate"
        )]
        config: Option<PathBuf>,
    },

    /// Show local process and backend status summary
    #[command(name = "status", about = "Show local process/backend summary")]
    Status {
        /// Configuration file path used for backend summary
        #[arg(
            short,
            long,
            value_name = "FILE",
            help = "Configuration file path for status summary"
        )]
        config: Option<PathBuf>,
    },
}

impl Cli {
    /// Parse CLI arguments and create Cli struct
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

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
  bal check                    # Validate static configuration
  bal doctor                   # Run runtime diagnostics/environment checks
  bal status                   # Observe local state summary
"#,
    version = env!("CARGO_PKG_VERSION"),
    author = "bal Team"
)]
pub struct Cli {
    /// Subcommand (start, stop, graceful, check, status, doctor)
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

    /// Validate static configuration
    ///
    /// Validates configuration syntax and static constraints only.
    #[command(name = "check", about = "Validate static configuration")]
    Check {
        /// Configuration file path to validate
        #[arg(
            short,
            long,
            value_name = "FILE",
            help = "Configuration file path to validate"
        )]
        config: Option<PathBuf>,

        /// Treat warnings as errors (non-zero exit)
        #[arg(long, help = "Return non-zero when warnings are present")]
        strict: bool,

        /// Print check report in JSON format
        #[arg(long, help = "Print check report in JSON format")]
        json: bool,
    },

    /// Observe local process and backend state
    #[command(name = "status", about = "Observe local process/backend state")]
    Status {
        /// Configuration file path used for backend summary
        #[arg(
            short,
            long,
            value_name = "FILE",
            help = "Configuration file path for status summary"
        )]
        config: Option<PathBuf>,

        /// Print status in JSON format
        #[arg(long, help = "Print status in JSON format")]
        json: bool,

        /// Print compact status output
        #[arg(long, help = "Print compact status output")]
        brief: bool,
    },

    /// Run runtime diagnostics and environment checks
    #[command(name = "doctor", about = "Run runtime diagnostics/environment checks")]
    Doctor {
        /// Configuration file path used for diagnostics
        #[arg(
            short,
            long,
            value_name = "FILE",
            help = "Configuration file path for diagnostics"
        )]
        config: Option<PathBuf>,

        /// Print diagnostics in JSON format
        #[arg(long, help = "Print diagnostics in JSON format")]
        json: bool,

        /// Print compact diagnostics output
        #[arg(long, help = "Print compact diagnostics output")]
        brief: bool,
    },
}

impl Cli {
    /// Parse CLI arguments and create Cli struct
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn check_accepts_strict_and_json_flags() {
        let cli = Cli::try_parse_from(["bal", "check", "--strict", "--json"])
            .expect("check command should parse");

        match cli.command {
            Commands::Check { strict, json, .. } => {
                assert!(strict);
                assert!(json);
            }
            _ => panic!("expected check command"),
        }
    }

    #[test]
    fn status_accepts_brief_flag() {
        let cli =
            Cli::try_parse_from(["bal", "status", "--brief"]).expect("status command should parse");

        match cli.command {
            Commands::Status { brief, .. } => assert!(brief),
            _ => panic!("expected status command"),
        }
    }

    #[test]
    fn doctor_accepts_brief_flag() {
        let cli =
            Cli::try_parse_from(["bal", "doctor", "--brief"]).expect("doctor command should parse");

        match cli.command {
            Commands::Doctor { brief, .. } => assert!(brief),
            _ => panic!("expected doctor command"),
        }
    }
}

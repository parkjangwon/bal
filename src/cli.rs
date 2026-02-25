//! CLI argument parsing module
//!
//! Uses clap derive macros to declaratively define commands and arguments.
//! This approach ensures type safety and automatically generates --help and --version.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// bal - Ultra-lightweight TCP Load Balancer
#[derive(Parser, Debug)]
#[command(
    name = "bal",
    about = "Ultra-lightweight TCP Load Balancer",
    long_about = r#"
bal is a high-performance L4 TCP load balancer.

Core operations (recommended flow: check -> doctor -> status):
  bal check     # Validate static configuration
  bal doctor    # Diagnose runtime environment and connectivity
  bal status    # Observe current daemon/backend state

Service control:
  bal start     # Start in foreground mode
  bal start -d  # Start as background daemon
  bal stop      # Stop running daemon
  bal graceful  # Reload config without downtime
"#,
    version = env!("CARGO_PKG_VERSION"),
    author = "bal Team"
)]
pub struct Cli {
    /// Subcommand (start, stop, graceful, check, status, doctor)
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose logging (advanced)
    #[arg(short, long, help = "[advanced] Enable verbose logging output")]
    pub verbose: bool,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the load balancer
    #[command(name = "start", about = "Start the load balancer")]
    Start {
        /// Configuration file path (optional)
        #[arg(short, long, value_name = "FILE", help = "Configuration file path")]
        config: Option<PathBuf>,

        /// Run as daemon in background
        #[arg(short, long, help = "Run as daemon in background")]
        daemon: bool,
    },

    /// Stop running daemon
    #[command(name = "stop", about = "Stop running daemon")]
    Stop,

    /// Reload configuration without downtime (graceful reload)
    #[command(name = "graceful", about = "Reload configuration without downtime")]
    Graceful,

    /// Validate static configuration
    #[command(
        name = "check",
        about = "Validate static configuration (core, step 1: check -> doctor -> status)"
    )]
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
        #[arg(long, help = "[advanced] Return non-zero when warnings are present")]
        strict: bool,

        /// Print check report in JSON format
        #[arg(long, help = "Print check report in JSON format")]
        json: bool,

        /// Print detailed check output
        #[arg(long, help = "Print detailed check output")]
        verbose: bool,
    },

    /// Observe local process and backend state
    #[command(
        name = "status",
        about = "Observe local process/backend state (core, step 3 after check -> doctor)"
    )]
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
        #[arg(long, help = "[advanced] Force compact status output (default)")]
        brief: bool,

        /// Print detailed status output
        #[arg(long, help = "Print detailed status output")]
        verbose: bool,
    },

    /// Run runtime diagnostics and environment checks
    #[command(
        name = "doctor",
        about = "Run runtime diagnostics/environment checks (core, step 2 between check -> status)"
    )]
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
        #[arg(long, help = "[advanced] Force compact diagnostics output (default)")]
        brief: bool,

        /// Print detailed diagnostics output
        #[arg(long, help = "Print detailed diagnostics output")]
        verbose: bool,
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
    fn check_accepts_strict_json_and_verbose_flags() {
        let cli = Cli::try_parse_from(["bal", "check", "--strict", "--json", "--verbose"])
            .expect("check command should parse");

        match cli.command {
            Commands::Check {
                strict,
                json,
                verbose,
                ..
            } => {
                assert!(strict);
                assert!(json);
                assert!(verbose);
            }
            _ => panic!("expected check command"),
        }
    }

    #[test]
    fn status_accepts_brief_and_verbose_flags() {
        let cli = Cli::try_parse_from(["bal", "status", "--brief", "--verbose"])
            .expect("status command should parse");

        match cli.command {
            Commands::Status { brief, verbose, .. } => {
                assert!(brief);
                assert!(verbose);
            }
            _ => panic!("expected status command"),
        }
    }

    #[test]
    fn doctor_accepts_brief_and_verbose_flags() {
        let cli = Cli::try_parse_from(["bal", "doctor", "--brief", "--verbose"])
            .expect("doctor command should parse");

        match cli.command {
            Commands::Doctor { brief, verbose, .. } => {
                assert!(brief);
                assert!(verbose);
            }
            _ => panic!("expected doctor command"),
        }
    }
}

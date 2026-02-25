//! bal - Ultra-lightweight TCP Load Balancer
//!
//! bal is a high-performance L4 TCP load balancer with these features:
//! - SSL Passthrough (transparent packet relay at L4 level)
//! - Zero-downtime config reload (arc-swap based hot reload)
//! - Async health checks (backend status monitoring every 5 seconds)
//! - Non-root execution (home directory based operations)
//! - Graceful Shutdown (existing connections preserved on SIGINT/SIGTERM)

use anyhow::Result;
use log::info;

mod backend_pool;
mod check;
mod cli;
mod config;
mod config_store;
mod constants;
mod doctor;
mod error;
mod health;
mod load_balancer;
mod logging;
mod process;
mod protection;
mod proxy;
mod state;
mod supervisor;

use cli::{Cli, Commands};
use config::Config;
use process::ProcessManager;

/// Application entry point
///
/// Parses CLI arguments and dispatches to appropriate subcommands.
#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse_args();

    // Determine if running in daemon mode
    let daemon_mode = matches!(cli.command, Commands::Start { daemon: true, .. });

    // For Start command, load config first to get log_level
    let log_level = match &cli.command {
        Commands::Start {
            config: cli_config, ..
        } => {
            // Try to load config to get log_level
            match Config::resolve_config_path(cli_config.as_deref()) {
                Ok(config_path) => {
                    match Config::load(&config_path).await {
                        Ok(config) => config.log_level,
                        Err(_) => "info".to_string(), // Default if config fails to load
                    }
                }
                Err(_) => "info".to_string(), // Default if no config found
            }
        }
        _ => "info".to_string(), // Default for non-start commands
    };

    // Initialize logging system with config's log_level
    logging::init_logging(&log_level, daemon_mode)?;

    info!("bal v{} starting", env!("CARGO_PKG_VERSION"));

    // Dispatch subcommands
    match cli.command {
        Commands::Start { config, daemon } => {
            if daemon {
                // Run as background daemon (detached)
                info!("Starting in daemon mode");
                supervisor::run_daemon(config.as_deref()).await?;
            } else {
                // Run in foreground
                info!("Starting in foreground mode");
                supervisor::run_foreground(config.as_deref()).await?;
            }
        }
        Commands::Stop => {
            // Stop running process
            info!("Stopping running bal process");
            ProcessManager::stop_daemon()?;
        }
        Commands::Graceful => {
            // Zero-downtime config reload (send SIGHUP signal)
            info!("Reloading configuration gracefully");
            ProcessManager::send_reload_signal()?;
        }
        Commands::Check {
            config,
            strict,
            json,
            verbose,
        } => {
            info!("Running static config check");
            check::run_and_print(config, strict, json, verbose).await?;
        }
        Commands::Status {
            config,
            json,
            brief,
            verbose,
        } => {
            info!("Showing bal state status");
            ProcessManager::print_status(config, json, verbose && !brief).await?;
        }
        Commands::Doctor {
            config,
            json,
            brief,
            verbose,
        } => {
            info!("Running bal doctor diagnostics");
            doctor::run_and_print(config, json, verbose && !brief).await?;
        }
    }

    Ok(())
}

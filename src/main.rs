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

mod cli;
mod config;
mod config_store;
mod load_balancer;
mod backend_pool;
mod proxy;
mod health;
mod supervisor;
mod process;
mod state;
mod logging;
mod constants;
mod error;

use cli::{Cli, Commands};
use process::ProcessManager;

/// Application entry point
/// 
/// Parses CLI arguments and dispatches to appropriate subcommands.
#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse_args();
    
    // Initialize logging system
    logging::init_logging(cli.verbose)?;
    
    info!("bal v{} starting", env!("CARGO_PKG_VERSION"));
    
    // Dispatch subcommands
    match cli.command {
        Commands::Start { config } => {
            // Run as background daemon
            info!("Starting in daemon mode");
            supervisor::run_daemon(config.as_deref()).await?;
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
        Commands::Check { config } => {
            // Validate config file (Dry-run)
            info!("Validating configuration file");
            config::validate_config_file(config).await?;
            println!("Configuration file is valid");
        }
    }
    
    Ok(())
}

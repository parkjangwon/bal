//! bal - Ultra-lightweight TCP Load Balancer
//!
//! bal is a high-performance L4 TCP load balancer with these features:
//! - SSL Passthrough (transparent packet relay at L4 level)
//! - Zero-downtime config reload (arc-swap based hot reload)
//! - Async health checks (backend status monitoring every 5 seconds)
//! - Non-root execution (home directory based operations)
//! - Graceful Shutdown (existing connections preserved on SIGINT/SIGTERM)

use anyhow::Result;
use daemonize::Daemonize;

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
mod operator_message;
mod process;
mod protection;
mod proxy;
mod state;
mod supervisor;

use cli::{Cli, Commands};
use config::Config;
use constants::get_pid_file_path;
use process::ProcessManager;

/// Fork and detach process to run as daemon
/// Note: PID file is created by supervisor::run_daemon, not here
fn fork_daemon() -> Result<()> {
    let daemonize = Daemonize::new()
        .working_directory("/tmp")
        .umask(0o027);

    match daemonize.start() {
        Ok(_) => {
            // Child process continues - parent has exited
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to daemonize: {}", e);
            std::process::exit(1);
        }
    }
}

/// Run async logic with the pre-parsed command
async fn run_with_command(command: Commands, daemon_mode: bool) -> Result<()> {
    // For Start command, load config first to get log_level
    let log_level = match &command {
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

    log::info!("bal v{} starting", env!("CARGO_PKG_VERSION"));

    // Dispatch subcommands
    match command {
        Commands::Start { config, daemon } => {
            if daemon {
                // Already forked, run daemon logic
                log::info!("Starting in daemon mode");
                supervisor::run_daemon(config.as_deref()).await?;
            } else {
                // Run in foreground
                log::info!("Starting in foreground mode");
                supervisor::run_foreground(config.as_deref()).await?;
            }
        }
        Commands::Stop => {
            // Stop running process
            log::info!("Stopping running bal process");
            ProcessManager::stop_daemon()?;
        }
        Commands::Graceful => {
            // Zero-downtime config reload (send SIGHUP signal)
            log::info!("Reloading configuration gracefully");
            ProcessManager::send_reload_signal()?;
        }
        Commands::Check {
            config,
            strict,
            json,
            verbose,
        } => {
            log::info!("Running static config check");
            check::run_and_print(config, strict, json, verbose).await?;
        }
        Commands::Status {
            config,
            json,
            brief,
            verbose,
        } => {
            log::info!("Showing bal state status");
            ProcessManager::print_status(config, json, verbose && !brief).await?;
        }
        Commands::Doctor {
            config,
            json,
            brief,
            verbose,
        } => {
            log::info!("Running bal doctor diagnostics");
            doctor::run_and_print(config, json, verbose && !brief).await?;
        }
    }

    Ok(())
}

/// Application entry point
/// Parses CLI arguments and dispatches to appropriate subcommands.
fn main() -> Result<()> {
    // Parse CLI arguments first (before any potential fork)
    let cli = Cli::parse_args();

    // Determine if running in daemon mode
    let daemon_mode = matches!(cli.command, Commands::Start { daemon: true, .. });

    // Fork to background if daemon mode (BEFORE initializing tokio runtime)
    if daemon_mode {
        fork_daemon()?;
    }

    // Create tokio runtime manually after potential fork
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run_with_command(cli.command, daemon_mode))
}

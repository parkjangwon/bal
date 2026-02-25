//! Supervisor module
//!
//! Manages entire application lifecycle.
//! Coordinates signal handling, graceful shutdown, and configuration reload,
//! managing all background tasks.

use anyhow::{Result, Context};
use log::{info, warn, error, debug};
use std::path::Path;
use std::sync::Arc;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{timeout, Duration};

use crate::config_store::ConfigStore;
use crate::constants::GRACEFUL_SHUTDOWN_TIMEOUT_SECS;
use crate::health::HealthChecker;
use crate::process::PidFileGuard;
use crate::proxy::ProxyServer;
use crate::state::{AppState, RuntimeConfig};

/// Supervisor
/// 
/// Manages daemon process main loop, signal handling, and task orchestration.
pub struct Supervisor;

impl Supervisor {
    /// Run as daemon
    /// 
    /// 1. Create PID file
    /// 2. Load initial configuration
    /// 3. Register signal handlers
    /// 4. Start tasks (proxy, health checker)
    /// 5. Main loop (wait for signals/reload)
    pub async fn run_daemon(cli_config_path: Option<&Path>) -> Result<()> {
        // Create PID file (prevent duplicate execution)
        let _pid_guard = PidFileGuard::new()
            .context("Failed to create PID file - check if already running")?;
        
        info!("bal daemon starting (PID: {})", std::process::id());
        
        // Load initial configuration
        let (runtime_config, config_path) = ConfigStore::load_initial_config(cli_config_path).await?;
        
        info!("Configuration loaded: {}", config_path.display());
        info!("  - Listen port: {}", runtime_config.port);
        info!("  - Load balancing: {:?}", runtime_config.method);
        info!("  - Backends: {}", runtime_config.backend_pool.total_count());
        
        // Initialize app state
        let (shutdown_tx, _) = broadcast::channel(16);
        let (reload_tx, mut reload_rx) = mpsc::channel(4);
        
        let state = Arc::new(AppState::new(runtime_config, shutdown_tx, reload_tx));
        
        // Register signal handlers
        let mut sigterm = signal(SignalKind::terminate())
            .context("Failed to register SIGTERM handler")?;
        let mut sigint = signal(SignalKind::interrupt())
            .context("Failed to register SIGINT handler")?;
        let mut sighup = signal(SignalKind::hangup())
            .context("Failed to register SIGHUP handler")?;
        
        info!("Signal handlers registered (SIGTERM, SIGINT, SIGHUP)");
        
        // Start background tasks
        let proxy_state = Arc::clone(&state);
        let health_state = Arc::clone(&state);
        
        let mut proxy_shutdown = state.subscribe_shutdown();
        let health_shutdown = state.subscribe_shutdown();
        
        // Proxy server task
        let proxy_handle = tokio::spawn(async move {
            let proxy = ProxyServer::new(proxy_state);
            if let Err(e) = proxy.run(&mut proxy_shutdown).await {
                error!("Proxy server error: {}", e);
            }
        });
        
        // Health checker task
        let health_handle = tokio::spawn(async move {
            let checker = HealthChecker::new(health_state);
            if let Err(e) = checker.run(health_shutdown).await {
                error!("Health checker error: {}", e);
            }
        });
        
        info!("All service tasks started");
        
        // Main loop
        loop {
            tokio::select! {
                // SIGTERM (stop command)
                _ = sigterm.recv() => {
                    info!("SIGTERM received - starting graceful shutdown");
                    break;
                }
                
                // SIGINT (Ctrl+C)
                _ = sigint.recv() => {
                    info!("SIGINT received - starting graceful shutdown");
                    break;
                }
                
                // SIGHUP (graceful reload)
                _ = sighup.recv() => {
                    info!("SIGHUP received - reloading configuration");
                    if let Err(e) = ConfigStore::reload_config(&state, None).await {
                        error!("Configuration reload failed: {}", e);
                    }
                }
                
                // Reload channel (programmatic)
                Some(()) = reload_rx.recv() => {
                    info!("Reload request received");
                    if let Err(e) = ConfigStore::reload_config(&state, None).await {
                        error!("Configuration reload failed: {}", e);
                    }
                }
            }
        }
        
        // Graceful shutdown
        info!("Starting graceful shutdown...");
        Self::graceful_shutdown(
            state,
            proxy_handle,
            health_handle,
        ).await?;
        
        info!("bal daemon shutdown complete");
        Ok(())
    }
    
    /// Perform graceful shutdown
    /// 
    /// 1. Send shutdown signal to all background tasks
    /// 2. Wait for existing connections to complete (up to timeout)
    /// 3. Confirm task termination
    async fn graceful_shutdown(
        state: Arc<AppState>,
        proxy_handle: tokio::task::JoinHandle<()>,
        health_handle: tokio::task::JoinHandle<()>,
    ) -> Result<()> {
        // Broadcast shutdown signal
        info!("Sending shutdown signal to all services");
        state.trigger_shutdown();
        
        // Check active connections
        let active = state.active_connections().await;
        if active > 0 {
            info!("Waiting for {} active connections...", active);
        }
        
        // Wait for task termination with timeout
        let shutdown_result = timeout(
            Duration::from_secs(GRACEFUL_SHUTDOWN_TIMEOUT_SECS),
            async {
                // Wait for proxy server to stop (stop accepting new connections)
                if let Err(e) = proxy_handle.await {
                    error!("Proxy task termination error: {}", e);
                }
                
                // Wait for health checker to stop
                if let Err(e) = health_handle.await {
                    error!("Health check task termination error: {}", e);
                }
                
                // Additional wait for all existing connections to close
                loop {
                    let active = state.active_connections().await;
                    if active == 0 {
                        break;
                    }
                    debug!("{} active connections remaining...", active);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        ).await;
        
        match shutdown_result {
            Ok(()) => {
                info!("All connections closed successfully");
            }
            Err(_) => {
                warn!(
                    "Graceful shutdown timeout ({} seconds). Force stopping.",
                    GRACEFUL_SHUTDOWN_TIMEOUT_SECS
                );
            }
        }
        
        Ok(())
    }
}

/// Public API for main.rs
pub async fn run_daemon(cli_config_path: Option<&Path>) -> Result<()> {
    Supervisor::run_daemon(cli_config_path).await
}

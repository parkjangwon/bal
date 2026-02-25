//! Application state management module
//!
//! Centralizes management of application shared state.
//! Uses arc-swap for lock-free configuration reading and atomic swapping.

use log::{info, warn};
use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::RwLock;

use crate::backend_pool::BackendPool;
use crate::config::{BalanceMethod, RuntimeTuning};
use crate::load_balancer::LoadBalancer;
use crate::protection::ProtectionMode;

/// Runtime configuration
///
/// Contains configuration values that may change during runtime.
/// Atomically replaced via arc-swap.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Listen port
    pub port: u16,
    /// Load balancing method
    pub method: BalanceMethod,
    /// Bind address for listener
    pub bind_address: String,
    /// Runtime tuning knobs
    pub runtime_tuning: RuntimeTuning,
    /// Backend pool (shared via Arc)
    pub backend_pool: Arc<BackendPool>,
    /// Configuration file path
    pub config_path: PathBuf,
}

impl RuntimeConfig {
    /// Create RuntimeConfig from Config
    pub fn from_config(config: crate::config::Config, config_path: PathBuf) -> Self {
        let backend_pool = Arc::new(BackendPool::new(config.backends));

        Self {
            port: config.port,
            method: config.method,
            bind_address: config.bind_address,
            runtime_tuning: config.runtime,
            backend_pool,
            config_path,
        }
    }
}

/// Application global state
///
/// Manages state shared by all components.
/// Uses arc-swap for lock-free config reading and atomic replacement.
pub struct AppState {
    /// Current runtime configuration (hot-swappable via arc-swap)
    config: ArcSwap<RuntimeConfig>,
    /// Load balancer (shared across all connections)
    load_balancer: LoadBalancer,
    /// Graceful shutdown trigger
    shutdown: tokio::sync::broadcast::Sender<()>,
    /// Config reload trigger
    reload: tokio::sync::mpsc::Sender<()>,
    /// Current active connection count
    active_connections: Arc<RwLock<usize>>,
    /// Automatic protection mode state
    protection_mode: Arc<ProtectionMode>,
}

impl AppState {
    /// Create new application state
    ///
    /// Initializes with initial configuration and shutdown/reload channels.
    pub fn new(
        runtime_config: RuntimeConfig,
        shutdown: tokio::sync::broadcast::Sender<()>,
        reload: tokio::sync::mpsc::Sender<()>,
    ) -> Self {
        let load_balancer = LoadBalancer::new(
            runtime_config.method,
            Arc::clone(&runtime_config.backend_pool),
        );

        let protection_mode = Arc::new(ProtectionMode::new(
            runtime_config.runtime_tuning.protection_trigger_threshold,
            runtime_config.runtime_tuning.protection_window_ms,
            runtime_config
                .runtime_tuning
                .protection_stable_success_threshold,
        ));

        Self {
            config: ArcSwap::new(Arc::new(runtime_config)),
            load_balancer,
            shutdown,
            reload,
            active_connections: Arc::new(RwLock::new(0)),
            protection_mode,
        }
    }

    /// Read current configuration
    ///
    /// Lock-free read via arc-swap.
    pub fn config(&self) -> Arc<RuntimeConfig> {
        self.config.load().clone()
    }

    /// Replace configuration (hot-swap)
    ///
    /// Atomically replaces configuration. Does not affect existing connections.
    pub fn swap_config(&self, new_config: RuntimeConfig) {
        let old_port = self.config.load().port;
        let new_port = new_config.port;

        self.config.store(Arc::new(new_config));

        info!("Configuration swapped without downtime");

        if old_port != new_port {
            warn!(
                "Port change detected ({} -> {}). New port will apply on next restart.",
                old_port, new_port
            );
        }
    }

    /// Subscribe to shutdown signal
    ///
    /// Creates broadcast channel receiver for graceful shutdown.
    pub fn subscribe_shutdown(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.shutdown.subscribe()
    }

    /// Trigger shutdown
    ///
    /// Sends shutdown signal to all subscribers.
    pub fn trigger_shutdown(&self) {
        let _ = self.shutdown.send(());
    }

    /// Trigger configuration reload
    ///
    /// Requests configuration reload from supervisor.
    pub async fn trigger_reload(&self) -> anyhow::Result<()> {
        self.reload
            .send(())
            .await
            .map_err(|_| anyhow::anyhow!("Reload channel closed"))?;
        Ok(())
    }

    /// Get reload channel sender
    pub fn reload_receiver(&self) -> &tokio::sync::mpsc::Sender<()> {
        &self.reload
    }

    /// Try to acquire one connection slot up to max_concurrent limit
    pub async fn try_acquire_connection(&self, max_concurrent_connections: usize) -> bool {
        let mut guard = self.active_connections.write().await;
        if *guard >= max_concurrent_connections {
            return false;
        }
        *guard += 1;
        true
    }

    /// Release one active connection slot
    pub async fn release_connection(&self) {
        let mut guard = self.active_connections.write().await;
        if *guard > 0 {
            *guard -= 1;
        }
    }

    /// Get current active connection count
    pub async fn active_connections(&self) -> usize {
        *self.active_connections.read().await
    }

    /// Get backend pool reference
    pub fn backend_pool(&self) -> Arc<BackendPool> {
        Arc::clone(&self.config.load().backend_pool)
    }

    /// Get load balancer reference
    pub fn load_balancer(&self) -> &LoadBalancer {
        &self.load_balancer
    }

    pub fn protection_mode(&self) -> Arc<ProtectionMode> {
        Arc::clone(&self.protection_mode)
    }

    /// Get listen port
    pub fn port(&self) -> u16 {
        self.config.load().port
    }

    /// Get load balancing method
    pub fn method(&self) -> BalanceMethod {
        self.config.load().method
    }
}

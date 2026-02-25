//! Health check module
//!
//! Periodically monitors backend server status.
//! Determines backend status based on TCP connectivity, transitioning
//! state based on consecutive failures/successes.

use anyhow::Result;
use log::{debug, error, info};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::{interval, timeout};

use crate::constants::{
    HEALTH_CHECK_INTERVAL_MS, HEALTH_CHECK_MAX_RETRIES, HEALTH_CHECK_MIN_SUCCESS,
    HEALTH_CHECK_TIMEOUT_MS,
};
use crate::state::AppState;

/// Health check manager
///
/// Periodically checks all backend statuses and updates state.
pub struct HealthChecker {
    state: Arc<AppState>,
}

impl HealthChecker {
    /// Create new health checker
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Run health check loop
    ///
    /// Periodically checks all backends, logging state changes.
    /// Exits loop on shutdown signal.
    pub async fn run(&self, mut shutdown: tokio::sync::broadcast::Receiver<()>) -> Result<()> {
        let mut ticker = interval(Duration::from_millis(HEALTH_CHECK_INTERVAL_MS));

        info!(
            "Health check started: {}ms interval, {}ms timeout",
            HEALTH_CHECK_INTERVAL_MS, HEALTH_CHECK_TIMEOUT_MS
        );

        // First check runs immediately
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.check_all_backends().await {
                        error!("Health check error: {}", e);
                    }
                }
                _ = shutdown.recv() => {
                    info!("Health check received shutdown signal");
                    break;
                }
            }
        }

        info!("Health check stopped");
        Ok(())
    }

    /// Check all backends
    async fn check_all_backends(&self) -> Result<()> {
        let config = self.state.config();
        let pool = &config.backend_pool;

        // Check each backend in parallel
        let mut handles = vec![];

        for backend in pool.all_backends() {
            let backend = Arc::clone(backend);
            let handle = tokio::spawn(async move {
                let addr = match backend.config.to_health_check_addr().await {
                    Ok(a) => a,
                    Err(e) => {
                        error!("Backend address conversion failed: {}", e);
                        return;
                    }
                };

                debug!(
                    "Health check: {}:{}",
                    backend.config.host, backend.config.port
                );

                // TCP connection test
                let result = timeout(
                    Duration::from_millis(HEALTH_CHECK_TIMEOUT_MS),
                    TcpStream::connect(&addr),
                )
                .await;

                match result {
                    Ok(Ok(_)) => {
                        // Connection success
                        backend.mark_success(HEALTH_CHECK_MIN_SUCCESS);
                    }
                    Ok(Err(e)) => {
                        // Connection failure
                        debug!(
                            "Backend {}:{} connection failed: {}",
                            backend.config.host, backend.config.port, e
                        );
                        backend.mark_failure(HEALTH_CHECK_MAX_RETRIES);
                    }
                    Err(_) => {
                        // Timeout
                        debug!(
                            "Backend {}:{} timeout",
                            backend.config.host, backend.config.port
                        );
                        backend.mark_failure(HEALTH_CHECK_MAX_RETRIES);
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all checks to complete
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Health check task error: {}", e);
            }
        }

        // Log overall status periodically
        pool.log_status();

        Ok(())
    }

    /// Single backend health check (for external use)
    pub async fn check_single_backend(host: &str, port: u16) -> Result<bool> {
        let addr = format!("{}:{}", host, port);

        match timeout(
            Duration::from_millis(HEALTH_CHECK_TIMEOUT_MS),
            TcpStream::connect(&addr),
        )
        .await
        {
            Ok(Ok(_)) => Ok(true),
            _ => Ok(false),
        }
    }
}

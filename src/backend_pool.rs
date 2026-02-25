//! Backend pool management module
//!
//! Manages runtime state of backend servers.
//! Tracks each backend's health status, active connection count, and consecutive
//! failure count, sharing state in a thread-safe manner.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::config::BackendConfig;

/// Backend server runtime state
///
/// Uses Atomic types for lock-free thread-safe state sharing.
/// Uses Ordering::Relaxed for performance optimization. (Only single Atomic
/// operation consistency is needed)
#[derive(Debug)]
pub struct BackendState {
    /// Backend configuration (immutable)
    pub config: BackendConfig,
    /// Health check status - true means healthy, false means unhealthy
    healthy: AtomicBool,
    /// Current active connection count
    active_connections: AtomicUsize,
    /// Consecutive health check failure count
    consecutive_failures: AtomicU32,
    /// Consecutive health check success count
    consecutive_successes: AtomicU32,
}

impl BackendState {
    /// Create new backend state
    pub fn new(config: BackendConfig) -> Self {
        Self {
            config,
            // Initially considered healthy (until health checks start)
            healthy: AtomicBool::new(true),
            active_connections: AtomicUsize::new(0),
            consecutive_failures: AtomicU32::new(0),
            consecutive_successes: AtomicU32::new(0),
        }
    }

    /// Get health status
    #[inline]
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    /// Set health status
    #[inline]
    pub fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::Relaxed);
    }

    /// Get active connection count
    #[inline]
    pub fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Increment active connection count
    ///
    /// Called when a new client connection connects to the backend.
    #[inline]
    pub fn increment_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active connection count
    ///
    /// Called when a client connection terminates.
    #[inline]
    pub fn decrement_connections(&self) {
        // Use saturating_sub to prevent underflow
        let prev = self.active_connections.fetch_sub(1, Ordering::Relaxed);
        if prev == 0 {
            // Reset to 0 to prevent underflow
            self.active_connections.store(0, Ordering::Relaxed);
        }
    }

    /// Get consecutive failure count
    #[inline]
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }

    /// Increment consecutive failure count
    ///
    /// Called on health check failure, transitions to unhealthy if threshold exceeded.
    #[inline]
    pub fn increment_failures(&self) {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        self.consecutive_successes.store(0, Ordering::Relaxed);
    }

    /// Increment consecutive success count
    ///
    /// Called on health check success, recovers to healthy if threshold exceeded.
    #[inline]
    pub fn increment_successes(&self) {
        self.consecutive_successes.fetch_add(1, Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Get consecutive success count
    #[inline]
    pub fn consecutive_successes(&self) -> u32 {
        self.consecutive_successes.load(Ordering::Relaxed)
    }

    /// Handle health check failure
    ///
    /// Transitions to unhealthy state if failures exceed max_failures.
    pub fn mark_failure(&self, max_failures: u32) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        self.consecutive_successes.store(0, Ordering::Relaxed);

        if failures >= max_failures {
            let was_healthy = self.healthy.swap(false, Ordering::Relaxed);
            if was_healthy {
                log::warn!(
                    "Backend {}:{} marked as unhealthy ({} consecutive failures)",
                    self.config.host,
                    self.config.port,
                    failures
                );
            }
        }
    }

    /// Handle health check success
    ///
    /// Recovers to healthy state if successes exceed min_successes.
    pub fn mark_success(&self, min_successes: u32) {
        let successes = self.consecutive_successes.fetch_add(1, Ordering::Relaxed) + 1;
        self.consecutive_failures.store(0, Ordering::Relaxed);

        if successes >= min_successes && !self.is_healthy() {
            self.healthy.store(true, Ordering::Relaxed);
            log::info!(
                "Backend {}:{} recovered to healthy ({} consecutive successes)",
                self.config.host,
                self.config.port,
                successes
            );
        }
    }

    /// Get backend address string (host:port format)
    pub fn address(&self) -> String {
        format!("{}:{}", self.config.host, self.config.port)
    }
}

/// Active connection counter RAII guard
///
/// Increments when backend connection is created, automatically decrements
/// when connection closes (on Drop). This pattern prevents connection count leaks.
pub struct ConnectionGuard {
    backend: Arc<BackendState>,
}

impl ConnectionGuard {
    pub fn new(backend: Arc<BackendState>) -> Self {
        backend.increment_connections();
        Self { backend }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.backend.decrement_connections();
    }
}

/// Backend pool
///
/// Manages all backend states and provides list of healthy backends.
#[derive(Debug)]
pub struct BackendPool {
    /// List of all backend states
    backends: Vec<Arc<BackendState>>,
}

impl BackendPool {
    /// Create new backend pool
    pub fn new(configs: Vec<BackendConfig>) -> Self {
        let backends = configs
            .into_iter()
            .map(|config| Arc::new(BackendState::new(config)))
            .collect();

        Self { backends }
    }

    /// Get all backend states
    pub fn all_backends(&self) -> &[Arc<BackendState>] {
        &self.backends
    }

    /// Get list of healthy backends
    ///
    /// Returns only backends that passed health checks.
    pub fn healthy_backends(&self) -> Vec<Arc<BackendState>> {
        self.backends
            .iter()
            .filter(|b| b.is_healthy())
            .cloned()
            .collect()
    }

    /// Get count of healthy backends
    pub fn healthy_count(&self) -> usize {
        self.backends.iter().filter(|b| b.is_healthy()).count()
    }

    /// Get total backend count
    pub fn total_count(&self) -> usize {
        self.backends.len()
    }

    /// Find specific backend (by host:port)
    pub fn find_backend(&self, host: &str, port: u16) -> Option<Arc<BackendState>> {
        self.backends
            .iter()
            .find(|b| b.config.host == host && b.config.port == port)
            .cloned()
    }

    /// Log pool status summary
    pub fn log_status(&self) {
        let total = self.total_count();
        let healthy = self.healthy_count();
        log::debug!("Backend pool status: {}/{} healthy", healthy, total);

        for backend in &self.backends {
            let status = if backend.is_healthy() {
                "healthy"
            } else {
                "unhealthy"
            };
            let conn = backend.active_connections();
            log::debug!(
                "  - {}:{} [{}] (connections: {})",
                backend.config.host,
                backend.config.port,
                status,
                conn
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_backend(host: &str, port: u16) -> BackendConfig {
        BackendConfig {
            host: host.to_string(),
            port,
        }
    }

    #[test]
    fn test_backend_state_healthy() {
        let config = create_test_backend("127.0.0.1", 8080);
        let state = BackendState::new(config);

        assert!(state.is_healthy());

        state.set_healthy(false);
        assert!(!state.is_healthy());
    }

    #[test]
    fn test_connection_counting() {
        let config = create_test_backend("127.0.0.1", 8080);
        let state = Arc::new(BackendState::new(config));

        assert_eq!(state.active_connections(), 0);

        {
            let _guard = ConnectionGuard::new(Arc::clone(&state));
            assert_eq!(state.active_connections(), 1);

            {
                let _guard2 = ConnectionGuard::new(Arc::clone(&state));
                assert_eq!(state.active_connections(), 2);
            }

            assert_eq!(state.active_connections(), 1);
        }

        assert_eq!(state.active_connections(), 0);
    }

    #[test]
    fn test_failure_tracking() {
        let config = create_test_backend("127.0.0.1", 8080);
        let state = BackendState::new(config);

        // Transition to unhealthy after 3 consecutive failures
        state.mark_failure(3);
        assert!(state.is_healthy()); // Still healthy

        state.mark_failure(3);
        assert!(state.is_healthy()); // Still healthy

        state.mark_failure(3);
        assert!(!state.is_healthy()); // Unhealthy transition

        // 2 successes don't recover
        state.mark_success(3);
        assert!(!state.is_healthy());

        state.mark_success(3);
        assert!(!state.is_healthy());

        state.mark_success(3);
        assert!(state.is_healthy()); // Recovered
    }
}

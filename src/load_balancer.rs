//! Load balancer module
//!
//! Implements load balancing algorithms.
//! Currently supports Round Robin algorithm, designed to allow adding
//! Least Connections and others in the future.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::backend_pool::{BackendPool, BackendState};
use crate::config::BalanceMethod;

/// Load balancer
/// 
/// Responsible for selecting backends to distribute traffic to.
/// Performs backend selection in a thread-safe manner.
pub struct LoadBalancer {
    /// Load balancing algorithm to use
    method: BalanceMethod,
    /// Reference to backend pool
    pool: Arc<BackendPool>,
    /// Round robin index (atomic increment)
    rr_index: AtomicUsize,
}

impl LoadBalancer {
    /// Create new load balancer
    /// 
    /// # Arguments
    /// * `method` - Load balancing algorithm to use
    /// * `pool` - Backend pool (shared via Arc)
    pub fn new(method: BalanceMethod, pool: Arc<BackendPool>) -> Self {
        Self {
            method,
            pool,
            rr_index: AtomicUsize::new(0),
        }
    }
    
    /// Select backend
    /// 
    /// Selects appropriate backend based on configured algorithm.
    /// Returns None if no healthy backends are available.
    /// 
    /// # Returns
    /// * `Some(Arc<BackendState>)` - Selected backend state
    /// * `None` - No available backends
    pub fn select_backend(&self) -> Option<Arc<BackendState>> {
        let healthy_backends = self.pool.healthy_backends();
        
        if healthy_backends.is_empty() {
            log::warn!("No healthy backends available");
            return None;
        }
        
        match self.method {
            BalanceMethod::RoundRobin => self.select_round_robin(&healthy_backends),
            BalanceMethod::LeastConnections => self.select_least_connections(&healthy_backends),
        }
    }
    
    /// Round robin backend selection
    /// 
    /// Selects next backend sequentially.
    /// Uses atomic index increment for lock-free thread-safe operation.
    fn select_round_robin(&self, backends: &[Arc<BackendState>]) -> Option<Arc<BackendState>> {
        // Atomically increment index and get previous value
        let index = self.rr_index.fetch_add(1, Ordering::Relaxed);
        
        // Cycle through using modulo
        let selected = &backends[index % backends.len()];
        
        log::debug!(
            "Round robin selection: {}:{} (index: {})",
            selected.config.host,
            selected.config.port,
            index % backends.len()
        );
        
        Some(Arc::clone(selected))
    }
    
    /// Least connections backend selection
    /// 
    /// Selects backend with fewest active connections.
    /// If tie, selects first backend.
    fn select_least_connections(&self, backends: &[Arc<BackendState>]) -> Option<Arc<BackendState>> {
        backends
            .iter()
            .min_by_key(|b| b.active_connections())
            .cloned()
    }
    
    /// Get load balancing method
    pub fn method(&self) -> BalanceMethod {
        self.method
    }
    
    /// Get backend pool reference
    pub fn pool(&self) -> &Arc<BackendPool> {
        &self.pool
    }
    
    /// Get current round robin index (for testing)
    #[cfg(test)]
    pub fn current_index(&self) -> usize {
        self.rr_index.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend_pool::BackendPool;
    use crate::config::BackendConfig;
    
    fn create_test_pool() -> Arc<BackendPool> {
        let configs = vec![
            BackendConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                weight: 1,
                health_check_port: None,
            },
            BackendConfig {
                host: "127.0.0.1".to_string(),
                port: 8081,
                weight: 1,
                health_check_port: None,
            },
            BackendConfig {
                host: "127.0.0.1".to_string(),
                port: 8082,
                weight: 1,
                health_check_port: None,
            },
        ];
        
        Arc::new(BackendPool::new(configs))
    }
    
    #[test]
    fn test_round_robin_selection() {
        let pool = create_test_pool();
        let lb = LoadBalancer::new(BalanceMethod::RoundRobin, Arc::clone(&pool));
        
        // Sequential selections should cycle
        let backend1 = lb.select_backend().unwrap();
        let backend2 = lb.select_backend().unwrap();
        let backend3 = lb.select_backend().unwrap();
        let backend4 = lb.select_backend().unwrap(); // Cycle
        
        // First and fourth should be same (3 backends cycle)
        assert_eq!(backend1.config.port, backend4.config.port);
        
        // All selections should be healthy
        assert!(backend1.is_healthy());
        assert!(backend2.is_healthy());
        assert!(backend3.is_healthy());
    }
    
    #[test]
    fn test_least_connections_selection() {
        let pool = create_test_pool();
        let lb = LoadBalancer::new(BalanceMethod::LeastConnections, Arc::clone(&pool));
        
        // Add connections to first backend
        let backends = pool.all_backends();
        backends[0].increment_connections();
        backends[0].increment_connections();
        
        // Select least connections backend
        let selected = lb.select_backend().unwrap();
        
        // Should select different backend with fewer connections
        assert_ne!(selected.config.port, 8080);
    }
    
    #[test]
    fn test_no_healthy_backend() {
        // Set all backends as unhealthy
        let pool = create_test_pool();
        for backend in pool.all_backends() {
            backend.set_healthy(false);
        }
        
        let lb = LoadBalancer::new(BalanceMethod::RoundRobin, pool);
        
        // Should not be able to select any backend
        assert!(lb.select_backend().is_none());
    }
}

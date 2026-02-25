//! TCP proxy module
//!
//! Proxies client connections to backend servers.
//! Uses tokio::io::copy_bidirectional for efficient bidirectional data transfer.

use anyhow::{Result, Context, bail};
use log::{info, debug, error, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::backend_pool::{ConnectionGuard, BackendState};
use crate::config::BackendConfig;
use crate::constants::BACKEND_CONNECT_TIMEOUT_SECS;
use crate::load_balancer::LoadBalancer;
use crate::state::AppState;

/// Proxy server
/// 
/// Accepts client connections and proxies them to backends.
pub struct ProxyServer {
    state: Arc<AppState>,
}

impl ProxyServer {
    /// Create new proxy server
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
    
    /// Run proxy server
    /// 
    /// Accepts client connections on specified port and handles each
    /// connection asynchronously. Stops accepting new connections on
    /// graceful shutdown signal.
    pub async fn run(&self, shutdown: &mut tokio::sync::broadcast::Receiver<()>) -> Result<()> {
        let port = self.state.port();
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        
        // Create TCP listener
        let listener = TcpListener::bind(&addr).await
            .with_context(|| format!("Failed to bind to port {}", port))?;
        
        info!("Proxy server started: {} (L4 Passthrough mode)", addr);
        
        loop {
            tokio::select! {
                // Accept new client connection
                result = listener.accept() => {
                    match result {
                        Ok((client_stream, client_addr)) => {
                            debug!("Client connection accepted: {}", client_addr);
                            
                            // Handle each connection in async task
                            let state = Arc::clone(&self.state);
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(client_stream, client_addr, state).await {
                                    error!("Proxy connection handling failed ({}): {}", client_addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Client connection accept failed: {}", e);
                        }
                    }
                }
                
                // Receive graceful shutdown signal
                _ = shutdown.recv() => {
                    info!("Proxy server received shutdown signal. Stopping new connection acceptance.");
                    break;
                }
            }
        }
        
        info!("Proxy server stopped");
        Ok(())
    }
}

/// Handle individual client connection
/// 
/// 1. Select backend with retry logic
/// 2. Connect to backend (retry on failure)
/// 3. Bidirectional data relay
async fn handle_connection(
    client_stream: TcpStream,
    client_addr: SocketAddr,
    state: Arc<AppState>,
) -> Result<()> {
    // Increment active connection count
    state.increment_connections().await;
    
    // Try to connect to a backend with retry logic
    let (backend, backend_stream, backend_addr) = 
        match connect_with_retry(&state, &client_addr).await {
            Ok(result) => result,
            Err(e) => {
                state.decrement_connections().await;
                return Err(e);
            }
        };
    
    // Backend connection success - increment active connection count
    backend.increment_connections();
    let _connection_guard = ConnectionGuard::new(Arc::clone(&backend));
    
    info!(
        "Proxy connection established: {} <-> {} (backend: {}:{})",
        client_addr, backend_addr, backend.config.host, backend.config.port
    );
    
    // Bidirectional data copy (L4 Passthrough)
    match relay_streams(client_stream, backend_stream).await {
        Ok((client_to_backend, backend_to_client)) => {
            debug!(
                "Proxy connection closed: {}. Transfer: client->backend {} bytes, backend->client {} bytes",
                client_addr, client_to_backend, backend_to_client
            );
        }
        Err(e) => {
            warn!("Proxy relay error ({}): {}", client_addr, e);
        }
    }
    
    // Decrement active connection count
    state.decrement_connections().await;
    
    Ok(())
}

/// Connect to backend with retry logic
/// 
/// If the first selected backend fails, immediately try the next one.
/// This ensures failover happens within milliseconds, not seconds.
async fn connect_with_retry(
    state: &Arc<AppState>,
    client_addr: &SocketAddr,
) -> Result<(Arc<BackendState>, TcpStream, SocketAddr)> {
    let load_balancer = state.load_balancer();
    let pool = &state.backend_pool();
    
    // Get list of healthy backends
    let healthy_backends = pool.healthy_backends();
    
    if healthy_backends.is_empty() {
        bail!("No healthy backends available");
    }
    
    // Try each backend in order until one succeeds
    let mut last_error = None;
    let max_attempts = healthy_backends.len();
    
    for attempt in 1..=max_attempts {
        // Select next backend using round robin
        let backend = match load_balancer.select_backend() {
            Some(b) => b,
            None => {
                bail!("No healthy backends available during retry");
            }
        };
        
        let backend_addr = match backend.config.to_socket_addr() {
            Ok(addr) => addr,
            Err(e) => {
                warn!("Invalid backend address: {}", e);
                continue;
            }
        };
        
        debug!("Connection attempt {}: {} -> {}", attempt, client_addr, backend_addr);
        
        // Try to connect with short timeout for quick failover
        match timeout(
            Duration::from_secs(2), // Shorter timeout for quick failover
            TcpStream::connect(&backend_addr)
        ).await {
            Ok(Ok(stream)) => {
                // Success!
                if attempt > 1 {
                    info!(
                        "Failover successful: {} -> {} (after {} attempts)",
                        client_addr, backend_addr, attempt
                    );
                }
                return Ok((backend, stream, backend_addr));
            }
            Ok(Err(e)) => {
                warn!(
                    "Backend {}:{} connection failed (attempt {}): {}",
                    backend.config.host, backend.config.port, attempt, e
                );
                // Mark backend as unhealthy immediately
                backend.mark_failure(1); // 1 failure = immediately mark unhealthy
                last_error = Some(format!("Connection refused: {}", e));
            }
            Err(_) => {
                warn!(
                    "Backend {}:{} connection timeout (attempt {})",
                    backend.config.host, backend.config.port, attempt
                );
                // Mark backend as unhealthy immediately
                backend.mark_failure(1);
                last_error = Some("Connection timeout".to_string());
            }
        }
    }
    
    // All backends failed
    bail!(
        "All {} backends failed. Last error: {}",
        max_attempts,
        last_error.unwrap_or_else(|| "Unknown error".to_string())
    );
}

/// Bidirectional stream relay
/// 
/// Uses tokio::io::copy_bidirectional for efficient bidirectional data
/// transfer between client and backend.
/// 
/// Uses kernel-level zero-copy for high performance.
async fn relay_streams(
    mut client: TcpStream,
    mut backend: TcpStream,
) -> Result<(u64, u64)> {
    // Perform bidirectional copy
    let (client_to_backend, backend_to_client) = io::copy_bidirectional(&mut client, &mut backend).await
        .context("Bidirectional data relay failed")?;
    
    Ok((client_to_backend, backend_to_client))
}

/// Test backend connection
/// 
/// Attempts TCP connection to backend within configured timeout.
pub async fn test_backend_connection(config: &BackendConfig) -> Result<()> {
    let addr = config.to_socket_addr()?;
    
    match timeout(
        Duration::from_secs(1),
        TcpStream::connect(&addr)
    ).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => bail!("Connection failed: {}", e),
        Err(_) => bail!("Connection timeout"),
    }
}

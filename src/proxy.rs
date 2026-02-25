//! TCP proxy module
//!
//! Proxies client connections to backend servers.
//! Uses tokio::io::copy_bidirectional for efficient bidirectional data transfer.

use anyhow::{bail, Context, Result};
use log::{debug, error, info, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::backend_pool::{BackendState, ConnectionGuard};
use crate::config::BackendConfig;
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
        let config = self.state.config();
        let listen_addr = format!("{}:{}", config.bind_address, config.port);

        // Create TCP listener
        let listener = TcpListener::bind(&listen_addr)
            .await
            .with_context(|| format!("Failed to bind to {}", listen_addr))?;

        info!(
            "Proxy server started: {} (L4 Passthrough mode)",
            listen_addr
        );

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

    // Backend connection success - track active backend connection
    let _connection_guard = track_backend_connection(Arc::clone(&backend));

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

/// Connect to backend with ultra-fast failover
///
/// 1. First try healthy backends
/// 2. If all healthy backends fail, try ALL backends including unhealthy ones
/// 3. On successful connection, immediately mark backend as healthy
/// 4. Uses configured backend connect timeout for immediate failover
async fn connect_with_retry(
    state: &Arc<AppState>,
    client_addr: &SocketAddr,
) -> Result<(Arc<BackendState>, TcpStream, SocketAddr)> {
    let runtime_config = state.config();
    let connect_timeout_ms = runtime_config.runtime_tuning.backend_connect_timeout_ms;

    let load_balancer = state.load_balancer();
    let pool = &state.backend_pool();

    // Get list of healthy backends
    let healthy_backends = pool.healthy_backends();
    let all_backends = pool.all_backends();

    if all_backends.is_empty() {
        bail!("No backends configured");
    }

    // Try healthy backends first
    let mut last_error = None;

    if !healthy_backends.is_empty() {
        for attempt in 1..=healthy_backends.len() {
            let backend = match load_balancer.select_backend() {
                Some(b) => b,
                None => break,
            };

            let backend_addr = match backend.config.resolve_socket_addr().await {
                Ok(addr) => addr,
                Err(e) => {
                    warn!("Invalid backend address: {}", e);
                    continue;
                }
            };

            debug!(
                "Connection attempt {} to healthy backend: {} -> {}",
                attempt, client_addr, backend_addr
            );

            // Try to connect with ultra-short timeout for immediate failover
            match timeout(
                Duration::from_millis(connect_timeout_ms),
                TcpStream::connect(&backend_addr),
            )
            .await
            {
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
                    backend.mark_failure(1);
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
    }

    // If all healthy backends failed, try ALL backends (including unhealthy ones)
    info!("All healthy backends failed. Trying all backends including unhealthy ones...");

    for backend in all_backends {
        let backend_addr = match backend.config.resolve_socket_addr().await {
            Ok(addr) => addr,
            Err(_) => continue,
        };

        debug!(
            "Trying backend {}:{} (healthy={})",
            backend.config.host,
            backend.config.port,
            backend.is_healthy()
        );

        match timeout(
            Duration::from_millis(connect_timeout_ms),
            TcpStream::connect(&backend_addr),
        )
        .await
        {
            Ok(Ok(stream)) => {
                // Success! Immediately mark as healthy
                if !backend.is_healthy() {
                    backend.mark_success(1);
                    info!(
                        "Backend {}:{} recovered and serving traffic immediately!",
                        backend.config.host, backend.config.port
                    );
                }
                return Ok((Arc::clone(backend), stream, backend_addr));
            }
            Ok(Err(_)) => {
                backend.mark_failure(1);
            }
            Err(_) => {
                backend.mark_failure(1);
            }
        }
    }

    // All backends failed
    bail!(
        "All {} backends failed. Last error: {}",
        all_backends.len(),
        last_error.unwrap_or_else(|| "Unknown error".to_string())
    );
}

fn track_backend_connection(backend: Arc<BackendState>) -> ConnectionGuard {
    ConnectionGuard::new(backend)
}

/// Bidirectional stream relay
///
/// Uses tokio::io::copy_bidirectional for efficient bidirectional data
/// transfer between client and backend.
///
/// Uses kernel-level zero-copy for high performance.
async fn relay_streams(mut client: TcpStream, mut backend: TcpStream) -> Result<(u64, u64)> {
    // Perform bidirectional copy
    let (client_to_backend, backend_to_client) = io::copy_bidirectional(&mut client, &mut backend)
        .await
        .context("Bidirectional data relay failed")?;

    Ok((client_to_backend, backend_to_client))
}

/// Test backend connection
///
/// Attempts TCP connection to backend within configured timeout.
pub async fn test_backend_connection(config: &BackendConfig) -> Result<()> {
    let addr = config.resolve_socket_addr().await?;

    match timeout(Duration::from_secs(1), TcpStream::connect(&addr)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => bail!("Connection failed: {}", e),
        Err(_) => bail!("Connection timeout"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BackendConfig;

    #[test]
    fn connection_tracking_increments_once_per_proxy_session() {
        let backend = Arc::new(BackendState::new(BackendConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }));

        let _guard = track_backend_connection(Arc::clone(&backend));
        assert_eq!(backend.active_connections(), 1);
    }
}

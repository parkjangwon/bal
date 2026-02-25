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
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::time::timeout;

use crate::backend_pool::{BackendErrorKind, BackendState, ConnectionGuard};
use crate::config::{BackendConfig, OverloadPolicy};
use crate::protection;
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
        let listener = if let Some(backlog) = config.runtime_tuning.tcp_backlog {
            let socket_addr: std::net::SocketAddr = listen_addr
                .parse()
                .with_context(|| format!("Invalid listen address {}", listen_addr))?;
            let socket = if socket_addr.is_ipv4() {
                TcpSocket::new_v4().context("Failed to create IPv4 listener socket")?
            } else {
                TcpSocket::new_v6().context("Failed to create IPv6 listener socket")?
            };
            socket
                .bind(socket_addr)
                .with_context(|| format!("Failed to bind to {}", listen_addr))?;
            socket
                .listen(backlog)
                .with_context(|| format!("Failed to listen on {}", listen_addr))?
        } else {
            TcpListener::bind(&listen_addr)
                .await
                .with_context(|| format!("Failed to bind to {}", listen_addr))?
        };

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
    // Increment active connection count with overload protection
    let runtime_config = state.config();
    if !state
        .try_acquire_connection(runtime_config.runtime_tuning.max_concurrent_connections)
        .await
    {
        match runtime_config.runtime_tuning.overload_policy {
            OverloadPolicy::Reject => {
                warn!(
                    "Rejecting client {} due to overload (max_concurrent_connections={})",
                    client_addr, runtime_config.runtime_tuning.max_concurrent_connections
                );
                return Ok(());
            }
        }
    }

    // Try to connect to a backend with retry logic
    let (backend, backend_stream, backend_addr) =
        match connect_with_retry(&state, &client_addr).await {
            Ok(result) => result,
            Err(e) => {
                state.release_connection().await;
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
    match relay_streams(
        client_stream,
        backend_stream,
        runtime_config.runtime_tuning.connection_idle_timeout_ms,
    )
    .await
    {
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
    state.release_connection().await;

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
    let fail_threshold = runtime_config.runtime_tuning.health_check_fail_threshold;
    let success_threshold = runtime_config.runtime_tuning.health_check_success_threshold;
    let mut backoff_initial_ms = runtime_config.runtime_tuning.failover_backoff_initial_ms;
    let mut backoff_max_ms = runtime_config.runtime_tuning.failover_backoff_max_ms;
    let mut cooldown_ms = runtime_config.runtime_tuning.backend_cooldown_ms;
    let protection_mode = state.protection_mode();

    if protection_mode.is_enabled() {
        backoff_initial_ms = backoff_initial_ms.saturating_mul(2);
        backoff_max_ms = backoff_max_ms.saturating_mul(2);
        cooldown_ms = cooldown_ms.saturating_mul(2);
    }

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

            if backend.is_in_cooldown() {
                debug!(
                    "Backend {}:{} is in cooldown until {}",
                    backend.config.host,
                    backend.config.port,
                    backend.cooldown_until_ms()
                );
                continue;
            }

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
                    backend.mark_connect_success(success_threshold);
                    if protection_mode.record_success() {
                        protection::write_snapshot(&protection_mode.snapshot());
                    }
                    return Ok((backend, stream, backend_addr));
                }
                Ok(Err(e)) => {
                    warn!(
                        "Backend {}:{} connection failed (attempt {}): {}",
                        backend.config.host, backend.config.port, attempt, e
                    );
                    let kind = classify_connect_error(&e);
                    backend.mark_connect_failure(
                        kind,
                        fail_threshold,
                        backoff_initial_ms,
                        backoff_max_ms,
                        cooldown_ms,
                    );
                    if protection_mode.record_failure(kind) {
                        protection::write_snapshot(&protection_mode.snapshot());
                    }
                    last_error = Some(format!("Connection failed: {}", e));
                }
                Err(_) => {
                    warn!(
                        "Backend {}:{} connection timeout (attempt {})",
                        backend.config.host, backend.config.port, attempt
                    );
                    backend.mark_connect_failure(
                        BackendErrorKind::Timeout,
                        fail_threshold,
                        backoff_initial_ms,
                        backoff_max_ms,
                        cooldown_ms,
                    );
                    if protection_mode.record_failure(BackendErrorKind::Timeout) {
                        protection::write_snapshot(&protection_mode.snapshot());
                    }
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

        if backend.is_in_cooldown() {
            debug!(
                "Skipping backend {}:{} due to cooldown",
                backend.config.host, backend.config.port
            );
            continue;
        }

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
                let was_healthy = backend.is_healthy();
                backend.mark_connect_success(success_threshold);
                if protection_mode.record_success() {
                    protection::write_snapshot(&protection_mode.snapshot());
                }
                if !was_healthy {
                    info!(
                        "Backend {}:{} recovered and serving traffic immediately!",
                        backend.config.host, backend.config.port
                    );
                }
                return Ok((Arc::clone(backend), stream, backend_addr));
            }
            Ok(Err(e)) => {
                let kind = classify_connect_error(&e);
                backend.mark_connect_failure(
                    kind,
                    fail_threshold,
                    backoff_initial_ms,
                    backoff_max_ms,
                    cooldown_ms,
                );
                if protection_mode.record_failure(kind) {
                    protection::write_snapshot(&protection_mode.snapshot());
                }
            }
            Err(_) => {
                backend.mark_connect_failure(
                    BackendErrorKind::Timeout,
                    fail_threshold,
                    backoff_initial_ms,
                    backoff_max_ms,
                    cooldown_ms,
                );
                if protection_mode.record_failure(BackendErrorKind::Timeout) {
                    protection::write_snapshot(&protection_mode.snapshot());
                }
            }
        }
    }

    // All backends failed
    if protection_mode.record_global_unavailable() {
        protection::write_snapshot(&protection_mode.snapshot());
    }

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
async fn relay_streams(
    mut client: TcpStream,
    mut backend: TcpStream,
    idle_timeout_ms: u64,
) -> Result<(u64, u64)> {
    let relay = io::copy_bidirectional(&mut client, &mut backend);
    let (client_to_backend, backend_to_client) =
        timeout(Duration::from_millis(idle_timeout_ms), relay)
            .await
            .context("Connection idle timeout reached")?
            .context("Bidirectional data relay failed")?;

    Ok((client_to_backend, backend_to_client))
}

fn classify_connect_error(err: &std::io::Error) -> BackendErrorKind {
    if err.kind() == std::io::ErrorKind::TimedOut {
        return BackendErrorKind::Timeout;
    }

    if err.kind() == std::io::ErrorKind::ConnectionRefused {
        return BackendErrorKind::ConnectionRefused;
    }

    match err.raw_os_error() {
        Some(61) | Some(111) => BackendErrorKind::ConnectionRefused,
        _ => BackendErrorKind::Other,
    }
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

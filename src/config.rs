//! Configuration file management module
//!
//! Handles YAML configuration file parsing, validation, and default values.
//! Uses Serde to declaratively define configuration structure with
//! strong validation.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::net::{lookup_host, TcpStream};

use crate::constants::{get_home_config_path, get_system_config_path, DEFAULT_PORT};

/// Load balancing algorithm types
///
/// Currently only Round Robin is implemented. Defined as enum for future extensions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BalanceMethod {
    /// Round Robin: Select backends sequentially
    RoundRobin,
    /// Least Connections: Select backend with fewest active connections (future implementation)
    #[serde(skip)]
    LeastConnections,
}

impl Default for BalanceMethod {
    fn default() -> Self {
        BalanceMethod::RoundRobin
    }
}

impl std::fmt::Display for BalanceMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BalanceMethod::RoundRobin => write!(f, "round_robin"),
            BalanceMethod::LeastConnections => write!(f, "least_connections"),
        }
    }
}

/// Individual backend server configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendConfig {
    /// Backend host (IP address or hostname)
    pub host: String,

    /// Backend port number
    pub port: u16,
}

impl BackendConfig {
    /// Convert to SocketAddr for TCP connection.
    ///
    /// This method validates literal IP:port input.
    /// For hostname support, use `resolve_socket_addr` in async contexts.
    pub fn to_socket_addr(&self) -> Result<SocketAddr> {
        let addr_str = format!("{}:{}", self.host, self.port);
        addr_str
            .parse()
            .with_context(|| format!("Invalid backend address: {}", addr_str))
    }

    /// Resolve backend host to a concrete socket address.
    ///
    /// Supports both literal IPs and DNS hostnames.
    pub async fn resolve_socket_addr(&self) -> Result<SocketAddr> {
        let host_port = format!("{}:{}", self.host, self.port);
        let mut addrs = lookup_host(&host_port)
            .await
            .with_context(|| format!("Failed to resolve backend address: {}", host_port))?;

        addrs
            .next()
            .with_context(|| format!("No resolved address found for backend: {}", host_port))
    }

    /// Convert to health check address (same as socket addr for TCP).
    pub async fn to_health_check_addr(&self) -> Result<SocketAddr> {
        self.resolve_socket_addr().await
    }

    /// Check connectivity to this backend.
    pub async fn check_connectivity(&self) -> Result<()> {
        let addr = self.resolve_socket_addr().await?;
        match tokio::time::timeout(Duration::from_secs(1), TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(anyhow::anyhow!("Connection failed: {}", e)),
            Err(_) => Err(anyhow::anyhow!("Connection timeout")),
        }
    }
}

/// Complete configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Port for load balancer to listen on
    #[serde(default = "default_port")]
    pub port: u16,

    /// Load balancing algorithm
    #[serde(default)]
    pub method: BalanceMethod,

    /// Log level (debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// List of backend servers
    pub backends: Vec<BackendConfig>,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    /// Create new Config with defaults
    pub fn new() -> Self {
        Self {
            port: DEFAULT_PORT,
            method: BalanceMethod::RoundRobin,
            log_level: "info".to_string(),
            backends: Vec::new(),
        }
    }

    /// Resolve configuration file path
    ///
    /// Uses CLI specified path if available, otherwise searches default paths.
    /// Priority:
    /// 1. Path specified via CLI argument
    /// 2. $HOME/.bal/config.yaml
    /// 3. /etc/bal/config.yaml
    pub fn resolve_config_path(cli_path: Option<&Path>) -> Result<std::path::PathBuf> {
        if let Some(path) = cli_path {
            return Ok(path.to_path_buf());
        }

        // Check home directory config
        let home_path = get_home_config_path();
        if home_path.exists() {
            return Ok(home_path);
        }

        // Check system config
        let system_path = get_system_config_path();
        if system_path.exists() {
            return Ok(system_path);
        }

        // If neither exists, return home path (will be created later)
        Ok(home_path)
    }

    /// Load configuration from file
    pub async fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Cannot read configuration file: {}", path.display()))?;

        let config: Config = serde_yaml::from_str(&content)
            .with_context(|| format!("Configuration file parsing failed: {}", path.display()))?;

        config.validate()?;
        Ok(config)
    }

    /// Alias for load_from_file
    pub async fn load(path: &Path) -> Result<Self> {
        Self::load_from_file(path).await
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate backend list
        if self.backends.is_empty() {
            bail!("At least one backend is required");
        }

        // Check for duplicate backends
        let mut seen = HashSet::new();
        for backend in &self.backends {
            let key = format!("{}:{}", backend.host, backend.port);
            if !seen.insert(key.clone()) {
                bail!("Duplicate backend configuration: {}", key);
            }
        }

        // Validate port number
        if self.port == 0 {
            bail!("Port cannot be 0");
        }

        Ok(())
    }

    /// Generate default configuration file template
    pub fn default_template() -> String {
        r#"# bal service port
port: 9295

# Load balancing method
method: "round_robin"

# Log level (debug, info, warn, error)
log_level: "info"

# Backend server list
backends:
  - host: "127.0.0.1"
    port: 9000
  - host: "127.0.0.1"
    port: 9100
"#
        .to_string()
    }

    /// Initialize default configuration file (create if not exists)
    pub async fn init_default_file() -> Result<std::path::PathBuf> {
        let path = get_home_config_path();

        // Create directory
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        // Create template if file doesn't exist
        if !path.exists() {
            fs::write(&path, Self::default_template())
                .await
                .with_context(|| {
                    format!("Failed to create default config file: {}", path.display())
                })?;
            log::info!("Default configuration file created: {}", path.display());
        }

        Ok(path)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate configuration file (for check command)
pub async fn validate_config_file(config_path: Option<std::path::PathBuf>) -> Result<()> {
    let path = if let Some(path) = config_path {
        path
    } else {
        Config::resolve_config_path(None)?
    };

    if !path.exists() {
        bail!("Configuration file not found: {}", path.display());
    }

    println!("Validating configuration file: {}", path.display());

    // Load and parse
    let config = Config::load_from_file(&path).await?;

    println!("  - Listen port: {}", config.port);
    println!("  - Load balancing: {:?}", config.method);
    println!("  - Log level: {}", config.log_level);
    println!("  - Number of backends: {}", config.backends.len());

    // Validate backend connectivity
    println!("Checking backend connectivity...");
    for backend in &config.backends {
        let addr = format!("{}:{}", backend.host, backend.port);
        match tokio::time::timeout(Duration::from_secs(1), TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => println!(
                "  [OK] {}:{} - Connection successful",
                backend.host, backend.port
            ),
            Ok(Err(e)) => println!("  [WARN] {}:{} - {}", backend.host, backend.port, e),
            Err(_) => println!(
                "  [WARN] {}:{} - Connection timeout",
                backend.host, backend.port
            ),
        }
    }

    println!(
        "Validation complete: {} healthy, 0 unhealthy",
        config.backends.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolves_hostname_backend_address() {
        let backend = BackendConfig {
            host: "localhost".to_string(),
            port: 80,
        };

        let resolved = backend
            .resolve_socket_addr()
            .await
            .expect("hostname should resolve");
        assert_eq!(resolved.port(), 80);
    }
}

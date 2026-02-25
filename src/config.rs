//! Configuration file management module
//!
//! Handles YAML configuration file parsing, validation, and default values.
//! Uses Serde to declaratively define configuration structure with
//! strong validation.

use anyhow::{Result, Context, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::net::TcpStream;

use crate::constants::{DEFAULT_PORT, get_home_config_path, get_system_config_path};

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
    /// Backend port
    pub port: u16,
    /// Backend weight (not used in round robin, for future extensions)
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// Health check port (uses service port if not specified)
    #[serde(rename = "check_port")]
    pub health_check_port: Option<u16>,
}

fn default_weight() -> u32 {
    1
}

impl BackendConfig {
    /// Convert to socket address
    pub fn to_socket_addr(&self) -> Result<SocketAddr> {
        let addr_str = format!("{}:{}", self.host, self.port);
        addr_str.parse::<SocketAddr>()
            .with_context(|| format!("Invalid backend address: {}", addr_str))
    }
    
    /// Convert to health check socket address
    pub fn to_health_check_addr(&self) -> Result<SocketAddr> {
        let port = self.health_check_port.unwrap_or(self.port);
        let addr_str = format!("{}:{}", self.host, port);
        addr_str.parse::<SocketAddr>()
            .with_context(|| format!("Invalid health check address: {}", addr_str))
    }
    
    /// Test backend connectivity (1 second timeout)
    pub async fn check_connectivity(&self) -> Result<()> {
        let addr = self.to_socket_addr()?;
        match tokio::time::timeout(
            Duration::from_secs(1),
            TcpStream::connect(&addr)
        ).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => bail!("Backend {} connection failed: {}", addr, e),
            Err(_) => bail!("Backend {} connection timeout", addr),
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
    
    /// List of backend servers
    pub backends: Vec<BackendConfig>,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Config {
    /// Create new Config with defaults
    pub fn new() -> Self {
        Self {
            port: DEFAULT_PORT,
            method: BalanceMethod::RoundRobin,
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
        
        // Return home directory path as default (file may not exist)
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
                bail!("Duplicate backend: {}", key);
            }
        }
        
        // Validate port range
        if self.port == 0 {
            bail!("Invalid port: {} (must be in range 1-65535)", self.port);
        }
        
        for backend in &self.backends {
            if backend.port == 0 {
                bail!("Invalid backend port: {} (must be in range 1-65535)", backend.port);
            }
        }
        
        Ok(())
    }
    
    /// Generate default configuration file template
    pub fn default_template() -> String {
        r#"# bal service port (9295: designer-assigned unique port)
port: 9295

# Load balancing method (default: round_robin)
method: "round_robin"

# Backend server list (host and port separated)
backends:
  - host: "127.0.0.1"
    port: 9000
  - host: "127.0.0.1"
    port: 9100
    port: 8080
  - host: "127.0.0.1"
    port: 8081
"#.to_string()
    }
    
    /// Initialize default configuration file (create if not exists)
    pub async fn init_default_file() -> Result<std::path::PathBuf> {
        let path = get_home_config_path();
        
        // Create directory
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }
        
        // Create template if file doesn't exist
        if !path.exists() {
            fs::write(&path, Self::default_template())
                .await
                .with_context(|| format!("Failed to create default config file: {}", path.display()))?;
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
pub async fn validate_config_file(cli_path: Option<std::path::PathBuf>) -> Result<()> {
    let path = Config::resolve_config_path(cli_path.as_deref())?;
    
    log::info!("Validating configuration file: {}", path.display());
    
    // Load and validate configuration file
    let config = Config::load_from_file(&path).await?;
    
    log::info!("Configuration file syntax validation passed");
    log::info!("  - Listen port: {}", config.port);
    log::info!("  - Load balancing method: {}", config.method);
    log::info!("  - Number of backends: {}", config.backends.len());
    
    // Validate backend connectivity
    log::info!("Checking backend connectivity...");
    let mut healthy_count = 0;
    let mut unhealthy_count = 0;
    
    for backend in &config.backends {
        match backend.check_connectivity().await {
            Ok(()) => {
                log::info!("  [OK] {}:{} - Connection successful", backend.host, backend.port);
                healthy_count += 1;
            }
            Err(e) => {
                log::warn!("  [FAIL] {}:{} - {}", backend.host, backend.port, e);
                unhealthy_count += 1;
            }
        }
    }
    
    log::info!("Validation complete: {} healthy, {} unhealthy", healthy_count, unhealthy_count);
    
    if healthy_count == 0 {
        bail!("Cannot connect to any backend. Please check your configuration.");
    }
    
    Ok(())
}

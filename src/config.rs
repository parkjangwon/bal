//! Configuration file management module
//!
//! Handles YAML configuration file parsing, validation, and default values.
//! Uses Serde to declaratively define configuration structure with
//! strong validation.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::net::{lookup_host, TcpStream};

use crate::constants::{
    get_home_config_path, get_system_config_path, DEFAULT_PORT, HEALTH_CHECK_INTERVAL_MS,
    HEALTH_CHECK_MAX_RETRIES, HEALTH_CHECK_MIN_SUCCESS, HEALTH_CHECK_TIMEOUT_MS,
};

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

/// Runtime tuning configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverloadPolicy {
    Reject,
}

impl Default for OverloadPolicy {
    fn default() -> Self {
        Self::Reject
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeTuning {
    #[serde(default = "default_health_check_interval_ms")]
    pub health_check_interval_ms: u64,

    #[serde(default = "default_health_check_timeout_ms")]
    pub health_check_timeout_ms: u64,

    #[serde(default = "default_health_check_fail_threshold")]
    pub health_check_fail_threshold: u32,

    #[serde(default = "default_health_check_success_threshold")]
    pub health_check_success_threshold: u32,

    #[serde(default = "default_backend_connect_timeout_ms")]
    pub backend_connect_timeout_ms: u64,

    #[serde(default = "default_failover_backoff_initial_ms")]
    pub failover_backoff_initial_ms: u64,

    #[serde(default = "default_failover_backoff_max_ms")]
    pub failover_backoff_max_ms: u64,

    #[serde(default = "default_backend_cooldown_ms")]
    pub backend_cooldown_ms: u64,

    #[serde(default = "default_protection_trigger_threshold")]
    pub protection_trigger_threshold: u32,

    #[serde(default = "default_protection_window_ms")]
    pub protection_window_ms: u64,

    #[serde(default = "default_protection_stable_success_threshold")]
    pub protection_stable_success_threshold: u32,

    #[serde(default = "default_max_concurrent_connections")]
    pub max_concurrent_connections: usize,

    #[serde(default = "default_connection_idle_timeout_ms")]
    pub connection_idle_timeout_ms: u64,

    #[serde(default)]
    pub overload_policy: OverloadPolicy,

    #[serde(default)]
    pub tcp_backlog: Option<u32>,
}

impl Default for RuntimeTuning {
    fn default() -> Self {
        Self {
            health_check_interval_ms: default_health_check_interval_ms(),
            health_check_timeout_ms: default_health_check_timeout_ms(),
            health_check_fail_threshold: default_health_check_fail_threshold(),
            health_check_success_threshold: default_health_check_success_threshold(),
            backend_connect_timeout_ms: default_backend_connect_timeout_ms(),
            failover_backoff_initial_ms: default_failover_backoff_initial_ms(),
            failover_backoff_max_ms: default_failover_backoff_max_ms(),
            backend_cooldown_ms: default_backend_cooldown_ms(),
            protection_trigger_threshold: default_protection_trigger_threshold(),
            protection_window_ms: default_protection_window_ms(),
            protection_stable_success_threshold: default_protection_stable_success_threshold(),
            max_concurrent_connections: default_max_concurrent_connections(),
            connection_idle_timeout_ms: default_connection_idle_timeout_ms(),
            overload_policy: OverloadPolicy::default(),
            tcp_backlog: None,
        }
    }
}

/// Complete configuration structure
#[derive(Debug, Clone, Serialize)]
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

    /// Bind address for listener
    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    /// Runtime tuning knobs
    pub runtime: RuntimeTuning,

    /// List of backend servers
    pub backends: Vec<BackendConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    #[serde(default)]
    _mode: Option<serde_yaml::Value>,
    port: Option<u16>,
    method: Option<BalanceMethod>,
    log_level: Option<String>,
    bind_address: Option<String>,
    runtime: Option<RuntimeTuning>,
    #[serde(default)]
    backends: Vec<BackendConfig>,
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawConfig::deserialize(deserializer)?;
        let backend_count = raw.backends.len();

        Ok(Self {
            port: raw.port.unwrap_or_else(default_port),
            method: raw.method.unwrap_or_default(),
            log_level: raw.log_level.unwrap_or_else(default_log_level),
            bind_address: raw.bind_address.unwrap_or_else(default_bind_address),
            runtime: raw
                .runtime
                .unwrap_or_else(|| auto_tuned_runtime_profile(backend_count)),
            backends: raw.backends,
        })
    }
}

fn auto_tuned_runtime_profile(backend_count: usize) -> RuntimeTuning {
    if backend_count <= 2 {
        RuntimeTuning {
            health_check_interval_ms: 500,
            health_check_timeout_ms: 800,
            health_check_fail_threshold: 2,
            health_check_success_threshold: 1,
            backend_connect_timeout_ms: 800,
            failover_backoff_initial_ms: 200,
            failover_backoff_max_ms: 5_000,
            backend_cooldown_ms: 500,
            protection_trigger_threshold: 10,
            protection_window_ms: 30_000,
            protection_stable_success_threshold: 12,
            max_concurrent_connections: 4_000,
            connection_idle_timeout_ms: default_connection_idle_timeout_ms(),
            overload_policy: OverloadPolicy::default(),
            tcp_backlog: None,
        }
    } else if backend_count <= 5 {
        RuntimeTuning {
            health_check_interval_ms: 700,
            health_check_timeout_ms: 1_000,
            health_check_fail_threshold: 2,
            health_check_success_threshold: 1,
            backend_connect_timeout_ms: 1_000,
            failover_backoff_initial_ms: 300,
            failover_backoff_max_ms: 7_000,
            backend_cooldown_ms: 700,
            protection_trigger_threshold: 12,
            protection_window_ms: 30_000,
            protection_stable_success_threshold: 14,
            max_concurrent_connections: 8_000,
            connection_idle_timeout_ms: default_connection_idle_timeout_ms(),
            overload_policy: OverloadPolicy::default(),
            tcp_backlog: None,
        }
    } else {
        RuntimeTuning {
            health_check_interval_ms: 1_000,
            health_check_timeout_ms: 1_200,
            health_check_fail_threshold: 3,
            health_check_success_threshold: 2,
            backend_connect_timeout_ms: 1_200,
            failover_backoff_initial_ms: 500,
            failover_backoff_max_ms: 10_000,
            backend_cooldown_ms: 1_000,
            protection_trigger_threshold: 14,
            protection_window_ms: 30_000,
            protection_stable_success_threshold: 16,
            max_concurrent_connections: 12_000,
            connection_idle_timeout_ms: default_connection_idle_timeout_ms(),
            overload_policy: OverloadPolicy::default(),
            tcp_backlog: None,
        }
    }
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_health_check_interval_ms() -> u64 {
    HEALTH_CHECK_INTERVAL_MS
}

fn default_health_check_timeout_ms() -> u64 {
    HEALTH_CHECK_TIMEOUT_MS
}

fn default_health_check_fail_threshold() -> u32 {
    HEALTH_CHECK_MAX_RETRIES
}

fn default_health_check_success_threshold() -> u32 {
    HEALTH_CHECK_MIN_SUCCESS
}

fn default_backend_connect_timeout_ms() -> u64 {
    HEALTH_CHECK_TIMEOUT_MS
}

fn default_failover_backoff_initial_ms() -> u64 {
    100
}

fn default_failover_backoff_max_ms() -> u64 {
    5_000
}

fn default_backend_cooldown_ms() -> u64 {
    300
}

fn default_protection_trigger_threshold() -> u32 {
    10
}

fn default_protection_window_ms() -> u64 {
    30_000
}

fn default_protection_stable_success_threshold() -> u32 {
    12
}

fn default_max_concurrent_connections() -> usize {
    10_000
}

fn default_connection_idle_timeout_ms() -> u64 {
    120_000
}

impl Config {
    /// Create new Config with defaults
    pub fn new() -> Self {
        Self {
            port: DEFAULT_PORT,
            method: BalanceMethod::RoundRobin,
            log_level: "info".to_string(),
            bind_address: default_bind_address(),
            runtime: RuntimeTuning::default(),
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

        if self.bind_address.trim().is_empty() {
            bail!("Bind address cannot be empty");
        }

        if self.runtime.health_check_interval_ms == 0 {
            bail!("health_check_interval_ms must be greater than 0");
        }

        if self.runtime.health_check_timeout_ms == 0 {
            bail!("health_check_timeout_ms must be greater than 0");
        }

        if self.runtime.health_check_fail_threshold == 0 {
            bail!("health_check_fail_threshold must be greater than 0");
        }

        if self.runtime.health_check_success_threshold == 0 {
            bail!("health_check_success_threshold must be greater than 0");
        }

        if self.runtime.backend_connect_timeout_ms == 0 {
            bail!("backend_connect_timeout_ms must be greater than 0");
        }

        if self.runtime.failover_backoff_initial_ms == 0 {
            bail!("failover_backoff_initial_ms must be greater than 0");
        }

        if self.runtime.failover_backoff_max_ms < self.runtime.failover_backoff_initial_ms {
            bail!("failover_backoff_max_ms must be >= failover_backoff_initial_ms");
        }

        if self.runtime.protection_trigger_threshold == 0 {
            bail!("protection_trigger_threshold must be greater than 0");
        }

        if self.runtime.protection_window_ms == 0 {
            bail!("protection_window_ms must be greater than 0");
        }

        if self.runtime.protection_stable_success_threshold == 0 {
            bail!("protection_stable_success_threshold must be greater than 0");
        }

        if self.runtime.max_concurrent_connections == 0 {
            bail!("max_concurrent_connections must be greater than 0");
        }

        if self.runtime.connection_idle_timeout_ms == 0 {
            bail!("connection_idle_timeout_ms must be greater than 0");
        }

        Ok(())
    }

    /// Generate default configuration file template
    pub fn default_template() -> String {
        r#"# minimal config (recommended)
# Add only the fields you want to override from defaults.

port: 9295
backends:
  - host: "127.0.0.1"
    port: 9000
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

    println!("  - Listen: {}:{}", config.bind_address, config.port);
    println!("  - Load balancing: {:?}", config.method);
    println!("  - Log level: {}", config.log_level);
    println!(
        "  - Runtime: health_interval={}ms health_timeout={}ms fail_threshold={} success_threshold={} backend_connect_timeout={}ms backoff_initial={}ms backoff_max={}ms cooldown={}ms protection_trigger={} protection_window={}ms protection_recover={} max_conns={} idle_timeout={}ms overload_policy={}",
        config.runtime.health_check_interval_ms,
        config.runtime.health_check_timeout_ms,
        config.runtime.health_check_fail_threshold,
        config.runtime.health_check_success_threshold,
        config.runtime.backend_connect_timeout_ms,
        config.runtime.failover_backoff_initial_ms,
        config.runtime.failover_backoff_max_ms,
        config.runtime.backend_cooldown_ms,
        config.runtime.protection_trigger_threshold,
        config.runtime.protection_window_ms,
        config.runtime.protection_stable_success_threshold,
        config.runtime.max_concurrent_connections,
        config.runtime.connection_idle_timeout_ms,
        match config.runtime.overload_policy { OverloadPolicy::Reject => "reject" },
    );
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

    #[test]
    fn parse_config_applies_defaults_and_auto_tuned_runtime_when_runtime_omitted() {
        let yaml = r#"
port: 9295
backends:
  - host: "127.0.0.1"
    port: 9000
"#;

        let config: Config = serde_yaml::from_str(yaml).expect("config should parse");

        assert_eq!(config.bind_address, "0.0.0.0");
        assert_eq!(config.runtime.health_check_interval_ms, 500);
        assert_eq!(config.runtime.health_check_timeout_ms, 800);
        assert_eq!(config.runtime.health_check_fail_threshold, 2);
        assert_eq!(config.runtime.health_check_success_threshold, 1);
        assert_eq!(config.runtime.backend_connect_timeout_ms, 800);
        assert_eq!(config.runtime.failover_backoff_initial_ms, 200);
        assert_eq!(config.runtime.failover_backoff_max_ms, 5000);
        assert_eq!(config.runtime.backend_cooldown_ms, 500);
        assert_eq!(config.runtime.max_concurrent_connections, 4000);
        assert_eq!(config.runtime.connection_idle_timeout_ms, 120000);
    }

    #[test]
    fn parse_config_accepts_legacy_mode_field_but_ignores_it() {
        let yaml = r#"
mode: "advanced"
port: 9295
bind_address: "127.0.0.1"
runtime:
  health_check_interval_ms: 750
  health_check_timeout_ms: 1200
  health_check_fail_threshold: 3
  health_check_success_threshold: 2
  backend_connect_timeout_ms: 900
  failover_backoff_initial_ms: 150
  failover_backoff_max_ms: 2000
  backend_cooldown_ms: 700
  max_concurrent_connections: 321
  connection_idle_timeout_ms: 33000
backends:
  - host: "127.0.0.1"
    port: 9000
"#;

        let config: Config = serde_yaml::from_str(yaml).expect("config should parse");

        assert_eq!(config.bind_address, "127.0.0.1");
        assert_eq!(config.runtime.health_check_interval_ms, 750);
        assert_eq!(config.runtime.health_check_timeout_ms, 1200);
        assert_eq!(config.runtime.health_check_fail_threshold, 3);
        assert_eq!(config.runtime.health_check_success_threshold, 2);
        assert_eq!(config.runtime.backend_connect_timeout_ms, 900);
        assert_eq!(config.runtime.failover_backoff_initial_ms, 150);
        assert_eq!(config.runtime.failover_backoff_max_ms, 2000);
        assert_eq!(config.runtime.backend_cooldown_ms, 700);
        assert_eq!(config.runtime.max_concurrent_connections, 321);
        assert_eq!(config.runtime.connection_idle_timeout_ms, 33000);
    }

    #[test]
    fn parse_config_auto_tuning_scales_conservatively_with_backend_count() {
        let yaml = r#"
port: 9295
backends:
  - host: "127.0.0.1"
    port: 9000
  - host: "127.0.0.1"
    port: 9001
  - host: "127.0.0.1"
    port: 9002
  - host: "127.0.0.1"
    port: 9003
  - host: "127.0.0.1"
    port: 9004
  - host: "127.0.0.1"
    port: 9005
"#;

        let config: Config = serde_yaml::from_str(yaml).expect("config should parse");

        assert_eq!(config.runtime.health_check_interval_ms, 1000);
        assert_eq!(config.runtime.health_check_timeout_ms, 1200);
        assert_eq!(config.runtime.health_check_fail_threshold, 3);
        assert_eq!(config.runtime.health_check_success_threshold, 2);
        assert_eq!(config.runtime.backend_connect_timeout_ms, 1200);
        assert_eq!(config.runtime.failover_backoff_initial_ms, 500);
        assert_eq!(config.runtime.failover_backoff_max_ms, 10000);
        assert_eq!(config.runtime.backend_cooldown_ms, 1000);
        assert_eq!(config.runtime.max_concurrent_connections, 12000);
    }

    #[test]
    fn default_template_shows_only_minimum_fields() {
        let template = Config::default_template();
        assert!(template.contains("port: 9295"));
        assert!(template.contains("backends:"));
        assert!(!template.contains("method:"));
        assert!(!template.contains("bind_address:"));
        assert!(!template.contains("log_level:"));
        assert!(!template.contains("runtime:"));
    }
}

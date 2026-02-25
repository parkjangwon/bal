//! Constants definition module
//!
//! Centralizes constants used throughout the application.
//! This improves maintainability by requiring changes in only one place.

use std::path::PathBuf;

/// Application basic settings
pub const APP_NAME: &str = "bal";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default port and network settings
///
/// Port 9295 is bal's unique identifier, specially designated by the designer
/// to avoid conflicts with other common ports.
pub const DEFAULT_PORT: u16 = 9295;

/// Health check settings
///
/// Ultra-fast failover: 200ms interval for sub-second detection and recovery.
pub const HEALTH_CHECK_INTERVAL_MS: u64 = 200;
pub const HEALTH_CHECK_TIMEOUT_MS: u64 = 500;
pub const HEALTH_CHECK_MAX_RETRIES: u32 = 1;
pub const HEALTH_CHECK_MIN_SUCCESS: u32 = 1;

/// Connection settings
///
/// Backend connection attempt timeout - too short causes unnecessary failure
/// detection during temporary network delays, too long causes failover delays.
pub const BACKEND_CONNECT_TIMEOUT_SECS: u64 = 5;
pub const PROXY_BUFFER_SIZE: usize = 8192;

/// Graceful shutdown settings
///
/// Maximum time to wait for existing connections to complete.
/// Forces shutdown after this time to prevent infinite waits.
pub const GRACEFUL_SHUTDOWN_TIMEOUT_SECS: u64 = 30;

/// File and directory settings
pub const PID_FILENAME: &str = "bal.pid";
pub const LOG_FILENAME: &str = "bal.log";
pub const CONFIG_FILENAME: &str = "config.yaml";

/// Configuration file priority (higher = more priority)
/// 1. Path specified via CLI argument
/// 2. $HOME/.bal/config.yaml
/// 3. /etc/bal/config.yaml
pub fn get_home_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".bal").join(CONFIG_FILENAME))
        .unwrap_or_else(|| PathBuf::from(CONFIG_FILENAME))
}

pub fn get_system_config_path() -> PathBuf {
    PathBuf::from("/etc/bal").join(CONFIG_FILENAME)
}

/// PID file path ($HOME/.bal/bal.pid)
pub fn get_pid_file_path() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".bal").join(PID_FILENAME))
        .unwrap_or_else(|| PathBuf::from(PID_FILENAME))
}

/// Log file path ($HOME/.bal/bal.log)
pub fn get_log_file_path() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".bal").join(LOG_FILENAME))
        .unwrap_or_else(|| PathBuf::from(LOG_FILENAME))
}

/// Runtime directory path ($HOME/.bal/)
pub fn get_runtime_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".bal"))
        .unwrap_or_else(|| PathBuf::from("."))
}

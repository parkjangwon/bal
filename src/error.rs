//! Error handling module
//!
//! Based on anyhow but adds domain-specific error contexts to improve
//! debugging and user feedback.

use std::io;

/// Main error types for bal application
/// 
/// Each error clearly expresses the context where it occurred (config, network,
/// process, etc.) to reduce problem resolution time.
#[derive(Debug)]
pub enum BalError {
    /// Configuration file related errors
    Config(String),
    /// Network/IO related errors
    Network(String),
    /// Process management related errors
    Process(String),
    /// Backend connection related errors
    Backend(String),
    /// Health check related errors
    HealthCheck(String),
}

impl std::fmt::Display for BalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BalError::Config(msg) => write!(f, "Config error: {}", msg),
            BalError::Network(msg) => write!(f, "Network error: {}", msg),
            BalError::Process(msg) => write!(f, "Process control error: {}", msg),
            BalError::Backend(msg) => write!(f, "Backend connection failed: {}", msg),
            BalError::HealthCheck(msg) => write!(f, "Health check failed: {}", msg),
        }
    }
}

impl std::error::Error for BalError {}

/// Helper trait for adding context to anyhow::Error
pub trait ResultExt<T> {
    /// Add configuration error context
    fn context_config(self, msg: &str) -> anyhow::Result<T>;
    /// Add network error context
    fn context_network(self, msg: &str) -> anyhow::Result<T>;
    /// Add process error context
    fn context_process(self, msg: &str) -> anyhow::Result<T>;
    /// Add backend error context
    fn context_backend(self, msg: &str) -> anyhow::Result<T>;
}

impl<T> ResultExt<T> for anyhow::Result<T> {
    fn context_config(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", BalError::Config(msg.to_string()), e))
    }
    
    fn context_network(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", BalError::Network(msg.to_string()), e))
    }
    
    fn context_process(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", BalError::Process(msg.to_string()), e))
    }
    
    fn context_backend(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", BalError::Backend(msg.to_string()), e))
    }
}

impl<T> ResultExt<T> for io::Result<T> {
    fn context_config(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", BalError::Config(msg.to_string()), e))
    }
    
    fn context_network(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", BalError::Network(msg.to_string()), e))
    }
    
    fn context_process(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", BalError::Process(msg.to_string()), e))
    }
    
    fn context_backend(self, msg: &str) -> anyhow::Result<T> {
        self.map_err(|e| anyhow::anyhow!("{}: {}", BalError::Backend(msg.to_string()), e))
    }
}

/// Generate user-friendly error message
/// 
/// Converts internal errors into messages users can understand and act upon.
pub fn format_user_error(error: &anyhow::Error) -> String {
    let error_str = error.to_string();
    
    // Convert common error patterns to user-friendly messages
    if error_str.contains("Connection refused") {
        "Cannot connect to backend server. Please check if the server is running.".to_string()
    } else if error_str.contains("Permission denied") {
        "Insufficient permissions. Please check file permissions if needed.".to_string()
    } else if error_str.contains("Address already in use") {
        "Port is already in use. Please check if another process is using this port.".to_string()
    } else if error_str.contains("No such file") {
        "File not found. Please check the path.".to_string()
    } else {
        error_str
    }
}

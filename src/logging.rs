//! Logging module
//!
//! Initializes and manages env_logger based logging system.
//! Supports both file and console logging with adjustable verbosity levels.

use anyhow::Result;
use log::LevelFilter;
use std::fs::OpenOptions;
use std::io::Write;

use crate::constants::get_log_file_path;

/// Initialize logging system
/// 
/// Adjusts log level based on verbose flag:
/// - verbose=false: Only INFO level and above
/// - verbose=true: DEBUG level and above
/// 
/// Logs are output to console (stdout).
pub fn init_logging(verbose: bool) -> Result<()> {
    let log_level = if verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    
    // Read log level from environment variable, but verbose flag takes precedence
    let env_filter = env_logger::Env::default()
        .default_filter_or(if verbose { "debug" } else { "info" });
    
    env_logger::Builder::from_env(env_filter)
        .format(|buf, record| {
            use std::io::Write;
            // Custom log format: [timestamp] [level] message
            writeln!(
                buf,
                "[{}] [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.args()
            )
        })
        .filter_level(log_level)
        .init();
    
    Ok(())
}

/// Append log message to file
/// 
/// Used to log specific events to file only, outside standard logging.
pub fn append_to_log_file(message: &str) -> Result<()> {
    let log_path = get_log_file_path();
    
    // Create directory
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    writeln!(file, "[{}] {}", timestamp, message)?;
    
    Ok(())
}

//! Logging module
//!
//! Initializes and manages env_logger based logging system.
//! Supports both file and console logging with adjustable verbosity levels.

use anyhow::Result;
use log::LevelFilter;
use std::fs::OpenOptions;
use std::io::Write;

use crate::constants::get_log_file_path;

/// Parse log level string to LevelFilter
fn parse_log_level(level: &str) -> LevelFilter {
    match level.to_lowercase().as_str() {
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        _ => LevelFilter::Info, // Default to info for unknown values
    }
}

/// Initialize logging system
///
/// Log level is determined by config (default: info).
/// Users can change log_level in config.yaml to debug, info, warn, or error.
///
/// - foreground mode: Logs to stdout
/// - daemon mode: Logs to file only
pub fn init_logging(log_level_str: &str, daemon_mode: bool) -> Result<()> {
    let log_level = parse_log_level(log_level_str);

    if daemon_mode {
        // Daemon mode: log to file only
        init_file_logging(log_level)?;
    } else {
        // Foreground mode: log to stdout
        init_console_logging(log_level)?;
    }

    Ok(())
}

/// Initialize console logging (stdout)
fn init_console_logging(log_level: LevelFilter) -> Result<()> {
    env_logger::Builder::new()
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

/// Initialize file logging (daemon mode)
fn init_file_logging(log_level: LevelFilter) -> Result<()> {
    let log_path = get_log_file_path();

    // Create directory
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // For daemon mode, we use stderr redirect approach
    // In production, you'd use a proper file logger like log4rs or tracing-appender
    env_logger::Builder::new()
        .format(|buf, record| {
            use std::io::Write;
            writeln!(
                buf,
                "[{}] [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.args()
            )
        })
        .filter_level(log_level)
        .target(env_logger::Target::Stderr)
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

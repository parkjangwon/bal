//! Logging module
//!
//! Initializes and manages env_logger based logging system.
//! Emits one-line JSON logs only.

use anyhow::Result;
use log::LevelFilter;
use serde_json::{json, Value};
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
        _ => LevelFilter::Info,
    }
}

/// Initialize logging system
///
/// - foreground mode: logs to stdout
/// - daemon mode: logs to file
pub fn init_logging(log_level_str: &str, daemon_mode: bool) -> Result<()> {
    let log_level = parse_log_level(log_level_str);

    if daemon_mode {
        init_file_logging(log_level)?;
    } else {
        init_console_logging(log_level)?;
    }

    Ok(())
}

fn init_console_logging(log_level: LevelFilter) -> Result<()> {
    env_logger::Builder::new()
        .format(move |buf, record| {
            let payload = build_json_payload(
                &chrono::Utc::now().to_rfc3339(),
                &record.level().to_string(),
                &record.args().to_string(),
                record.module_path().unwrap_or(record.target()),
                "log",
                json!({}),
            );
            writeln!(buf, "{}", payload)
        })
        .filter_level(log_level)
        .init();

    Ok(())
}

fn init_file_logging(log_level: LevelFilter) -> Result<()> {
    let log_path = get_log_file_path();
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let target = Box::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?,
    );

    env_logger::Builder::new()
        .target(env_logger::Target::Pipe(target))
        .format(move |buf, record| {
            let payload = build_json_payload(
                &chrono::Utc::now().to_rfc3339(),
                &record.level().to_string(),
                &record.args().to_string(),
                record.module_path().unwrap_or(record.target()),
                "log",
                json!({}),
            );
            writeln!(buf, "{}", payload)
        })
        .filter_level(log_level)
        .init();

    Ok(())
}

fn build_json_payload(
    timestamp: &str,
    level: &str,
    message: &str,
    module: &str,
    event: &str,
    fields: Value,
) -> Value {
    json!({
        "timestamp": timestamp,
        "level": level,
        "message": message,
        "module": module,
        "event": event,
        "fields": fields
    })
}

/// Append log message to file in one-line JSON format.
pub fn append_to_log_file(message: &str) -> Result<()> {
    let log_path = get_log_file_path();

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let payload = build_json_payload(
        &chrono::Utc::now().to_rfc3339(),
        "INFO",
        message,
        "bal::logging",
        "log",
        json!({}),
    );
    writeln!(file, "{}", payload)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_log_payload_uses_stable_keys() {
        let payload = build_json_payload(
            "2026-01-01T00:00:00Z",
            "INFO",
            "bal started",
            "bal::main",
            "service_started",
            serde_json::json!({"daemon": false}),
        );

        assert_eq!(payload["timestamp"], "2026-01-01T00:00:00Z");
        assert_eq!(payload["level"], "INFO");
        assert_eq!(payload["message"], "bal started");
        assert_eq!(payload["module"], "bal::main");
        assert_eq!(payload["event"], "service_started");
        assert_eq!(payload["fields"]["daemon"], false);
    }
}

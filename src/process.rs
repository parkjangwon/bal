//! Process management module
//!
//! Handles PID file creation/management, process termination signals,
//! and process status checks. Operates based on home directory for
//! non-root user support.

use anyhow::{bail, Result};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process;

use crate::config::Config;
use crate::constants::{get_pid_file_path, get_runtime_dir};
use crate::error::ResultExt;

/// Process manager
///
/// Identifies and controls daemon process via PID file.
pub struct ProcessManager;

#[derive(Debug, Clone, Serialize)]
pub struct BackendErrorCounters {
    pub timeout: u64,
    pub refused: u64,
    pub other: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendStatusSummary {
    pub address: String,
    pub reachable: bool,
    pub active_connections: usize,
    pub last_check_time: String,
    pub counters: BackendErrorCounters,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessStatusSummary {
    pub running: bool,
    pub pid: Option<i32>,
    pub config_path: Option<String>,
    pub bind_address: String,
    pub port: Option<u16>,
    pub method: Option<String>,
    pub backend_total: Option<usize>,
    pub backend_reachable: Option<usize>,
    pub backends: Vec<BackendStatusSummary>,
    pub active_connections: usize,
    pub last_check_time: String,
}

impl ProcessManager {
    /// Write current process PID to file
    ///
    /// If PID file already exists, considers it a duplicate execution and returns error.
    pub fn write_pid_file() -> Result<()> {
        let pid_path = get_pid_file_path();

        // Create runtime directory
        let runtime_dir = get_runtime_dir();
        std::fs::create_dir_all(&runtime_dir).context_process(&format!(
            "Failed to create runtime directory: {}",
            runtime_dir.display()
        ))?;

        // Check existing PID file
        if pid_path.exists() {
            // Check if existing process is running
            if let Ok(old_pid) = Self::read_pid_file() {
                if Self::is_process_running(old_pid) {
                    bail!(
                        "bal is already running (PID: {}). Run 'bal stop' first.",
                        old_pid
                    );
                }
            }
            // Remove file if not running
            let _ = fs::remove_file(&pid_path);
        }

        // Write new PID file
        let pid = process::id();
        let mut file = fs::File::create(&pid_path).context_process(&format!(
            "Failed to create PID file: {}",
            pid_path.display()
        ))?;

        writeln!(file, "{}", pid)
            .context_process(&format!("Failed to write PID file: {}", pid_path.display()))?;

        log::debug!("PID file created: {} (PID: {})", pid_path.display(), pid);
        Ok(())
    }

    /// Read PID from PID file
    pub fn read_pid_file() -> Result<i32> {
        let pid_path = get_pid_file_path();

        let content = fs::read_to_string(&pid_path)
            .context_process(&format!("Failed to read PID file: {}", pid_path.display()))?;

        let pid: i32 = content
            .trim()
            .parse::<i32>()
            .map_err(|e| anyhow::anyhow!("Invalid PID file content: {}", e))?;

        Ok(pid)
    }

    /// Remove PID file
    pub fn remove_pid_file() -> Result<()> {
        let pid_path = get_pid_file_path();

        if pid_path.exists() {
            fs::remove_file(&pid_path).context_process(&format!(
                "Failed to remove PID file: {}",
                pid_path.display()
            ))?;
            log::debug!("PID file removed: {}", pid_path.display());
        }

        Ok(())
    }

    /// Check if process is running
    ///
    /// Uses kill(pid, 0) to check process existence.
    /// Signal 0 doesn't actually send signal to process, only checks existence.
    fn is_process_running(pid: i32) -> bool {
        let pid = Pid::from_raw(pid);
        signal::kill(pid, None).is_ok()
    }

    /// Stop running daemon
    ///
    /// Reads PID file and sends SIGTERM signal to gracefully
    /// terminate and clean up files.
    pub fn stop_daemon() -> Result<()> {
        let pid = Self::read_pid_file().context_process(
            "Cannot find running bal process. PID file does not exist or is corrupted.",
        )?;

        if !Self::is_process_running(pid) {
            // Process already terminated - clean up file
            log::warn!(
                "Process with PID {} does not exist. Cleaning up PID file.",
                pid
            );
            Self::remove_pid_file()?;
            bail!("bal is not running.");
        }

        // Send SIGTERM signal
        let nix_pid = Pid::from_raw(pid);
        signal::kill(nix_pid, Signal::SIGTERM)
            .map_err(|e| anyhow::anyhow!("Failed to send SIGTERM to process {}: {}", pid, e))?;

        log::info!("Sent termination signal to bal process (PID: {})", pid);

        // File is automatically cleaned up when process terminates
        Ok(())
    }

    /// Send configuration reload signal (SIGHUP)
    ///
    /// Sends SIGHUP signal to running daemon to reload configuration
    /// without downtime.
    pub fn send_reload_signal() -> Result<()> {
        let pid = Self::read_pid_file().context_process("Cannot find running bal process.")?;

        if !Self::is_process_running(pid) {
            bail!("bal is not running. Clean up the PID file and try again.");
        }

        // Send SIGHUP signal
        let nix_pid = Pid::from_raw(pid);
        signal::kill(nix_pid, Signal::SIGHUP)
            .map_err(|e| anyhow::anyhow!("Failed to send SIGHUP to process {}: {}", pid, e))?;

        log::info!(
            "Sent configuration reload signal to bal process (PID: {})",
            pid
        );
        Ok(())
    }

    /// Check daemon running status
    pub fn is_daemon_running() -> bool {
        match Self::read_pid_file() {
            Ok(pid) => Self::is_process_running(pid),
            Err(_) => false,
        }
    }

    /// Probe process existence for diagnostics and tests.
    pub(crate) fn probe_process_running(pid: i32) -> bool {
        Self::is_process_running(pid)
    }

    pub async fn collect_status(config_path: Option<PathBuf>) -> Result<ProcessStatusSummary> {
        let running = Self::is_daemon_running();
        let pid = if running {
            Self::read_pid_file().ok()
        } else {
            None
        };

        let resolved_config_path = Config::resolve_config_path(config_path.as_deref()).ok();
        let mut summary = ProcessStatusSummary {
            running,
            pid,
            config_path: resolved_config_path
                .as_ref()
                .map(|path| path.display().to_string()),
            bind_address: "0.0.0.0".to_string(),
            port: None,
            method: None,
            backend_total: None,
            backend_reachable: None,
            backends: Vec::new(),
            active_connections: 0,
            last_check_time: chrono::Utc::now().to_rfc3339(),
        };

        if let Some(path) = resolved_config_path {
            if path.exists() {
                if let Ok(config) = Config::load_from_file(&path).await {
                    let mut reachable = 0usize;
                    let mut backend_summaries = Vec::new();
                    let check_time = chrono::Utc::now().to_rfc3339();

                    for backend in &config.backends {
                        let result = backend.check_connectivity().await;
                        let (is_reachable, counters) = match result {
                            Ok(_) => (
                                true,
                                BackendErrorCounters {
                                    timeout: 0,
                                    refused: 0,
                                    other: 0,
                                },
                            ),
                            Err(err) => {
                                let lower = err.to_string().to_lowercase();
                                if lower.contains("timeout") {
                                    (
                                        false,
                                        BackendErrorCounters {
                                            timeout: 1,
                                            refused: 0,
                                            other: 0,
                                        },
                                    )
                                } else if lower.contains("refused") {
                                    (
                                        false,
                                        BackendErrorCounters {
                                            timeout: 0,
                                            refused: 1,
                                            other: 0,
                                        },
                                    )
                                } else {
                                    (
                                        false,
                                        BackendErrorCounters {
                                            timeout: 0,
                                            refused: 0,
                                            other: 1,
                                        },
                                    )
                                }
                            }
                        };

                        if is_reachable {
                            reachable += 1;
                        }

                        backend_summaries.push(BackendStatusSummary {
                            address: format!("{}:{}", backend.host, backend.port),
                            reachable: is_reachable,
                            active_connections: 0,
                            last_check_time: check_time.clone(),
                            counters,
                        });
                    }

                    summary.bind_address = config.bind_address;
                    summary.port = Some(config.port);
                    summary.method = Some(config.method.to_string());
                    summary.backend_total = Some(config.backends.len());
                    summary.backend_reachable = Some(reachable);
                    summary.backends = backend_summaries;
                }
            }
        }

        Ok(summary)
    }

    pub fn build_status_report(summary: ProcessStatusSummary) -> String {
        let running_text = if summary.running { "yes" } else { "no" };
        let pid_text = summary
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string());
        let config_text = summary.config_path.unwrap_or_else(|| "-".to_string());
        let listen_text = summary
            .port
            .map(|port| format!("{}:{}", summary.bind_address, port))
            .unwrap_or_else(|| "-".to_string());
        let method_text = summary.method.unwrap_or_else(|| "-".to_string());
        let backend_text = match (summary.backend_reachable, summary.backend_total) {
            (Some(reachable), Some(total)) => format!("{}/{} reachable", reachable, total),
            _ => "-".to_string(),
        };

        let mut report = format!(
            "bal status\n  running: {}\n  pid: {}\n  config: {}\n  listen: {}\n  method: {}\n  backends: {}\n  active_connections: {}\n  last_check_time: {}",
            running_text,
            pid_text,
            config_text,
            listen_text,
            method_text,
            backend_text,
            summary.active_connections,
            summary.last_check_time
        );

        if !summary.backends.is_empty() {
            report.push_str("\n  backend_details:");
            for backend in &summary.backends {
                report.push_str(&format!(
                    "\n    - {} reachable={} active={} last_check={} counters(timeout={}, refused={}, other={})",
                    backend.address,
                    backend.reachable,
                    backend.active_connections,
                    backend.last_check_time,
                    backend.counters.timeout,
                    backend.counters.refused,
                    backend.counters.other
                ));
            }
        }

        report
    }

    pub async fn print_status(config_path: Option<PathBuf>, json: bool) -> Result<()> {
        let summary = Self::collect_status(config_path).await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            println!("{}", Self::build_status_report(summary));
        }
        Ok(())
    }
}

/// Cleanup guard - PID file auto-cleanup using RAII pattern
///
/// Automatically cleans up PID file on normal/abnormal process termination.
pub struct PidFileGuard;

impl PidFileGuard {
    pub fn new() -> Result<Self> {
        ProcessManager::write_pid_file()?;
        Ok(Self)
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        // Clean up PID file on termination
        if let Err(e) = ProcessManager::remove_pid_file() {
            log::error!("Failed to clean up PID file: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_summary_serializes_to_json() {
        let summary = ProcessStatusSummary {
            running: false,
            pid: None,
            config_path: None,
            bind_address: "0.0.0.0".to_string(),
            port: None,
            method: None,
            backend_total: Some(1),
            backend_reachable: Some(0),
            backends: vec![BackendStatusSummary {
                address: "127.0.0.1:9000".to_string(),
                reachable: false,
                active_connections: 0,
                last_check_time: "2026-01-01T00:00:00Z".to_string(),
                counters: BackendErrorCounters {
                    timeout: 1,
                    refused: 0,
                    other: 0,
                },
            }],
            active_connections: 0,
            last_check_time: "2026-01-01T00:00:00Z".to_string(),
        };

        let encoded = serde_json::to_string(&summary).expect("json encoding should work");
        assert!(encoded.contains("\"backends\""));
        assert!(encoded.contains("\"timeout\":1"));
    }

    #[test]
    fn build_status_report_contains_practical_runtime_summary() {
        let report = ProcessManager::build_status_report(ProcessStatusSummary {
            running: true,
            pid: Some(4242),
            config_path: Some("/tmp/bal-config.yaml".to_string()),
            bind_address: "0.0.0.0".to_string(),
            port: Some(9295),
            method: Some("round_robin".to_string()),
            backend_total: Some(2),
            backend_reachable: Some(1),
            backends: vec![BackendStatusSummary {
                address: "127.0.0.1:9000".to_string(),
                reachable: true,
                active_connections: 3,
                last_check_time: "2026-01-01T00:00:00Z".to_string(),
                counters: BackendErrorCounters {
                    timeout: 0,
                    refused: 0,
                    other: 0,
                },
            }],
            active_connections: 3,
            last_check_time: "2026-01-01T00:00:00Z".to_string(),
        });

        assert!(report.contains("running: yes"));
        assert!(report.contains("pid: 4242"));
        assert!(report.contains("listen: 0.0.0.0:9295"));
        assert!(report.contains("backends: 1/2 reachable"));
        assert!(report.contains("backend_details"));
    }
}

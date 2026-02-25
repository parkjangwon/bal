use anyhow::{bail, Result};
use serde::Serialize;
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
use std::path::PathBuf;

use crate::config::Config;
use crate::constants::get_pid_file_path;
use crate::operator_message::render_operator_message;
use crate::process::{ProcessManager, ProtectionModeSummary};
use crate::protection;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckLevel {
    Ok,
    Warn,
    Critical,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DoctorCheck {
    pub name: String,
    pub level: CheckLevel,
    pub summary: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
    pub protection_mode: ProtectionModeSummary,
}

impl DoctorReport {
    pub fn has_critical_failure(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.level == CheckLevel::Critical)
    }

    pub fn to_plain_text(&self, verbose: bool) -> String {
        let mut lines = Vec::new();
        let critical_count = self
            .checks
            .iter()
            .filter(|check| check.level == CheckLevel::Critical)
            .count();
        let warn_count = self
            .checks
            .iter()
            .filter(|check| check.level == CheckLevel::Warn)
            .count();
        let overall = if critical_count > 0 {
            "FAILED"
        } else if warn_count > 0 {
            "WARN"
        } else {
            "OK"
        };

        lines.push("bal doctor".to_string());
        lines.push(format!("  overall: {}", overall));
        lines.push(format!("  critical: {}", critical_count));
        lines.push(format!("  warnings: {}", warn_count));
        lines.push(format!(
            "  protection_mode: {}{}",
            if self.protection_mode.enabled {
                "on"
            } else {
                "off"
            },
            self.protection_mode
                .reason
                .as_ref()
                .map(|r| format!(" ({})", r))
                .unwrap_or_default()
        ));

        if !verbose {
            if critical_count > 0 {
                lines.extend(render_operator_message(
                    "runtime diagnostics found critical failures",
                    "daemon state, bind target, or backend connectivity is broken",
                    "run 'bal doctor --verbose' and fix critical checks before 'bal status'",
                ));
            } else if warn_count > 0 {
                lines.extend(render_operator_message(
                    "runtime diagnostics found warnings",
                    "partial connectivity or port ownership needs confirmation",
                    "run 'bal status' now, then inspect details with 'bal doctor --verbose'",
                ));
            } else {
                lines.push("  next: run 'bal status'".to_string());
            }

            return lines.join("\n");
        }

        for check in &self.checks {
            lines.push(format!(
                "  - [{}] {}: {}",
                check.level.label(),
                check.name,
                check.summary
            ));
            if let Some(hint) = &check.hint {
                lines.push(format!("    hint: {}", hint));
            }
        }

        lines.join("\n")
    }
}

impl CheckLevel {
    fn label(&self) -> &'static str {
        match self {
            CheckLevel::Ok => "OK",
            CheckLevel::Warn => "WARN",
            CheckLevel::Critical => "CRITICAL",
        }
    }
}

pub async fn run_doctor(config_path: Option<PathBuf>) -> DoctorReport {
    let mut checks = Vec::new();
    let protection_mode = current_protection_mode();

    checks.push(check_pid_consistency());

    let resolved_config = match Config::resolve_config_path(config_path.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            checks.push(DoctorCheck {
                name: "config".to_string(),
                level: CheckLevel::Critical,
                summary: format!("cannot resolve config path: {}", err),
                hint: Some("Provide a config path with '--config <FILE>'".to_string()),
            });
            return DoctorReport {
                checks,
                protection_mode,
            };
        }
    };

    if !resolved_config.exists() {
        checks.push(DoctorCheck {
            name: "config".to_string(),
            level: CheckLevel::Critical,
            summary: format!("config file not found: {}", resolved_config.display()),
            hint: Some(format!(
                "Create a config file at {} or pass '--config <FILE>'",
                resolved_config.display()
            )),
        });
        return DoctorReport {
            checks,
            protection_mode,
        };
    }

    let config = match Config::load_from_file(&resolved_config).await {
        Ok(config) => {
            checks.push(DoctorCheck {
                name: "config".to_string(),
                level: CheckLevel::Ok,
                summary: format!("loaded {}", resolved_config.display()),
                hint: None,
            });
            config
        }
        Err(err) => {
            checks.push(DoctorCheck {
                name: "config".to_string(),
                level: CheckLevel::Critical,
                summary: format!("failed to load config: {}", err),
                hint: Some("Fix YAML syntax and required fields in the config file".to_string()),
            });
            return DoctorReport {
                checks,
                protection_mode,
            };
        }
    };

    checks.push(check_bindability(&config));
    checks.push(check_backends(&config).await);

    DoctorReport {
        checks,
        protection_mode,
    }
}

pub async fn run_and_print(config_path: Option<PathBuf>, json: bool, verbose: bool) -> Result<()> {
    let report = run_doctor(config_path).await;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.to_plain_text(verbose));
    }

    if report.has_critical_failure() {
        bail!("doctor found critical issues")
    }

    Ok(())
}

fn check_pid_consistency() -> DoctorCheck {
    let pid_path = get_pid_file_path();

    if !pid_path.exists() {
        return DoctorCheck {
            name: "pid".to_string(),
            level: CheckLevel::Ok,
            summary: "pid file absent and no daemon state conflict".to_string(),
            hint: None,
        };
    }

    match ProcessManager::read_pid_file() {
        Ok(pid) => {
            if ProcessManager::probe_process_running(pid) {
                DoctorCheck {
                    name: "pid".to_string(),
                    level: CheckLevel::Ok,
                    summary: format!("pid file is consistent (PID: {})", pid),
                    hint: None,
                }
            } else {
                DoctorCheck {
                    name: "pid".to_string(),
                    level: CheckLevel::Critical,
                    summary: format!("stale pid file detected (PID: {})", pid),
                    hint: Some(format!("Remove stale pid file: {}", pid_path.display())),
                }
            }
        }
        Err(err) => DoctorCheck {
            name: "pid".to_string(),
            level: CheckLevel::Critical,
            summary: format!("cannot read pid file: {}", err),
            hint: Some(format!(
                "Fix or remove corrupted pid file: {}",
                pid_path.display()
            )),
        },
    }
}

fn check_bindability(config: &Config) -> DoctorCheck {
    let bind_target = format!("{}:{}", config.bind_address, config.port);

    let socket_addr = match resolve_bind_target(&bind_target) {
        Ok(addr) => addr,
        Err(err) => {
            return DoctorCheck {
                name: "bind".to_string(),
                level: CheckLevel::Critical,
                summary: err,
                hint: Some("Set a valid IP or hostname in 'bind_address'".to_string()),
            }
        }
    };

    match TcpListener::bind(socket_addr) {
        Ok(listener) => {
            drop(listener);
            DoctorCheck {
                name: "bind".to_string(),
                level: CheckLevel::Ok,
                summary: format!("{} is bindable", bind_target),
                hint: None,
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            if ProcessManager::is_daemon_running() {
                DoctorCheck {
                    name: "bind".to_string(),
                    level: CheckLevel::Warn,
                    summary: format!("{} is already in use by an active process", bind_target),
                    hint: Some("Run 'bal status' to confirm it is the expected daemon".to_string()),
                }
            } else {
                DoctorCheck {
                    name: "bind".to_string(),
                    level: CheckLevel::Critical,
                    summary: format!("{} is already in use", bind_target),
                    hint: Some(
                        "Stop the conflicting process or update bind_address/port in config"
                            .to_string(),
                    ),
                }
            }
        }
        Err(err) => DoctorCheck {
            name: "bind".to_string(),
            level: CheckLevel::Critical,
            summary: format!("cannot bind {}: {}", bind_target, err),
            hint: Some("Check permissions and bind_address/port settings".to_string()),
        },
    }
}

async fn check_backends(config: &Config) -> DoctorCheck {
    let mut resolved_count = 0usize;
    let mut reachable_count = 0usize;
    let mut unresolved = Vec::new();
    let mut unreachable = Vec::new();

    for backend in &config.backends {
        let backend_addr = format!("{}:{}", backend.host, backend.port);

        match backend.resolve_socket_addr().await {
            Ok(_) => {
                resolved_count += 1;
                if backend.check_connectivity().await.is_ok() {
                    reachable_count += 1;
                } else {
                    unreachable.push(backend_addr);
                }
            }
            Err(_) => unresolved.push(backend_addr),
        }
    }

    let total = config.backends.len();

    if total == 0 {
        return DoctorCheck {
            name: "backend".to_string(),
            level: CheckLevel::Critical,
            summary: "no backends configured".to_string(),
            hint: Some("Configure at least one backend server".to_string()),
        };
    }

    let summary = format!(
        "resolvable {}/{} | reachable {}/{}",
        resolved_count, total, reachable_count, total
    );

    if reachable_count == 0 {
        return DoctorCheck {
            name: "backend".to_string(),
            level: CheckLevel::Critical,
            summary,
            hint: Some(
                "No backend is reachable. Verify backend host/port and network path".to_string(),
            ),
        };
    }

    if unresolved.is_empty() && unreachable.is_empty() {
        DoctorCheck {
            name: "backend".to_string(),
            level: CheckLevel::Ok,
            summary,
            hint: None,
        }
    } else {
        let mut hint_parts = Vec::new();
        if !unresolved.is_empty() {
            hint_parts.push(format!("Unresolved: {}", unresolved.join(", ")));
        }
        if !unreachable.is_empty() {
            hint_parts.push(format!("Unreachable: {}", unreachable.join(", ")));
        }

        DoctorCheck {
            name: "backend".to_string(),
            level: CheckLevel::Warn,
            summary,
            hint: Some(format!(
                "{} | Check DNS/firewall/service health",
                hint_parts.join("; ")
            )),
        }
    }
}

fn current_protection_mode() -> ProtectionModeSummary {
    if let Some(snapshot) = protection::read_snapshot() {
        return ProtectionModeSummary {
            enabled: snapshot.enabled,
            reason: snapshot.reason,
        };
    }

    ProtectionModeSummary {
        enabled: false,
        reason: None,
    }
}

fn resolve_bind_target(bind_target: &str) -> std::result::Result<SocketAddr, String> {
    bind_target
        .to_socket_addrs()
        .map_err(|err| format!("cannot resolve {}: {}", bind_target, err))?
        .next()
        .ok_or_else(|| format!("no socket address resolved for {}", bind_target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_report_marks_critical_failure_when_any_critical_exists() {
        let report = DoctorReport {
            checks: vec![
                DoctorCheck {
                    name: "config".to_string(),
                    level: CheckLevel::Ok,
                    summary: "config loaded".to_string(),
                    hint: None,
                },
                DoctorCheck {
                    name: "pid".to_string(),
                    level: CheckLevel::Critical,
                    summary: "stale pid file".to_string(),
                    hint: Some("Remove ~/.bal/bal.pid".to_string()),
                },
            ],
            protection_mode: ProtectionModeSummary {
                enabled: false,
                reason: None,
            },
        };

        assert!(report.has_critical_failure());
    }

    #[test]
    fn doctor_report_verbose_includes_hint_for_failed_check() {
        let report = DoctorReport {
            checks: vec![DoctorCheck {
                name: "bind".to_string(),
                level: CheckLevel::Critical,
                summary: "address is already in use".to_string(),
                hint: Some("Run 'bal status' to verify which process owns the port".to_string()),
            }],
            protection_mode: ProtectionModeSummary {
                enabled: true,
                reason: Some("timeout_or_refused_storm".to_string()),
            },
        };

        let rendered = report.to_plain_text(true);
        assert!(rendered.contains("bind"));
        assert!(rendered.contains("address is already in use"));
        assert!(rendered.contains("hint:"));
        assert!(rendered.contains("bal status"));
        assert!(rendered.contains("protection_mode: on"));
        assert!(rendered.contains("[CRITICAL] bind"));
    }

    #[test]
    fn doctor_report_default_concise_hides_ok_checks_and_hints() {
        let report = DoctorReport {
            checks: vec![
                DoctorCheck {
                    name: "pid".to_string(),
                    level: CheckLevel::Ok,
                    summary: "healthy".to_string(),
                    hint: None,
                },
                DoctorCheck {
                    name: "bind".to_string(),
                    level: CheckLevel::Warn,
                    summary: "already in use".to_string(),
                    hint: Some("Run 'bal status'".to_string()),
                },
            ],
            protection_mode: ProtectionModeSummary {
                enabled: false,
                reason: None,
            },
        };

        let rendered = report.to_plain_text(false);
        assert!(!rendered.contains("[OK]"));
        assert!(!rendered.contains("hint:"));
        assert!(!rendered.contains("[WARN]"));
        assert!(rendered.contains("what_happened:"));
        assert!(rendered.contains("why_likely:"));
        assert!(rendered.contains("do_this_now:"));

        let top_level_lines = rendered
            .lines()
            .filter(|line| line.starts_with("  "))
            .count();
        assert!(
            top_level_lines <= 8,
            "expected concise output, got: {rendered}"
        );
    }

    #[test]
    fn resolve_bind_target_rejects_invalid_host() {
        let result = resolve_bind_target("invalid host name:9295");
        assert!(result.is_err());
    }
}

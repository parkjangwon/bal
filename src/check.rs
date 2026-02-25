use anyhow::{bail, Result};
use serde::Serialize;
use std::path::PathBuf;

use crate::config::{Config, ConfigMode};

#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub config_path: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub mode: String,
    pub backend_count: usize,
}

impl CheckReport {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn to_plain_text(&self, verbose: bool) -> String {
        let mut lines = vec![
            "bal check".to_string(),
            format!(
                "  result: {}",
                if self.has_errors() { "FAILED" } else { "OK" }
            ),
            format!("  mode: {}", self.mode),
            format!("  backends: {}", self.backend_count),
            format!("  warnings: {}", self.warnings.len()),
        ];

        if verbose {
            lines.push(format!("  config: {}", self.config_path));

            if self.errors.is_empty() {
                lines.push("  errors: none".to_string());
            } else {
                lines.push(format!("  errors: {}", self.errors.len()));
                for error in &self.errors {
                    lines.push(format!("    - {}", error));
                }
            }

            if self.warnings.is_empty() {
                lines.push("  warning_details: none".to_string());
            } else {
                lines.push("  warning_details:".to_string());
                for warning in &self.warnings {
                    lines.push(format!("    - {}", warning));
                }
            }
        }

        lines.join("\n")
    }
}

pub async fn run_check(config_path: Option<PathBuf>) -> Result<CheckReport> {
    let path = if let Some(path) = config_path {
        path
    } else {
        Config::resolve_config_path(None)?
    };

    if !path.exists() {
        bail!("Configuration file not found: {}", path.display());
    }

    let config = Config::load_from_file(&path).await?;
    let mut warnings = Vec::new();

    if config.mode == ConfigMode::Simple {
        warnings.push("simple mode uses auto-tuned runtime defaults".to_string());
    }

    if config.bind_address == "0.0.0.0" {
        warnings.push("bind_address is 0.0.0.0 (listens on all interfaces)".to_string());
    }

    Ok(CheckReport {
        config_path: path.display().to_string(),
        errors: Vec::new(),
        warnings,
        mode: match config.mode {
            ConfigMode::Simple => "simple".to_string(),
            ConfigMode::Advanced => "advanced".to_string(),
        },
        backend_count: config.backends.len(),
    })
}

pub async fn run_and_print(
    config_path: Option<PathBuf>,
    strict: bool,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let report = run_check(config_path).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.to_plain_text(verbose));
    }

    if report.has_errors() || (strict && report.has_warnings()) {
        bail!("static check failed")
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> CheckReport {
        CheckReport {
            config_path: "/tmp/bal.yaml".to_string(),
            errors: Vec::new(),
            warnings: vec!["bind_address is 0.0.0.0 (listens on all interfaces)".to_string()],
            mode: "simple".to_string(),
            backend_count: 2,
        }
    }

    #[test]
    fn plain_text_default_is_concise() {
        let rendered = sample_report().to_plain_text(false);
        assert!(rendered.contains("bal check"));
        assert!(rendered.contains("warnings: 1"));
        assert!(!rendered.contains("warning_details:"));
        assert!(!rendered.contains("config:"));
    }

    #[test]
    fn plain_text_verbose_includes_details() {
        let rendered = sample_report().to_plain_text(true);
        assert!(rendered.contains("config: /tmp/bal.yaml"));
        assert!(rendered.contains("warning_details:"));
        assert!(rendered.contains("bind_address is 0.0.0.0"));
    }
}

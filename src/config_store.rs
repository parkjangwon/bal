//! Configuration store module
//!
//! Uses arc-swap for lock-free configuration hot-swapping.
//! This module ensures atomic configuration replacement while maintaining
//! backend state across config reloads.

use anyhow::{bail, Context, Result};
use log::{debug, info, warn};
use std::path::Path;

use crate::config::Config;
use crate::state::{AppState, RuntimeConfig};

/// Configuration store
///
/// Handles configuration file loading, validation, and hot-swapping.
pub struct ConfigStore;

impl ConfigStore {
    /// Validate and load configuration file
    ///
    /// Validates new configuration file and converts to RuntimeConfig if valid.
    /// Also pre-checks backend connectivity.
    pub async fn validate_and_load(path: &Path) -> Result<RuntimeConfig> {
        debug!("Loading configuration file: {}", path.display());

        // Load configuration file
        let config = Config::load_from_file(path)
            .await
            .context("Configuration file load failed")?;

        // Pre-validate backend connectivity
        info!("Pre-validating backend connectivity...");
        let mut failed_count = 0;

        for backend in &config.backends {
            match backend.check_connectivity().await {
                Ok(()) => {
                    debug!(
                        "  [OK] {}:{} - Connection successful",
                        backend.host, backend.port
                    );
                }
                Err(e) => {
                    warn!("  [FAIL] {}:{} - {}", backend.host, backend.port, e);
                    failed_count += 1;
                }
            }
        }

        if failed_count == config.backends.len() {
            bail!("Cannot connect to any backend. Please check your configuration.");
        }

        if failed_count > 0 {
            warn!(
                "{} backends are unreachable. Some traffic may fail.",
                failed_count
            );
        }

        info!("Configuration file validation passed");

        Ok(RuntimeConfig::from_config(config, path.to_path_buf()))
    }

    /// Validate a candidate config for reload without applying it.
    pub async fn validate_reload_candidate(path: &Path) -> Result<RuntimeConfig> {
        Self::validate_and_load(path)
            .await
            .with_context(|| format!("Pre-reload validation failed: {}", path.display()))
    }

    /// Perform configuration hot-swap
    ///
    /// 1. Load and validate new configuration file
    /// 2. Check backend connectivity
    /// 3. Atomically replace via arc-swap
    ///
    /// Does not affect existing connections.
    pub async fn reload_config(state: &AppState, new_path: Option<&Path>) -> Result<()> {
        let current_config = state.config();

        // Determine configuration file path
        let path = match new_path {
            Some(p) => p.to_path_buf(),
            None => current_config.config_path.clone(),
        };

        info!("Configuration reload starting: {}", path.display());

        // Load and validate new configuration
        let new_runtime_config = match Self::validate_reload_candidate(&path).await {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!(
                    "Configuration reload rejected. Keeping previous runtime configuration: {}",
                    e
                );
                return Err(e);
            }
        };

        // Check for port change
        if current_config.port != new_runtime_config.port {
            warn!(
                "Port change detected ({} -> {}). Port changes require a restart.",
                current_config.port, new_runtime_config.port
            );
        }

        // Replace configuration (atomic via arc-swap)
        state.swap_config(new_runtime_config);

        info!("Configuration successfully reloaded");
        Ok(())
    }

    /// Load initial configuration
    ///
    /// Loads configuration file at application startup, or creates default
    /// template if file doesn't exist.
    pub async fn load_initial_config(
        cli_path: Option<&Path>,
    ) -> Result<(RuntimeConfig, std::path::PathBuf)> {
        let path = if let Some(p) = cli_path {
            // Use path specified via CLI
            if !p.exists() {
                bail!(
                    "Specified configuration file does not exist: {}",
                    p.display()
                );
            }
            p.to_path_buf()
        } else {
            // Search default paths or create
            let home_path = crate::constants::get_home_config_path();

            if home_path.exists() {
                home_path
            } else {
                // Create default template if no config file exists
                info!("No configuration file found. Creating default template.");
                Config::init_default_file().await?;
                home_path
            }
        };

        info!("Loading configuration file: {}", path.display());
        let runtime_config = Self::validate_and_load(&path).await?;

        Ok((runtime_config, path))
    }
}

//! Process management module
//!
//! Handles PID file creation/management, process termination signals,
//! and process status checks. Operates based on home directory for
//! non-root user support.

use anyhow::{Result, bail};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::fs;
use std::io::Write;
use std::process;

use crate::constants::{get_pid_file_path, get_runtime_dir};
use crate::error::ResultExt;

/// Process manager
/// 
/// Identifies and controls daemon process via PID file.
pub struct ProcessManager;

impl ProcessManager {
    /// Write current process PID to file
    /// 
    /// If PID file already exists, considers it a duplicate execution and returns error.
    pub fn write_pid_file() -> Result<()> {
        let pid_path = get_pid_file_path();
        
        // Create runtime directory
        let runtime_dir = get_runtime_dir();
        std::fs::create_dir_all(&runtime_dir)
            .context_process(&format!("Failed to create runtime directory: {}", runtime_dir.display()))?;
        
        // Check existing PID file
        if pid_path.exists() {
            // Check if existing process is running
            if let Ok(old_pid) = Self::read_pid_file() {
                if Self::is_process_running(old_pid) {
                    bail!("bal is already running (PID: {}). Run 'bal stop' first.", old_pid);
                }
            }
            // Remove file if not running
            let _ = fs::remove_file(&pid_path);
        }
        
        // Write new PID file
        let pid = process::id();
        let mut file = fs::File::create(&pid_path)
            .context_process(&format!("Failed to create PID file: {}", pid_path.display()))?;
        
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
        
        let pid: i32 = content.trim()
            .parse::<i32>()
            .map_err(|e| anyhow::anyhow!("Invalid PID file content: {}", e))?;
        
        Ok(pid)
    }
    
    /// Remove PID file
    pub fn remove_pid_file() -> Result<()> {
        let pid_path = get_pid_file_path();
        
        if pid_path.exists() {
            fs::remove_file(&pid_path)
                .context_process(&format!("Failed to remove PID file: {}", pid_path.display()))?;
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
        let pid = Self::read_pid_file()
            .context_process("Cannot find running bal process. PID file does not exist or is corrupted.")?;
        
        if !Self::is_process_running(pid) {
            // Process already terminated - clean up file
            log::warn!("Process with PID {} does not exist. Cleaning up PID file.", pid);
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
        let pid = Self::read_pid_file()
            .context_process("Cannot find running bal process.")?;
        
        if !Self::is_process_running(pid) {
            bail!("bal is not running. Clean up the PID file and try again.");
        }
        
        // Send SIGHUP signal
        let nix_pid = Pid::from_raw(pid);
        signal::kill(nix_pid, Signal::SIGHUP)
            .map_err(|e| anyhow::anyhow!("Failed to send SIGHUP to process {}: {}", pid, e))?;
        
        log::info!("Sent configuration reload signal to bal process (PID: {})", pid);
        Ok(())
    }
    
    /// Check daemon running status
    pub fn is_daemon_running() -> bool {
        match Self::read_pid_file() {
            Ok(pid) => Self::is_process_running(pid),
            Err(_) => false,
        }
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

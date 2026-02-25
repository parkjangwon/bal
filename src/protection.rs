use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::backend_pool::BackendErrorKind;
use crate::constants::get_runtime_dir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtectionSnapshot {
    pub enabled: bool,
    pub reason: Option<String>,
    pub updated_at_ms: u64,
}

#[derive(Debug)]
pub struct ProtectionMode {
    enabled: AtomicBool,
    timeout_refused_window_count: AtomicU32,
    window_started_ms: AtomicU64,
    stable_success_count: AtomicU32,
    reason_code: AtomicU32,
    threshold: u32,
    window_ms: u64,
    stable_recoveries_required: u32,
}

const REASON_NONE: u32 = 0;
const REASON_TIMEOUT_REFUSED_STORM: u32 = 1;
const REASON_ALL_BACKENDS_UNAVAILABLE: u32 = 2;

impl ProtectionMode {
    pub fn new(threshold: u32, window_ms: u64, stable_recoveries_required: u32) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            timeout_refused_window_count: AtomicU32::new(0),
            window_started_ms: AtomicU64::new(now_unix_ms()),
            stable_success_count: AtomicU32::new(0),
            reason_code: AtomicU32::new(REASON_NONE),
            threshold,
            window_ms,
            stable_recoveries_required,
        }
    }

    pub fn record_failure(&self, kind: BackendErrorKind) -> bool {
        if matches!(
            kind,
            BackendErrorKind::Timeout | BackendErrorKind::ConnectionRefused
        ) {
            let now = now_unix_ms();
            let window_start = self.window_started_ms.load(Ordering::Relaxed);
            if now.saturating_sub(window_start) > self.window_ms {
                self.window_started_ms.store(now, Ordering::Relaxed);
                self.timeout_refused_window_count
                    .store(0, Ordering::Relaxed);
            }

            let storm_count = self
                .timeout_refused_window_count
                .fetch_add(1, Ordering::Relaxed)
                + 1;
            self.stable_success_count.store(0, Ordering::Relaxed);

            if storm_count >= self.threshold {
                return self.enable(REASON_TIMEOUT_REFUSED_STORM);
            }
        } else {
            self.stable_success_count.store(0, Ordering::Relaxed);
        }

        false
    }

    pub fn record_global_unavailable(&self) -> bool {
        self.stable_success_count.store(0, Ordering::Relaxed);
        self.enable(REASON_ALL_BACKENDS_UNAVAILABLE)
    }

    pub fn record_success(&self) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }

        let stable = self.stable_success_count.fetch_add(1, Ordering::Relaxed) + 1;
        if stable >= self.stable_recoveries_required {
            self.disable();
            return true;
        }

        false
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> ProtectionSnapshot {
        ProtectionSnapshot {
            enabled: self.enabled.load(Ordering::Relaxed),
            reason: reason_label(self.reason_code.load(Ordering::Relaxed)),
            updated_at_ms: now_unix_ms(),
        }
    }

    fn enable(&self, reason_code: u32) -> bool {
        self.reason_code.store(reason_code, Ordering::Relaxed);
        !self.enabled.swap(true, Ordering::Relaxed)
    }

    fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
        self.reason_code.store(REASON_NONE, Ordering::Relaxed);
        self.timeout_refused_window_count
            .store(0, Ordering::Relaxed);
        self.window_started_ms
            .store(now_unix_ms(), Ordering::Relaxed);
        self.stable_success_count.store(0, Ordering::Relaxed);
    }
}

pub fn protection_state_path() -> PathBuf {
    get_runtime_dir().join("protection_state.json")
}

pub fn write_snapshot(snapshot: &ProtectionSnapshot) {
    let runtime_dir = get_runtime_dir();
    if std::fs::create_dir_all(&runtime_dir).is_err() {
        return;
    }

    let path = protection_state_path();
    if let Ok(encoded) = serde_json::to_vec_pretty(snapshot) {
        let _ = std::fs::write(path, encoded);
    }
}

pub fn read_snapshot() -> Option<ProtectionSnapshot> {
    let path = protection_state_path();
    let content = std::fs::read(path).ok()?;
    serde_json::from_slice(&content).ok()
}

fn reason_label(code: u32) -> Option<String> {
    match code {
        REASON_TIMEOUT_REFUSED_STORM => Some("timeout_or_refused_storm".to_string()),
        REASON_ALL_BACKENDS_UNAVAILABLE => Some("all_backends_unavailable".to_string()),
        _ => None,
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enables_on_timeout_storm_and_recovers_after_stable_successes() {
        let mode = ProtectionMode::new(2, 60_000, 2);

        assert!(!mode.is_enabled());
        mode.record_failure(BackendErrorKind::Timeout);
        assert!(!mode.is_enabled());
        mode.record_failure(BackendErrorKind::ConnectionRefused);
        assert!(mode.is_enabled());

        mode.record_success();
        assert!(mode.is_enabled());

        mode.record_success();
        assert!(!mode.is_enabled());
    }

    #[test]
    fn enables_immediately_when_all_backends_are_unavailable() {
        let mode = ProtectionMode::new(10, 60_000, 3);
        mode.record_global_unavailable();

        let snapshot = mode.snapshot();
        assert!(snapshot.enabled);
        assert_eq!(snapshot.reason.as_deref(), Some("all_backends_unavailable"));
    }
}

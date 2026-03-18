use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config;

/// Persisted state for update checks, stored in update.toml.
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct UpdateState {
    /// ISO 8601 timestamp of last successful API check.
    #[serde(default)]
    pub last_checked: Option<String>,
    /// Cached latest version from last check.
    #[serde(default)]
    pub latest_version: Option<String>,
    /// Versions the user explicitly chose to skip.
    #[serde(default)]
    pub skipped_versions: Vec<String>,
}

/// Return the path to update.toml (same directory as config.toml).
pub fn state_file_path() -> Option<PathBuf> {
    config::default_config_path().map(|p| p.with_file_name("update.toml"))
}

/// Load update state from update.toml. Returns default if file missing or unparseable.
pub fn load_state() -> UpdateState {
    let Some(path) = state_file_path() else {
        return UpdateState::default();
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| toml::from_str(&content).ok())
        .unwrap_or_default()
}

/// Save update state to update.toml. Silently ignores errors.
pub fn save_state(state: &UpdateState) {
    let Some(path) = state_file_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(content) = toml::to_string_pretty(state) {
        let _ = std::fs::write(&path, content);
    }
}

/// Compare two semver strings (e.g. "0.1.1" vs "0.2.0").
/// Returns true if `remote` is newer than `local`.
pub fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Option<(u32, u32, u32)> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    };
    match (parse(remote), parse(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

/// The compiled-in version of this binary.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

const GITHUB_API_URL: &str =
    "https://api.github.com/repos/grimurjonsson/lux/releases/latest";

/// Check if enough days have elapsed since last_checked.
pub fn check_interval_elapsed(state: &UpdateState, interval_days: u32) -> bool {
    if interval_days == 0 {
        return false;
    }
    let Some(ref last) = state.last_checked else {
        return true; // never checked
    };
    // Parse ISO 8601 date portion (YYYY-MM-DD) and compare to today
    let last_date = &last[..10]; // "2026-03-18"
    let today = now_iso8601();
    let today_date = &today[..10];
    // Simple day-difference: parse as days-since-epoch and compare
    days_since(today_date).saturating_sub(days_since(last_date)) >= interval_days as i64
}

/// Fetch the latest release version from GitHub via curl.
/// Returns None on any error (network, parse, timeout).
pub fn fetch_latest_version() -> Option<String> {
    let output = std::process::Command::new("curl")
        .args([
            "--connect-timeout", "5",
            "--max-time", "10",
            "-s",
            GITHUB_API_URL,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let re = Regex::new(r#""tag_name"\s*:\s*"v?([^"]+)""#).ok()?;
    let caps = re.captures(&body)?;
    Some(caps[1].to_string())
}

/// Return current UTC time as ISO 8601 string (e.g. "2026-03-18T14:30:00Z").
/// Uses `date -u +%Y-%m-%dT%H:%M:%SZ` to avoid adding a time crate.
pub fn now_iso8601() -> String {
    std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// Rough days-since-epoch from a "YYYY-MM-DD" string.
/// Only needs to be accurate enough for interval comparison.
fn days_since(date: &str) -> i64 {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return 0;
    }
    let y: i64 = parts[0].parse().unwrap_or(0);
    let m: i64 = parts[1].parse().unwrap_or(0);
    let d: i64 = parts[2].parse().unwrap_or(0);
    y * 365 + m * 30 + d // approximate, only used for >= comparison
}

/// Result of a background update check.
pub struct CheckResult {
    pub new_version: Option<String>,
}

/// Run the background update check. Called from a spawned thread.
///
/// - Reads state file and config interval
/// - Skips if disabled or interval not elapsed
/// - Fetches latest version from GitHub if needed
/// - Updates state file
/// - Returns whether a newer version was found
pub fn background_check(interval_days: u32, update_mode: Option<&str>) -> CheckResult {
    // Disabled via config
    if update_mode == Some("disabled") || interval_days == 0 {
        return CheckResult { new_version: None };
    }

    let mut state = load_state();
    let current = current_version();

    // Check if we need to hit the API or can use cached value
    let latest = if check_interval_elapsed(&state, interval_days) {
        // Interval elapsed — fetch fresh
        if let Some(version) = fetch_latest_version() {
            state.last_checked = Some(now_iso8601());
            state.latest_version = Some(version.clone());
            save_state(&state);
            version
        } else {
            // Fetch failed — use cached if available
            match state.latest_version {
                Some(ref v) => v.clone(),
                None => return CheckResult { new_version: None },
            }
        }
    } else {
        // Interval not elapsed — use cached
        match state.latest_version {
            Some(ref v) => v.clone(),
            None => return CheckResult { new_version: None },
        }
    };

    // Compare
    if is_newer(&latest, current) && !state.skipped_versions.contains(&latest) {
        CheckResult {
            new_version: Some(latest),
        }
    } else {
        CheckResult { new_version: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_patch() {
        assert!(is_newer("0.1.2", "0.1.1"));
    }

    #[test]
    fn newer_minor() {
        assert!(is_newer("0.2.0", "0.1.9"));
    }

    #[test]
    fn newer_major() {
        assert!(is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn same_version() {
        assert!(!is_newer("0.1.1", "0.1.1"));
    }

    #[test]
    fn older_version() {
        assert!(!is_newer("0.1.0", "0.1.1"));
    }

    #[test]
    fn strips_v_prefix() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.2.0", "v0.1.0"));
    }

    #[test]
    fn invalid_version_returns_false() {
        assert!(!is_newer("abc", "0.1.0"));
        assert!(!is_newer("0.1.0", "abc"));
        assert!(!is_newer("", ""));
    }

    #[test]
    fn state_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let state_path = tmp.path().join("update.toml");

        let state = UpdateState {
            last_checked: Some("2026-03-18T14:30:00Z".to_string()),
            latest_version: Some("0.2.0".to_string()),
            skipped_versions: vec!["0.1.2".to_string()],
        };

        let content = toml::to_string_pretty(&state).unwrap();
        std::fs::write(&state_path, &content).unwrap();

        let loaded: UpdateState =
            toml::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
        assert_eq!(loaded.last_checked, Some("2026-03-18T14:30:00Z".to_string()));
        assert_eq!(loaded.latest_version, Some("0.2.0".to_string()));
        assert_eq!(loaded.skipped_versions, vec!["0.1.2"]);
    }

    #[test]
    fn missing_state_returns_default() {
        let state: UpdateState = toml::from_str("").unwrap();
        assert!(state.last_checked.is_none());
        assert!(state.latest_version.is_none());
        assert!(state.skipped_versions.is_empty());
    }

    #[test]
    fn current_version_is_valid() {
        let v = current_version();
        assert!(v.split('.').count() == 3, "version should be X.Y.Z, got {v}");
    }

    #[test]
    fn interval_elapsed_never_checked() {
        let state = UpdateState::default();
        assert!(check_interval_elapsed(&state, 7));
    }

    #[test]
    fn interval_not_elapsed_recent() {
        let state = UpdateState {
            last_checked: Some(now_iso8601()),
            ..Default::default()
        };
        assert!(!check_interval_elapsed(&state, 7));
    }

    #[test]
    fn interval_disabled_zero() {
        let state = UpdateState::default();
        assert!(!check_interval_elapsed(&state, 0));
    }

    #[test]
    fn interval_elapsed_old_date() {
        let state = UpdateState {
            last_checked: Some("2020-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };
        assert!(check_interval_elapsed(&state, 7));
    }
}

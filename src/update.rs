use std::path::PathBuf;

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
}

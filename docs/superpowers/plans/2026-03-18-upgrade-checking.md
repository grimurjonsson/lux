# Upgrade Checking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add background version checking on normal runs and a `lux update` subcommand for explicit upgrade — never blocking normal output.

**Architecture:** New `src/update.rs` module owns all logic: state file I/O, GitHub API check via curl subprocess, semver comparison, interactive prompt, and direct binary download. Background thread spawns early in `main()`, result is checked (non-blocking) after output completes. `lux update` runs synchronously. All messaging to stderr only.

**Tech Stack:** Rust std threads, `std::process::Command` (curl, uname, tar, mv), TOML serde for state file, regex for JSON parsing

**Spec:** `docs/superpowers/specs/2026-03-18-upgrade-checking-design.md`

---

### Task 1: Add config fields and `Update` subcommand

**Files:**
- Modify: `src/config.rs:42-57` (add fields to `Config` struct)
- Modify: `src/cli.rs:98-110` (add `Update` variant to `Command` enum)

- [ ] **Step 1: Add `update_check_interval_days` and `update_mode` to Config struct**

In `src/config.rs`, add two new fields to the `Config` struct (after the `syntax_map` field):

```rust
/// Days between automatic update checks (default: 7, set to 0 to disable)
#[serde(default = "default_update_interval")]
pub update_check_interval_days: u32,
/// Update behavior: "notify", "auto", or "disabled". None = first-time prompt.
#[serde(default)]
pub update_mode: Option<String>,
```

Add the default function near the existing `default_scope()`:

```rust
fn default_update_interval() -> u32 {
    7
}
```

- [ ] **Step 2: Add `Update` variant to `Command` enum**

In `src/cli.rs`, add to the `Command` enum:

```rust
/// Check for updates and upgrade interactively
Update,
```

- [ ] **Step 3: Add CLI parsing test for `lux update`**

Add to the `tests` module in `src/cli.rs`:

```rust
    #[test]
    fn test_update_subcommand() {
        let cli = Cli::try_parse_from(["lux", "update"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Update)));
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/cli.rs
git commit -m "feat: add update config fields and Update subcommand variant"
```

---

### Task 2: Version comparison and state file I/O

**Files:**
- Create: `src/update.rs`
- Modify: `src/lib.rs` (add `pub mod update;`)

This task creates the foundation of the update module: version comparison, state file reading/writing, and the config path helper. No network calls yet.

- [ ] **Step 1: Create `src/update.rs` with state types and version comparison**

```rust
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
```

- [ ] **Step 2: Add `pub mod update;` to `src/lib.rs`**

Add after the `pub mod wizard;` line:

```rust
pub mod update;
```

- [ ] **Step 3: Write tests for version comparison and state I/O**

Add at the bottom of `src/update.rs`:

```rust
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test update`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/update.rs src/lib.rs
git commit -m "feat: add update module with version comparison and state file I/O"
```

---

### Task 3: GitHub API check via curl

**Files:**
- Modify: `src/update.rs`

Adds the function that shells out to curl to fetch the latest release version from GitHub.

- [ ] **Step 1: Add `fetch_latest_version` and `check_interval_elapsed` functions**

Add these functions to `src/update.rs` (before the `#[cfg(test)]` block):

```rust
use regex::Regex;

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
```

- [ ] **Step 2: Add tests for interval checking**

Add to the `tests` module:

```rust
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
```

- [ ] **Step 3: Run tests**

Run: `cargo test update`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add src/update.rs
git commit -m "feat: add GitHub API check via curl and interval logic"
```

---

### Task 4: Background check function

**Files:**
- Modify: `src/update.rs`

Adds the function that the background thread will run: checks interval, fetches if needed, updates state, returns whether a new version is available.

- [ ] **Step 1: Add `background_check` function**

Add to `src/update.rs`:

```rust
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test update`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add src/update.rs
git commit -m "feat: add background_check function for update thread"
```

---

### Task 5: Interactive prompt, preference saving, and binary download

**Files:**
- Modify: `src/update.rs`
- Modify: `src/config.rs` (add `set_config_field` helper)

Adds the interactive prompt, preference saving, and `perform_update` (direct binary download). These are in the same task because `handle_update_result` calls `perform_update`.

- [ ] **Step 1: Add generic `set_config_field` to `src/config.rs`**

Add after the `set_default_profile` function. This follows the same pattern but works for any top-level string field:

```rust
/// Set or clear a top-level string field in the config file.
///
/// Reads the raw TOML, updates/inserts the field, and writes back.
/// Creates the config file if it doesn't exist.
pub fn set_config_field(field: &str, value: Option<&str>) -> Result<()> {
    let path = default_config_path()
        .ok_or_else(|| anyhow!("cannot determine config path: HOME not set"))?;

    let content = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|e| anyhow!("failed to read {}: {e}", path.display()))?
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow!("failed to create {}: {e}", parent.display()))?;
        }
        String::new()
    };

    let has_field = content.lines().any(|l| l.trim_start().starts_with(field));
    let new_content = if let Some(val) = value {
        if has_field {
            content
                .lines()
                .map(|l| {
                    if l.trim_start().starts_with(field) {
                        format!("{field} = \"{val}\"")
                    } else {
                        l.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + if content.ends_with('\n') { "\n" } else { "" }
        } else {
            format!("{field} = \"{val}\"\n{content}")
        }
    } else {
        content
            .lines()
            .filter(|l| !l.trim_start().starts_with(field))
            .collect::<Vec<_>>()
            .join("\n")
            + if content.ends_with('\n') { "\n" } else { "" }
    };

    std::fs::write(&path, new_content)
        .map_err(|e| anyhow!("failed to write {}: {e}", path.display()))?;

    Ok(())
}
```

- [ ] **Step 2: Add all prompt, action, and download functions to `src/update.rs`**

Add these imports at the top of `src/update.rs`:

```rust
use std::io::{self, BufRead, Write};
```

Add these functions (before `#[cfg(test)]`):

```rust
/// Check if stderr is a TTY.
fn stderr_is_tty() -> bool {
    use std::os::unix::fs::FileTypeExt;
    // If /dev/stderr is a character device (terminal), it's a TTY
    std::fs::metadata("/dev/stderr")
        .map(|m| m.file_type().is_char_device())
        .unwrap_or(false)
}

/// Actions from the interactive prompt.
#[derive(Debug, PartialEq)]
pub enum UpdateAction {
    UpdateNow,
    SetNotify,
    SetAuto,
    SkipVersion,
    Disable,
    Notify,
}

/// Show the interactive update prompt. Returns the action chosen.
/// Only call when stderr is a TTY.
pub fn show_update_prompt(new_version: &str) -> UpdateAction {
    let current = current_version();
    eprintln!();
    eprintln!("lux v{new_version} is available (you have v{current})");
    eprintln!();
    eprintln!("  1) Update now");
    eprintln!("  2) Always notify me (don't ask again)");
    eprintln!("  3) Always auto-update (don't ask again)");
    eprintln!("  4) Skip this version");
    eprintln!("  5) Disable update checks");
    eprintln!();
    eprint!("Choose [1-5]: ");
    let _ = io::stderr().flush();

    let mut input = String::new();
    let stdin = io::stdin();
    if stdin.lock().read_line(&mut input).is_err() {
        return UpdateAction::Notify;
    }

    match input.trim() {
        "1" => UpdateAction::UpdateNow,
        "2" => UpdateAction::SetNotify,
        "3" => UpdateAction::SetAuto,
        "4" => UpdateAction::SkipVersion,
        "5" => UpdateAction::Disable,
        _ => UpdateAction::Notify, // default to notify on bad input
    }
}

/// Handle the result of an update check, showing prompt or notification.
/// Called after normal output is complete.
pub fn handle_update_result(new_version: &str, update_mode: Option<&str>) {
    match update_mode {
        Some("disabled") => {}
        Some("notify") => {
            eprintln!(
                "\nlux v{new_version} available — run 'lux update' to upgrade"
            );
        }
        Some("auto") => {
            perform_update(new_version);
        }
        _ => {
            // No preference set yet
            if stderr_is_tty() {
                let action = show_update_prompt(new_version);
                execute_action(action, new_version);
            } else {
                eprintln!(
                    "\nlux v{new_version} available — run 'lux update' to upgrade"
                );
            }
        }
    }
}

/// Execute the chosen action from the interactive prompt.
fn execute_action(action: UpdateAction, new_version: &str) {
    match action {
        UpdateAction::UpdateNow => {
            perform_update(new_version);
        }
        UpdateAction::SetNotify => {
            if let Err(e) = config::set_config_field("update_mode", Some("notify")) {
                eprintln!("lux: failed to save preference: {e}");
            } else {
                eprintln!("Preference saved. Future updates will show a notification.");
            }
        }
        UpdateAction::SetAuto => {
            if let Err(e) = config::set_config_field("update_mode", Some("auto")) {
                eprintln!("lux: failed to save preference: {e}");
            } else {
                eprintln!("Preference saved. Future updates will install automatically.");
            }
        }
        UpdateAction::SkipVersion => {
            let mut state = load_state();
            if !state.skipped_versions.contains(&new_version.to_string()) {
                state.skipped_versions.push(new_version.to_string());
                save_state(&state);
            }
            eprintln!("Skipped v{new_version}. You won't be notified about this version again.");
        }
        UpdateAction::Disable => {
            if let Err(e) = config::set_config_field("update_mode", Some("disabled")) {
                eprintln!("lux: failed to save preference: {e}");
            } else {
                eprintln!("Update checks disabled. Re-enable in ~/.config/lux/config.toml");
            }
        }
        UpdateAction::Notify => {
            eprintln!(
                "\nlux v{new_version} available — run 'lux update' to upgrade"
            );
        }
    }
}

/// Detect the platform target string (e.g. "aarch64-apple-darwin").
fn detect_target() -> Option<String> {
    let os_output = std::process::Command::new("uname").arg("-s").output().ok()?;
    let arch_output = std::process::Command::new("uname").arg("-m").output().ok()?;

    let os = String::from_utf8_lossy(&os_output.stdout).trim().to_lowercase();
    let arch_raw = String::from_utf8_lossy(&arch_output.stdout).trim().to_string();

    let arch = match arch_raw.as_str() {
        "x86_64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        _ => return None,
    };

    let platform = match os.as_str() {
        "darwin" => "apple-darwin",
        "linux" => "unknown-linux-gnu",
        _ => return None,
    };

    Some(format!("{arch}-{platform}"))
}

/// Download and replace the current binary with the new version.
pub fn perform_update(new_version: &str) {
    let Some(target) = detect_target() else {
        eprintln!("lux: could not detect platform");
        return;
    };

    let current_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("lux: could not determine binary path: {e}");
            return;
        }
    };

    let url = format!(
        "https://github.com/grimurjonsson/lux/releases/download/v{new_version}/lux-{target}.tar.gz"
    );

    eprintln!("Downloading lux v{new_version}...");

    let tmp_dir = format!("/tmp/lux-update-{}", std::process::id());
    let tmp_archive = format!("{tmp_dir}/lux.tar.gz");

    // Create temp dir
    let _ = std::fs::create_dir_all(&tmp_dir);

    // Download
    let download = std::process::Command::new("curl")
        .args([
            "-L", "-f",
            "--connect-timeout", "10",
            "--max-time", "120",
            "-o", &tmp_archive,
            &url,
        ])
        .status();

    match download {
        Ok(s) if s.success() => {}
        _ => {
            eprintln!("lux: download failed");
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return;
        }
    }

    // Extract
    let extract = std::process::Command::new("tar")
        .args(["-xzf", &tmp_archive, "-C", &tmp_dir])
        .status();

    match extract {
        Ok(s) if s.success() => {}
        _ => {
            eprintln!("lux: extraction failed");
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return;
        }
    }

    let extracted = format!("{tmp_dir}/lux");
    if !std::path::Path::new(&extracted).exists() {
        eprintln!("lux: binary not found in archive");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return;
    }

    // Replace binary via mv (atomic if same filesystem).
    // If the directory requires elevated permissions, mv will fail and we report it.
    let mv = std::process::Command::new("mv")
        .args([&extracted, &current_exe.to_string_lossy().to_string()])
        .status();

    match mv {
        Ok(s) if s.success() => {
            // Ensure executable
            let _ = std::process::Command::new("chmod")
                .args(["+x", &current_exe.to_string_lossy().to_string()])
                .status();
            eprintln!("Updated to lux v{new_version}");
        }
        _ => {
            eprintln!("lux: failed to replace binary. Try the install script:");
            eprintln!(
                "  curl -fsSL https://raw.githubusercontent.com/grimurjonsson/lux/main/scripts/install.sh | bash"
            );
        }
    }

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

/// Run the `lux update` subcommand. Synchronous, always hits the API.
pub fn run_update_command() {
    let current = current_version();

    eprintln!("Checking for updates...");

    let Some(latest) = fetch_latest_version() else {
        eprintln!("lux: could not check for updates (network error or curl not found)");
        return;
    };

    if !is_newer(&latest, current) {
        eprintln!("lux v{current} is up to date");
        return;
    }

    // Always show interactive prompt for explicit `lux update`
    let action = show_update_prompt(&latest);
    execute_action(action, &latest);
}
```

- [ ] **Step 3: Add test for `set_config_field`**

Add to the `tests` module in `src/config.rs`:

```rust
    #[test]
    fn set_config_field_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("lux").join("config.toml");

        // Temporarily override HOME so default_config_path resolves to our temp dir
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());

        // Set a field on a new file
        set_config_field("update_mode", Some("notify")).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("update_mode = \"notify\""));

        // Update the field
        set_config_field("update_mode", Some("auto")).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("update_mode = \"auto\""));
        assert!(!content.contains("notify"));

        // Clear the field
        set_config_field("update_mode", None).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("update_mode"));

        // Clean up env var
        std::env::remove_var("XDG_CONFIG_HOME");
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/update.rs src/config.rs
git commit -m "feat: add interactive update prompt, preference saving, and binary download"
```

---

### Task 6: Handle `Command::Update` in main.rs

**Files:**
- Modify: `src/main.rs` (handle `Command::Update`)

- [ ] **Step 1: Add the `Command::Update` match arm**

In `src/main.rs`, add a new match arm inside the `if let Some(cmd) = &cli.command` block, after the `Command::Profile { action } => { ... }` arm (after line 61):

```rust
Command::Update => {
    lux::update::run_update_command();
    std::process::exit(0);
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 3: Manual smoke test**

Run: `cargo run -- update`
Expected: "Checking for updates..." then either "up to date" or the interactive prompt

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: handle 'lux update' subcommand in main"
```

---

### Task 7: Background thread integration in main.rs

**Files:**
- Modify: `src/main.rs`

Wire the background check thread into the main flow: spawn after config load, check (non-blocking) after output completes.

- [ ] **Step 1: Add background thread spawn and non-blocking check**

In `src/main.rs`, add `use std::thread;` at the top.

After config loading (after line 110: `let config = config::load_config(...)`), spawn the background thread:

```rust
// Spawn background update check thread (non-blocking)
let update_interval = config
    .as_ref()
    .map(|(c, _)| c.update_check_interval_days)
    .unwrap_or(7);
let update_mode_for_thread = config
    .as_ref()
    .and_then(|(c, _)| c.update_mode.clone());
let update_handle = thread::spawn(move || {
    lux::update::background_check(
        update_interval,
        update_mode_for_thread.as_deref(),
    )
});
```

Then, at the very end of the `run()` function (just before `Ok(())`), add the non-blocking check:

```rust
// Check if background update thread finished (never block)
if update_handle.is_finished() {
    if let Ok(result) = update_handle.join() {
        if let Some(ref new_version) = result.new_version {
            let update_mode = config
                .as_ref()
                .and_then(|(c, _)| c.update_mode.as_deref());
            lux::update::handle_update_result(new_version, update_mode);
        }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 3: Manual smoke test**

Run: `echo "hello world" | cargo run`
Expected: Colored output, no hang, no update message (first run, interval not elapsed yet or no newer version)

Run: `cargo run -- update`
Expected: "Checking for updates..." then either "up to date" or the interactive prompt

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire background update check into main pipeline"
```

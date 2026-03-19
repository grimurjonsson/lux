use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use owo_colors::{OwoColorize, Style, XtermColors};
use serde::Deserialize;

/// A single rule entry from the TOML config file.
#[derive(Deserialize, Debug, Clone)]
pub struct RuleConfig {
    pub pattern: String,
    pub style: String,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "line".to_string()
}

fn default_update_interval() -> u32 {
    7
}

/// A named profile containing its own set of rules and optional trigger settings.
#[derive(Deserialize, Debug, Clone)]
pub struct ProfileConfig {
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    /// Trigger patterns — suppress output until a match, then show context window.
    #[serde(default)]
    pub trigger: Vec<String>,
    /// Context before trigger: line count or regex boundary pattern.
    pub before: Option<String>,
    /// Context after trigger: line count or regex boundary pattern.
    pub after: Option<String>,
    /// Number of initial lines to show (like -n flag).
    pub lines: Option<String>,
    /// File extensions that auto-select this profile (e.g., ["md", "markdown"]).
    #[serde(default)]
    pub extensions: Vec<String>,
}

/// Top-level config structure matching the TOML schema.
#[derive(Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
    #[serde(default)]
    pub default_profile: Option<String>,
    /// Syntect color theme name (e.g. "Solarized (dark)").
    #[serde(default)]
    pub theme: Option<String>,
    /// Custom syntax mappings: extension or filename → syntax name.
    /// e.g. { "justfile" = "Makefile", "tf" = "HCL" }
    #[serde(default)]
    pub syntax_map: HashMap<String, String>,
    /// Days between automatic update checks (default: 7, set to 0 to disable)
    #[serde(default = "default_update_interval")]
    pub update_check_interval_days: u32,
    /// Update behavior: "notify", "auto", or "disabled". None = first-time prompt.
    #[serde(default)]
    pub update_mode: Option<String>,
    /// Default file mode: "less" (pager) or "cat" (print-and-exit). None = "less".
    #[serde(default)]
    pub default_file_mode: Option<String>,
}

/// Return the default config file path using XDG_CONFIG_HOME or $HOME/.config.
///
/// On macOS, intentionally uses ~/.config (NOT ~/Library/Application Support)
/// per locked project decision.
pub fn default_config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("lux").join("config.toml"));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config").join("lux").join("config.toml"))
}

/// Load config from an explicit path or the default XDG location.
///
/// - If `explicit_path` is Some: error if file doesn't exist, read and parse.
/// - If `explicit_path` is None: use default_config_path(), return Ok(None) if not found.
/// - Malformed TOML returns an error with file path context.
pub fn load_config(explicit_path: Option<&Path>) -> Result<Option<(Config, PathBuf)>> {
    let path = if let Some(p) = explicit_path {
        if !p.exists() {
            return Err(anyhow!("config file not found: {}", p.display()));
        }
        p.to_path_buf()
    } else {
        match default_config_path() {
            Some(p) if p.exists() => p,
            _ => return Ok(None),
        }
    };

    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow!("failed to read {}: {e}", path.display()))?;

    let config: Config = toml::from_str(&content)
        .map_err(|e| anyhow!("failed to parse {}: {e}", path.display()))?;

    Ok(Some((config, path)))
}

/// Set or clear the default_profile field in the config file.
///
/// Reads the raw TOML, updates/inserts the `default_profile` line, and writes back.
/// Creates the config file if it doesn't exist.
pub fn set_default_profile(explicit_path: Option<&Path>, name: Option<&str>) -> Result<()> {
    let path = if let Some(p) = explicit_path {
        p.to_path_buf()
    } else {
        default_config_path()
            .ok_or_else(|| anyhow!("cannot determine config path: HOME not set"))?
    };

    let content = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|e| anyhow!("failed to read {}: {e}", path.display()))?
    } else {
        // Create parent dirs and start with empty config
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow!("failed to create {}: {e}", parent.display()))?;
        }
        String::new()
    };

    // Update or insert the default_profile line
    let has_field = content.lines().any(|l| l.trim_start().starts_with("default_profile"));
    let new_content = if let Some(profile_name) = name {
        if has_field {
            content
                .lines()
                .map(|l| {
                    if l.trim_start().starts_with("default_profile") {
                        format!("default_profile = \"{profile_name}\"")
                    } else {
                        l.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + if content.ends_with('\n') { "\n" } else { "" }
        } else {
            // Insert at the top, before any [[rules]] or [profiles]
            format!("default_profile = \"{profile_name}\"\n{content}")
        }
    } else {
        // Clear: remove the line
        content
            .lines()
            .filter(|l| !l.trim_start().starts_with("default_profile"))
            .collect::<Vec<_>>()
            .join("\n")
            + if content.ends_with('\n') { "\n" } else { "" }
    };

    std::fs::write(&path, new_content)
        .map_err(|e| anyhow!("failed to write {}: {e}", path.display()))?;

    Ok(())
}

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

/// Return built-in profiles that ship with lux.
///
/// Only "logs" remains as a built-in. Syntect handles syntax highlighting
/// for source code files (markdown, yaml, toml, shell, etc.).
pub fn builtin_profiles() -> HashMap<String, ProfileConfig> {
    let mut profiles = HashMap::new();
    profiles.insert(
        "logs".to_string(),
        ProfileConfig {
            rules: vec![
                RuleConfig { pattern: r"(?i)fatal|critical".to_string(), style: "bold+red".to_string(), scope: "line".to_string() },
                RuleConfig { pattern: r"(?i)error".to_string(), style: "red".to_string(), scope: "line".to_string() },
                RuleConfig { pattern: r"(?i)warn".to_string(), style: "yellow".to_string(), scope: "line".to_string() },
                RuleConfig { pattern: r"(?i)info".to_string(), style: "white".to_string(), scope: "line".to_string() },
                RuleConfig { pattern: r"(?i)debug".to_string(), style: "dim".to_string(), scope: "line".to_string() },
                RuleConfig { pattern: r"(?i)trace".to_string(), style: "240".to_string(), scope: "line".to_string() },
            ],
            trigger: vec![],
            before: None,
            after: None,
            lines: None,
            extensions: vec!["log".to_string()],
        },
    );
    profiles.insert(
        "help".to_string(),
        ProfileConfig {
            rules: vec![
                // 1. Section headers: "Usage:", "Commands:", "Options:", etc.
                RuleConfig { pattern: r"^[A-Z][a-zA-Z ]+:$".to_string(), style: "bold+cyan".to_string(), scope: "line".to_string() },
                // 2. Long flags: --color, --profile, --strip-ansi
                RuleConfig { pattern: r"--[a-zA-Z][-a-zA-Z0-9]*".to_string(), style: "bold+green".to_string(), scope: "match".to_string() },
                // 3. Short flags: -r, -f, -h (word boundary prevents matching inside long flags)
                RuleConfig { pattern: r"-[a-zA-Z]\b".to_string(), style: "bold+green".to_string(), scope: "match".to_string() },
                // 4. Placeholder values: <RULES>, <COLOR>, [FILE], [OPTIONS]
                RuleConfig { pattern: r"<[A-Z_]+>|\[[A-Z_]+\]".to_string(), style: "cyan".to_string(), scope: "match".to_string() },
                // 5. Default values: [default: auto], [default: 20]
                RuleConfig { pattern: r"\[default: [^\]]+\]".to_string(), style: "dim".to_string(), scope: "match".to_string() },
                // 6. Possible values: [possible values: auto, always, never]
                RuleConfig { pattern: r"\[possible values: [^\]]+\]".to_string(), style: "dim".to_string(), scope: "match".to_string() },
                // 7. Subcommand names: indented words at start of line
                RuleConfig { pattern: r"^  \w[-\w]*".to_string(), style: "yellow".to_string(), scope: "match".to_string() },
                // 8. Quoted strings: "10", "+5", "^==="
                RuleConfig { pattern: r#""[^"]*""#.to_string(), style: "magenta".to_string(), scope: "match".to_string() },
                // 9. Tool name in Usage line
                RuleConfig { pattern: r"Usage: (\w+)".to_string(), style: "bold+white".to_string(), scope: "cap1".to_string() },
            ],
            trigger: vec![],
            before: None,
            after: None,
            lines: None,
            extensions: vec![],
        },
    );
    profiles
}

/// Detect a profile by sniffing the first lines of stdin content.
///
/// Currently detects help text (clap/argparse style) by looking for "Usage:" and
/// flag patterns like "--option" or section headers like "Options:".
pub fn detect_profile_from_content(lines: &[String]) -> Option<String> {
    let has_usage = lines.iter().any(|l| l.contains("Usage:"));
    let has_flags = lines.iter().any(|l| l.contains("--"));
    let has_section = lines.iter().any(|l| {
        let trimmed = l.trim();
        trimmed == "Options:" || trimmed == "Commands:" || trimmed == "Arguments:"
    });

    if has_usage && (has_flags || has_section) {
        return Some("help".to_string());
    }

    // Detect log content: look for common log level keywords
    let log_level_count = lines.iter().filter(|l| {
        let upper = l.to_uppercase();
        upper.contains("ERROR") || upper.contains("WARN") || upper.contains("INFO")
            || upper.contains("DEBUG") || upper.contains("TRACE")
    }).count();

    // If at least 2 of the first 10 lines contain log levels, treat as logs
    if log_level_count >= 2 {
        return Some("logs".to_string());
    }

    None
}

/// Find a profile name that matches the given file extension.
///
/// Searches both user-defined and built-in profiles. User profiles take priority.
pub fn find_profile_by_extension(
    extension: &str,
    merged_profiles: &HashMap<String, ProfileConfig>,
) -> Option<String> {
    for (name, profile) in merged_profiles {
        if profile.extensions.iter().any(|ext| ext.eq_ignore_ascii_case(extension)) {
            return Some(name.clone());
        }
    }
    None
}

/// Print available profiles from the config file to stdout.
///
/// Loads config from the given explicit path or the default XDG location.
/// Prints profile names sorted alphabetically with rule counts, then the config path.
pub fn print_profiles(config_path: Option<&Path>) -> Result<()> {
    owo_colors::set_override(true);
    let config = load_config(config_path)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let result = print_profiles_to(config_path, config.as_ref().map(|(c, p)| (c, p.clone())), &mut out);
    owo_colors::set_override(false);
    result
}

/// Write profile listing to the given writer (testable version).
pub fn print_profiles_to(
    _explicit_path: Option<&Path>,
    config: Option<(&Config, PathBuf)>,
    out: &mut dyn Write,
) -> Result<()> {
    let builtins = builtin_profiles();

    // Collect user-defined profile names for deduplication
    let user_profile_names: std::collections::HashSet<&String> = config
        .as_ref()
        .map(|(cfg, _)| cfg.profiles.keys().collect())
        .unwrap_or_default();

    // Determine which built-in profiles to show (those not overridden by user)
    let visible_builtins: Vec<&String> = builtins
        .keys()
        .filter(|k| !user_profile_names.contains(k))
        .collect();

    match config {
        None => {
            if visible_builtins.is_empty() {
                writeln!(out, "No config file found.")?;
                if let Some(default) = default_config_path() {
                    writeln!(out, "Default location: {}", default.display())?;
                }
            } else {
                // Show built-in profiles even without config
                writeln!(out, "{}", "Available profiles:".bold())?;
                writeln!(out)?;
                let mut names: Vec<&String> = visible_builtins;
                names.sort();
                for name in &names {
                    print_profile_entry(out, name, &builtins[*name], true)?;
                }
                writeln!(out)?;
                writeln!(
                    out,
                    "{} lux --profile {} {}",
                    "Usage:".dimmed(),
                    "<name>".cyan(),
                    "<file>".dimmed(),
                )?;
                writeln!(
                    out,
                    "       command | lux --profile {}",
                    "<name>".cyan(),
                )?;
            }
        }
        Some((cfg, path)) => {
            let has_user_profiles = !cfg.profiles.is_empty();
            let has_builtin_profiles = !visible_builtins.is_empty();

            if !has_user_profiles && !has_builtin_profiles {
                writeln!(out, "No profiles defined in {}", path.display())?;
            } else {
                writeln!(out, "{}", "Available profiles:".bold())?;
                writeln!(out)?;

                // Show user-defined profiles first
                let mut names: Vec<&String> = cfg.profiles.keys().collect();
                names.sort();
                for name in &names {
                    let profile = &cfg.profiles[*name];
                    print_profile_entry(out, name, profile, false)?;
                }

                // Show built-in profiles not overridden by user
                let mut builtin_names: Vec<&String> = visible_builtins;
                builtin_names.sort();
                for name in &builtin_names {
                    print_profile_entry(out, name, &builtins[*name], true)?;
                }

                writeln!(out)?;
                if has_user_profiles {
                    writeln!(out, "{} {}", "Config:".dimmed(), path.display())?;
                    writeln!(out)?;
                }
                writeln!(
                    out,
                    "{} lux --profile {} {}",
                    "Usage:".dimmed(),
                    "<name>".cyan(),
                    "<file>".dimmed(),
                )?;
                writeln!(
                    out,
                    "       command | lux --profile {}",
                    "<name>".cyan(),
                )?;
            }
        }
    }
    Ok(())
}

/// Print a single profile entry in the listing.
fn print_profile_entry(
    out: &mut dyn Write,
    name: &str,
    profile: &ProfileConfig,
    is_builtin: bool,
) -> Result<()> {
    if is_builtin {
        writeln!(out, "  {} {}", name.cyan().bold(), "(built-in)".dimmed())?;
    } else {
        writeln!(out, "  {}", name.cyan().bold())?;
    }

    for rc in &profile.rules {
        let rule_display = if rc.scope == "line" || rc.scope.is_empty() {
            format!("{}:{}", rc.pattern, rc.style)
        } else {
            format!("{}:{}:{}", rc.pattern, rc.style, rc.scope)
        };
        writeln!(out, "    {}  {}", "style:".dimmed(), rule_display)?;
    }
    if !profile.trigger.is_empty() {
        for t in &profile.trigger {
            writeln!(out, "    {} {t}", "trigger:".dimmed())?;
        }
    }
    if let Some(ref b) = profile.before {
        writeln!(out, "    {}  {b}", "before:".dimmed())?;
    }
    if let Some(ref a) = profile.after {
        writeln!(out, "    {}   {a}", "after:".dimmed())?;
    }
    if let Some(ref l) = profile.lines {
        writeln!(out, "    {}   {l}", "lines:".dimmed())?;
    }
    if !profile.extensions.is_empty() {
        let parts: Vec<String> = profile.extensions.iter().map(|e| format!(".{e}")).collect();
        writeln!(
            out,
            "    {} {}",
            "auto:".dimmed(),
            parts.join(", ")
        )?;
    }
    Ok(())
}

/// Print the color/style catalog to stdout with forced color output.
pub fn print_colors() {
    owo_colors::set_override(true);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    print_colors_to(&mut out);
    owo_colors::set_override(false);
}

/// Write color catalog to the given writer (testable version).
///
/// Note: When called from tests, colors may not render to the buffer,
/// but the text content is verified.
pub fn print_colors_to(out: &mut dyn Write) {
    // ANSI Colors
    let _ = writeln!(out, "ANSI Colors:");
    let ansi_colors: &[(&str, Style)] = &[
        ("black", Style::new().black()),
        ("red", Style::new().red()),
        ("green", Style::new().green()),
        ("yellow", Style::new().yellow()),
        ("blue", Style::new().blue()),
        ("magenta", Style::new().magenta()),
        ("cyan", Style::new().cyan()),
        ("white", Style::new().white()),
        ("bright-black", Style::new().bright_black()),
        ("bright-red", Style::new().bright_red()),
        ("bright-green", Style::new().bright_green()),
        ("bright-yellow", Style::new().bright_yellow()),
        ("bright-blue", Style::new().bright_blue()),
        ("bright-magenta", Style::new().bright_magenta()),
        ("bright-cyan", Style::new().bright_cyan()),
        ("bright-white", Style::new().bright_white()),
    ];
    for (name, style) in ansi_colors {
        let _ = writeln!(out, "  {}", name.style(*style));
    }

    // Extended Colors
    let _ = writeln!(out);
    let _ = writeln!(out, "Extended Colors:");
    let extended_colors: &[(&str, u8)] = &[
        ("orange", 208),
        ("pink", 213),
        ("grey", 245),
        ("purple", 129),
        ("teal", 30),
        ("lime", 118),
        ("brown", 130),
        ("olive", 142),
        ("coral", 209),
        ("lavender", 183),
    ];
    for (name, index) in extended_colors {
        let style = Style::new().color(XtermColors::from(*index));
        let _ = writeln!(out, "  {}", name.style(style));
    }

    // Styles
    let _ = writeln!(out);
    let _ = writeln!(out, "Styles:");
    let _ = writeln!(out, "  {}", "bold".style(Style::new().bold()));
    let _ = writeln!(out, "  {}", "dim".style(Style::new().dimmed()));
    let _ = writeln!(out, "  {}", "italic".style(Style::new().italic()));
    let _ = writeln!(out, "  {}", "underline".style(Style::new().underline()));

    // Formats
    let _ = writeln!(out);
    let _ = writeln!(out, "Formats:");
    let _ = writeln!(out, "  #rrggbb  Hex RGB (e.g., #ff0000)");
    let _ = writeln!(out, "  0-255    256-color index (e.g., 196)");
    let _ = writeln!(out, "  bg-COLOR Background (e.g., bg-red, bg-#00ff00)");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    unsafe fn set_env(key: &str, val: &str) {
        unsafe { std::env::set_var(key, val) }
    }

    unsafe fn remove_env(key: &str) {
        unsafe { std::env::remove_var(key) }
    }

    unsafe fn restore_env(key: &str, prev: Option<String>) {
        match prev {
            Some(v) => unsafe { set_env(key, &v) },
            None => unsafe { remove_env(key) },
        }
    }

    #[test]
    fn load_config_none_no_file() {
        // With an explicit None and no file at default path, returns Ok(None)
        let tmp = TempDir::new().unwrap();
        let prev = std::env::var("XDG_CONFIG_HOME").ok();
        unsafe { set_env("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()) };

        let result = load_config(None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        unsafe { restore_env("XDG_CONFIG_HOME", prev) };
    }

    #[test]
    fn load_config_none_with_file() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join("lux");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.toml");
        std::fs::write(&config_path, "[[rules]]\npattern = \"ERROR\"\nstyle = \"red\"\n").unwrap();

        let prev = std::env::var("XDG_CONFIG_HOME").ok();
        unsafe { set_env("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()) };

        let result = load_config(None).unwrap();
        assert!(result.is_some());
        let (config, path) = result.unwrap();
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].pattern, "ERROR");
        assert_eq!(path, config_path);

        unsafe { restore_env("XDG_CONFIG_HOME", prev) };
    }

    #[test]
    fn load_config_explicit_existing() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("custom.toml");
        std::fs::write(
            &config_path,
            "[[rules]]\npattern = \"WARN\"\nstyle = \"yellow\"\n",
        )
        .unwrap();

        let result = load_config(Some(&config_path)).unwrap();
        assert!(result.is_some());
        let (config, _) = result.unwrap();
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].pattern, "WARN");
    }

    #[test]
    fn load_config_explicit_missing() {
        let result = load_config(Some(Path::new("/nonexistent/missing.toml")));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not found"), "Got: {msg}");
    }

    #[test]
    fn load_config_malformed_toml() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("bad.toml");
        std::fs::write(&config_path, "[[rules]\nbad toml here\n").unwrap();

        let result = load_config(Some(&config_path));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("failed to parse"), "Got: {msg}");
        assert!(msg.contains(&config_path.display().to_string()), "Got: {msg}");
    }

    #[test]
    fn config_rules_deserialized() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[[rules]]
pattern = "ERROR"
style = "bold+red"
scope = "match"

[[rules]]
pattern = "WARN"
style = "yellow"
"#,
        )
        .unwrap();

        let (config, _) = load_config(Some(&config_path)).unwrap().unwrap();
        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.rules[0].scope, "match");
        assert_eq!(config.rules[1].scope, "line"); // default
    }

    #[test]
    fn config_profiles_deserialized() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[profiles.django]
[[profiles.django.rules]]
pattern = "django"
style = "green"
scope = "match"

[profiles.spring]
[[profiles.spring.rules]]
pattern = "spring"
style = "blue"
"#,
        )
        .unwrap();

        let (config, _) = load_config(Some(&config_path)).unwrap().unwrap();
        assert_eq!(config.profiles.len(), 2);
        assert!(config.profiles.contains_key("django"));
        assert!(config.profiles.contains_key("spring"));
        assert_eq!(config.profiles["django"].rules.len(), 1);
        assert_eq!(config.profiles["spring"].rules[0].scope, "line"); // default
    }

    #[test]
    fn scope_defaults_to_line_when_omitted() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "[[rules]]\npattern = \"test\"\nstyle = \"red\"\n",
        )
        .unwrap();

        let (config, _) = load_config(Some(&config_path)).unwrap().unwrap();
        assert_eq!(config.rules[0].scope, "line");
    }

    #[test]
    fn default_config_path_uses_xdg() {
        let prev = std::env::var("XDG_CONFIG_HOME").ok();
        unsafe { set_env("XDG_CONFIG_HOME", "/custom/xdg") };

        let path = default_config_path().unwrap();
        assert_eq!(path, PathBuf::from("/custom/xdg/lux/config.toml"));

        unsafe { restore_env("XDG_CONFIG_HOME", prev) };
    }

    // === print_profiles tests ===

    fn strip_ansi(s: &str) -> String {
        let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        re.replace_all(s, "").to_string()
    }

    fn profile_output(config_path: &Path) -> String {
        let mut buf = Vec::new();
        print_profiles_to(
            None,
            Some((&load_config(Some(config_path)).unwrap().unwrap().0, config_path.to_path_buf())),
            &mut buf,
        )
        .unwrap();
        strip_ansi(&String::from_utf8(buf).unwrap())
    }

    #[test]
    fn print_profiles_no_config_file() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("nonexistent.toml");
        let mut buf = Vec::new();
        print_profiles_to(Some(&nonexistent), None, &mut buf).unwrap();
        let output = strip_ansi(&String::from_utf8(buf).unwrap());
        // Even without a config file, built-in profiles are shown
        assert!(output.contains("logs"), "Expected built-in logs profile. Got: {output}");
        assert!(output.contains("(built-in)"), "Expected built-in tag. Got: {output}");
    }

    #[test]
    fn print_profiles_with_profiles() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[profiles.django]
[[profiles.django.rules]]
pattern = "django"
style = "green"

[profiles.spring-boot]
[[profiles.spring-boot.rules]]
pattern = "spring1"
style = "blue"

[[profiles.spring-boot.rules]]
pattern = "spring2"
style = "red"
"#,
        )
        .unwrap();

        let output = profile_output(&config_path);
        assert!(output.contains("django"), "Got: {output}");
        assert!(output.contains("style:  django:green"), "Got: {output}");
        assert!(output.contains("style:  spring1:blue"), "Got: {output}");
        assert!(output.contains("style:  spring2:red"), "Got: {output}");
        assert!(output.contains(&config_path.display().to_string()), "Got: {output}");
    }

    #[test]
    fn print_profiles_no_user_profiles_shows_builtins() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "[[rules]]\npattern = \"test\"\nstyle = \"red\"\n",
        )
        .unwrap();

        let mut buf = Vec::new();
        print_profiles_to(None, Some((&load_config(Some(&config_path)).unwrap().unwrap().0, config_path.clone())), &mut buf).unwrap();
        let output = strip_ansi(&String::from_utf8(buf).unwrap());
        // Built-in profiles should still be shown
        assert!(output.contains("logs"), "Expected built-in logs profile. Got: {output}");
        assert!(output.contains("(built-in)"), "Expected built-in tag. Got: {output}");
    }

    #[test]
    fn print_profiles_sorted_alphabetically() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[profiles.zebra]
[[profiles.zebra.rules]]
pattern = "z"
style = "red"

[profiles.alpha]
[[profiles.alpha.rules]]
pattern = "a"
style = "green"
"#,
        )
        .unwrap();

        let output = profile_output(&config_path);
        let alpha_pos = output.find("alpha").unwrap();
        let zebra_pos = output.find("zebra").unwrap();
        assert!(alpha_pos < zebra_pos, "Expected alpha before zebra in: {output}");
    }

    #[test]
    fn print_profiles_with_trigger_settings() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[profiles.unittest-errors]
trigger = ['\.go:[0-9]+$']
before = "^===.*"
after = "^---.*"
lines = "+1"
"#,
        )
        .unwrap();

        let output = profile_output(&config_path);
        assert!(output.contains("    trigger:"), "Expected trigger info. Got: {output}");
        assert!(output.contains("    before:  ^==="), "Expected before info. Got: {output}");
        assert!(output.contains("    after:   ^---"), "Expected after info. Got: {output}");
        assert!(output.contains("    lines:   +1"), "Expected lines info. Got: {output}");
        // The unittest-errors profile itself should have no style rules
        // (built-in profiles will have them, but we check only the unittest-errors section)
        let unittest_section = output.split("unittest-errors").nth(1).unwrap_or("");
        // Split at the next profile entry (indicated by "(built-in)" tag for built-in profiles)
        let unittest_before_next = unittest_section.split("(built-in)").next().unwrap_or(unittest_section);
        assert!(!unittest_before_next.contains("style:"), "unittest-errors should not show style rules. Got: {output}");
    }

    // === print_colors tests ===

    #[test]
    fn print_colors_contains_ansi_section() {
        let mut buf = Vec::new();
        print_colors_to(&mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("ANSI Colors"), "Got: {output}");
        assert!(output.contains("red"), "Got: {output}");
        assert!(output.contains("green"), "Got: {output}");
        assert!(output.contains("blue"), "Got: {output}");
    }

    #[test]
    fn print_colors_contains_extended_section() {
        let mut buf = Vec::new();
        print_colors_to(&mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Extended Colors"), "Got: {output}");
        assert!(output.contains("orange"), "Got: {output}");
        assert!(output.contains("purple"), "Got: {output}");
    }

    #[test]
    fn print_colors_contains_styles_section() {
        let mut buf = Vec::new();
        print_colors_to(&mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Styles"), "Got: {output}");
        assert!(output.contains("bold"), "Got: {output}");
        assert!(output.contains("dim"), "Got: {output}");
        assert!(output.contains("italic"), "Got: {output}");
        assert!(output.contains("underline"), "Got: {output}");
    }

    #[test]
    fn print_colors_contains_formats_section() {
        let mut buf = Vec::new();
        print_colors_to(&mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Formats"), "Got: {output}");
        assert!(output.contains("#rrggbb"), "Got: {output}");
        assert!(output.contains("0-255"), "Got: {output}");
        assert!(output.contains("bg-"), "Got: {output}");
    }

    // === builtin_profiles tests ===

    #[test]
    fn builtin_profiles_contains_logs() {
        let profiles = builtin_profiles();
        assert!(profiles.contains_key("logs"), "Expected 'logs' built-in profile");
        let logs = &profiles["logs"];
        assert!(!logs.rules.is_empty(), "Logs profile should have rules");
        assert_eq!(logs.extensions, vec!["log"]);
        assert_eq!(logs.lines, None);
    }

    #[test]
    fn builtin_profiles_count() {
        let profiles = builtin_profiles();
        assert_eq!(profiles.len(), 2, "Expected 'logs' and 'help' built-in profiles. Got: {:?}", profiles.keys().collect::<Vec<_>>());
    }

    #[test]
    fn builtin_profiles_contains_help() {
        let profiles = builtin_profiles();
        assert!(profiles.contains_key("help"), "Expected 'help' built-in profile");
        let help = &profiles["help"];
        assert!(!help.rules.is_empty(), "Help profile should have rules");
        assert!(help.extensions.is_empty(), "Help profile should not have file extensions");
    }

    #[test]
    fn user_profile_overrides_builtin() {
        let mut buf = Vec::new();
        let config = Config {
            rules: vec![],
            default_profile: None,
            theme: None,
            syntax_map: HashMap::new(),
            update_check_interval_days: 7,
            update_mode: None,
            default_file_mode: None,
            profiles: {
                let mut p = HashMap::new();
                p.insert(
                    "logs".to_string(),
                    ProfileConfig {
                        rules: vec![RuleConfig {
                            pattern: "CUSTOM".to_string(),
                            style: "red".to_string(),
                            scope: "line".to_string(),
                        }],
                        trigger: vec![],
                        before: None,
                        after: None,
                        lines: None,
                        extensions: vec![],
                    },
                );
                p
            },
        };
        print_profiles_to(
            None,
            Some((&config, PathBuf::from("/tmp/test.toml"))),
            &mut buf,
        )
        .unwrap();
        let output = strip_ansi(&String::from_utf8(buf).unwrap());
        // User logs profile should appear without (built-in) tag
        assert!(output.contains("logs"), "Expected logs in output. Got: {output}");
        assert!(!output.contains("logs (built-in)"), "User override should not show built-in tag for logs. Got: {output}");
        assert!(output.contains("CUSTOM"), "Expected user's CUSTOM rule. Got: {output}");
    }

    #[test]
    fn find_profile_by_extension_log() {
        let profiles = builtin_profiles();
        let result = find_profile_by_extension("log", &profiles);
        assert_eq!(result, Some("logs".to_string()));
    }

    #[test]
    fn find_profile_by_extension_unknown() {
        let profiles = builtin_profiles();
        let result = find_profile_by_extension("txt", &profiles);
        assert!(result.is_none());
    }

    #[test]
    fn default_config_path_falls_back_to_home() {
        let prev_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        let prev_home = std::env::var("HOME").ok();
        unsafe { remove_env("XDG_CONFIG_HOME") };
        unsafe { set_env("HOME", "/test/home") };

        let path = default_config_path().unwrap();
        assert_eq!(path, PathBuf::from("/test/home/.config/lux/config.toml"));

        unsafe { restore_env("XDG_CONFIG_HOME", prev_xdg) };
        unsafe { restore_env("HOME", prev_home) };
    }

    #[test]
    fn config_default_file_mode_deserialized() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "default_file_mode = \"less\"\n").unwrap();

        let (config, _) = load_config(Some(&config_path)).unwrap().unwrap();
        assert_eq!(config.default_file_mode.as_deref(), Some("less"));
    }

    #[test]
    fn config_default_file_mode_none_when_omitted() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "[[rules]]\npattern = \"x\"\nstyle = \"red\"\n").unwrap();

        let (config, _) = load_config(Some(&config_path)).unwrap().unwrap();
        assert!(config.default_file_mode.is_none());
    }

    #[test]
    fn set_config_field_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let prev = std::env::var("XDG_CONFIG_HOME").ok();
        unsafe { set_env("XDG_CONFIG_HOME", tmp.path().to_str().unwrap()) };

        let config_path = tmp.path().join("lux").join("config.toml");

        // Set a field on a new file
        set_config_field("update_mode", Some("notify")).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains(r#"update_mode = "notify""#));

        // Update the field
        set_config_field("update_mode", Some("auto")).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains(r#"update_mode = "auto""#));
        assert!(!content.contains("notify"));

        // Clear the field
        set_config_field("update_mode", None).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("update_mode"));

        unsafe { restore_env("XDG_CONFIG_HOME", prev) };
    }
}

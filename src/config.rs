use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use owo_colors::{OwoColorize, Style, XtermColors};
use serde::Deserialize;

/// A single rule entry from the TOML config file.
#[derive(Deserialize, Debug)]
pub struct RuleConfig {
    pub pattern: String,
    pub style: String,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "line".to_string()
}

/// A named profile containing its own set of rules.
#[derive(Deserialize, Debug)]
pub struct ProfileConfig {
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

/// Top-level config structure matching the TOML schema.
#[derive(Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

/// Return the default config file path using XDG_CONFIG_HOME or $HOME/.config.
///
/// On macOS, intentionally uses ~/.config (NOT ~/Library/Application Support)
/// per locked project decision.
pub fn default_config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("ctail").join("config.toml"));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config").join("ctail").join("config.toml"))
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

/// Print available profiles from the config file to stdout.
///
/// Loads config from the given explicit path or the default XDG location.
/// Prints profile names sorted alphabetically with rule counts, then the config path.
pub fn print_profiles(config_path: Option<&Path>) -> Result<()> {
    let config = load_config(config_path)?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    print_profiles_to(config_path, config.as_ref().map(|(c, p)| (c, p.clone())), &mut out)
}

/// Write profile listing to the given writer (testable version).
pub fn print_profiles_to(
    _explicit_path: Option<&Path>,
    config: Option<(&Config, PathBuf)>,
    out: &mut dyn Write,
) -> Result<()> {
    match config {
        None => {
            writeln!(out, "No config file found.")?;
            if let Some(default) = default_config_path() {
                writeln!(out, "Default location: {}", default.display())?;
            }
        }
        Some((cfg, path)) => {
            if cfg.profiles.is_empty() {
                writeln!(out, "No profiles defined in {}", path.display())?;
            } else {
                let mut names: Vec<&String> = cfg.profiles.keys().collect();
                names.sort();
                for name in names {
                    let count = cfg.profiles[name].rules.len();
                    let label = if count == 1 { "rule" } else { "rules" };
                    writeln!(out, "  {name} ({count} {label})")?;
                }
                writeln!(out)?;
                writeln!(out, "Config: {}", path.display())?;
            }
        }
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
    let _ = writeln!(out, "  bg:COLOR Background (e.g., bg:red, bg:#00ff00)");
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
        let config_dir = tmp.path().join("ctail");
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
        assert_eq!(path, PathBuf::from("/custom/xdg/ctail/config.toml"));

        unsafe { restore_env("XDG_CONFIG_HOME", prev) };
    }

    // === print_profiles tests ===

    #[test]
    fn print_profiles_no_config_file() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("nonexistent.toml");
        let mut buf = Vec::new();
        print_profiles_to(Some(&nonexistent), None, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No config file found"), "Got: {output}");
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

        let mut buf = Vec::new();
        print_profiles_to(None, Some((&load_config(Some(&config_path)).unwrap().unwrap().0, config_path.clone())), &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("django (1 rule)"), "Got: {output}");
        assert!(output.contains("spring-boot (2 rules)"), "Got: {output}");
        assert!(output.contains(&config_path.display().to_string()), "Got: {output}");
    }

    #[test]
    fn print_profiles_no_profiles_defined() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "[[rules]]\npattern = \"test\"\nstyle = \"red\"\n",
        )
        .unwrap();

        let mut buf = Vec::new();
        print_profiles_to(None, Some((&load_config(Some(&config_path)).unwrap().unwrap().0, config_path.clone())), &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No profiles defined"), "Got: {output}");
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

        let mut buf = Vec::new();
        print_profiles_to(None, Some((&load_config(Some(&config_path)).unwrap().unwrap().0, config_path.clone())), &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let alpha_pos = output.find("alpha").unwrap();
        let zebra_pos = output.find("zebra").unwrap();
        assert!(alpha_pos < zebra_pos, "Expected alpha before zebra in: {output}");
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
        assert!(output.contains("bg:"), "Got: {output}");
    }

    #[test]
    fn default_config_path_falls_back_to_home() {
        let prev_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        let prev_home = std::env::var("HOME").ok();
        unsafe { remove_env("XDG_CONFIG_HOME") };
        unsafe { set_env("HOME", "/test/home") };

        let path = default_config_path().unwrap();
        assert_eq!(path, PathBuf::from("/test/home/.config/ctail/config.toml"));

        unsafe { restore_env("XDG_CONFIG_HOME", prev_xdg) };
        unsafe { restore_env("HOME", prev_home) };
    }
}

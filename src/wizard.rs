use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::{bail, Result};
use owo_colors::OwoColorize;

use crate::color;
use crate::config;

/// Run the interactive new-profile wizard.
pub fn run_new_profile(config_path: Option<&Path>) -> Result<()> {
    with_color(|reader, out| run_wizard(reader, out, config_path))
}

/// Run the interactive edit-profile wizard.
pub fn run_edit_profile(config_path: Option<&Path>, name: Option<&str>) -> Result<()> {
    with_color(|reader, out| run_edit(reader, out, config_path, name))
}

/// Run the interactive delete-profile wizard.
pub fn run_delete_profile(config_path: Option<&Path>, name: Option<&str>) -> Result<()> {
    with_color(|reader, out| run_delete(reader, out, config_path, name))
}

fn with_color(
    f: impl FnOnce(&mut io::StdinLock<'_>, &mut io::StdoutLock<'_>) -> Result<()>,
) -> Result<()> {
    owo_colors::set_override(true);
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let result = f(&mut reader, &mut out);
    owo_colors::set_override(false);
    result
}

fn run_wizard(
    reader: &mut impl BufRead,
    out: &mut impl Write,
    explicit_config: Option<&Path>,
) -> Result<()> {
    writeln!(out, "{}", "Create a new ctail profile".bold())?;
    writeln!(out)?;

    // 1. Profile name
    let name = ask(reader, out, "Profile name", None)?;
    if name.is_empty() {
        bail!("profile name cannot be empty");
    }

    // Check for existing profile
    let config_path = if let Some(p) = explicit_config {
        p.to_path_buf()
    } else {
        config::default_config_path()
            .ok_or_else(|| anyhow::anyhow!("cannot determine config path"))?
    };

    if config_path.exists() {
        if let Ok(Some((cfg, _))) = config::load_config(Some(&config_path)) {
            if cfg.profiles.contains_key(&name) {
                writeln!(
                    out,
                    "  {} profile '{}' already exists in {}",
                    "warning:".yellow().bold(),
                    name.cyan(),
                    config_path.display(),
                )?;
                let overwrite = ask(reader, out, "Overwrite? (y/N)", Some("N"))?;
                if !overwrite.eq_ignore_ascii_case("y") {
                    writeln!(out, "Aborted.")?;
                    return Ok(());
                }
            }
        }
    }

    writeln!(out)?;

    // 2. Trigger patterns
    writeln!(
        out,
        "{}",
        "Trigger patterns (suppress output until a match):".dimmed()
    )?;
    let mut triggers: Vec<String> = Vec::new();
    loop {
        let prompt = if triggers.is_empty() {
            "Trigger regex (empty to skip)"
        } else {
            "Another trigger (empty to finish)"
        };
        let t = ask(reader, out, prompt, None)?;
        if t.is_empty() {
            break;
        }
        // Validate regex
        if let Err(e) = regex::Regex::new(&t) {
            writeln!(out, "  {} {e}", "invalid regex:".red())?;
            continue;
        }
        triggers.push(t);
    }

    // 3. Before/after context (only if triggers were set)
    let mut before: Option<String> = None;
    let mut after: Option<String> = None;
    if !triggers.is_empty() {
        writeln!(out)?;
        writeln!(
            out,
            "{}",
            "Context window (line count or regex boundary):".dimmed()
        )?;
        let b = ask(reader, out, "Before context (empty for default 20)", None)?;
        if !b.is_empty() {
            validate_context_spec(&b, out);
            before = Some(b);
        }
        let a = ask(reader, out, "After context (empty for default 20)", None)?;
        if !a.is_empty() {
            validate_context_spec(&a, out);
            after = Some(a);
        }
    }

    // 4. Lines
    writeln!(out)?;
    writeln!(
        out,
        "{}",
        "Initial lines to show (e.g. \"10\", \"+1\" for from start):".dimmed()
    )?;
    let lines_input = ask(reader, out, "Lines (empty to skip)", None)?;
    let lines = if lines_input.is_empty() {
        None
    } else {
        Some(lines_input)
    };

    // 5. Coloring rules
    writeln!(out)?;
    writeln!(out, "{}", "Coloring rules:".dimmed())?;
    let mut rules: Vec<(String, String, String)> = Vec::new();
    while let Some(rule) = ask_rule_or_raw(reader, out)? {
        rules.push(rule);
    }

    // 6. Preview
    writeln!(out)?;
    writeln!(out, "{}", "Preview:".bold())?;
    writeln!(out)?;
    let toml = build_toml(&name, &triggers, &before, &after, &lines, &rules);
    for line in toml.lines() {
        writeln!(out, "  {}", line.dimmed())?;
    }
    writeln!(out)?;

    let confirm = ask(reader, out, "Save this profile? (Y/n)", Some("Y"))?;
    if confirm.eq_ignore_ascii_case("n") {
        writeln!(out, "Aborted.")?;
        return Ok(());
    }

    // 7. Write to config file
    write_profile(&config_path, &name, &toml)?;

    writeln!(out)?;
    writeln!(
        out,
        "{} profile '{}' to {}",
        "Saved".green().bold(),
        name.cyan(),
        config_path.display(),
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "{} ctail --profile {} {}",
        "Usage:".dimmed(),
        name.cyan(),
        "<file>".dimmed(),
    )?;

    Ok(())
}

/// Resolve config path and load existing config. Bail if no config or no profiles.
fn load_existing_config(
    explicit_config: Option<&Path>,
) -> Result<(config::Config, std::path::PathBuf)> {
    let config_path = if let Some(p) = explicit_config {
        p.to_path_buf()
    } else {
        config::default_config_path()
            .ok_or_else(|| anyhow::anyhow!("cannot determine config path"))?
    };

    let (cfg, path) = config::load_config(Some(&config_path))?
        .ok_or_else(|| anyhow::anyhow!("no config file found at {}", config_path.display()))?;

    if cfg.profiles.is_empty() {
        bail!("no profiles defined in {}", path.display());
    }

    Ok((cfg, path))
}

/// Pick a profile by name or interactively from a numbered list.
fn pick_profile(
    reader: &mut impl BufRead,
    out: &mut impl Write,
    cfg: &config::Config,
    name: Option<&str>,
    verb: &str,
) -> Result<String> {
    if let Some(n) = name {
        if !cfg.profiles.contains_key(n) {
            let mut available: Vec<&str> = cfg.profiles.keys().map(|k| k.as_str()).collect();
            available.sort();
            bail!(
                "profile '{}' not found. Available: {}",
                n,
                available.join(", ")
            );
        }
        return Ok(n.to_string());
    }

    // Show numbered list
    let mut names: Vec<&String> = cfg.profiles.keys().collect();
    names.sort();
    writeln!(out, "{}", format!("Select a profile to {verb}:").bold())?;
    writeln!(out)?;
    for (i, name) in names.iter().enumerate() {
        writeln!(out, "  {} {}", format!("{}.", i + 1).dimmed(), name.cyan())?;
    }
    writeln!(out)?;

    let choice = ask(reader, out, "Profile number or name", None)?;
    if choice.is_empty() {
        bail!("no profile selected");
    }

    // Try as number first
    if let Ok(n) = choice.parse::<usize>() {
        if n >= 1 && n <= names.len() {
            return Ok(names[n - 1].clone());
        }
        bail!("invalid selection: {n}");
    }

    // Try as name
    if cfg.profiles.contains_key(&choice) {
        return Ok(choice);
    }
    bail!("profile '{}' not found", choice);
}

// === Delete ===

fn run_delete(
    reader: &mut impl BufRead,
    out: &mut impl Write,
    explicit_config: Option<&Path>,
    name: Option<&str>,
) -> Result<()> {
    let (cfg, config_path) = load_existing_config(explicit_config)?;
    let name = pick_profile(reader, out, &cfg, name, "delete")?;

    let confirm = ask(
        reader,
        out,
        &format!("Delete profile '{}'? (y/N)", name.cyan()),
        Some("N"),
    )?;
    if !confirm.eq_ignore_ascii_case("y") {
        writeln!(out, "Aborted.")?;
        return Ok(());
    }

    remove_profile_from_file(&config_path, &name)?;

    writeln!(out)?;
    writeln!(
        out,
        "{} profile '{}' from {}",
        "Deleted".green().bold(),
        name.cyan(),
        config_path.display(),
    )?;

    Ok(())
}

fn remove_profile_from_file(config_path: &Path, name: &str) -> Result<()> {
    let mut content = std::fs::read_to_string(config_path)?;

    if let Some(start) = find_profile_block(&content, name) {
        let end = find_next_section(&content, start + 1);
        let remove_end = end.unwrap_or(content.len());
        content = format!(
            "{}{}",
            &content[..start],
            content[remove_end..].trim_start_matches('\n')
        );
        // Clean up trailing whitespace
        while content.ends_with("\n\n") {
            content.pop();
        }
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        std::fs::write(config_path, content)?;
    }

    Ok(())
}

// === Edit ===

fn run_edit(
    reader: &mut impl BufRead,
    out: &mut impl Write,
    explicit_config: Option<&Path>,
    name: Option<&str>,
) -> Result<()> {
    let (cfg, config_path) = load_existing_config(explicit_config)?;
    let name = pick_profile(reader, out, &cfg, name, "edit")?;
    let profile = &cfg.profiles[&name];

    writeln!(out, "{} {}", "Editing profile:".bold(), name.cyan().bold())?;
    writeln!(
        out,
        "{}",
        "Press Enter to keep the current value, or type a new one.".dimmed()
    )?;
    writeln!(out)?;

    // Triggers
    writeln!(out, "{}", "Trigger patterns:".dimmed())?;
    let mut triggers: Vec<String> = Vec::new();
    for (i, existing) in profile.trigger.iter().enumerate() {
        let input = ask(
            reader,
            out,
            &format!("Trigger {} (- to remove)", i + 1),
            Some(existing),
        )?;
        if input == "-" {
            continue;
        }
        if !input.is_empty() {
            if let Err(e) = regex::Regex::new(&input) {
                writeln!(out, "  {} {e} — keeping original", "invalid regex:".red())?;
                triggers.push(existing.clone());
            } else {
                triggers.push(input);
            }
        }
    }
    // Offer to add new triggers
    loop {
        let t = ask(reader, out, "Add trigger (empty to finish)", None)?;
        if t.is_empty() {
            break;
        }
        if let Err(e) = regex::Regex::new(&t) {
            writeln!(out, "  {} {e}", "invalid regex:".red())?;
            continue;
        }
        triggers.push(t);
    }

    // Before/after (only if triggers exist)
    let mut before: Option<String> = profile.before.clone();
    let mut after: Option<String> = profile.after.clone();
    if !triggers.is_empty() {
        writeln!(out)?;
        writeln!(out, "{}", "Context window:".dimmed())?;
        let b = ask(
            reader,
            out,
            "Before context",
            before.as_deref().or(Some("20")),
        )?;
        before = if b == "20" { None } else { Some(b) };

        let a = ask(
            reader,
            out,
            "After context",
            after.as_deref().or(Some("20")),
        )?;
        after = if a == "20" { None } else { Some(a) };
    } else {
        before = None;
        after = None;
    }

    // Lines
    writeln!(out)?;
    let lines_input = ask(
        reader,
        out,
        "Lines (empty to clear)",
        profile.lines.as_deref(),
    )?;
    let lines = if lines_input.is_empty() {
        None
    } else {
        Some(lines_input)
    };

    // Rules
    writeln!(out)?;
    writeln!(out, "{}", "Coloring rules:".dimmed())?;
    let mut rules: Vec<(String, String, String)> = Vec::new();
    for (i, rc) in profile.rules.iter().enumerate() {
        let display = if rc.scope == "line" {
            format!("{}:{}", rc.pattern, rc.style)
        } else {
            format!("{}:{}:{}", rc.pattern, rc.style, rc.scope)
        };
        let input = ask(
            reader,
            out,
            &format!("Rule {} (- to remove)", i + 1),
            Some(&display),
        )?;
        if input == "-" {
            continue;
        }
        if input == display {
            // Kept as-is
            rules.push((rc.pattern.clone(), rc.style.clone(), rc.scope.clone()));
        } else {
            // Validate new rule
            match crate::rules::parse_rule(&input, 0) {
                Ok(rule) => {
                    let scope = match rule.scope {
                        crate::rules::MatchScope::Line => "line".to_string(),
                        crate::rules::MatchScope::Match => "match".to_string(),
                        crate::rules::MatchScope::Capture(n) => format!("cap{n}"),
                    };
                    let parts: Vec<&str> = input.rsplitn(3, ':').collect();
                    let (pattern, style) = match parts.len() {
                        2 => (parts[1].to_string(), parts[0].to_string()),
                        3 => {
                            if crate::rules::parse_scope(parts[0]).is_some() {
                                (parts[2].to_string(), parts[1].to_string())
                            } else {
                                (
                                    format!("{}:{}", parts[2], parts[1]),
                                    parts[0].to_string(),
                                )
                            }
                        }
                        _ => {
                            rules.push((rc.pattern.clone(), rc.style.clone(), rc.scope.clone()));
                            continue;
                        }
                    };
                    rules.push((pattern, style, scope));
                }
                Err(e) => {
                    writeln!(out, "  {} {e} — keeping original", "invalid rule:".red())?;
                    rules.push((rc.pattern.clone(), rc.style.clone(), rc.scope.clone()));
                }
            }
        }
    }
    // Offer to add new rules via interactive wizard
    while let Some(rule) = ask_rule_or_raw(reader, out)? {
        rules.push(rule);
    }

    // Preview
    writeln!(out)?;
    writeln!(out, "{}", "Preview:".bold())?;
    writeln!(out)?;
    let toml = build_toml(&name, &triggers, &before, &after, &lines, &rules);
    for line in toml.lines() {
        writeln!(out, "  {}", line.dimmed())?;
    }
    writeln!(out)?;

    let confirm = ask(reader, out, "Save changes? (Y/n)", Some("Y"))?;
    if confirm.eq_ignore_ascii_case("n") {
        writeln!(out, "Aborted.")?;
        return Ok(());
    }

    write_profile(&config_path, &name, &toml)?;

    writeln!(out)?;
    writeln!(
        out,
        "{} profile '{}' in {}",
        "Updated".green().bold(),
        name.cyan(),
        config_path.display(),
    )?;

    Ok(())
}

fn ask(
    reader: &mut impl BufRead,
    out: &mut impl Write,
    prompt: &str,
    default: Option<&str>,
) -> Result<String> {
    if let Some(d) = default {
        write!(out, "  {} {} ", prompt.bold(), format!("[{d}]").dimmed())?;
    } else {
        write!(out, "  {} ", prompt.bold())?;
    }
    out.flush()?;

    let mut input = String::new();
    reader.read_line(&mut input)?;
    let trimmed = input.trim().to_string();

    if trimmed.is_empty() {
        if let Some(d) = default {
            return Ok(d.to_string());
        }
    }
    Ok(trimmed)
}

/// Color names available in the numbered menu.
const FG_COLORS: &[&str] = &[
    "red", "green", "blue", "yellow", "cyan", "magenta",
    "white", "orange", "pink", "purple", "teal", "lime",
];

/// Effect names available in the numbered menu.
const EFFECTS: &[&str] = &["bold", "dim", "italic", "underline"];

/// Walk the user through building a coloring rule step-by-step.
///
/// Returns `Ok(None)` if the user enters empty pattern (finish signal),
/// or `Ok(Some((pattern, style_spec, scope)))`.
fn ask_rule_interactive(
    reader: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<Option<(String, String, String)>> {
    ask_rule_interactive_inner(reader, out, None)
}

fn ask_rule_interactive_inner(
    reader: &mut impl BufRead,
    out: &mut impl Write,
    prefill_pattern: Option<&str>,
) -> Result<Option<(String, String, String)>> {
    // 1. Pattern
    let pattern = loop {
        let input = ask(
            reader,
            out,
            "Regex pattern (empty to finish)",
            prefill_pattern,
        )?;
        if input.is_empty() {
            return Ok(None);
        }
        match regex::Regex::new(&input) {
            Ok(_) => break input,
            Err(e) => {
                writeln!(out, "  {} {e}", "invalid regex:".red())?;
                continue;
            }
        }
    };

    // 2. Foreground color
    writeln!(out)?;
    write_color_menu(out, "Foreground")?;
    let fg = loop {
        let input = ask(
            reader,
            out,
            "Foreground color (number, name, hex, or empty to skip)",
            None,
        )?;
        if input.is_empty() {
            break None;
        }
        if let Some(color) = resolve_color_input(&input) {
            // Validate it
            if color::parse_style(&color).is_ok() {
                break Some(color);
            }
        }
        // Try direct validation
        if color::parse_style(&input).is_ok() {
            break Some(input);
        }
        writeln!(out, "  {} '{input}'", "unknown color:".red())?;
    };

    // 3. Background color
    writeln!(out)?;
    write_color_menu(out, "Background")?;
    let bg = loop {
        let input = ask(
            reader,
            out,
            "Background color (empty to skip)",
            None,
        )?;
        if input.is_empty() {
            break None;
        }
        let color_name = if let Some(c) = resolve_color_input(&input) {
            c
        } else {
            input.clone()
        };
        // Validate with bg: prefix
        let bg_spec = format!("bg:{color_name}");
        if color::parse_style(&bg_spec).is_ok() {
            break Some(bg_spec);
        }
        writeln!(out, "  {} '{input}'", "unknown color:".red())?;
    };

    // 4. Effects
    writeln!(out)?;
    writeln!(
        out,
        "  {}",
        "Effects:".dimmed()
    )?;
    write!(out, "    ")?;
    for (i, effect) in EFFECTS.iter().enumerate() {
        let styled = match *effect {
            "bold" => format!("{}", effect.bold()),
            "dim" => format!("{}", effect.dimmed()),
            "italic" => format!("{}", effect.italic()),
            "underline" => format!("{}", effect.underline()),
            _ => effect.to_string(),
        };
        write!(out, "{}. {}  ", i + 1, styled)?;
    }
    writeln!(out)?;
    let effects: Vec<String> = {
        let input = ask(
            reader,
            out,
            "Effects (comma-separated, empty to skip)",
            None,
        )?;
        if input.is_empty() {
            Vec::new()
        } else {
            input
                .split(',')
                .filter_map(|s| {
                    let s = s.trim();
                    if s.is_empty() {
                        return None;
                    }
                    // Try as number
                    if let Ok(n) = s.parse::<usize>()
                        && n >= 1 && n <= EFFECTS.len()
                    {
                        return Some(EFFECTS[n - 1].to_string());
                    }
                    // Try as name
                    if EFFECTS.contains(&s) {
                        return Some(s.to_string());
                    }
                    None
                })
                .collect()
        }
    };

    // 5. Scope
    writeln!(out)?;
    writeln!(
        out,
        "  {}  1. line (whole line)  2. match (only match)",
        "Scope:".dimmed(),
    )?;
    let scope = {
        let input = ask(reader, out, "Scope", Some("line"))?;
        match input.as_str() {
            "2" | "match" => "match".to_string(),
            _ => "line".to_string(),
        }
    };

    // 6. Build style string
    let mut style_parts: Vec<String> = Vec::new();
    style_parts.extend(effects);
    if let Some(ref fg) = fg {
        style_parts.push(fg.clone());
    }
    if let Some(ref bg) = bg {
        style_parts.push(bg.clone());
    }
    let style_spec = if style_parts.is_empty() {
        // Need at least something -- default to empty which won't validate.
        // If user skipped everything, use a basic style.
        "white".to_string()
    } else {
        style_parts.join("+")
    };

    // Validate combined style
    let style = match color::parse_style(&style_spec) {
        Ok(s) => s,
        Err(e) => {
            writeln!(out, "  {} {e}", "style error:".red())?;
            return Ok(None);
        }
    };

    // 7. Live preview
    writeln!(out)?;
    let sample = "2024-01-15 ERROR: Connection failed";
    write!(out, "  {} ", "Preview:".bold())?;
    if scope == "match" {
        // Only color the matched portion
        if let Ok(re) = regex::Regex::new(&pattern) {
            if let Some(m) = re.find(sample) {
                let matched: &str = &sample[m.start()..m.end()];
                write!(
                    out,
                    "{}{}{}",
                    &sample[..m.start()],
                    matched.to_string().style(style),
                    &sample[m.end()..],
                )?;
            } else {
                write!(out, "{}", sample.style(style))?;
            }
        } else {
            write!(out, "{}", sample.style(style))?;
        }
    } else {
        write!(out, "{}", sample.style(style))?;
    }
    writeln!(out)?;

    // 8. Confirm
    let keep = ask(reader, out, "Keep this rule? (Y/n)", Some("Y"))?;
    if keep.eq_ignore_ascii_case("n") {
        // Restart with same pattern pre-filled
        return ask_rule_interactive_inner(reader, out, Some(&pattern));
    }

    Ok(Some((pattern, style_spec, scope)))
}

/// Show a numbered color menu with each color rendered in its own color.
fn write_color_menu(out: &mut impl Write, label: &str) -> Result<()> {
    writeln!(out, "  {}", format!("{label} color:").dimmed())?;
    for row in 0..3 {
        write!(out, "    ")?;
        for col in 0..4 {
            let idx = row * 4 + col;
            if idx < FG_COLORS.len() {
                let name = FG_COLORS[idx];
                let styled_name = match color::parse_style(name) {
                    Ok(s) => format!("{}", name.style(s)),
                    Err(_) => name.to_string(),
                };
                write!(out, "{:>2}. {:<14}", idx + 1, styled_name)?;
            }
        }
        writeln!(out)?;
    }
    Ok(())
}

/// Resolve user input (number or name) to a color name from the FG_COLORS menu.
fn resolve_color_input(input: &str) -> Option<String> {
    let input = input.trim();
    // Try as number
    if let Ok(n) = input.parse::<usize>()
        && n >= 1 && n <= FG_COLORS.len()
    {
        return Some(FG_COLORS[n - 1].to_string());
    }
    // Return input as-is for direct name/hex validation
    None
}

/// Ask the user whether to use the wizard or raw format, then collect a rule.
fn ask_rule_or_raw(
    reader: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<Option<(String, String, String)>> {
    let mode = ask(
        reader,
        out,
        "Add style rule? (Enter)wizard / (r)aw / (d)one",
        None,
    )?;
    if mode.eq_ignore_ascii_case("d") {
        return Ok(None);
    }
    if mode.eq_ignore_ascii_case("r") {
        let input = ask(reader, out, "Rule (PATTERN:STYLE[:SCOPE])", None)?;
        if input.is_empty() {
            return Ok(None);
        }
        parse_raw_rule_input(&input, out)
    } else {
        // Empty (Enter) or 'w' → wizard
        ask_rule_interactive(reader, out)
    }
}

/// Parse a raw PATTERN:STYLE[:SCOPE] input string into (pattern, style, scope) tuple.
fn parse_raw_rule_input(
    input: &str,
    out: &mut impl Write,
) -> Result<Option<(String, String, String)>> {
    match crate::rules::parse_rule(input, 0) {
        Ok(rule) => {
            let scope = match rule.scope {
                crate::rules::MatchScope::Line => "line".to_string(),
                crate::rules::MatchScope::Match => "match".to_string(),
                crate::rules::MatchScope::Capture(n) => format!("cap{n}"),
            };
            let parts: Vec<&str> = input.rsplitn(3, ':').collect();
            let (pattern, style) = match parts.len() {
                2 => (parts[1].to_string(), parts[0].to_string()),
                3 => {
                    if crate::rules::parse_scope(parts[0]).is_some() {
                        (parts[2].to_string(), parts[1].to_string())
                    } else {
                        (
                            format!("{}:{}", parts[2], parts[1]),
                            parts[0].to_string(),
                        )
                    }
                }
                _ => return Ok(None),
            };
            Ok(Some((pattern, style, scope)))
        }
        Err(e) => {
            writeln!(out, "  {} {e}", "invalid rule:".red())?;
            Ok(None)
        }
    }
}

fn validate_context_spec(spec: &str, out: &mut impl Write) -> bool {
    // Number is always valid
    if spec.parse::<usize>().is_ok() {
        return true;
    }
    // Otherwise must be valid regex
    if let Err(e) = regex::Regex::new(spec) {
        let _ = writeln!(out, "  {} {e}", "warning: invalid regex:".yellow());
        return false;
    }
    true
}

fn build_toml(
    name: &str,
    triggers: &[String],
    before: &Option<String>,
    after: &Option<String>,
    lines: &Option<String>,
    rules: &[(String, String, String)],
) -> String {
    let mut toml = format!("[profiles.{name}]\n");

    if !triggers.is_empty() {
        let quoted: Vec<String> = triggers.iter().map(|t| format!("'{t}'")).collect();
        toml.push_str(&format!("trigger = [{}]\n", quoted.join(", ")));
    }
    if let Some(b) = before {
        toml.push_str(&format!("before = \"{b}\"\n"));
    }
    if let Some(a) = after {
        toml.push_str(&format!("after = \"{a}\"\n"));
    }
    if let Some(l) = lines {
        toml.push_str(&format!("lines = \"{l}\"\n"));
    }

    for (pattern, style, scope) in rules {
        toml.push_str(&format!("\n[[profiles.{name}.rules]]\n"));
        toml.push_str(&format!("pattern = \"{pattern}\"\n"));
        toml.push_str(&format!("style = \"{style}\"\n"));
        if scope != "line" {
            toml.push_str(&format!("scope = \"{scope}\"\n"));
        }
    }

    toml
}

fn write_profile(config_path: &Path, name: &str, toml_block: &str) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if config_path.exists() {
        let mut content = std::fs::read_to_string(config_path)?;

        // Check if profile already exists — remove the old one
        if let Some(start) = find_profile_block(&content, name) {
            let end = find_next_section(&content, start + 1);
            // Trim trailing blank lines from the removed block
            let remove_end = end.unwrap_or(content.len());
            content = format!(
                "{}{}",
                &content[..start],
                content[remove_end..].trim_start_matches('\n')
            );
        }

        // Append the new profile
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        if !content.is_empty() && !content.ends_with("\n\n") {
            content.push('\n');
        }
        content.push_str(toml_block);

        std::fs::write(config_path, content)?;
    } else {
        std::fs::write(config_path, toml_block)?;
    }

    Ok(())
}

/// Find the byte offset of a `[profiles.NAME]` section header.
fn find_profile_block(content: &str, name: &str) -> Option<usize> {
    let header = format!("[profiles.{name}]");
    content.find(&header)
}

/// Find the byte offset of the next `[` section header after `start`.
fn find_next_section(content: &str, start: usize) -> Option<usize> {
    for (i, line) in content[start..].lines().enumerate() {
        if i > 0 && line.starts_with('[') && !line.starts_with("[[") {
            // Find the byte offset of this line
            let offset = content[start..]
                .find(line)
                .map(|pos| start + pos)?;
            return Some(offset);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wizard_with_input(input: &str) -> (String, Result<()>) {
        let mut reader = io::Cursor::new(input.as_bytes().to_vec());
        let mut out = Vec::new();
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        let result = run_wizard(&mut reader, &mut out, Some(&config_path));
        let output = String::from_utf8(out).unwrap();
        (output, result)
    }

    #[test]
    fn test_empty_name_aborts() {
        let (_, result) = wizard_with_input("\n");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("empty"),
        );
    }

    #[test]
    fn test_minimal_profile() {
        // name, skip trigger, skip lines, wizard/raw prompt (default w), empty pattern (finish), confirm save
        let input = "test-profile\n\n\n\n\nY\n";
        let (output, result) = wizard_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        assert!(output.contains("Saved"), "Got: {output}");
        assert!(output.contains("test-profile"), "Got: {output}");
    }

    #[test]
    fn test_full_profile() {
        // name, trigger, empty (finish triggers), before, after, lines,
        // raw rule shortcut: "r", rule, wizard/raw (default w), empty pattern (finish), confirm
        let input = "my-profile\nERROR\n\n^===\n^---\n+1\nr\nERROR:red\n\n\nY\n";
        let (output, result) = wizard_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        assert!(output.contains("Saved"), "Got: {output}");
        assert!(output.contains("[profiles.my-profile]"), "Got: {output}");
    }

    #[test]
    fn test_abort_on_no() {
        // name, skip trigger, skip lines, wizard/raw (w), empty pattern (finish), abort
        let input = "test-profile\n\n\n\n\nn\n";
        let (output, result) = wizard_with_input(input);
        assert!(result.is_ok());
        assert!(output.contains("Aborted"), "Got: {output}");
    }

    #[test]
    fn test_invalid_trigger_regex_retries() {
        // bad regex, then good one, then finish, skip rest, wizard/raw (w), empty pattern (finish), confirm
        let input = "test-profile\n(bad\nERROR\n\n\n\n\n\nY\n";
        let (output, result) = wizard_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        assert!(output.contains("invalid regex"), "Got: {output}");
        assert!(output.contains("Saved"), "Got: {output}");
    }

    #[test]
    fn test_build_toml_triggers_and_rules() {
        let toml = build_toml(
            "unittest",
            &["ERROR".to_string()],
            &Some("^===.*".to_string()),
            &Some("^---.*".to_string()),
            &Some("+1".to_string()),
            &[("ERROR".to_string(), "red".to_string(), "line".to_string())],
        );
        assert!(toml.contains("[profiles.unittest]"));
        assert!(toml.contains("trigger = ['ERROR']"));
        assert!(toml.contains("before = \"^===.*\""));
        assert!(toml.contains("after = \"^---.*\""));
        assert!(toml.contains("lines = \"+1\""));
        assert!(toml.contains("[[profiles.unittest.rules]]"));
        assert!(toml.contains("pattern = \"ERROR\""));
        assert!(toml.contains("style = \"red\""));
        // scope = "line" should be omitted (it's the default)
        assert!(!toml.contains("scope"));
    }

    #[test]
    fn test_build_toml_empty_profile() {
        let toml = build_toml("empty", &[], &None, &None, &None, &[]);
        assert_eq!(toml, "[profiles.empty]\n");
    }

    #[test]
    fn test_write_profile_creates_new_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let toml = "[profiles.test]\ntrigger = ['ERROR']\n";
        write_profile(&path, "test", toml).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[profiles.test]"));
    }

    #[test]
    fn test_write_profile_appends_to_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[[rules]]\npattern = \"test\"\nstyle = \"red\"\n").unwrap();
        let toml = "[profiles.new]\ntrigger = ['WARN']\n";
        write_profile(&path, "new", toml).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[[rules]]"));
        assert!(content.contains("[profiles.new]"));
    }

    // === Delete tests ===

    fn delete_with_input(config_content: &str, input: &str) -> (String, Result<()>, String) {
        let mut reader = io::Cursor::new(input.as_bytes().to_vec());
        let mut out = Vec::new();
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, config_content).unwrap();
        let result = run_delete(&mut reader, &mut out, Some(&config_path), None);
        let output = String::from_utf8(out).unwrap();
        let remaining = std::fs::read_to_string(&config_path).unwrap_or_default();
        (output, result, remaining)
    }

    #[test]
    fn test_delete_by_number() {
        let config = "[profiles.alpha]\ntrigger = ['A']\n\n[profiles.beta]\ntrigger = ['B']\n";
        // pick #1 (alpha), confirm y
        let (output, result, remaining) = delete_with_input(config, "1\ny\n");
        assert!(result.is_ok(), "Got: {:?}", result.unwrap_err());
        assert!(output.contains("Deleted"), "Got: {output}");
        assert!(!remaining.contains("[profiles.alpha]"), "Got: {remaining}");
        assert!(remaining.contains("[profiles.beta]"), "Got: {remaining}");
    }

    #[test]
    fn test_delete_by_name() {
        let config = "[profiles.alpha]\ntrigger = ['A']\n";
        let (output, result, remaining) = delete_with_input(config, "alpha\ny\n");
        assert!(result.is_ok(), "Got: {:?}", result.unwrap_err());
        assert!(output.contains("Deleted"), "Got: {output}");
        assert!(!remaining.contains("[profiles.alpha]"), "Got: {remaining}");
    }

    #[test]
    fn test_delete_abort() {
        let config = "[profiles.alpha]\ntrigger = ['A']\n";
        let (output, result, remaining) = delete_with_input(config, "alpha\nn\n");
        assert!(result.is_ok());
        assert!(output.contains("Aborted"), "Got: {output}");
        assert!(remaining.contains("[profiles.alpha]"), "Got: {remaining}");
    }

    fn delete_named_with_input(config_content: &str, name: &str, input: &str) -> (String, Result<()>, String) {
        let mut reader = io::Cursor::new(input.as_bytes().to_vec());
        let mut out = Vec::new();
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, config_content).unwrap();
        let result = run_delete(&mut reader, &mut out, Some(&config_path), Some(name));
        let output = String::from_utf8(out).unwrap();
        let remaining = std::fs::read_to_string(&config_path).unwrap_or_default();
        (output, result, remaining)
    }

    #[test]
    fn test_delete_named_directly() {
        let config = "[profiles.alpha]\ntrigger = ['A']\n";
        let (output, result, _) = delete_named_with_input(config, "alpha", "y\n");
        assert!(result.is_ok(), "Got: {:?}", result.unwrap_err());
        assert!(output.contains("Deleted"), "Got: {output}");
    }

    #[test]
    fn test_delete_named_not_found() {
        let config = "[profiles.alpha]\ntrigger = ['A']\n";
        let (_, result, _) = delete_named_with_input(config, "nope", "y\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    // === Edit tests ===

    fn edit_named_with_input(config_content: &str, name: &str, input: &str) -> (String, Result<()>, String) {
        let mut reader = io::Cursor::new(input.as_bytes().to_vec());
        let mut out = Vec::new();
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, config_content).unwrap();
        let result = run_edit(&mut reader, &mut out, Some(&config_path), Some(name));
        let output = String::from_utf8(out).unwrap();
        let remaining = std::fs::read_to_string(&config_path).unwrap_or_default();
        (output, result, remaining)
    }

    #[test]
    fn test_edit_keep_defaults() {
        let config = "[profiles.test]\ntrigger = ['ERROR']\nbefore = \"^===\"\nafter = \"^---\"\nlines = \"+1\"\n";
        // Keep all defaults, wizard/raw (w), empty pattern (finish), confirm save
        let input = "\n\n\n\n\n\n\nY\n";
        let (output, result, remaining) = edit_named_with_input(config, "test", input);
        assert!(result.is_ok(), "Got: {:?}", result.unwrap_err());
        assert!(output.contains("Updated"), "Got: {output}");
        assert!(remaining.contains("trigger = ['ERROR']"), "Got: {remaining}");
    }

    #[test]
    fn test_edit_abort() {
        let config = "[profiles.test]\ntrigger = ['ERROR']\n";
        // keep trigger, no new trigger, before, after, lines, wizard/raw (w), empty pattern (finish), abort
        let input = "\n\n\n\n\n\n\nn\n";
        let (output, result, _) = edit_named_with_input(config, "test", input);
        assert!(result.is_ok(), "Got: {:?}", result.unwrap_err());
        assert!(output.contains("Aborted"), "Got: {output}");
    }

    #[test]
    fn test_write_profile_replaces_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "[profiles.old]\ntrigger = ['OLD']\n\n[profiles.keep]\ntrigger = ['KEEP']\n",
        )
        .unwrap();
        let toml = "[profiles.old]\ntrigger = ['NEW']\n";
        write_profile(&path, "old", toml).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("trigger = ['NEW']"));
        assert!(!content.contains("trigger = ['OLD']"));
        assert!(content.contains("[profiles.keep]"));
    }

    // === Interactive rule wizard tests ===

    fn interactive_with_input(input: &str) -> (String, Result<Option<(String, String, String)>>) {
        let mut reader = io::Cursor::new(input.as_bytes().to_vec());
        let mut out = Vec::new();
        let result = ask_rule_interactive(&mut reader, &mut out);
        let output = String::from_utf8(out).unwrap();
        (output, result)
    }

    #[test]
    fn test_ask_rule_interactive_empty_skips() {
        // Empty pattern returns None
        let (_, result) = interactive_with_input("\n");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_ask_rule_interactive_basic_flow() {
        // pattern "ERROR", pick color "1" (red), empty bg, empty effects, default scope (line), confirm "Y"
        let input = "ERROR\n1\n\n\n\n\nY\n";
        let (output, result) = interactive_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        let rule = result.unwrap().expect("expected Some rule");
        assert_eq!(rule.0, "ERROR");
        assert_eq!(rule.1, "red");
        assert_eq!(rule.2, "line");
        // Should show preview
        assert!(output.contains("Preview:"), "Got: {output}");
    }

    #[test]
    fn test_ask_rule_interactive_full_style() {
        // pattern "WARN", color "yellow" (by name), bg "blue" (by name),
        // effects "1,3" (bold+italic), scope "2" (match), confirm Y
        let input = "WARN\nyellow\nblue\n1,3\n2\nY\n";
        let (_, result) = interactive_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        let rule = result.unwrap().expect("expected Some rule");
        assert_eq!(rule.0, "WARN");
        assert_eq!(rule.1, "bold+italic+yellow+bg:blue");
        assert_eq!(rule.2, "match");
    }

    #[test]
    fn test_ask_rule_interactive_invalid_regex_retries() {
        // Invalid regex "(bad", then valid "ERROR", then complete the flow
        // (bad -> error shown -> ERROR -> fg empty -> bg empty -> effects empty -> scope default -> confirm Y
        let input = "(bad\nERROR\n\n\n\n\n\nY\n";
        let (output, result) = interactive_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        assert!(output.contains("invalid regex"), "Got: {output}");
        let rule = result.unwrap().expect("expected Some rule");
        assert_eq!(rule.0, "ERROR");
    }

    #[test]
    fn test_ask_rule_interactive_redo() {
        // Complete a rule then answer "n" to keep, re-enter different pattern, complete again, answer "Y"
        // First: pattern "ERROR", fg "1" (red), skip bg, skip effects, scope default, keep? "n"
        // Second (pre-filled "ERROR"): accept pre-filled, fg "4" (yellow), skip bg, skip effects, scope default, keep? "Y"
        let input = "ERROR\n1\n\n\n\nn\n\n4\n\n\n\nY\n";
        let (_, result) = interactive_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        let rule = result.unwrap().expect("expected Some rule");
        assert_eq!(rule.0, "ERROR");
        assert_eq!(rule.1, "yellow");
        assert_eq!(rule.2, "line");
    }

    #[test]
    fn test_wizard_with_interactive_rule() {
        // Full wizard flow: profile name, skip triggers, skip lines,
        // wizard mode (w), pattern "ERROR", fg "1" (red), skip bg, skip effects, scope default, confirm rule,
        // wizard mode again (w), empty pattern (finish rules),
        // confirm save
        let input = "interactive-test\n\n\n\nERROR\n1\n\n\n\n\nY\n\n\nY\n";
        let (output, result) = wizard_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        assert!(output.contains("Saved"), "Got: {output}");
        assert!(output.contains("pattern = \"ERROR\""), "Got: {output}");
        assert!(output.contains("style = \"red\""), "Got: {output}");
    }

    #[test]
    fn test_raw_format_shortcut() {
        // Full wizard flow using raw format shortcut
        // profile name, skip triggers, skip lines,
        // raw mode (r), rule "ERROR:red:match",
        // wizard mode again (w), empty pattern (finish rules),
        // confirm save
        let input = "raw-test\n\n\nr\nERROR:red:match\n\n\nY\n";
        let (output, result) = wizard_with_input(input);
        assert!(result.is_ok(), "Got error: {:?}", result.unwrap_err());
        assert!(output.contains("Saved"), "Got: {output}");
        assert!(output.contains("pattern = \"ERROR\""), "Got: {output}");
        assert!(output.contains("style = \"red\""), "Got: {output}");
        assert!(output.contains("scope = \"match\""), "Got: {output}");
    }
}

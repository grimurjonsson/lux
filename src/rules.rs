use anyhow::{bail, Result};
use owo_colors::Style;
use regex::Regex;

use crate::color;
use crate::config::Config;
use crate::markup::{self, Segment};

/// A pre-parsed template for text-insertion rules.
///
/// Contains the original template string and its pre-parsed segments
/// (from markup tag parsing). Equality is based on the template string only.
#[derive(Debug, Clone)]
pub struct InsertTemplate {
    pub template: String,
    pub segments: Vec<Segment>,
}

impl PartialEq for InsertTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.template == other.template
    }
}

/// Defines which part of the line gets colored when a rule matches.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchScope {
    /// Color the entire line
    Line,
    /// Color only the matched portion (Phase 2)
    Match,
    /// Color a specific capture group (Phase 2)
    Capture(usize),
    /// Color the next N lines after a match (not the matching line itself)
    Next(usize),
    /// Insert a template line before the matching line
    InsertBefore(InsertTemplate),
    /// Insert a template line after the matching line
    InsertAfter(InsertTemplate),
    /// Prepend template text to the beginning of the matching line
    Prepend(InsertTemplate),
    /// Append template text to the end of the matching line
    Append(InsertTemplate),
}

/// A compiled coloring rule: a regex pattern, a style, a scope, and a priority.
#[derive(Debug, Clone)]
pub struct Rule {
    pub pattern: Regex,
    pub style: Style,
    pub scope: MatchScope,
    pub priority: usize,
}

/// Parse a scope string into a MatchScope.
///
/// Supported scopes: "line", "match", "cap1", "cap2", etc.
/// Returns `None` for unrecognized scope strings.
pub fn parse_scope(s: &str) -> Option<MatchScope> {
    match s {
        "line" => Some(MatchScope::Line),
        "match" => Some(MatchScope::Match),
        // Insert keywords are recognized but return None — they need a template,
        // so they're handled by the keyword-boundary scanner in parse_rule().
        "insert-before" | "insert-after" | "prepend" | "append" => None,
        _ if s.starts_with("cap") => {
            s[3..].parse::<usize>().ok().map(MatchScope::Capture)
        }
        _ if s.starts_with("next") => {
            s[4..].parse::<usize>().ok().filter(|&n| n > 0).map(MatchScope::Next)
        }
        _ => None,
    }
}

/// Parse a rule specification string into a Rule.
///
/// Format: `PATTERN:STYLE[:SCOPE]`
///
/// Uses right-to-left splitting to handle colons inside regex patterns.
/// If 3 parts are found, checks if the last is a valid scope; if not,
/// re-interprets as `PATTERN_WITH_COLON:STYLE`.
/// Insert scope keywords, ordered longest-first so scanning is unambiguous.
const INSERT_KEYWORDS: &[(&str, &str)] = &[
    (":insert-before:", "insert-before"),
    (":insert-after:", "insert-after"),
    (":prepend:", "prepend"),
    (":append:", "append"),
];

pub fn parse_rule(input: &str, priority: usize) -> Result<Rule> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("empty rule specification");
    }

    // --- Phase 1: scan for rightmost insert keyword ---
    // Scan the *trimmed-start* input (preserving trailing content for templates).
    // We look for the rightmost occurrence of any `:insert-before:`, `:insert-after:`,
    // `:prepend:`, or `:append:` keyword. If found, everything after the keyword is
    // the template (verbatim, including any colons), everything before is PATTERN:STYLE.
    let trimmed_start = input.trim_start();
    let mut best: Option<(usize, usize, &str)> = None; // (pos, keyword_len, scope_name)
    for &(needle, scope_name) in INSERT_KEYWORDS {
        if let Some(pos) = trimmed_start.rfind(needle) {
            match best {
                None => best = Some((pos, needle.len(), scope_name)),
                Some((prev_pos, _, _)) if pos > prev_pos => {
                    best = Some((pos, needle.len(), scope_name));
                }
                _ => {}
            }
        }
    }

    if let Some((pos, kw_len, scope_name)) = best {
        // Left side = PATTERN:STYLE (everything before the keyword's leading colon)
        let left = &trimmed_start[..pos];
        // Template = everything after the keyword (after its trailing colon)
        let template = &trimmed_start[pos + kw_len..];

        if template.is_empty() {
            bail!("missing template text after scope '{scope_name}'");
        }

        // Split left into PATTERN:STYLE using rfind(':')
        let (pattern_str, style_str) = if let Some(colon) = left.rfind(':') {
            (&left[..colon], &left[colon + 1..])
        } else {
            bail!("invalid rule '{trimmed_start}': expected PATTERN:STYLE:{scope_name}:TEMPLATE format (no colon found in pattern:style)");
        };

        let compiled = Regex::new(pattern_str)
            .map_err(|e| anyhow::anyhow!("invalid regex pattern '{pattern_str}': {e}"))?;

        let parsed_style = if style_str.is_empty() {
            Style::new()
        } else {
            color::parse_style(style_str)?
        };

        let segments = markup::validate_template(template, compiled.captures_len())?;
        let tmpl = InsertTemplate {
            template: template.to_string(),
            segments,
        };

        let scope = match scope_name {
            "insert-before" => MatchScope::InsertBefore(tmpl),
            "insert-after" => MatchScope::InsertAfter(tmpl),
            "prepend" => MatchScope::Prepend(tmpl),
            "append" => MatchScope::Append(tmpl),
            _ => unreachable!(),
        };

        return Ok(Rule {
            pattern: compiled,
            style: parsed_style,
            scope,
            priority,
        });
    }

    // --- Phase 2: no insert keyword found — fall back to existing 2/3-part parsing ---
    // Use fully-trimmed input for non-insert rules.
    let input = trimmed;
    let parts: Vec<&str> = input.rsplitn(3, ':').collect();

    let (pattern_str, style_str, scope) = match parts.len() {
        1 => {
            bail!("invalid rule '{input}': expected PATTERN:STYLE[:SCOPE] format (no colon found)");
        }
        2 => {
            // PATTERN:STYLE
            (parts[1], parts[0], MatchScope::Line)
        }
        3 => {
            // Could be PATTERN:STYLE:SCOPE or PATTERN_WITH_COLON:STYLE
            if let Some(scope) = parse_scope(parts[0]) {
                // Valid scope: PATTERN:STYLE:SCOPE
                (parts[2], parts[1], scope)
            } else {
                // Not a valid scope: rejoin as PATTERN_WITH_COLON:STYLE
                let pattern = format!("{}:{}", parts[2], parts[1]);
                // parts[0] is the style (rightmost)
                return build_rule(&pattern, parts[0], MatchScope::Line, priority);
            }
        }
        _ => unreachable!(),
    };

    build_rule(pattern_str, style_str, scope, priority)
}

fn build_rule(pattern_str: &str, style_str: &str, scope: MatchScope, priority: usize) -> Result<Rule> {
    let pattern = Regex::new(pattern_str)
        .map_err(|e| anyhow::anyhow!("invalid regex pattern '{pattern_str}': {e}"))?;

    let style = color::parse_style(style_str)?;

    Ok(Rule {
        pattern,
        style,
        scope,
        priority,
    })
}

/// Return the built-in default rules for common log levels.
///
/// Uses case-insensitive matching. Priority starts at `offset`.
pub fn default_rules(offset: usize) -> Vec<Rule> {
    let defaults = [
        ("(?i)fatal|critical", "bold+red"),
        ("(?i)error", "red"),
        ("(?i)warn", "yellow"),
        ("(?i)debug", "dim"),
        ("(?i)trace", "240"),
    ];

    defaults
        .iter()
        .enumerate()
        .map(|(i, (pattern, style))| {
            Rule {
                pattern: Regex::new(pattern).unwrap(),
                style: color::parse_style(style).unwrap(),
                scope: MatchScope::Line,
                priority: offset + i,
            }
        })
        .collect()
}

/// Build the complete rule set from CLI-provided rules.
///
/// User rules get lower priority numbers (checked first).
pub fn build_rules(cli_rules: &[String]) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();

    for (i, rule_str) in cli_rules.iter().enumerate() {
        rules.push(parse_rule(rule_str, i)?);
    }

    Ok(rules)
}

/// Build the complete rule set from CLI rules, config file rules, profile rules, and defaults.
///
/// Priority layering (lower number = higher priority):
/// 1. CLI rules (priority 0+)
/// 2. Global config rules (next band)
/// 3. Profile rules (next band, if profile active)
/// 4. Default rules (highest priority numbers)
///
/// The `merged_profiles` parameter allows passing pre-merged profiles (user + built-in)
/// so that built-in profiles like "markdown" work without a config file.
pub fn build_rules_with_config(
    cli_rules: &[String],
    config: Option<&Config>,
    profile_name: Option<&str>,
    merged_profiles: Option<&std::collections::HashMap<String, crate::config::ProfileConfig>>,
) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();

    // CLI rules at lowest priority numbers (highest priority)
    for (i, rule_str) in cli_rules.iter().enumerate() {
        rules.push(parse_rule(rule_str, i)?);
    }

    let mut offset = rules.len();

    // Profile rules -- higher priority than global rules so profile styling wins
    if let Some(name) = profile_name {
        let profile = if let Some(merged) = merged_profiles {
            merged.get(name)
        } else {
            config.and_then(|cfg| cfg.profiles.get(name))
        };

        match profile {
            Some(p) => {
                for rc in &p.rules {
                    let rule = if let Some(ref text) = rc.text {
                        rule_from_config_with_text(&rc.pattern, &rc.style, &rc.scope, text, offset)?
                    } else {
                        rule_from_config(&rc.pattern, &rc.style, &rc.scope, offset)?
                    };
                    rules.push(rule);
                    offset += 1;
                }
            }
            None => {
                // Build available list from merged_profiles or config
                let available: Vec<String> = if let Some(merged) = merged_profiles {
                    let mut keys: Vec<String> = merged.keys().cloned().collect();
                    keys.sort();
                    keys
                } else if let Some(cfg) = config {
                    let mut keys: Vec<&str> = cfg.profiles.keys().map(|k| k.as_str()).collect();
                    keys.sort();
                    keys.into_iter().map(|s| s.to_string()).collect()
                } else {
                    bail!("no config file found; cannot use --profile");
                };

                if available.is_empty() {
                    bail!("profile '{name}' not found. No profiles defined in config");
                } else {
                    bail!(
                        "profile '{name}' not found. Available: {}",
                        available.join(", ")
                    );
                }
            }
        }
    }

    // Global config rules -- lower priority than profile rules
    if let Some(cfg) = config {
        for rc in &cfg.rules {
            let rule = if let Some(ref text) = rc.text {
                rule_from_config_with_text(&rc.pattern, &rc.style, &rc.scope, text, offset)?
            } else {
                rule_from_config(&rc.pattern, &rc.style, &rc.scope, offset)?
            };
            rules.push(rule);
            offset += 1;
        }
    }

    Ok(rules)
}

/// Create a Rule from individual config fields (pattern, style, scope, priority).
///
/// Used by the config module to construct rules from TOML/YAML config files.
/// If `scope` is empty, defaults to `Line`. Returns an error for invalid
/// regex, style, or scope values.
pub fn rule_from_config(pattern: &str, style: &str, scope: &str, priority: usize) -> Result<Rule> {
    let scope = if scope.is_empty() {
        MatchScope::Line
    } else {
        parse_scope(scope)
            .ok_or_else(|| anyhow::anyhow!("invalid scope '{scope}' in config rule"))?
    };

    build_rule(pattern, style, scope, priority)
}

/// Create a Rule from config fields including a template text for insert scopes.
///
/// Used when a config rule has a `text` field, indicating an insert rule.
/// The `scope` must be one of: "insert-before", "insert-after", "prepend", "append".
pub fn rule_from_config_with_text(
    pattern: &str, style: &str, scope: &str, text: &str, priority: usize,
) -> Result<Rule> {
    let compiled = Regex::new(pattern)
        .map_err(|e| anyhow::anyhow!("invalid regex pattern '{pattern}': {e}"))?;
    let parsed_style = if style.is_empty() { Style::new() } else { color::parse_style(style)? };
    let segments = markup::validate_template(text, compiled.captures_len())?;
    let tmpl = InsertTemplate { template: text.to_string(), segments };
    let scope = match scope {
        "insert-before" => MatchScope::InsertBefore(tmpl),
        "insert-after" => MatchScope::InsertAfter(tmpl),
        "prepend" => MatchScope::Prepend(tmpl),
        "append" => MatchScope::Append(tmpl),
        _ => bail!("rule_from_config_with_text called with non-insert scope '{scope}'"),
    };
    Ok(Rule { pattern: compiled, style: parsed_style, scope, priority })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_rule() {
        let rule = parse_rule("ERROR:red", 0).unwrap();
        assert!(rule.pattern.is_match("ERROR"));
        assert_eq!(rule.scope, MatchScope::Line);
        assert_eq!(rule.priority, 0);
    }

    #[test]
    fn test_parse_rule_with_explicit_scope() {
        let rule = parse_rule("ERROR:bold+red:line", 0).unwrap();
        assert!(rule.pattern.is_match("ERROR"));
        assert_eq!(rule.scope, MatchScope::Line);
    }

    #[test]
    fn test_parse_rule_colon_in_pattern() {
        // Pattern \d{2}:\d{2}:\d{2} contains colons
        let rule = parse_rule(r"\d{2}:\d{2}:\d{2}:blue", 0).unwrap();
        assert!(rule.pattern.is_match("12:34:56"));
        assert_eq!(rule.scope, MatchScope::Line);
    }

    #[test]
    fn test_parse_rule_no_colon_error() {
        let result = parse_rule("invalid", 0);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("no colon found"), "Got: {msg}");
    }

    #[test]
    fn test_parse_rule_bad_regex() {
        let result = parse_rule("(bad regex:red", 0);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("invalid regex"), "Got: {msg}");
    }

    #[test]
    fn test_parse_rule_invalid_style() {
        let result = parse_rule("test:invalidcolor", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rule_match_scope() {
        let rule = parse_rule("ERROR:red:match", 0).unwrap();
        assert_eq!(rule.scope, MatchScope::Match);
    }

    #[test]
    fn test_parse_rule_capture_scope() {
        let rule = parse_rule("(\\d+):blue:cap1", 0).unwrap();
        assert_eq!(rule.scope, MatchScope::Capture(1));
    }

    #[test]
    fn test_parse_rule_next_scope() {
        let rule = parse_rule("^---:cyan:next1", 0).unwrap();
        assert_eq!(rule.scope, MatchScope::Next(1));
    }

    #[test]
    fn test_parse_rule_next_scope_multi() {
        let rule = parse_rule("HEADER:green:next3", 0).unwrap();
        assert_eq!(rule.scope, MatchScope::Next(3));
    }

    #[test]
    fn test_parse_scope_next_zero_invalid() {
        assert!(parse_scope("next0").is_none());
    }

    #[test]
    fn test_default_rules_count() {
        let rules = default_rules(0);
        assert_eq!(rules.len(), 5);
    }

    #[test]
    fn test_default_rules_case_insensitive() {
        let rules = default_rules(0);
        // ERROR rule (index 1) should match case-insensitively
        assert!(rules[1].pattern.is_match("error"));
        assert!(rules[1].pattern.is_match("ERROR"));
        assert!(rules[1].pattern.is_match("Error"));
    }

    #[test]
    fn test_default_rules_fatal_critical() {
        let rules = default_rules(0);
        assert!(rules[0].pattern.is_match("FATAL"));
        assert!(rules[0].pattern.is_match("CRITICAL"));
        assert!(rules[0].pattern.is_match("fatal"));
    }

    #[test]
    fn test_default_rules_warn() {
        let rules = default_rules(0);
        assert!(rules[2].pattern.is_match("WARN"));
        assert!(rules[2].pattern.is_match("WARNING"));
    }

    #[test]
    fn test_default_rules_debug() {
        let rules = default_rules(0);
        assert!(rules[3].pattern.is_match("DEBUG"));
    }

    #[test]
    fn test_default_rules_trace() {
        let rules = default_rules(0);
        assert!(rules[4].pattern.is_match("TRACE"));
    }

    #[test]
    fn test_default_rules_all_line_scope() {
        let rules = default_rules(0);
        for rule in &rules {
            assert_eq!(rule.scope, MatchScope::Line);
        }
    }

    #[test]
    fn test_build_rules_empty_cli() {
        let rules = build_rules(&[]).unwrap();
        assert_eq!(rules.len(), 0); // No default rules
    }

    #[test]
    fn test_build_rules_user_rules_only() {
        let cli_rules = vec!["mypattern:blue".to_string()];
        let rules = build_rules(&cli_rules).unwrap();
        assert_eq!(rules.len(), 1); // Just the user rule
        assert_eq!(rules[0].priority, 0);
        assert!(rules[0].pattern.is_match("mypattern"));
    }

    #[test]
    fn test_build_rules_invalid_rule_error() {
        let cli_rules = vec!["invalid".to_string()];
        let result = build_rules(&cli_rules);
        assert!(result.is_err());
    }

    #[test]
    fn test_priority_assignment() {
        let rule = parse_rule("test:red", 5).unwrap();
        assert_eq!(rule.priority, 5);
    }

    // === rule_from_config tests ===

    #[test]
    fn test_rule_from_config_valid() {
        let rule = rule_from_config("ERROR", "red", "match", 0).unwrap();
        assert!(rule.pattern.is_match("ERROR"));
        assert_eq!(rule.scope, MatchScope::Match);
        assert_eq!(rule.priority, 0);
    }

    #[test]
    fn test_rule_from_config_empty_scope_defaults_to_line() {
        let rule = rule_from_config("WARN", "yellow", "", 1).unwrap();
        assert_eq!(rule.scope, MatchScope::Line);
    }

    #[test]
    fn test_rule_from_config_capture_scope() {
        let rule = rule_from_config(r"user=(\d+)", "blue", "cap1", 2).unwrap();
        assert_eq!(rule.scope, MatchScope::Capture(1));
    }

    #[test]
    fn test_rule_from_config_invalid_regex() {
        let result = rule_from_config("(bad", "red", "line", 0);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("invalid regex"), "Got: {msg}");
    }

    #[test]
    fn test_rule_from_config_invalid_style() {
        let result = rule_from_config("test", "notacolor", "line", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_from_config_invalid_scope() {
        let result = rule_from_config("test", "red", "bogus", 0);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("invalid scope"), "Got: {msg}");
    }

    // === build_rules_with_config tests ===

    use crate::config::{Config, ProfileConfig, RuleConfig};
    use std::collections::HashMap;

    fn make_rule_config(pattern: &str, style: &str) -> RuleConfig {
        RuleConfig {
            pattern: pattern.to_string(),
            style: style.to_string(),
            scope: "line".to_string(),
            text: None,
        }
    }

    #[test]
    fn test_build_with_config_no_config() {
        let rules = build_rules_with_config(&[], None, None, None).unwrap();
        assert_eq!(rules.len(), 0); // no default rules, no profile
    }

    #[test]
    fn test_build_with_config_cli_plus_config() {
        let config = Config {
            rules: vec![make_rule_config("CUSTOM", "green")],
            profiles: HashMap::new(),
            default_profile: None,
            theme: None,
            syntax_map: HashMap::new(),
            update_check_interval_days: 7,
            update_mode: None,
            default_file_mode: None,
        };
        let cli = vec!["ERROR:red".to_string()];
        let rules = build_rules_with_config(&cli, Some(&config), None, None).unwrap();
        // 1 CLI + 1 config = 2
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].priority, 0); // CLI
        assert_eq!(rules[1].priority, 1); // config
    }

    #[test]
    fn test_build_with_config_profile_rules() {
        let mut profiles = HashMap::new();
        profiles.insert(
            "django".to_string(),
            ProfileConfig {
                rules: vec![make_rule_config("django", "green")],
                trigger: vec![],
                before: None,
                after: None,
                lines: None,
                extensions: vec![],
                slow: None,
                slow_style: None,
            },
        );
        let config = Config {
            rules: vec![make_rule_config("GLOBAL", "blue")],
            profiles,
            default_profile: None,
            theme: None,
            syntax_map: HashMap::new(),
            update_check_interval_days: 7,
            update_mode: None,
            default_file_mode: None,
        };
        let rules = build_rules_with_config(&[], Some(&config), Some("django"), None).unwrap();
        // 1 profile + 1 global = 2
        assert_eq!(rules.len(), 2);
        assert!(rules[0].pattern.is_match("django"));
        assert!(rules[1].pattern.is_match("GLOBAL"));
    }

    #[test]
    fn test_build_with_config_unknown_profile() {
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
                    "spring".to_string(),
                    ProfileConfig { rules: vec![], trigger: vec![], before: None, after: None, lines: None, extensions: vec![], slow: None, slow_style: None },
                );
                p
            },
        };
        let result = build_rules_with_config(&[], Some(&config), Some("nginx"), None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("nginx"), "Got: {msg}");
        assert!(msg.contains("spring"), "Got: {msg}");
    }

    #[test]
    fn test_build_with_config_profile_no_config() {
        let result = build_rules_with_config(&[], None, Some("django"), None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("no config file found"), "Got: {msg}");
    }

    // === Insert scope parsing tests ===

    #[test]
    fn test_parse_rule_insert_before() {
        let rule = parse_rule("ERROR::insert-before:--- alert ---", 0).unwrap();
        assert!(rule.pattern.is_match("ERROR"));
        match &rule.scope {
            MatchScope::InsertBefore(tmpl) => {
                assert_eq!(tmpl.template, "--- alert ---");
            }
            other => panic!("expected InsertBefore, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_rule_insert_after() {
        let rule = parse_rule("FATAL::insert-after:[red]^^^[/]", 0).unwrap();
        assert!(rule.pattern.is_match("FATAL"));
        match &rule.scope {
            MatchScope::InsertAfter(tmpl) => {
                assert_eq!(tmpl.template, "[red]^^^[/]");
                assert_eq!(tmpl.segments.len(), 1);
            }
            other => panic!("expected InsertAfter, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_rule_prepend() {
        let rule = parse_rule("WARN:yellow:prepend:⚠ ", 0).unwrap();
        assert!(rule.pattern.is_match("WARN"));
        assert_ne!(rule.style, Style::new(), "style should not be default");
        match &rule.scope {
            MatchScope::Prepend(tmpl) => {
                assert_eq!(tmpl.template, "⚠ ");
            }
            other => panic!("expected Prepend, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_rule_append() {
        let rule = parse_rule("DEBUG::append: [dim](debug)[/]", 0).unwrap();
        assert!(rule.pattern.is_match("DEBUG"));
        match &rule.scope {
            MatchScope::Append(tmpl) => {
                assert_eq!(tmpl.template, " [dim](debug)[/]");
            }
            other => panic!("expected Append, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_rule_insert_with_colons_in_template() {
        let rule = parse_rule("ERROR::insert-before:--- 12:34:56 ---", 0).unwrap();
        assert!(rule.pattern.is_match("ERROR"));
        match &rule.scope {
            MatchScope::InsertBefore(tmpl) => {
                assert_eq!(tmpl.template, "--- 12:34:56 ---");
            }
            other => panic!("expected InsertBefore, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_rule_insert_missing_template() {
        let result = parse_rule("ERROR::insert-before:", 0);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("missing template"), "Got: {msg}");
    }

    #[test]
    fn test_parse_rule_backward_compat_two_parts() {
        let rule = parse_rule("ERROR:red", 0).unwrap();
        assert_eq!(rule.scope, MatchScope::Line);
    }

    #[test]
    fn test_parse_rule_backward_compat_three_parts() {
        let rule = parse_rule("ERROR:red:match", 0).unwrap();
        assert_eq!(rule.scope, MatchScope::Match);
    }

    // === rule_from_config_with_text tests ===

    #[test]
    fn test_rule_from_config_insert_before() {
        let rule = rule_from_config_with_text(
            "ERROR", "red", "insert-before", "--- alert ---", 0,
        ).unwrap();
        assert!(rule.pattern.is_match("ERROR"));
        match &rule.scope {
            MatchScope::InsertBefore(tmpl) => {
                assert_eq!(tmpl.template, "--- alert ---");
            }
            other => panic!("expected InsertBefore, got: {other:?}"),
        }
    }
}

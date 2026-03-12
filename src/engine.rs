use std::ops::Range;

use owo_colors::{OwoColorize, Style};

use crate::rules::{MatchScope, Rule};

/// A styled region of text within a line.
struct Span {
    range: Range<usize>,
    style: Style,
    priority: usize,
}

/// The color engine applies rules to lines of text using span-based coloring.
///
/// Each rule produces styled spans based on its scope (line/match/capture).
/// When multiple rules match, overlapping regions resolve by priority
/// (lowest number wins).
pub struct Engine {
    rules: Vec<Rule>,
    color_enabled: bool,
}

impl Engine {
    /// Create a new engine with the given rules and color mode.
    pub fn new(rules: Vec<Rule>, color_enabled: bool) -> Self {
        Self {
            rules,
            color_enabled,
        }
    }

    /// Apply rules to a line of text. Returns the styled line.
    ///
    /// Each matching rule produces spans based on its scope:
    /// - `Line`: colors the entire line
    /// - `Match`: colors only the matched text region
    /// - `Capture(n)`: colors only capture group `n`
    ///
    /// When spans overlap, the one with the lowest priority number wins.
    pub fn apply(&self, line: &str) -> String {
        if !self.color_enabled || self.rules.is_empty() || line.is_empty() {
            return line.to_string();
        }

        // Collect spans from all matching rules
        let mut spans = Vec::new();
        for rule in &self.rules {
            if let Some(span) = self.rule_span(rule, line) {
                spans.push(span);
            }
        }

        if spans.is_empty() {
            return line.to_string();
        }

        // Build per-byte style map: lowest priority number wins
        let len = line.len();
        let mut style_map: Vec<Option<(usize, Style)>> = vec![None; len];

        // Sort by priority ascending (lowest number = highest priority)
        spans.sort_by_key(|s| s.priority);

        for span in &spans {
            for pos in span.range.clone() {
                if pos < len && style_map[pos].is_none() {
                    style_map[pos] = Some((span.priority, span.style));
                }
            }
        }

        // Coalesce consecutive positions with the same style into segments
        self.render(line, &style_map)
    }

    /// Produce a span from a rule if it matches the line.
    fn rule_span(&self, rule: &Rule, line: &str) -> Option<Span> {
        match &rule.scope {
            MatchScope::Line => {
                if rule.pattern.is_match(line) {
                    Some(Span {
                        range: 0..line.len(),
                        style: rule.style,
                        priority: rule.priority,
                    })
                } else {
                    None
                }
            }
            MatchScope::Match => {
                rule.pattern.find(line).map(|m| Span {
                    range: m.start()..m.end(),
                    style: rule.style,
                    priority: rule.priority,
                })
            }
            MatchScope::Capture(n) => {
                let caps = rule.pattern.captures(line)?;
                let group = caps.get(*n)?;
                Some(Span {
                    range: group.start()..group.end(),
                    style: rule.style,
                    priority: rule.priority,
                })
            }
        }
    }

    /// Render a line using the per-byte style map.
    /// Coalesces consecutive bytes with the same style into segments.
    fn render(&self, line: &str, style_map: &[Option<(usize, Style)>]) -> String {
        let bytes = line.as_bytes();
        let len = bytes.len();
        if len == 0 {
            return String::new();
        }

        let mut result = String::with_capacity(len + 64);
        let mut pos = 0;

        while pos < len {
            let current_style = style_map[pos];

            // Find the end of this segment (same style)
            let seg_start = pos;
            pos += 1;
            while pos < len && style_map[pos] == current_style {
                pos += 1;
            }

            let segment = &line[seg_start..pos];

            match current_style {
                Some((_, style)) => {
                    result.push_str(&segment.style(style).to_string());
                }
                None => {
                    result.push_str(segment);
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::build_rules;

    // Helper: create a rule directly for testing (bypasses string parsing)
    impl crate::rules::Rule {
        #[cfg(test)]
        fn test_new(pattern: &str, style_spec: &str, priority: usize) -> Self {
            Self::test_with_scope(pattern, style_spec, MatchScope::Line, priority)
        }

        #[cfg(test)]
        fn test_with_scope(
            pattern: &str,
            style_spec: &str,
            scope: MatchScope,
            priority: usize,
        ) -> Self {
            use crate::color;
            use regex::Regex;
            Self {
                pattern: Regex::new(pattern).unwrap(),
                style: color::parse_style(style_spec).unwrap(),
                scope,
                priority,
            }
        }
    }

    fn parse_rule(pattern: &str, style: &str, priority: usize) -> Rule {
        Rule::test_new(pattern, style, priority)
    }

    // === Existing tests that should still pass ===

    #[test]
    fn test_single_rule_match() {
        let rules = vec![parse_rule("ERROR", "red", 0)];
        let engine = Engine::new(rules, true);
        let result = engine.apply("ERROR: something broke");
        // Should be styled (not equal to plain text)
        assert_ne!(result, "ERROR: something broke");
    }

    #[test]
    fn test_no_match_passthrough() {
        let rules = vec![parse_rule("ERROR", "red", 0)];
        let engine = Engine::new(rules, true);
        let result = engine.apply("just a normal line");
        assert_eq!(result, "just a normal line");
    }

    #[test]
    fn test_no_rules_passthrough() {
        let engine = Engine::new(vec![], true);
        let result = engine.apply("anything here");
        assert_eq!(result, "anything here");
    }

    #[test]
    fn test_color_disabled_passthrough() {
        let rules = vec![parse_rule("ERROR", "red", 0)];
        let engine = Engine::new(rules, false);
        let result = engine.apply("ERROR: something");
        assert_eq!(result, "ERROR: something");
    }

    #[test]
    fn test_default_rules_error_case_insensitive() {
        let rules = build_rules(&[]).unwrap();
        let engine = Engine::new(rules, true);
        let result = engine.apply("error happened");
        assert_ne!(result, "error happened");
    }

    #[test]
    fn test_default_rules_warn() {
        let rules = build_rules(&[]).unwrap();
        let engine = Engine::new(rules, true);
        let result = engine.apply("WARNING: disk full");
        assert_ne!(result, "WARNING: disk full");
    }

    #[test]
    fn test_default_rules_debug() {
        let rules = build_rules(&[]).unwrap();
        let engine = Engine::new(rules, true);
        let result = engine.apply("DEBUG: entering fn");
        assert_ne!(result, "DEBUG: entering fn");
    }

    // === New span-based coloring tests ===

    #[test]
    fn test_match_scope_colors_only_matched_text() {
        // Match-scope: only "ERROR" should be red, rest plain
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Match, 0)];
        let engine = Engine::new(rules, true);
        let result = engine.apply("2024 ERROR: fail");

        // The plain text portions should appear unchanged
        assert!(result.contains("2024 "), "should contain plain '2024 '");
        assert!(result.contains(": fail"), "should contain plain ': fail'");
        // The styled output should differ from whole-line styling
        let whole_line_styled = "2024 ERROR: fail"
            .style(crate::color::parse_style("red").unwrap())
            .to_string();
        assert_ne!(
            result, whole_line_styled,
            "match scope should not color the whole line"
        );
        // But it should not be plain either
        assert_ne!(
            result, "2024 ERROR: fail",
            "match scope should produce styled output"
        );
    }

    #[test]
    fn test_capture_scope_colors_only_capture_group() {
        // Capture scope: only "42" should be blue
        let rules = vec![Rule::test_with_scope(
            r"user=(\d+)",
            "blue",
            MatchScope::Capture(1),
            0,
        )];
        let engine = Engine::new(rules, true);
        let result = engine.apply("user=42 action=login");

        // "user=" should be plain, " action=login" should be plain
        assert!(result.contains("user="), "should contain plain 'user='");
        assert!(
            result.contains(" action=login"),
            "should contain plain ' action=login'"
        );
        assert_ne!(
            result, "user=42 action=login",
            "capture scope should produce styled output"
        );
    }

    #[test]
    fn test_line_scope_colors_entire_line() {
        // Line-scope regression check: entire line should be colored
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Line, 0)];
        let engine = Engine::new(rules, true);
        let result = engine.apply("ERROR: fail");

        let expected = "ERROR: fail"
            .style(crate::color::parse_style("red").unwrap())
            .to_string();
        assert_eq!(result, expected, "line scope should color the entire line");
    }

    #[test]
    fn test_multi_rule_overlap() {
        // Two rules: digits in blue (match scope), ERROR makes whole line red (line scope)
        let rules = vec![
            Rule::test_with_scope(r"\d{4}", "blue", MatchScope::Match, 0),
            Rule::test_with_scope("ERROR", "red", MatchScope::Line, 1),
        ];
        let engine = Engine::new(rules, true);
        let result = engine.apply("2024 ERROR: fail");

        // Should not be plain
        assert_ne!(result, "2024 ERROR: fail");
        // Should not be entirely blue (only digits are blue)
        let all_blue = "2024 ERROR: fail"
            .style(crate::color::parse_style("blue").unwrap())
            .to_string();
        assert_ne!(result, all_blue);
        // Should not be entirely red (digits region should be blue)
        let all_red = "2024 ERROR: fail"
            .style(crate::color::parse_style("red").unwrap())
            .to_string();
        assert_ne!(result, all_red);
    }

    #[test]
    fn test_priority_conflict_lower_number_wins() {
        // Two rules both matching "ERROR": priority 0 (blue) wins over priority 1 (red)
        let rules = vec![
            Rule::test_with_scope("ERROR", "blue", MatchScope::Match, 0),
            Rule::test_with_scope("ERROR", "red", MatchScope::Match, 1),
        ];
        let engine = Engine::new(rules, true);
        let result = engine.apply("ERROR: test");

        // Build what blue-only match scope would produce
        let blue_only = {
            let r = vec![Rule::test_with_scope("ERROR", "blue", MatchScope::Match, 0)];
            Engine::new(r, true).apply("ERROR: test")
        };
        assert_eq!(result, blue_only, "lower priority number should win");
    }

    #[test]
    fn test_missing_capture_group_silently_skipped() {
        // Rule asks for cap2 but regex only has one capture group
        let rules = vec![Rule::test_with_scope(
            "(foo)",
            "green",
            MatchScope::Capture(2),
            0,
        )];
        let engine = Engine::new(rules, true);
        let result = engine.apply("foo bar");
        // Missing capture group -> no coloring, returned unchanged
        assert_eq!(result, "foo bar");
    }

    #[test]
    fn test_empty_rules_passthrough() {
        let engine = Engine::new(vec![], true);
        let result = engine.apply("some line");
        assert_eq!(result, "some line");
    }

    #[test]
    fn test_color_disabled_returns_unchanged() {
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Match, 0)];
        let engine = Engine::new(rules, false);
        let result = engine.apply("ERROR: test");
        assert_eq!(result, "ERROR: test");
    }
}

use std::ops::Range;

use owo_colors::{OwoColorize, Style};

use crate::rules::{MatchScope, Rule};
use crate::trigger::strip_ansi;

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
/// (lowest number wins). An optional syntect-based syntax highlighter provides
/// a base layer underneath all rules (priority `usize::MAX`).
pub struct Engine {
    rules: Vec<Rule>,
    color_enabled: bool,
    syntax: Option<crate::syntax::SyntaxHighlighter>,
    /// Carry-forward styles from Next(n) rules: (style, remaining_lines, priority)
    pending_styles: Vec<(Style, usize, usize)>,
}

impl Engine {
    /// Create a new engine with the given rules, color mode, and optional syntax highlighter.
    pub fn new(rules: Vec<Rule>, color_enabled: bool, syntax: Option<crate::syntax::SyntaxHighlighter>) -> Self {
        Self {
            rules,
            color_enabled,
            syntax,
            pending_styles: Vec::new(),
        }
    }

    /// Return the number of rules in this engine.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Apply rules to a line of text. Returns the styled line.
    ///
    /// Each matching rule produces spans based on its scope:
    /// - `Line`: colors the entire line
    /// - `Match`: colors only the matched text region
    /// - `Capture(n)`: colors only capture group `n`
    ///
    /// When spans overlap, the one with the lowest priority number wins.
    /// Syntect base-layer spans have `usize::MAX` priority (lowest — only fills
    /// bytes that no rule claimed).
    pub fn apply(&mut self, line: &str) -> String {
        let has_syntax = self.syntax.is_some();
        let has_pending = !self.pending_styles.is_empty();
        if !self.color_enabled || line.is_empty() {
            return line.to_string();
        }
        if self.rules.is_empty() && !has_syntax && !has_pending {
            return line.to_string();
        }

        // When pending styles are active (carry-forward from Next rules) or
        // cap/next rules exist, strip ANSI from the line so patterns match
        // clean text and lux owns the styling of affected lines.
        let needs_strip = has_pending || self.rules.iter().any(|r| {
            matches!(&r.scope, MatchScope::Next(_) | MatchScope::Capture(_))
        });
        let clean;
        let work_line = if needs_strip {
            clean = strip_ansi(line);
            &clean
        } else {
            line
        };

        // Collect spans from all matching rules
        let mut spans = Vec::new();

        // Apply any pending carry-forward styles (from previous Next rules)
        let mut i = 0;
        while i < self.pending_styles.len() {
            let (style, remaining, priority) = &mut self.pending_styles[i];
            spans.push(Span {
                range: 0..work_line.len(),
                style: *style,
                priority: *priority,
            });
            *remaining -= 1;
            if *remaining == 0 {
                self.pending_styles.swap_remove(i);
            } else {
                i += 1;
            }
        }

        for rule in &self.rules {
            if let MatchScope::Next(n) = &rule.scope {
                // Next scope: match against clean text, queue style for upcoming lines
                if rule.pattern.is_match(work_line) {
                    self.pending_styles.push((rule.style, *n, rule.priority));
                }
            } else {
                spans.extend(Self::rule_spans(rule, work_line));
            }
        }

        // Add syntect base-layer spans (lowest priority)
        if let Some(ref sh) = self.syntax {
            for (range, style) in sh.highlight_line(work_line) {
                spans.push(Span {
                    range,
                    style,
                    priority: usize::MAX,
                });
            }
        }

        if spans.is_empty() {
            return work_line.to_string();
        }

        // Build per-byte style map: lowest priority number wins
        let len = work_line.len();
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
        self.render(work_line, &style_map)
    }

    /// Produce spans from a rule for all matches in the line.
    fn rule_spans(rule: &Rule, line: &str) -> Vec<Span> {
        match &rule.scope {
            MatchScope::Line => {
                if rule.pattern.is_match(line) {
                    vec![Span {
                        range: 0..line.len(),
                        style: rule.style,
                        priority: rule.priority,
                    }]
                } else {
                    vec![]
                }
            }
            MatchScope::Match => {
                rule.pattern.find_iter(line).map(|m| Span {
                    range: m.start()..m.end(),
                    style: rule.style,
                    priority: rule.priority,
                }).collect()
            }
            MatchScope::Capture(n) => {
                rule.pattern.captures_iter(line).filter_map(|caps| {
                    caps.get(*n).map(|group| Span {
                        range: group.start()..group.end(),
                        style: rule.style,
                        priority: rule.priority,
                    })
                }).collect()
            }
            MatchScope::Next(_) => vec![], // handled in apply() directly
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
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("ERROR: something broke");
        // Should be styled (not equal to plain text)
        assert_ne!(result, "ERROR: something broke");
    }

    #[test]
    fn test_no_match_passthrough() {
        let rules = vec![parse_rule("ERROR", "red", 0)];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("just a normal line");
        assert_eq!(result, "just a normal line");
    }

    #[test]
    fn test_no_rules_passthrough() {
        let mut engine = Engine::new(vec![], true, None);
        let result = engine.apply("anything here");
        assert_eq!(result, "anything here");
    }

    #[test]
    fn test_color_disabled_passthrough() {
        let rules = vec![parse_rule("ERROR", "red", 0)];
        let mut engine = Engine::new(rules, false, None);
        let result = engine.apply("ERROR: something");
        assert_eq!(result, "ERROR: something");
    }

    #[test]
    fn test_no_rules_no_coloring() {
        let rules = build_rules(&[]).unwrap();
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("error happened");
        assert_eq!(result, "error happened");
    }

    // === New span-based coloring tests ===

    #[test]
    fn test_match_scope_colors_only_matched_text() {
        // Match-scope: only "ERROR" should be red, rest plain
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Match, 0)];
        let mut engine = Engine::new(rules, true, None);
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
        let mut engine = Engine::new(rules, true, None);
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
        let mut engine = Engine::new(rules, true, None);
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
        let mut engine = Engine::new(rules, true, None);
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
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("ERROR: test");

        // Build what blue-only match scope would produce
        let blue_only = {
            let r = vec![Rule::test_with_scope("ERROR", "blue", MatchScope::Match, 0)];
            Engine::new(r, true, None).apply("ERROR: test")
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
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("foo bar");
        // Missing capture group -> no coloring, returned unchanged
        assert_eq!(result, "foo bar");
    }

    #[test]
    fn test_empty_rules_passthrough() {
        let mut engine = Engine::new(vec![], true, None);
        let result = engine.apply("some line");
        assert_eq!(result, "some line");
    }

    #[test]
    fn test_color_disabled_returns_unchanged() {
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Match, 0)];
        let mut engine = Engine::new(rules, false, None);
        let result = engine.apply("ERROR: test");
        assert_eq!(result, "ERROR: test");
    }

    // === Next scope tests ===

    #[test]
    fn test_next_scope_colors_following_line() {
        let rules = vec![Rule::test_with_scope("^---+$", "cyan", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        // The separator line itself should NOT be colored
        let sep = engine.apply("----------");
        assert_eq!(sep, "----------");

        // The next line SHOULD be colored
        let next = engine.apply("this should be cyan");
        assert_ne!(next, "this should be cyan", "next line should be styled");

        // The line after that should NOT be colored (only next1)
        let after = engine.apply("this should be plain");
        assert_eq!(after, "this should be plain");
    }

    #[test]
    fn test_next_scope_colors_multiple_following_lines() {
        let rules = vec![Rule::test_with_scope("HEADER", "green", MatchScope::Next(2), 0)];
        let mut engine = Engine::new(rules, true, None);

        let header = engine.apply("HEADER");
        assert_eq!(header, "HEADER", "trigger line should not be colored");

        let line1 = engine.apply("line one");
        assert_ne!(line1, "line one", "first next line should be styled");

        let line2 = engine.apply("line two");
        assert_ne!(line2, "line two", "second next line should be styled");

        let line3 = engine.apply("line three");
        assert_eq!(line3, "line three", "third line should be plain");
    }

    #[test]
    fn test_next_scope_with_other_rules() {
        // Next scope + a regular line-scope rule
        let rules = vec![
            Rule::test_with_scope("^---", "cyan", MatchScope::Next(1), 1),
            Rule::test_with_scope("ERROR", "red", MatchScope::Line, 0),
        ];
        let mut engine = Engine::new(rules, true, None);

        engine.apply("----------"); // trigger

        // Next line has both: carry-forward cyan AND ERROR match (red wins by priority)
        let result = engine.apply("ERROR happened");
        // Should be styled (not plain)
        assert_ne!(result, "ERROR happened");
    }

    #[test]
    fn test_next_scope_no_match_no_carry() {
        let rules = vec![Rule::test_with_scope("TRIGGER", "red", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        // No trigger matched
        let line1 = engine.apply("no trigger here");
        assert_eq!(line1, "no trigger here");

        let line2 = engine.apply("still plain");
        assert_eq!(line2, "still plain");
    }

    #[test]
    fn test_next_scope_retrigger_resets() {
        let rules = vec![Rule::test_with_scope("^---", "cyan", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        engine.apply("---"); // trigger
        let next1 = engine.apply("colored");
        assert_ne!(next1, "colored");

        let plain = engine.apply("plain");
        assert_eq!(plain, "plain");

        engine.apply("---"); // re-trigger
        let next2 = engine.apply("colored again");
        assert_ne!(next2, "colored again");
    }

    // === ANSI stripping for cap/next ===

    #[test]
    fn test_next_scope_strips_ansi_for_matching() {
        let rules = vec![Rule::test_with_scope("^-+$", "magenta", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        // Trigger line has ANSI codes wrapping the dashes (like colored output from another tool)
        engine.apply("\x1b[38;5;243m------------------------------\x1b[0m");

        // The next line should still be colored (trigger matched the stripped version)
        let next = engine.apply("this line should be magenta");
        assert_ne!(next, "this line should be magenta", "next line should be styled");
    }

    #[test]
    fn test_next_carryforward_strips_ansi_from_following_lines() {
        let rules = vec![Rule::test_with_scope("TRIGGER", "cyan", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        engine.apply("TRIGGER");

        // Following line has ANSI codes — they should be stripped in output
        let result = engine.apply("\x1b[31mred text\x1b[0m");
        // Output should NOT contain the original ANSI red code
        assert!(!result.contains("\x1b[31m"), "original ANSI should be stripped. Got: {result}");
        // But should contain the text, styled by lux
        assert!(result.contains("red text"), "text content should be preserved. Got: {result}");
    }

    #[test]
    fn test_capture_scope_strips_ansi_for_matching() {
        // Pattern captures text inside quotes
        let rules = vec![Rule::test_with_scope(
            r#"running: "(.*)""#,
            "cyan",
            MatchScope::Capture(1),
            0,
        )];
        let mut engine = Engine::new(rules, true, None);

        // Input has ANSI codes around the quoted value
        let result = engine.apply("running: \"\x1b[32mkubectl apply\x1b[0m\"");
        // Should match and style the capture group from the clean text
        assert_ne!(
            result, "running: \"kubectl apply\"",
            "capture should produce styled output"
        );
        // Original ANSI codes should be stripped
        assert!(!result.contains("\x1b[32m"), "original ANSI should be stripped. Got: {result}");
    }

    #[test]
    fn test_line_scope_preserves_ansi_when_no_cap_next() {
        // With only line/match scope rules, original ANSI is preserved
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Line, 0)];
        let mut engine = Engine::new(rules, true, None);

        let result = engine.apply("\x1b[33mERROR: something\x1b[0m");
        // Line scope should style the whole line but the input text includes ANSI
        assert_ne!(result, "\x1b[33mERROR: something\x1b[0m");
    }
}

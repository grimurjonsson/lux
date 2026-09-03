use std::ops::Range;

use owo_colors::{OwoColorize, Style};

use crate::markup;
use crate::rules::{InsertTemplate, MatchScope, Rule};

/// A styled region of text within a line.
struct Span {
    range: Range<usize>,
    style: Style,
    priority: usize,
}

/// The result of applying engine rules to a single input line.
///
/// Contains the styled main line plus any lines to insert before/after it
/// (from InsertBefore/InsertAfter rules) and any text prepended/appended
/// (from Prepend/Append rules, folded into `line`).
pub struct ApplyResult {
    /// Lines to emit before the main line (InsertBefore rules).
    pub before: Vec<String>,
    /// The styled (and possibly prepended/appended) main line.
    pub line: String,
    /// Lines to emit after the main line (InsertAfter rules).
    pub after: Vec<String>,
}

impl ApplyResult {
    /// Flatten the result into a single Vec of lines in order: before, line, after.
    pub fn flatten(self) -> Vec<String> {
        let mut lines = self.before;
        lines.push(self.line);
        lines.extend(self.after);
        lines
    }
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

    /// Check whether any rule uses an insert scope (InsertBefore/InsertAfter/Prepend/Append).
    fn has_insert_rules(&self) -> bool {
        self.rules.iter().any(|r| {
            matches!(
                &r.scope,
                MatchScope::InsertBefore(_)
                    | MatchScope::InsertAfter(_)
                    | MatchScope::Prepend(_)
                    | MatchScope::Append(_)
            )
        })
    }

    /// Apply rules to a line of text. Returns an `ApplyResult` containing
    /// the styled main line plus any before/after insert lines.
    ///
    /// Each matching rule produces spans based on its scope:
    /// - `Line`: colors the entire line
    /// - `Match`: colors only the matched text region
    /// - `Capture(n)`: colors only capture group `n`
    /// - `InsertBefore/InsertAfter`: inserts a new line before/after the main line
    /// - `Prepend/Append`: prepends/appends text to the main line
    ///
    /// When spans overlap, the one with the lowest priority number wins.
    /// Syntect base-layer spans have `usize::MAX` priority (lowest — only fills
    /// bytes that no rule claimed).
    pub fn apply(&mut self, line: &str) -> ApplyResult {
        let empty = ApplyResult {
            before: vec![],
            line: line.to_string(),
            after: vec![],
        };

        if line.is_empty() {
            // Nothing to style, but the syntax parser must still see the line:
            // it carries state across lines, and in grammars like Markdown a
            // blank line is what closes a list item or paragraph.
            if let Some(ref mut sh) = self.syntax {
                sh.highlight_line("");
            }
            return empty;
        }

        let has_syntax = self.syntax.is_some();
        let has_pending = !self.pending_styles.is_empty();
        let has_inserts = self.has_insert_rules();

        // If color is disabled and there are no insert rules, return the line as-is.
        // If color is disabled but insert rules exist, skip span styling but still
        // process inserts (with color_enabled=false so templates render plain).
        if !self.color_enabled && !has_inserts {
            return empty;
        }

        if self.rules.is_empty() && !has_syntax && !has_pending {
            return empty;
        }

        // Always build a clean (ANSI-stripped) view for pattern matching, plus a
        // byte-level map back to the raw line. Lux matches rules against the clean
        // text but emits from the raw line, preserving any existing ANSI outside
        // regions that lux actually styles.
        let (clean, clean_to_raw) = strip_ansi_with_map(line);
        let work_line = clean.as_str();

        // Collect spans from all matching rules; also collect insert actions.
        let mut spans = Vec::new();
        let mut before_lines: Vec<String> = Vec::new();
        let mut after_lines: Vec<String> = Vec::new();
        let mut prepend_parts: Vec<String> = Vec::new();
        let mut append_parts: Vec<String> = Vec::new();

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
            match &rule.scope {
                MatchScope::Next(n) => {
                    // Next scope: match against clean text, queue style for upcoming lines
                    if rule.pattern.is_match(work_line) {
                        self.pending_styles.push((rule.style, *n, rule.priority));
                    }
                }
                MatchScope::InsertBefore(tmpl) => {
                    self.collect_inserts(work_line, &rule.pattern, &rule.style, tmpl, &mut before_lines);
                }
                MatchScope::InsertAfter(tmpl) => {
                    self.collect_inserts(work_line, &rule.pattern, &rule.style, tmpl, &mut after_lines);
                }
                MatchScope::Prepend(tmpl) => {
                    self.collect_inserts(work_line, &rule.pattern, &rule.style, tmpl, &mut prepend_parts);
                }
                MatchScope::Append(tmpl) => {
                    self.collect_inserts(work_line, &rule.pattern, &rule.style, tmpl, &mut append_parts);
                }
                _ => {
                    spans.extend(Self::rule_spans(rule, work_line));
                }
            }
        }

        // Add syntect base-layer spans (lowest priority)
        if let Some(ref mut sh) = self.syntax {
            for (range, style) in sh.highlight_line(work_line) {
                spans.push(Span {
                    range,
                    style,
                    priority: usize::MAX,
                });
            }
        }

        // Build the styled main line.
        // - color disabled, or no spans: pass the raw line through unchanged.
        // - otherwise: overlay lux styles onto raw, preserving surrounding ANSI.
        let styled_line = if !self.color_enabled || spans.is_empty() {
            line.to_string()
        } else {
            let len = work_line.len();
            let mut style_map: Vec<Option<(usize, Style)>> = vec![None; len];

            spans.sort_by_key(|s| s.priority);

            for span in &spans {
                for pos in span.range.clone() {
                    if pos < len && style_map[pos].is_none() {
                        style_map[pos] = Some((span.priority, span.style));
                    }
                }
            }

            render_preserving_ansi(line, work_line, &clean_to_raw, &style_map)
        };

        // Apply prepend/append to the main line
        let mut final_line = String::new();
        for p in &prepend_parts {
            final_line.push_str(p);
        }
        final_line.push_str(&styled_line);
        for a in &append_parts {
            final_line.push_str(a);
        }

        ApplyResult {
            before: before_lines,
            line: final_line,
            after: after_lines,
        }
    }

    /// Check if a rule's pattern matches the work line, and if so, render the
    /// template with capture interpolation and push the result into the target vec.
    fn collect_inserts(
        &self,
        work_line: &str,
        pattern: &regex::Regex,
        style: &Style,
        tmpl: &InsertTemplate,
        target: &mut Vec<String>,
    ) {
        if let Some(caps) = pattern.captures(work_line) {
            let default_style = if *style == Style::new() {
                None
            } else {
                Some(*style)
            };
            let rendered = markup::render_template(
                &tmpl.segments,
                &caps,
                default_style,
                self.color_enabled,
            );
            target.push(rendered);
        }
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
            MatchScope::InsertBefore(_) | MatchScope::InsertAfter(_)
            | MatchScope::Prepend(_) | MatchScope::Append(_) => vec![],
        }
    }

}

/// Strip ANSI SGR escape sequences from `raw` while recording, for every clean
/// byte `k`, the corresponding byte index in the original raw string.
///
/// Returns `(clean, clean_to_raw)` where:
/// - `clean` is the raw text with `\x1b[...m` sequences removed.
/// - `clean_to_raw[k]` is the raw byte offset of clean byte `k`, for `k` in
///   `0..clean.len()`. `clean_to_raw[clean.len()]` holds the raw length as a
///   sentinel so callers can compute tail ranges.
///
/// Raw bytes carrying UTF-8 continuation bytes share the same single-byte
/// correspondence as their leading byte (each raw non-ANSI byte maps 1:1 to a
/// clean byte).
fn strip_ansi_with_map(raw: &str) -> (String, Vec<usize>) {
    let bytes = raw.as_bytes();
    let mut clean = String::with_capacity(bytes.len());
    let mut map: Vec<usize> = Vec::with_capacity(bytes.len() + 1);
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // Match the strip_ansi semantics: OSC sequences (ESC ]) run to
            // BEL or ST (ESC \); everything else skips through the next 'm'.
            i += 1;
            if bytes.get(i) == Some(&b']') {
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if b == 0x07 {
                        break;
                    }
                    if b == 0x1b && bytes.get(i) == Some(&b'\\') {
                        i += 1;
                        break;
                    }
                }
            } else {
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if b == b'm' {
                        break;
                    }
                }
            }
        } else {
            map.push(i);
            // Copy this UTF-8 scalar whole. ANSI escapes never split a scalar
            // because they use only ASCII bytes, so the raw slice here is
            // guaranteed valid UTF-8.
            let char_len = utf8_char_len(bytes[i]);
            let end = (i + char_len).min(bytes.len());
            for extra in 1..(end - i) {
                map.push(i + extra);
            }
            clean.push_str(&raw[i..end]);
            i = end;
        }
    }
    map.push(bytes.len());
    (clean, map)
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 { 1 }
    else if b < 0xC0 { 1 }
    else if b < 0xE0 { 2 }
    else if b < 0xF0 { 3 }
    else { 4 }
}

/// Scan `text` for ANSI SGR escape sequences and update `active` to reflect the
/// terminal state after processing them. A `\x1b[0m` or `\x1b[m` reset clears
/// `active`; any other sequence is appended.
fn update_active_ansi(active: &mut String, text: &str) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if b == b'm' {
                    break;
                }
            }
            let seq = &text[start..i];
            if seq == "\x1b[0m" || seq == "\x1b[m" {
                active.clear();
            } else {
                active.push_str(seq);
            }
        } else {
            i += 1;
        }
    }
}

/// Render the line by overlaying lux styles onto the raw input. Regions the
/// rules did not claim keep their original ANSI verbatim; regions lux styles
/// are emitted as `reset + lux-style + clean text + reset`, with any ANSI
/// state that was active at the moment of the match restored for subsequent
/// unstyled regions.
fn render_preserving_ansi(
    raw: &str,
    clean: &str,
    clean_to_raw: &[usize],
    style_map: &[Option<(usize, Style)>],
) -> String {
    let mut out = String::with_capacity(raw.len() + 64);
    let mut active_ansi = String::new();
    let mut last_raw_end = 0usize;
    let clean_len = clean.len();
    let mut clean_pos = 0usize;

    while clean_pos < clean_len {
        let current_style = style_map[clean_pos];
        let seg_start = clean_pos;
        clean_pos += 1;
        while clean_pos < clean_len && style_map[clean_pos] == current_style {
            clean_pos += 1;
        }
        let seg_end = clean_pos;

        let seg_raw_start = clean_to_raw[seg_start];
        let seg_raw_end = clean_to_raw[seg_end - 1] + 1;

        // ANSI in the gap between the previous segment and this one: update
        // the running terminal state without emitting the codes directly —
        // they'd either double up with something we restore later or stomp on
        // a styled region that follows.
        let gap = &raw[last_raw_end..seg_raw_start];
        update_active_ansi(&mut active_ansi, gap);

        match current_style {
            None => {
                // Unstyled: restore whatever style was active before this
                // segment, then pass raw bytes through (including any ANSI
                // embedded between clean bytes within this run).
                if !active_ansi.is_empty() {
                    out.push_str(&active_ansi);
                }
                let content = &raw[seg_raw_start..seg_raw_end];
                out.push_str(content);
                update_active_ansi(&mut active_ansi, content);
            }
            Some((_, style)) => {
                // Styled: lux owns this region. Scan the raw span for any
                // embedded ANSI so state is correct for later segments, but
                // don't emit those codes — our style would override them.
                let content_raw = &raw[seg_raw_start..seg_raw_end];
                update_active_ansi(&mut active_ansi, content_raw);
                // Reset only when there is prior styling to clear; avoids
                // emitting noise on plain-text input.
                if !active_ansi.is_empty() {
                    out.push_str("\x1b[0m");
                }
                let clean_segment = &clean[seg_start..seg_end];
                out.push_str(&clean_segment.style(style).to_string());
            }
        }

        last_raw_end = seg_raw_end;
    }

    // Trailing raw bytes (typically a closing \x1b[0m). Emit verbatim.
    out.push_str(&raw[last_raw_end..]);

    out
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
            let style = if style_spec.is_empty() {
                Style::new()
            } else {
                color::parse_style(style_spec).unwrap()
            };
            Self {
                pattern: Regex::new(pattern).unwrap(),
                style,
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
        assert_ne!(result.line, "ERROR: something broke");
    }

    #[test]
    fn test_no_match_passthrough() {
        let rules = vec![parse_rule("ERROR", "red", 0)];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("just a normal line");
        assert_eq!(result.line, "just a normal line");
    }

    #[test]
    fn test_no_rules_passthrough() {
        let mut engine = Engine::new(vec![], true, None);
        let result = engine.apply("anything here");
        assert_eq!(result.line, "anything here");
    }

    #[test]
    fn test_color_disabled_passthrough() {
        let rules = vec![parse_rule("ERROR", "red", 0)];
        let mut engine = Engine::new(rules, false, None);
        let result = engine.apply("ERROR: something");
        assert_eq!(result.line, "ERROR: something");
    }

    #[test]
    fn test_no_rules_no_coloring() {
        let rules = build_rules(&[]).unwrap();
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("error happened");
        assert_eq!(result.line, "error happened");
    }

    // === New span-based coloring tests ===

    #[test]
    fn test_match_scope_colors_only_matched_text() {
        // Match-scope: only "ERROR" should be red, rest plain
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Match, 0)];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("2024 ERROR: fail");

        // The plain text portions should appear unchanged
        assert!(result.line.contains("2024 "), "should contain plain '2024 '");
        assert!(result.line.contains(": fail"), "should contain plain ': fail'");
        // The styled output should differ from whole-line styling
        let whole_line_styled = "2024 ERROR: fail"
            .style(crate::color::parse_style("red").unwrap())
            .to_string();
        assert_ne!(
            result.line, whole_line_styled,
            "match scope should not color the whole line"
        );
        // But it should not be plain either
        assert_ne!(
            result.line, "2024 ERROR: fail",
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
        assert!(result.line.contains("user="), "should contain plain 'user='");
        assert!(
            result.line.contains(" action=login"),
            "should contain plain ' action=login'"
        );
        assert_ne!(
            result.line, "user=42 action=login",
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
        assert_eq!(result.line, expected, "line scope should color the entire line");
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
        assert_ne!(result.line, "2024 ERROR: fail");
        // Should not be entirely blue (only digits are blue)
        let all_blue = "2024 ERROR: fail"
            .style(crate::color::parse_style("blue").unwrap())
            .to_string();
        assert_ne!(result.line, all_blue);
        // Should not be entirely red (digits region should be blue)
        let all_red = "2024 ERROR: fail"
            .style(crate::color::parse_style("red").unwrap())
            .to_string();
        assert_ne!(result.line, all_red);
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
        assert_eq!(result.line, blue_only.line, "lower priority number should win");
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
        assert_eq!(result.line, "foo bar");
    }

    #[test]
    fn test_empty_rules_passthrough() {
        let mut engine = Engine::new(vec![], true, None);
        let result = engine.apply("some line");
        assert_eq!(result.line, "some line");
    }

    #[test]
    fn test_color_disabled_returns_unchanged() {
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Match, 0)];
        let mut engine = Engine::new(rules, false, None);
        let result = engine.apply("ERROR: test");
        assert_eq!(result.line, "ERROR: test");
    }

    // === Next scope tests ===

    #[test]
    fn test_next_scope_colors_following_line() {
        let rules = vec![Rule::test_with_scope("^---+$", "cyan", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        // The separator line itself should NOT be colored
        let sep = engine.apply("----------");
        assert_eq!(sep.line, "----------");

        // The next line SHOULD be colored
        let next = engine.apply("this should be cyan");
        assert_ne!(next.line, "this should be cyan", "next line should be styled");

        // The line after that should NOT be colored (only next1)
        let after = engine.apply("this should be plain");
        assert_eq!(after.line, "this should be plain");
    }

    #[test]
    fn test_next_scope_colors_multiple_following_lines() {
        let rules = vec![Rule::test_with_scope("HEADER", "green", MatchScope::Next(2), 0)];
        let mut engine = Engine::new(rules, true, None);

        let header = engine.apply("HEADER");
        assert_eq!(header.line, "HEADER", "trigger line should not be colored");

        let line1 = engine.apply("line one");
        assert_ne!(line1.line, "line one", "first next line should be styled");

        let line2 = engine.apply("line two");
        assert_ne!(line2.line, "line two", "second next line should be styled");

        let line3 = engine.apply("line three");
        assert_eq!(line3.line, "line three", "third line should be plain");
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
        assert_ne!(result.line, "ERROR happened");
    }

    #[test]
    fn test_next_scope_no_match_no_carry() {
        let rules = vec![Rule::test_with_scope("TRIGGER", "red", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        // No trigger matched
        let line1 = engine.apply("no trigger here");
        assert_eq!(line1.line, "no trigger here");

        let line2 = engine.apply("still plain");
        assert_eq!(line2.line, "still plain");
    }

    #[test]
    fn test_next_scope_retrigger_resets() {
        let rules = vec![Rule::test_with_scope("^---", "cyan", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        engine.apply("---"); // trigger
        let next1 = engine.apply("colored");
        assert_ne!(next1.line, "colored");

        let plain = engine.apply("plain");
        assert_eq!(plain.line, "plain");

        engine.apply("---"); // re-trigger
        let next2 = engine.apply("colored again");
        assert_ne!(next2.line, "colored again");
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
        assert_ne!(next.line, "this line should be magenta", "next line should be styled");
    }

    #[test]
    fn test_next_carryforward_strips_ansi_from_following_lines() {
        let rules = vec![Rule::test_with_scope("TRIGGER", "cyan", MatchScope::Next(1), 0)];
        let mut engine = Engine::new(rules, true, None);

        engine.apply("TRIGGER");

        // Following line has ANSI codes — they should be stripped in output
        let result = engine.apply("\x1b[31mred text\x1b[0m");
        // Output should NOT contain the original ANSI red code
        assert!(!result.line.contains("\x1b[31m"), "original ANSI should be stripped. Got: {}", result.line);
        // But should contain the text, styled by lux
        assert!(result.line.contains("red text"), "text content should be preserved. Got: {}", result.line);
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
            result.line, "running: \"kubectl apply\"",
            "capture should produce styled output"
        );
        // Original ANSI codes should be stripped
        assert!(!result.line.contains("\x1b[32m"), "original ANSI should be stripped. Got: {}", result.line);
    }

    #[test]
    fn test_line_scope_preserves_ansi_when_no_cap_next() {
        // With only line/match scope rules, original ANSI is preserved
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Line, 0)];
        let mut engine = Engine::new(rules, true, None);

        let result = engine.apply("\x1b[33mERROR: something\x1b[0m");
        // Line scope should style the whole line but the input text includes ANSI
        assert_ne!(result.line, "\x1b[33mERROR: something\x1b[0m");
    }

    // === ANSI preservation: non-matching lines pass through raw ===

    #[test]
    fn test_non_matching_line_preserves_ansi_even_with_capture_rules() {
        // Regression: config rules using Capture scope used to strip ANSI
        // from every line globally. A line that doesn't match any rule should
        // pass through with its original ANSI intact.
        let rules = vec![
            Rule::test_with_scope("(?i)error", "white", MatchScope::Match, 0),
            Rule::test_with_scope(r"^--- (FAIL)", "red", MatchScope::Capture(1), 0),
            Rule::test_with_scope(r"^--- (PASS)", "green", MatchScope::Capture(1), 0),
        ];
        let mut engine = Engine::new(rules, true, None);

        let input = "\x1b[32mbuild\x1b[0m    \x1b[90m# Build the mod\x1b[0m";
        let result = engine.apply(input);

        assert_eq!(result.line, input, "non-matching line must preserve raw ANSI");
    }

    #[test]
    fn test_capture_preserves_surrounding_ansi() {
        // A Capture rule should restyle only the captured text and leave
        // surrounding ANSI intact.
        let rules = vec![Rule::test_with_scope(
            r"^--- (PASS)",
            "green",
            MatchScope::Capture(1),
            0,
        )];
        let mut engine = Engine::new(rules, true, None);

        let result = engine.apply("\x1b[33m--- PASS: TestFoo\x1b[0m");
        // Original yellow wrapper must survive for the non-captured parts.
        assert!(
            result.line.contains("\x1b[33m"),
            "surrounding ANSI should be preserved. Got: {:?}",
            result.line
        );
        // Captured text must have been restyled (not equal to plain).
        assert_ne!(result.line, "\x1b[33m--- PASS: TestFoo\x1b[0m");
        // Restored yellow should appear after our reset so ': TestFoo' stays yellow.
        let reset_pos = result.line.find("\x1b[0m").expect("reset emitted");
        assert!(
            result.line[reset_pos..].contains("\x1b[33m"),
            "yellow should be restored after the styled capture. Got: {:?}",
            result.line
        );
    }

    #[test]
    fn test_match_scope_preserves_surrounding_ansi() {
        let rules = vec![Rule::test_with_scope("ERROR", "red", MatchScope::Match, 0)];
        let mut engine = Engine::new(rules, true, None);

        let result = engine.apply("\x1b[33mfoo ERROR bar\x1b[0m");
        // Original yellow wrapper preserved for "foo " and " bar".
        assert!(
            result.line.contains("\x1b[33m"),
            "surrounding ANSI should be preserved. Got: {:?}",
            result.line
        );
        // And "foo " / " bar" appear as clean text somewhere in the output.
        assert!(result.line.contains("foo "), "got: {:?}", result.line);
        assert!(result.line.contains(" bar"), "got: {:?}", result.line);
    }

    #[test]
    fn test_no_rules_match_passes_raw_ansi_through() {
        // Even with Capture/Next rules present, a line that matches none of them
        // should round-trip raw (no clean-coordinate rendering).
        let rules = vec![Rule::test_with_scope(
            r"^--- (PASS)",
            "green",
            MatchScope::Capture(1),
            0,
        )];
        let mut engine = Engine::new(rules, true, None);

        let input = "\x1b[32mjust some green text\x1b[0m";
        let result = engine.apply(input);
        assert_eq!(result.line, input);
    }

    // === Internal helpers ===

    #[test]
    fn test_strip_ansi_with_map_osc8_hyperlink() {
        let raw = "\x1b]8;;https://x.io\x1b\\docs\x1b]8;;\x1b\\ end";
        let (clean, map) = strip_ansi_with_map(raw);
        assert_eq!(clean, "docs end");
        // Mapped positions point at the original bytes of the clean chars.
        let d = map[0];
        assert_eq!(&raw[d..d + 4], "docs");
        let raw_bel = "\x1b]8;;https://x.io\x07docs\x1b]8;;\x07 end";
        let (clean, _) = strip_ansi_with_map(raw_bel);
        assert_eq!(clean, "docs end");
    }

    #[test]
    fn test_strip_ansi_with_map_basic() {
        let raw = "\x1b[32mhi\x1b[0m";
        let (clean, map) = strip_ansi_with_map(raw);
        assert_eq!(clean, "hi");
        // 'h' at raw[5], 'i' at raw[6], sentinel = raw.len()
        assert_eq!(map, vec![5, 6, raw.len()]);
    }

    #[test]
    fn test_strip_ansi_with_map_no_ansi() {
        let raw = "plain";
        let (clean, map) = strip_ansi_with_map(raw);
        assert_eq!(clean, "plain");
        assert_eq!(map, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_strip_ansi_with_map_preserves_utf8() {
        let raw = "\x1b[31mhé\x1b[0m"; // 'é' is two bytes in UTF-8
        let (clean, _map) = strip_ansi_with_map(raw);
        assert_eq!(clean, "hé");
    }

    #[test]
    fn test_update_active_ansi_reset_clears() {
        let mut active = String::from("\x1b[31m");
        update_active_ansi(&mut active, "\x1b[0m");
        assert_eq!(active, "");
    }

    #[test]
    fn test_update_active_ansi_appends_non_reset() {
        let mut active = String::new();
        update_active_ansi(&mut active, "\x1b[31mhi\x1b[1m");
        assert_eq!(active, "\x1b[31m\x1b[1m");
    }

    // === Insert scope tests (ApplyResult) ===

    #[test]
    fn test_apply_result_insert_before() {
        let rules = vec![crate::rules::parse_rule("ERROR::insert-before:--- alert ---", 0).unwrap()];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("ERROR: something broke");
        assert_eq!(result.before.len(), 1);
        assert_eq!(result.before[0], "--- alert ---");
        assert_eq!(result.line, "ERROR: something broke");
        assert!(result.after.is_empty());
    }

    #[test]
    fn test_apply_result_insert_after() {
        let rules = vec![crate::rules::parse_rule("ERROR::insert-after:^^^ see above ^^^", 0).unwrap()];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("ERROR: something broke");
        assert!(result.before.is_empty());
        assert_eq!(result.line, "ERROR: something broke");
        assert_eq!(result.after.len(), 1);
        assert_eq!(result.after[0], "^^^ see above ^^^");
    }

    #[test]
    fn test_apply_result_prepend() {
        let rules = vec![crate::rules::parse_rule("WARN::prepend:>> ", 0).unwrap()];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("WARN: watch out");
        assert!(result.before.is_empty());
        assert!(result.after.is_empty());
        assert!(result.line.starts_with(">> "), "line should start with '>> ', got: {}", result.line);
        assert!(result.line.ends_with("WARN: watch out"), "line should end with original text");
    }

    #[test]
    fn test_apply_result_append() {
        let rules = vec![crate::rules::parse_rule("DEBUG::append: (done)", 0).unwrap()];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("DEBUG: stuff");
        assert!(result.before.is_empty());
        assert!(result.after.is_empty());
        assert!(result.line.starts_with("DEBUG: stuff"), "line should start with original text");
        assert!(result.line.ends_with(" (done)"), "line should end with appended text, got: {}", result.line);
    }

    #[test]
    fn test_apply_result_stacking_with_style_rule() {
        // A line-color rule plus an insert-before rule
        let rules = vec![
            Rule::test_with_scope("ERROR", "red", MatchScope::Line, 0),
            crate::rules::parse_rule("ERROR::insert-before:---", 1).unwrap(),
        ];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("ERROR: broke");
        assert_eq!(result.before.len(), 1);
        assert_eq!(result.before[0], "---");
        // The main line should be styled (not plain)
        assert_ne!(result.line, "ERROR: broke", "main line should be styled");
    }

    #[test]
    fn test_apply_result_no_match_empty_vecs() {
        let rules = vec![crate::rules::parse_rule("ERROR::insert-before:---", 0).unwrap()];
        let mut engine = Engine::new(rules, true, None);
        let result = engine.apply("just a normal line");
        assert!(result.before.is_empty());
        assert_eq!(result.line, "just a normal line");
        assert!(result.after.is_empty());
    }

    #[test]
    fn test_apply_result_plain_mode_still_inserts() {
        // color_enabled=false, but insert rules should still fire (rendering plain text)
        let rules = vec![crate::rules::parse_rule("ERROR::insert-before:--- alert ---", 0).unwrap()];
        let mut engine = Engine::new(rules, false, None);
        let result = engine.apply("ERROR: broke");
        assert_eq!(result.before.len(), 1, "insert-before should still fire in plain mode");
        assert_eq!(result.before[0], "--- alert ---");
        assert_eq!(result.line, "ERROR: broke");
    }

    #[test]
    fn test_apply_result_flatten() {
        let result = ApplyResult {
            before: vec!["above".to_string()],
            line: "main".to_string(),
            after: vec!["below".to_string()],
        };
        assert_eq!(result.flatten(), vec!["above", "main", "below"]);
    }

    #[test]
    fn test_next_scope_does_not_style_prepended_text() {
        // When Next(1) carry-forward is active, it styles the main line but should not
        // affect prepended text (since prepend text comes from its own template rendering)
        let rules = vec![
            Rule::test_with_scope("TRIGGER", "cyan", MatchScope::Next(1), 0),
            crate::rules::parse_rule("DATA::prepend:>> ", 1).unwrap(),
        ];
        let mut engine = Engine::new(rules, true, None);
        engine.apply("TRIGGER"); // activate next(1)
        let result = engine.apply("DATA here");
        // The prepended ">> " should be plain (no cyan styling from carry-forward)
        assert!(result.line.starts_with(">> "), "should have prepend, got: {}", result.line);
    }

    #[test]
    fn blank_line_resets_markdown_list_context() {
        // Engine::apply short-circuits on empty lines, but the syntax parser
        // still needs to see them: in Markdown a blank line is what closes a
        // list item / paragraph. If it is skipped, the list colour leaks into
        // the paragraph that follows.
        use crate::syntax::SyntaxHighlighter;
        // Use a theme that colours `markup.list` so a leak is visible.
        let md = || {
            SyntaxHighlighter::for_file(
                std::path::Path::new("notes.md"),
                Some("base16-eighties.dark"),
                None,
            )
            .unwrap()
        };

        let mut engine = Engine::new(vec![], true, Some(md()));
        engine.apply("- a list item");
        engine.apply("");
        let after_list = engine.apply("Plain paragraph text").line;

        let mut fresh = Engine::new(vec![], true, Some(md()));
        let expected = fresh.apply("Plain paragraph text").line;

        assert_eq!(after_list, expected);
    }
}

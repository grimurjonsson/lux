use std::collections::VecDeque;

use regex::Regex;

/// What the trigger filter decides for each line.
#[derive(Debug, PartialEq)]
pub enum OutputDecision {
    /// Pass the (colored) line through to output.
    Pass(String),
    /// Flush buffered lines (with optional separator prefix) then the trigger line.
    Flush(Vec<String>),
    /// Suppress this line (buffered internally).
    Suppress,
}

/// Before-context specification: either a fixed number of lines or a regex boundary.
#[derive(Debug)]
pub enum ContextBefore {
    /// Keep the last N lines in the buffer.
    Lines(usize),
    /// Keep lines back to (and including) the most recent match of this pattern.
    Pattern(Regex),
}

/// After-context specification: either a fixed number of lines or a regex boundary.
#[derive(Debug)]
pub enum ContextAfter {
    /// Emit N lines after the trigger.
    Lines(usize),
    /// Emit lines after the trigger until this pattern matches (inclusive).
    Pattern(Regex),
}

/// Parse a before/after spec: if it parses as a non-negative integer, treat as line count;
/// otherwise compile as a regex.
pub fn parse_context_spec(s: &str) -> anyhow::Result<(Option<usize>, Option<Regex>)> {
    if let Ok(n) = s.parse::<usize>() {
        Ok((Some(n), None))
    } else {
        let re = Regex::new(s)
            .map_err(|e| anyhow::anyhow!("bad context pattern '{}': {}", s, e))?;
        Ok((None, Some(re)))
    }
}

/// Internal state of the trigger filter state machine.
enum State {
    /// Output is suppressed; lines go into rolling buffer.
    Suppressing,
    /// Output is being emitted (count-based after context).
    EmittingCount { remaining: usize },
    /// Output is being emitted (pattern-based after context).
    EmittingPattern,
}

/// A filter that gates output based on trigger pattern matches.
///
/// When active, output is suppressed until a trigger pattern matches.
/// On match, the before-context is flushed, then after-context lines are emitted
/// until the after condition is met (count exhausted or boundary pattern matched).
pub struct TriggerFilter {
    patterns: Vec<Regex>,
    buffer: VecDeque<String>,
    before: ContextBefore,
    after: ContextAfter,
    state: State,
    has_emitted: bool,
    color_enabled: bool,
}

impl TriggerFilter {
    /// Create a new trigger filter.
    ///
    /// `before_spec` and `after_spec` are either a number string or a regex string.
    pub fn new(
        patterns: &[String],
        before_spec: &str,
        after_spec: &str,
        color_enabled: bool,
    ) -> anyhow::Result<Self> {
        let compiled: Vec<Regex> = patterns
            .iter()
            .map(|p| {
                Regex::new(p)
                    .map_err(|e| anyhow::anyhow!("bad trigger pattern '{}': {}", p, e))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let (before_n, before_re) = parse_context_spec(before_spec)?;
        let before = if let Some(re) = before_re {
            ContextBefore::Pattern(re)
        } else {
            ContextBefore::Lines(before_n.unwrap_or(20))
        };

        let (after_n, after_re) = parse_context_spec(after_spec)?;
        let after = if let Some(re) = after_re {
            ContextAfter::Pattern(re)
        } else {
            ContextAfter::Lines(after_n.unwrap_or(20))
        };

        let capacity = match &before {
            ContextBefore::Lines(n) => *n,
            ContextBefore::Pattern(_) => 1024, // reasonable max for pattern-based buffer
        };

        Ok(Self {
            patterns: compiled,
            buffer: VecDeque::with_capacity(capacity),
            before,
            after,
            state: State::Suppressing,
            has_emitted: false,
            color_enabled,
        })
    }

    /// Returns true if the filter has patterns (is active).
    pub fn is_active(&self) -> bool {
        !self.patterns.is_empty()
    }

    /// Return the separator string between trigger groups.
    fn separator(&self) -> String {
        if self.color_enabled {
            "\x1b[36m--- ctail ---\x1b[0m".to_string()
        } else {
            "--- ctail ---".to_string()
        }
    }

    /// Check if any trigger pattern matches the raw line.
    fn is_trigger(&self, raw_line: &str) -> bool {
        self.patterns.iter().any(|p| p.is_match(raw_line))
    }

    /// Extract before-context from the buffer based on the before spec.
    fn drain_before_context(&mut self) -> Vec<String> {
        match &self.before {
            ContextBefore::Lines(_) => {
                // Buffer already capped to N lines — drain all
                self.buffer.drain(..).collect()
            }
            ContextBefore::Pattern(re) => {
                // Find the last line matching the before-pattern and drain from there
                let mut start_idx = 0;
                for (i, line) in self.buffer.iter().enumerate() {
                    // Strip ANSI codes for matching (buffer holds colored lines)
                    let raw = strip_ansi(line);
                    if re.is_match(&raw) {
                        start_idx = i;
                    }
                }
                // Drain everything from start_idx onwards
                let before_lines: Vec<String> =
                    self.buffer.drain(start_idx..).collect();
                // Clear remaining old lines
                self.buffer.clear();
                before_lines
            }
        }
    }

    /// Push a colored line into the rolling buffer, respecting capacity for count-based.
    fn buffer_line(&mut self, colored_line: String) {
        match &self.before {
            ContextBefore::Lines(n) => {
                if *n == 0 {
                    return;
                }
                if self.buffer.len() >= *n {
                    self.buffer.pop_front();
                }
                self.buffer.push_back(colored_line);
            }
            ContextBefore::Pattern(_) => {
                // For pattern-based, keep a generous buffer (cap at 10k lines to prevent OOM)
                if self.buffer.len() >= 10_000 {
                    // Drop oldest half
                    let half = self.buffer.len() / 2;
                    self.buffer.drain(..half);
                }
                self.buffer.push_back(colored_line);
            }
        }
    }

    /// Process a line through the trigger filter.
    ///
    /// `raw_line` is the original text (for pattern matching).
    /// `colored_line` is the engine-processed text (for output).
    pub fn process_line(&mut self, raw_line: &str, colored_line: String) -> OutputDecision {
        if self.patterns.is_empty() {
            return OutputDecision::Pass(colored_line);
        }

        let is_match = self.is_trigger(raw_line);

        match &mut self.state {
            State::Suppressing => {
                if is_match {
                    let mut flush = Vec::new();
                    if self.has_emitted {
                        flush.push(self.separator());
                    }
                    let before_lines = self.drain_before_context();
                    flush.extend(before_lines);
                    flush.push(colored_line);

                    // Determine after-state
                    match &self.after {
                        ContextAfter::Lines(0) => {
                            // No after context — stay suppressing
                        }
                        ContextAfter::Lines(n) => {
                            self.state = State::EmittingCount { remaining: *n };
                        }
                        ContextAfter::Pattern(_) => {
                            self.state = State::EmittingPattern;
                        }
                    }
                    self.has_emitted = true;
                    OutputDecision::Flush(flush)
                } else {
                    self.buffer_line(colored_line);
                    OutputDecision::Suppress
                }
            }
            State::EmittingCount { remaining } => {
                if is_match {
                    // Re-trigger: reset counter
                    match &self.after {
                        ContextAfter::Lines(n) => *remaining = *n,
                        _ => unreachable!(),
                    }
                    OutputDecision::Pass(colored_line)
                } else if *remaining > 0 {
                    *remaining -= 1;
                    if *remaining == 0 {
                        let decision = OutputDecision::Pass(colored_line);
                        self.state = State::Suppressing;
                        decision
                    } else {
                        OutputDecision::Pass(colored_line)
                    }
                } else {
                    self.state = State::Suppressing;
                    self.buffer_line(colored_line);
                    OutputDecision::Suppress
                }
            }
            State::EmittingPattern => {
                if is_match {
                    // Re-trigger during pattern emit — just keep going
                    OutputDecision::Pass(colored_line)
                } else {
                    // Check if this line matches the after-boundary pattern
                    let is_boundary = match &self.after {
                        ContextAfter::Pattern(re) => re.is_match(raw_line),
                        _ => unreachable!(),
                    };
                    if is_boundary {
                        // Emit this line (inclusive) then go back to suppressing
                        self.state = State::Suppressing;
                        OutputDecision::Pass(colored_line)
                    } else {
                        OutputDecision::Pass(colored_line)
                    }
                }
            }
        }
    }
}

/// Strip ANSI escape codes from a string for pattern matching.
pub(crate) fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until 'm' (end of SGR sequence)
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(pats: &[&str]) -> Vec<String> {
        pats.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_patterns_passthrough() {
        let mut tf = TriggerFilter::new(&[], "5", "5", false).unwrap();
        assert!(!tf.is_active());
        let decision = tf.process_line("hello", "hello".to_string());
        assert_eq!(decision, OutputDecision::Pass("hello".to_string()));
    }

    #[test]
    fn active_with_patterns() {
        let tf = TriggerFilter::new(&patterns(&["ERROR"]), "5", "5", false).unwrap();
        assert!(tf.is_active());
    }

    #[test]
    fn suppressing_buffers_lines() {
        let mut tf = TriggerFilter::new(&patterns(&["TRIGGER"]), "3", "2", false).unwrap();
        assert_eq!(
            tf.process_line("line1", "line1".to_string()),
            OutputDecision::Suppress
        );
        assert_eq!(
            tf.process_line("line2", "line2".to_string()),
            OutputDecision::Suppress
        );
    }

    #[test]
    fn buffer_capacity_evicts_oldest() {
        let mut tf = TriggerFilter::new(&patterns(&["TRIGGER"]), "2", "1", false).unwrap();
        tf.process_line("line1", "line1".to_string());
        tf.process_line("line2", "line2".to_string());
        tf.process_line("line3", "line3".to_string());

        let decision = tf.process_line("TRIGGER", "TRIGGER".to_string());
        match decision {
            OutputDecision::Flush(lines) => {
                assert_eq!(lines, vec!["line2", "line3", "TRIGGER"]);
            }
            other => panic!("Expected Flush, got {:?}", other),
        }
    }

    #[test]
    fn trigger_flushes_buffer_and_trigger_line() {
        let mut tf = TriggerFilter::new(&patterns(&["ERROR"]), "3", "2", false).unwrap();
        tf.process_line("before1", "before1".to_string());
        tf.process_line("before2", "before2".to_string());

        let decision = tf.process_line("ERROR here", "ERROR here".to_string());
        match decision {
            OutputDecision::Flush(lines) => {
                assert_eq!(lines, vec!["before1", "before2", "ERROR here"]);
            }
            other => panic!("Expected Flush, got {:?}", other),
        }
    }

    #[test]
    fn after_window_countdown() {
        let mut tf = TriggerFilter::new(&patterns(&["ERROR"]), "1", "2", false).unwrap();
        tf.process_line("ERROR", "ERROR".to_string());

        assert_eq!(
            tf.process_line("after1", "after1".to_string()),
            OutputDecision::Pass("after1".to_string())
        );
        assert_eq!(
            tf.process_line("after2", "after2".to_string()),
            OutputDecision::Pass("after2".to_string())
        );
        assert_eq!(
            tf.process_line("suppressed", "suppressed".to_string()),
            OutputDecision::Suppress
        );
    }

    #[test]
    fn retrigger_resets_after_counter() {
        let mut tf = TriggerFilter::new(&patterns(&["ERROR"]), "1", "2", false).unwrap();
        tf.process_line("ERROR first", "ERROR first".to_string());
        tf.process_line("after1", "after1".to_string());
        tf.process_line("ERROR second", "ERROR second".to_string());
        assert_eq!(
            tf.process_line("new-after1", "new-after1".to_string()),
            OutputDecision::Pass("new-after1".to_string())
        );
        assert_eq!(
            tf.process_line("new-after2", "new-after2".to_string()),
            OutputDecision::Pass("new-after2".to_string())
        );
        assert_eq!(
            tf.process_line("gone", "gone".to_string()),
            OutputDecision::Suppress
        );
    }

    #[test]
    fn separator_between_trigger_groups() {
        let mut tf = TriggerFilter::new(&patterns(&["TRIGGER"]), "1", "0", false).unwrap();

        tf.process_line("ctx1", "ctx1".to_string());
        let first = tf.process_line("TRIGGER one", "TRIGGER one".to_string());
        match first {
            OutputDecision::Flush(lines) => {
                assert!(!lines.contains(&"--- ctail ---".to_string()));
                assert_eq!(lines, vec!["ctx1", "TRIGGER one"]);
            }
            other => panic!("Expected Flush, got {:?}", other),
        }

        tf.process_line("ctx2", "ctx2".to_string());
        let second = tf.process_line("TRIGGER two", "TRIGGER two".to_string());
        match second {
            OutputDecision::Flush(lines) => {
                assert_eq!(lines[0], "--- ctail ---");
                assert_eq!(lines, vec!["--- ctail ---", "ctx2", "TRIGGER two"]);
            }
            other => panic!("Expected Flush, got {:?}", other),
        }
    }

    #[test]
    fn multiple_patterns_or() {
        let mut tf = TriggerFilter::new(&patterns(&["ERROR", "WARN"]), "0", "0", false).unwrap();

        let d1 = tf.process_line("ERROR here", "ERROR here".to_string());
        assert!(matches!(d1, OutputDecision::Flush(_)));

        let d2 = tf.process_line("WARN here", "WARN here".to_string());
        assert!(matches!(d2, OutputDecision::Flush(_)));
    }

    #[test]
    fn raw_line_used_for_matching() {
        let mut tf = TriggerFilter::new(&patterns(&["ERROR"]), "0", "0", false).unwrap();
        let decision =
            tf.process_line("ERROR here", "\x1b[31mERROR here\x1b[0m".to_string());
        match decision {
            OutputDecision::Flush(lines) => {
                assert_eq!(lines, vec!["\x1b[31mERROR here\x1b[0m"]);
            }
            other => panic!("Expected Flush, got {:?}", other),
        }
    }

    #[test]
    fn bad_regex_returns_error() {
        let result = TriggerFilter::new(&patterns(&["(unclosed"]), "5", "5", false);
        assert!(result.is_err());
    }

    // --- Pattern-based context tests ---

    #[test]
    fn before_pattern_finds_boundary() {
        let mut tf =
            TriggerFilter::new(&patterns(&["ERROR"]), "^===", "0", false).unwrap();

        tf.process_line("old stuff", "old stuff".to_string());
        tf.process_line("=== START", "=== START".to_string());
        tf.process_line("context line", "context line".to_string());

        let decision = tf.process_line("ERROR boom", "ERROR boom".to_string());
        match decision {
            OutputDecision::Flush(lines) => {
                assert_eq!(
                    lines,
                    vec!["=== START", "context line", "ERROR boom"]
                );
            }
            other => panic!("Expected Flush, got {:?}", other),
        }
    }

    #[test]
    fn before_pattern_uses_last_match() {
        let mut tf =
            TriggerFilter::new(&patterns(&["ERROR"]), "^===", "0", false).unwrap();

        tf.process_line("=== FIRST", "=== FIRST".to_string());
        tf.process_line("middle", "middle".to_string());
        tf.process_line("=== SECOND", "=== SECOND".to_string());
        tf.process_line("context", "context".to_string());

        let decision = tf.process_line("ERROR", "ERROR".to_string());
        match decision {
            OutputDecision::Flush(lines) => {
                // Should start from the LAST match of the before pattern
                assert_eq!(
                    lines,
                    vec!["=== SECOND", "context", "ERROR"]
                );
            }
            other => panic!("Expected Flush, got {:?}", other),
        }
    }

    #[test]
    fn after_pattern_emits_until_boundary() {
        let mut tf =
            TriggerFilter::new(&patterns(&["ERROR"]), "0", "^---", false).unwrap();

        tf.process_line("ERROR happened", "ERROR happened".to_string());

        assert_eq!(
            tf.process_line("detail 1", "detail 1".to_string()),
            OutputDecision::Pass("detail 1".to_string())
        );
        assert_eq!(
            tf.process_line("detail 2", "detail 2".to_string()),
            OutputDecision::Pass("detail 2".to_string())
        );
        // Boundary line is included, then suppressing resumes
        assert_eq!(
            tf.process_line("--- END", "--- END".to_string()),
            OutputDecision::Pass("--- END".to_string())
        );
        assert_eq!(
            tf.process_line("suppressed", "suppressed".to_string()),
            OutputDecision::Suppress
        );
    }

    #[test]
    fn both_patterns_before_and_after() {
        let mut tf =
            TriggerFilter::new(&patterns(&["ERROR"]), "^===", "^---", false).unwrap();

        tf.process_line("noise", "noise".to_string());
        tf.process_line("=== BEGIN", "=== BEGIN".to_string());
        tf.process_line("setup", "setup".to_string());

        let decision = tf.process_line("ERROR fail", "ERROR fail".to_string());
        match decision {
            OutputDecision::Flush(lines) => {
                assert_eq!(
                    lines,
                    vec!["=== BEGIN", "setup", "ERROR fail"]
                );
            }
            other => panic!("Expected Flush, got {:?}", other),
        }

        assert_eq!(
            tf.process_line("trace info", "trace info".to_string()),
            OutputDecision::Pass("trace info".to_string())
        );
        assert_eq!(
            tf.process_line("--- END", "--- END".to_string()),
            OutputDecision::Pass("--- END".to_string())
        );
        assert_eq!(
            tf.process_line("gone", "gone".to_string()),
            OutputDecision::Suppress
        );
    }

    #[test]
    fn no_before_pattern_match_uses_nothing() {
        let mut tf =
            TriggerFilter::new(&patterns(&["ERROR"]), "^===", "0", false).unwrap();

        // No line matches ^=== before the trigger
        tf.process_line("no boundary here", "no boundary here".to_string());
        tf.process_line("still no boundary", "still no boundary".to_string());

        let decision = tf.process_line("ERROR", "ERROR".to_string());
        match decision {
            OutputDecision::Flush(lines) => {
                // All buffered lines included since no boundary was found
                // (start_idx stays 0, so all lines drain)
                assert_eq!(
                    lines,
                    vec!["no boundary here", "still no boundary", "ERROR"]
                );
            }
            other => panic!("Expected Flush, got {:?}", other),
        }
    }

    #[test]
    fn strip_ansi_works() {
        assert_eq!(strip_ansi("\x1b[31mhello\x1b[0m"), "hello");
        assert_eq!(strip_ansi("no codes"), "no codes");
        assert_eq!(strip_ansi("\x1b[1;32mtest\x1b[0m rest"), "test rest");
    }

    #[test]
    fn parse_context_spec_number() {
        let (n, re) = parse_context_spec("20").unwrap();
        assert_eq!(n, Some(20));
        assert!(re.is_none());
    }

    #[test]
    fn parse_context_spec_pattern() {
        let (n, re) = parse_context_spec("^===").unwrap();
        assert!(n.is_none());
        assert!(re.is_some());
    }

    #[test]
    fn parse_context_spec_bad_regex() {
        let result = parse_context_spec("(unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn separator_has_color_when_enabled() {
        let mut tf = TriggerFilter::new(&patterns(&["TRIGGER"]), "1", "0", true).unwrap();

        tf.process_line("ctx1", "ctx1".to_string());
        tf.process_line("TRIGGER one", "TRIGGER one".to_string());

        tf.process_line("ctx2", "ctx2".to_string());
        let second = tf.process_line("TRIGGER two", "TRIGGER two".to_string());
        match second {
            OutputDecision::Flush(lines) => {
                assert!(
                    lines[0].contains("\x1b[36m"),
                    "Expected cyan ANSI code in separator: {:?}",
                    lines[0]
                );
                assert_eq!(lines[0], "\x1b[36m--- ctail ---\x1b[0m");
            }
            other => panic!("Expected Flush, got {:?}", other),
        }
    }
}

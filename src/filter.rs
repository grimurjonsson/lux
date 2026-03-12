use regex::Regex;

/// A line filter that decides whether to show or hide lines based on
/// include/exclude regex patterns, with optional ANSI code stripping.
#[derive(Debug)]
pub struct LineFilter {
    include: Vec<Regex>,
    exclude: Vec<Regex>,
    strip_ansi: bool,
}

impl LineFilter {
    /// Create a new LineFilter.
    ///
    /// - `include`: patterns that lines must match (OR logic; empty = match all)
    /// - `exclude`: patterns that reject lines (AND logic; line rejected if any match)
    /// - `strip_ansi`: if true, strip ANSI escape codes before pattern matching
    pub fn new(include: &[String], exclude: &[String], strip_ansi: bool) -> anyhow::Result<Self> {
        let include = include
            .iter()
            .map(|p| {
                Regex::new(p)
                    .map_err(|e| anyhow::anyhow!("invalid include pattern '{}': {}", p, e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let exclude = exclude
            .iter()
            .map(|p| {
                Regex::new(p)
                    .map_err(|e| anyhow::anyhow!("invalid exclude pattern '{}': {}", p, e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            include,
            exclude,
            strip_ansi,
        })
    }

    /// Returns true if the filter has any patterns (is active).
    pub fn is_active(&self) -> bool {
        !self.include.is_empty() || !self.exclude.is_empty()
    }

    /// Returns true if the line should be shown (passes the filter).
    pub fn should_show(&self, line: &str) -> bool {
        let text = if self.strip_ansi {
            crate::trigger::strip_ansi(line)
        } else {
            line.to_string()
        };

        let pass_include =
            self.include.is_empty() || self.include.iter().any(|r| r.is_match(&text));
        let pass_exclude = self.exclude.iter().all(|r| !r.is_match(&text));
        pass_include && pass_exclude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(vals: &[&str]) -> Vec<String> {
        vals.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn no_patterns_passes_all() {
        let f = LineFilter::new(&[], &[], false).unwrap();
        assert!(!f.is_active());
        assert!(f.should_show("anything"));
        assert!(f.should_show("ERROR: bad"));
        assert!(f.should_show(""));
    }

    #[test]
    fn include_only_matching() {
        let f = LineFilter::new(&s(&["ERROR"]), &[], false).unwrap();
        assert!(f.is_active());
        assert!(f.should_show("ERROR: something broke"));
        assert!(!f.should_show("WARN: caution"));
        assert!(!f.should_show("DEBUG: verbose"));
    }

    #[test]
    fn exclude_hides_matching() {
        let f = LineFilter::new(&[], &s(&["DEBUG"]), false).unwrap();
        assert!(f.is_active());
        assert!(f.should_show("ERROR: bad"));
        assert!(f.should_show("WARN: caution"));
        assert!(!f.should_show("DEBUG: verbose"));
    }

    #[test]
    fn multiple_includes_or() {
        let f = LineFilter::new(&s(&["ERROR", "WARN"]), &[], false).unwrap();
        assert!(f.should_show("ERROR: bad"));
        assert!(f.should_show("WARN: caution"));
        assert!(!f.should_show("DEBUG: verbose"));
        assert!(!f.should_show("INFO: normal"));
    }

    #[test]
    fn multiple_excludes_all_apply() {
        let f = LineFilter::new(&[], &s(&["DEBUG", "TRACE"]), false).unwrap();
        assert!(f.should_show("ERROR: bad"));
        assert!(!f.should_show("DEBUG: verbose"));
        assert!(!f.should_show("TRACE: detailed"));
    }

    #[test]
    fn include_exclude_combo() {
        let f = LineFilter::new(&s(&["ERROR|WARN"]), &s(&["timeout"]), false).unwrap();
        assert!(f.should_show("ERROR: disk"));
        assert!(!f.should_show("ERROR: timeout"));
        assert!(f.should_show("WARN: memory"));
        assert!(!f.should_show("WARN: timeout issue"));
        assert!(!f.should_show("DEBUG: normal"));
    }

    #[test]
    fn strip_ansi_before_matching() {
        let f = LineFilter::new(&s(&["ERROR"]), &[], true).unwrap();
        assert!(f.should_show("\x1b[31mERROR\x1b[0m: something"));
        assert!(!f.should_show("\x1b[33mWARN\x1b[0m: something"));
    }

    #[test]
    fn ansi_line_matches_include() {
        let f = LineFilter::new(&s(&["ERROR"]), &[], true).unwrap();
        assert!(f.should_show("\x1b[31mERROR\x1b[0m"));
    }

    #[test]
    fn no_strip_ansi_matches_raw() {
        let f = LineFilter::new(&s(&["ERROR"]), &[], false).unwrap();
        // Without stripping, the raw text contains ANSI codes around ERROR
        // but the text "ERROR" is still present in the raw string
        assert!(f.should_show("\x1b[31mERROR\x1b[0m"));
        // Pattern that only matches clean text should still find ERROR in raw
        let f2 = LineFilter::new(&s(&["^ERROR$"]), &[], false).unwrap();
        // This should NOT match because raw input has ANSI codes
        assert!(!f2.should_show("\x1b[31mERROR\x1b[0m"));
    }

    #[test]
    fn invalid_include_regex_errors() {
        let result = LineFilter::new(&s(&["(unclosed"]), &[], false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid include pattern"), "got: {err}");
    }

    #[test]
    fn invalid_exclude_regex_errors() {
        let result = LineFilter::new(&[], &s(&["(unclosed"]), false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid exclude pattern"), "got: {err}");
    }
}

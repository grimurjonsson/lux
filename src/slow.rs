use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use owo_colors::{OwoColorize, Style};

use crate::color;

/// Parse a human-readable duration string into a `Duration`.
///
/// Supported formats: `500ms`, `5s`, `1m`, `1m30s`, `1h`, `1h30m`, `1h30m15s`.
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty duration string");
    }

    let mut total_ms: u64 = 0;
    let mut num_buf = String::new();
    let mut chars = s.chars().peekable();
    let mut matched_any = false;

    while chars.peek().is_some() {
        num_buf.clear();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                num_buf.push(c);
                chars.next();
            } else {
                break;
            }
        }

        if num_buf.is_empty() {
            bail!("invalid duration format: '{s}'");
        }

        let value: u64 = num_buf
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid number in duration: '{s}'"))?;

        let mut unit = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                unit.push(c);
                chars.next();
            } else {
                break;
            }
        }

        match unit.as_str() {
            "ms" => total_ms += value,
            "s" => total_ms += value * 1000,
            "m" => total_ms += value * 60 * 1000,
            "h" => total_ms += value * 3600 * 1000,
            _ => bail!("unknown duration unit '{unit}' in '{s}'. Valid units: ms, s, m, h"),
        }
        matched_any = true;
    }

    if !matched_any {
        bail!("invalid duration format: '{s}'");
    }

    Ok(Duration::from_millis(total_ms))
}

/// Format a Duration into a human-readable string.
pub fn format_duration(d: Duration) -> String {
    let total_ms = d.as_millis() as u64;
    let total_secs = total_ms / 1000;
    let frac_ms = total_ms % 1000;

    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{hours}h {mins}m {secs}.{frac_ms:03}s")
    } else if mins > 0 {
        format!("{mins}m {secs}.{frac_ms:03}s")
    } else {
        format!("{secs}.{frac_ms:03}s")
    }
}

/// Annotates lines that precede a slow gap between consecutive outputs.
///
/// Buffers one line at a time. When the next line arrives and the elapsed time
/// since buffering exceeds the threshold, the *previous* (buffered) line is
/// emitted with a `[took: …]` annotation. Call [`flush`] at EOF or end-of-batch
/// to emit the last buffered line without annotation.
pub struct SlowLineAnnotator {
    threshold: Duration,
    style: Style,
    last_line_time: Instant,
    pending_line: Option<String>,
}

impl SlowLineAnnotator {
    /// Create a new annotator with the given duration threshold and style.
    pub fn new(threshold: Duration, style_spec: &str, color_enabled: bool) -> Self {
        let style = if color_enabled {
            color::parse_style(style_spec).unwrap_or_else(|_| Style::new())
        } else {
            Style::new()
        };
        Self {
            threshold,
            style,
            last_line_time: Instant::now(),
            pending_line: None,
        }
    }

    /// Accept a new line, returning the previously buffered line (possibly annotated).
    ///
    /// Returns `None` on the first call (nothing buffered yet).
    /// On subsequent calls, returns the previous line with `[took: …]` appended
    /// if the elapsed time since it was buffered exceeds the threshold.
    pub fn annotate(&mut self, line: &str) -> Option<String> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_line_time);
        self.last_line_time = now;

        let result = self.pending_line.take().map(|prev| {
            if elapsed > self.threshold {
                let formatted = format_duration(elapsed);
                let annotation = format!("[took: {formatted}]");
                let styled = annotation.style(self.style).to_string();
                format!("{prev} {styled}")
            } else {
                prev
            }
        });

        self.pending_line = Some(line.to_string());
        result
    }

    /// Flush the buffered line without annotation (for EOF / end-of-batch).
    pub fn flush(&mut self) -> Option<String> {
        self.last_line_time = Instant::now();
        self.pending_line.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_milliseconds() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
    }

    #[test]
    fn parse_seconds() {
        assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
    }

    #[test]
    fn parse_minutes() {
        assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn parse_minutes_seconds() {
        assert_eq!(parse_duration("1m30s").unwrap(), Duration::from_secs(90));
    }

    #[test]
    fn parse_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn parse_invalid_empty() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn parse_invalid_garbage() {
        assert!(parse_duration("abc").is_err());
    }

    #[test]
    fn parse_invalid_no_unit() {
        assert!(parse_duration("42").is_err());
    }

    #[test]
    fn format_sub_second() {
        let d = Duration::from_millis(523);
        assert_eq!(format_duration(d), "0.523s");
    }

    #[test]
    fn format_seconds() {
        let d = Duration::from_millis(13433);
        assert_eq!(format_duration(d), "13.433s");
    }

    #[test]
    fn format_minutes() {
        let d = Duration::from_millis(220_332);
        assert_eq!(format_duration(d), "3m 40.332s");
    }

    #[test]
    fn format_hours() {
        let d = Duration::from_millis(3_820_332);
        assert_eq!(format_duration(d), "1h 3m 40.332s");
    }

    #[test]
    fn format_exact_seconds() {
        let d = Duration::from_secs(5);
        assert_eq!(format_duration(d), "5.000s");
    }

    #[test]
    fn first_line_returns_none() {
        let mut annotator = SlowLineAnnotator::new(
            Duration::from_millis(100),
            "dim+yellow",
            false,
        );
        assert!(annotator.annotate("first line").is_none(), "first call should buffer, not emit");
    }

    #[test]
    fn second_line_returns_first_unannotated() {
        let mut annotator = SlowLineAnnotator::new(
            Duration::from_secs(10),
            "dim+yellow",
            false,
        );
        annotator.annotate("line one");
        let result = annotator.annotate("line two").unwrap();
        assert_eq!(result, "line one");
    }

    #[test]
    fn slow_gap_annotates_previous_line() {
        let mut annotator = SlowLineAnnotator::new(
            Duration::from_millis(0),
            "dim+yellow",
            false,
        );
        annotator.annotate("slow operation");
        std::thread::sleep(Duration::from_millis(5));
        let result = annotator.annotate("next line").unwrap();
        assert!(
            result.starts_with("slow operation ") && result.contains("[took:"),
            "expected PREVIOUS line annotated, got: {result}"
        );
    }

    #[test]
    fn timestamp_resets_after_annotation() {
        let mut annotator = SlowLineAnnotator::new(
            Duration::from_millis(50),
            "dim+yellow",
            false,
        );
        annotator.annotate("first");
        std::thread::sleep(Duration::from_millis(100));
        let slow = annotator.annotate("second").unwrap();
        assert!(slow.contains("[took:"), "first line should be annotated: {slow}");

        let fast = annotator.annotate("third").unwrap();
        assert!(!fast.contains("[took:"), "second line should NOT be annotated: {fast}");
    }

    #[test]
    fn flush_returns_buffered_line() {
        let mut annotator = SlowLineAnnotator::new(
            Duration::from_millis(100),
            "dim+yellow",
            false,
        );
        annotator.annotate("buffered");
        let flushed = annotator.flush().unwrap();
        assert_eq!(flushed, "buffered");
    }

    #[test]
    fn flush_when_empty_returns_none() {
        let mut annotator = SlowLineAnnotator::new(
            Duration::from_millis(100),
            "dim+yellow",
            false,
        );
        assert!(annotator.flush().is_none());
    }

    #[test]
    fn annotation_format() {
        let mut annotator = SlowLineAnnotator::new(
            Duration::from_millis(0),
            "dim+yellow",
            false,
        );
        annotator.annotate("first");
        std::thread::sleep(Duration::from_millis(5));
        let result = annotator.annotate("second").unwrap();
        assert!(
            result.contains("[took: ") && result.ends_with(']'),
            "wrong format: {result}"
        );
    }
}

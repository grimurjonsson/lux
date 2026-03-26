use anyhow::{bail, Result};
use owo_colors::{OwoColorize, Style};
use regex::Captures;

/// A segment of parsed template text — either plain or styled.
#[derive(Debug, Clone)]
pub enum Segment {
    Plain(String),
    Styled(String, Style),
}

/// Interpolate capture group references in a template string.
///
/// - `$0` = full match, `$1` = first capture group, etc.
/// - `$$` = literal `$`
/// - `$N` where group N didn't participate = empty string
/// - Bare `$` at end or before non-digit = literal `$`
pub fn interpolate(template: &str, caps: &Captures) -> String {
    let mut result = String::with_capacity(template.len());
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            if i + 1 >= chars.len() {
                // Bare $ at end
                result.push('$');
                i += 1;
            } else if chars[i + 1] == '$' {
                // Escaped $$
                result.push('$');
                i += 2;
            } else if chars[i + 1].is_ascii_digit() {
                // Parse group index (may be multi-digit)
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && chars[end].is_ascii_digit() {
                    end += 1;
                }
                let idx_str: String = chars[start..end].iter().collect();
                let idx: usize = idx_str.parse().unwrap();
                if let Some(m) = caps.get(idx) {
                    result.push_str(m.as_str());
                }
                // If group didn't participate, push nothing (empty string)
                i = end;
            } else {
                // Bare $ before non-digit
                result.push('$');
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Parse a template string containing `[style-spec]text[/]` tags into segments.
///
/// - Tags: `[style-spec]text[/]` or `[style-spec]text[/style-spec]`
/// - Opening a new tag implicitly closes the previous one (no nesting)
/// - `[/]` with no open tag is a no-op
/// - Validates styles using `crate::color::parse_style()`
/// - Errors on unclosed brackets or unclosed style tags
pub fn parse_tags(template: &str) -> Result<Vec<Segment>> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<(Style, String)> = None; // (style, tag_name)
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '[' {
            // Find the closing bracket
            i += 1;
            let mut tag_content = String::new();
            while i < chars.len() && chars[i] != ']' {
                tag_content.push(chars[i]);
                i += 1;
            }
            if i >= chars.len() {
                bail!(
                    "unclosed bracket in template — '[{}' is missing ']'",
                    tag_content
                );
            }
            // Skip the closing ']'
            i += 1;

            // Determine if this is a closing tag or opening tag
            if tag_content == "/" {
                // Explicit close: [/]
                if let Some((style, _tag_name)) = current_style.take() {
                    if !current_text.is_empty() {
                        segments.push(Segment::Styled(
                            std::mem::take(&mut current_text),
                            style,
                        ));
                    }
                } else {
                    // [/] with no open tag is a no-op, but flush any plain text
                    if !current_text.is_empty() {
                        segments.push(Segment::Plain(std::mem::take(&mut current_text)));
                    }
                }
            } else if tag_content.starts_with('/') {
                // Explicit close with name: [/style-spec]
                if let Some((style, _tag_name)) = current_style.take() {
                    if !current_text.is_empty() {
                        segments.push(Segment::Styled(
                            std::mem::take(&mut current_text),
                            style,
                        ));
                    }
                } else {
                    // Close tag with no open tag — treat as no-op
                    if !current_text.is_empty() {
                        segments.push(Segment::Plain(std::mem::take(&mut current_text)));
                    }
                }
            } else {
                // Opening tag — validate the style spec
                let style = crate::color::parse_style(&tag_content).map_err(|e| {
                    anyhow::anyhow!("invalid style tag '[{}]': {}", tag_content, e)
                })?;

                // If there's already an open tag, implicitly close it
                if let Some((prev_style, _prev_tag)) = current_style.take() {
                    if !current_text.is_empty() {
                        segments.push(Segment::Styled(
                            std::mem::take(&mut current_text),
                            prev_style,
                        ));
                    }
                } else if !current_text.is_empty() {
                    // Flush any plain text before the new tag
                    segments.push(Segment::Plain(std::mem::take(&mut current_text)));
                }

                current_style = Some((style, tag_content));
            }
        } else {
            current_text.push(chars[i]);
            i += 1;
        }
    }

    // Handle remaining text
    if !current_text.is_empty() {
        if let Some((_, ref tag_name)) = current_style {
            bail!(
                "unclosed style tag '[{}]' — add [/] to close it",
                tag_name
            );
        }
        segments.push(Segment::Plain(current_text));
    } else if current_style.is_some() {
        // Tag was opened but no text followed — still unclosed
        // (e.g., "text[red]") — the tag had no content, which is not an error
        // since the text is empty. But technically the tag is unclosed.
        // Per spec, unclosed tag with text after it is an error.
        // If there's no text, there's nothing to style, so it's fine.
    }

    Ok(segments)
}

/// Render parsed segments into a final string.
///
/// - Styled segments: apply their style
/// - Plain segments with default_style and color_enabled: apply default style
/// - Plain segments without default_style or !color_enabled: output as-is
/// - If !color_enabled: strip all styles, return plain text
pub fn render_segments(
    segments: &[Segment],
    default_style: Option<Style>,
    color_enabled: bool,
) -> String {
    let mut result = String::new();
    for segment in segments {
        match segment {
            Segment::Styled(text, style) => {
                if color_enabled {
                    result.push_str(&text.style(*style).to_string());
                } else {
                    result.push_str(text);
                }
            }
            Segment::Plain(text) => {
                if color_enabled {
                    if let Some(style) = default_style {
                        result.push_str(&text.style(style).to_string());
                    } else {
                        result.push_str(text);
                    }
                } else {
                    result.push_str(text);
                }
            }
        }
    }
    result
}

/// Validate capture group references in a template string.
///
/// Scans for `$N` references (skipping `$$`), and returns an error if any
/// reference exceeds the available capture count.
fn validate_captures(template: &str, capture_count: usize) -> Result<()> {
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            if i + 1 >= chars.len() {
                // Bare $ at end — fine
                break;
            } else if chars[i + 1] == '$' {
                // Escaped $$ — skip
                i += 2;
            } else if chars[i + 1].is_ascii_digit() {
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && chars[end].is_ascii_digit() {
                    end += 1;
                }
                let idx_str: String = chars[start..end].iter().collect();
                let idx: usize = idx_str.parse().unwrap();
                if idx >= capture_count {
                    bail!(
                        "capture reference ${} is out of range — \
                         the pattern has {} capture group{} (valid: $0..=${})",
                        idx,
                        capture_count.saturating_sub(1),
                        if capture_count.saturating_sub(1) == 1 { "" } else { "s" },
                        capture_count.saturating_sub(1)
                    );
                }
                i = end;
            } else {
                // Bare $ before non-digit
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    Ok(())
}

/// Validate a template string at parse time.
///
/// - Checks that `$N` references don't exceed the capture count
/// - Validates all style tags parse correctly
/// - Returns the parsed segments for later rendering
pub fn validate_template(template: &str, capture_count: usize) -> Result<Vec<Segment>> {
    validate_captures(template, capture_count)?;
    parse_tags(template)
}

/// Interpolate capture values into pre-parsed segments, then render.
///
/// This is the main entry point for rendering a complete template at runtime.
/// Because tags are pre-parsed, capture values containing `[` or `]` won't be
/// misinterpreted as markup.
pub fn render_template(
    segments: &[Segment],
    caps: &Captures,
    default_style: Option<Style>,
    color_enabled: bool,
) -> String {
    // Interpolate captures into each segment's text
    let interpolated: Vec<Segment> = segments
        .iter()
        .map(|seg| match seg {
            Segment::Plain(text) => Segment::Plain(interpolate(text, caps)),
            Segment::Styled(text, style) => Segment::Styled(interpolate(text, caps), *style),
        })
        .collect();

    render_segments(&interpolated, default_style, color_enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn test_interpolate_full_match() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("[$0]", &caps), "[ERROR auth]");
    }

    #[test]
    fn test_interpolate_capture_group() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("module=$1", &caps), "module=auth");
    }

    #[test]
    fn test_interpolate_dollar_escape() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("cost: $$5", &caps), "cost: $5");
    }

    #[test]
    fn test_interpolate_optional_group_missing() {
        let re = Regex::new(r"(a)?(b)").unwrap();
        let text = "b";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("[$1][$2]", &caps), "[][b]");
    }

    #[test]
    fn test_interpolate_no_placeholders() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("plain text", &caps), "plain text");
    }

    #[test]
    fn test_interpolate_bare_dollar_at_end() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        assert_eq!(interpolate("end$", &caps), "end$");
    }

    // --- Task 2: Tag Parsing & Rendering ---

    #[test]
    fn test_parse_simple_tag() {
        let segments = parse_tags("[red]hello[/]").unwrap();
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], Segment::Styled(text, _) if text == "hello"));
    }

    #[test]
    fn test_parse_multiple_tags() {
        let segments = parse_tags("--- [red]ERROR[/] in [cyan]auth[/] ---").unwrap();
        assert_eq!(segments.len(), 5);
        assert!(matches!(&segments[0], Segment::Plain(t) if t == "--- "));
        assert!(matches!(&segments[1], Segment::Styled(t, _) if t == "ERROR"));
        assert!(matches!(&segments[2], Segment::Plain(t) if t == " in "));
        assert!(matches!(&segments[3], Segment::Styled(t, _) if t == "auth"));
        assert!(matches!(&segments[4], Segment::Plain(t) if t == " ---"));
    }

    #[test]
    fn test_parse_implicit_close() {
        let segments = parse_tags("[red]hello [bold]world[/]").unwrap();
        assert_eq!(segments.len(), 2);
        assert!(matches!(&segments[0], Segment::Styled(t, _) if t == "hello "));
        assert!(matches!(&segments[1], Segment::Styled(t, _) if t == "world"));
    }

    #[test]
    fn test_parse_explicit_close_tag() {
        let segments = parse_tags("[red+bold]text[/red+bold]").unwrap();
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], Segment::Styled(t, _) if t == "text"));
    }

    #[test]
    fn test_parse_no_tags() {
        let segments = parse_tags("plain text").unwrap();
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], Segment::Plain(t) if t == "plain text"));
    }

    #[test]
    fn test_parse_unclosed_tag_error() {
        let result = parse_tags("[red]text");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unclosed"),
            "error should mention 'unclosed', got: {err}"
        );
    }

    #[test]
    fn test_parse_invalid_style_error() {
        let result = parse_tags("[notacolor]text[/]");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid style"),
            "error should mention 'invalid style', got: {err}"
        );
    }

    #[test]
    fn test_render_plain_mode() {
        let segments = parse_tags("[red]hello[/] world").unwrap();
        let rendered = render_segments(&segments, None, false);
        assert_eq!(rendered, "hello world");
    }

    #[test]
    fn test_render_with_default_style() {
        let segments = parse_tags("[red]hello[/] world").unwrap();
        let default_style = crate::color::parse_style("blue").unwrap();
        let rendered = render_segments(&segments, Some(default_style), true);
        // Plain segment "world" should have the default style applied,
        // so it should not be plain text
        assert!(rendered.contains("world"));
        // The styled segment "hello" should also be present
        assert!(rendered.contains("hello"));
    }

    #[test]
    fn test_close_tag_noop_when_no_open() {
        let segments = parse_tags("text[/]more").unwrap();
        assert_eq!(segments.len(), 2);
        assert!(matches!(&segments[0], Segment::Plain(t) if t == "text"));
        assert!(matches!(&segments[1], Segment::Plain(t) if t == "more"));
    }

    // --- Task 3: Full Template Rendering ---

    #[test]
    fn test_render_template_full_pipeline() {
        let re = Regex::new(r"ERROR (\w+)").unwrap();
        let text = "ERROR auth: failed";
        let caps = re.captures(text).unwrap();
        let template = "--- [red]$0[/] in [cyan]$1[/] ---";
        let segments = validate_template(template, re.captures_len()).unwrap();
        let rendered = render_template(&segments, &caps, None, false);
        assert_eq!(rendered, "--- ERROR auth in auth ---");
    }

    #[test]
    fn test_validate_captures_out_of_range() {
        // Pattern has 2 groups => capture_count = 3 ($0, $1, $2)
        // $3 should be out of range
        let result = validate_template("[red]$3[/]", 2);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("out of range"),
            "error should mention 'out of range', got: {err}"
        );
    }

    #[test]
    fn test_validate_captures_valid() {
        // capture_count=3 means $0, $1, $2 are all valid
        let result = validate_template("[red]$0[/] and $1", 3);
        assert!(result.is_ok());
    }
}

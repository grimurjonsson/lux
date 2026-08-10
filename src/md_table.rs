//! GFM table detection and box-drawn rendering for markdown content.

use std::sync::LazyLock;

use owo_colors::{OwoColorize, Style};
use regex::Regex;
use unicode_width::UnicodeWidthStr;

use crate::trigger::strip_ansi;

/// Column alignment parsed from a delimiter row (`:--`, `:-:`, `--:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

/// Split a table line into trimmed cell strings.
///
/// - Splits on `|`, ignoring `\|` escapes (backslash dropped) and pipes
///   inside backtick code spans.
/// - Outer pipes (`| a |`) produce leading/trailing empty cells — dropped.
pub fn split_cells(line: &str) -> Vec<String> {
    let s = line.trim().trim_end_matches('\r');
    let mut cells: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_code = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cur.push('|');
                chars.next();
            }
            '`' => {
                in_code = !in_code;
                cur.push('`');
            }
            '|' if !in_code => {
                cells.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    cells.push(cur.trim().to_string());
    if cells.first().is_some_and(|c| c.is_empty()) {
        cells.remove(0);
    }
    if cells.last().is_some_and(|c| c.is_empty()) {
        cells.pop();
    }
    cells
}

/// If the line is a GFM delimiter row (`|---|:--:|--:|`), return the
/// per-column alignments. Requires at least one `|` on the line.
pub fn is_delimiter_row(line: &str) -> Option<Vec<Alignment>> {
    let s = line.trim();
    if !s.contains('|') {
        return None;
    }
    let cells = split_cells(s);
    if cells.is_empty() {
        return None;
    }
    let mut aligns = Vec::with_capacity(cells.len());
    for cell in &cells {
        let left_colon = cell.starts_with(':');
        let body = cell.strip_prefix(':').unwrap_or(cell);
        let (body, right_colon) = match body.strip_suffix(':') {
            Some(b) => (b, true),
            None => (body, false),
        };
        if body.is_empty() || !body.chars().all(|ch| ch == '-') {
            return None;
        }
        aligns.push(match (left_colon, right_colon) {
            (true, true) => Alignment::Center,
            (false, true) => Alignment::Right,
            _ => Alignment::Left,
        });
    }
    Some(aligns)
}

/// True if the line could be a table row: non-blank and contains a `|`.
pub fn looks_like_row(line: &str) -> bool {
    let s = line.trim();
    !s.is_empty() && s.contains('|')
}

/// One alternation per inline construct. Order matters: code spans are
/// opaque, links before emphasis, `**bold**` before `*italic*`.
static INLINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?P<code>`[^`]+`)",
        r"|(?P<link>\[(?P<ltext>[^\]]*)\]\([^)]*\))",
        r"|(?P<bold>\*\*(?P<btext>[^*]+)\*\*)",
        r"|(?P<bold2>__(?P<b2text>[^_]+)__)",
        r"|(?P<strike>~~(?P<stext>[^~]+)~~)",
        r"|(?P<italic>\*(?P<itext>[^*]+)\*)",
        r"|(?P<italic2>_(?P<i2text>[^_]+)_)",
    ))
    .unwrap()
});

fn base_style(header: bool) -> Style {
    if header {
        Style::new().bold()
    } else {
        Style::new()
    }
}

pub fn render_inline(text: &str, header: bool) -> (String, usize) {
    let mut styled = String::new();
    let mut plain = String::new();
    let mut last = 0;

    let push_plain = |styled: &mut String, plain: &mut String, seg: &str| {
        if seg.is_empty() {
            return;
        }
        if header {
            styled.push_str(&seg.style(base_style(true)).to_string());
        } else {
            styled.push_str(seg);
        }
        plain.push_str(seg);
    };

    for caps in INLINE_RE.captures_iter(text) {
        let whole = caps.get(0).unwrap();
        push_plain(&mut styled, &mut plain, &text[last..whole.start()]);

        let (inner, style) = if let Some(m) = caps.name("code") {
            let s = m.as_str();
            (
                &s[1..s.len() - 1],
                base_style(header).truecolor(166, 227, 161),
            )
        } else if caps.name("link").is_some() {
            (
                caps.name("ltext").unwrap().as_str(),
                base_style(header).truecolor(137, 180, 250).underline(),
            )
        } else if caps.name("bold").is_some() {
            (caps.name("btext").unwrap().as_str(), base_style(header).bold())
        } else if caps.name("bold2").is_some() {
            (caps.name("b2text").unwrap().as_str(), base_style(header).bold())
        } else if caps.name("strike").is_some() {
            (
                caps.name("stext").unwrap().as_str(),
                base_style(header).strikethrough(),
            )
        } else if caps.name("italic").is_some() {
            (caps.name("itext").unwrap().as_str(), base_style(header).italic())
        } else {
            (caps.name("i2text").unwrap().as_str(), base_style(header).italic())
        };

        styled.push_str(&inner.style(style).to_string());
        plain.push_str(inner);
        last = whole.end();
    }
    push_plain(&mut styled, &mut plain, &text[last..]);

    let width = UnicodeWidthStr::width(strip_ansi(&plain).as_str());
    (styled, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_outer_pipes_dropped() {
        assert_eq!(split_cells("| a | b |"), vec!["a", "b"]);
    }

    #[test]
    fn split_no_outer_pipes() {
        assert_eq!(split_cells("a | b"), vec!["a", "b"]);
    }

    #[test]
    fn split_empty_middle_cell_kept() {
        assert_eq!(split_cells("| a |  | c |"), vec!["a", "", "c"]);
    }

    #[test]
    fn split_escaped_pipe_stays_in_cell() {
        assert_eq!(split_cells(r"| a \| b | c |"), vec!["a | b", "c"]);
    }

    #[test]
    fn split_pipe_inside_code_span_stays() {
        assert_eq!(split_cells("| `a|b` | c |"), vec!["`a|b`", "c"]);
    }

    #[test]
    fn split_trailing_cr_tolerated() {
        assert_eq!(split_cells("| a | b |\r"), vec!["a", "b"]);
    }

    #[test]
    fn delimiter_basic() {
        assert_eq!(
            is_delimiter_row("|---|---|"),
            Some(vec![Alignment::Left, Alignment::Left])
        );
    }

    #[test]
    fn delimiter_alignments() {
        assert_eq!(
            is_delimiter_row("| :-- | :-: | --: |"),
            Some(vec![Alignment::Left, Alignment::Center, Alignment::Right])
        );
    }

    #[test]
    fn delimiter_requires_pipe() {
        // A markdown HR / setext-ish line is not a table delimiter.
        assert_eq!(is_delimiter_row("---"), None);
    }

    #[test]
    fn delimiter_rejects_text() {
        assert_eq!(is_delimiter_row("| a | b |"), None);
        assert_eq!(is_delimiter_row("| -a- | --- |"), None);
    }

    #[test]
    fn delimiter_rejects_empty_cell() {
        assert_eq!(is_delimiter_row("| --- |  |"), None);
    }

    #[test]
    fn row_detection() {
        assert!(looks_like_row("| a | b |"));
        assert!(looks_like_row("a | b"));
        assert!(!looks_like_row("plain text"));
        assert!(!looks_like_row(""));
        assert!(!looks_like_row("   "));
    }

    #[test]
    fn inline_plain_text_passthrough() {
        let (styled, width) = render_inline("hello", false);
        assert_eq!(styled, "hello");
        assert_eq!(width, 5);
    }

    #[test]
    fn inline_bold_markers_stripped() {
        let (styled, width) = render_inline("**hi**", false);
        assert!(!styled.contains("**"), "markers must be stripped: {styled:?}");
        assert!(styled.contains("hi"));
        assert!(styled.contains("\x1b["), "should carry ANSI styling");
        assert_eq!(width, 2);
    }

    #[test]
    fn inline_code_markers_stripped() {
        let (styled, width) = render_inline("`code x`", false);
        assert!(!styled.contains('`'), "backticks must be stripped: {styled:?}");
        assert!(styled.contains("code x"));
        assert_eq!(width, 6);
    }

    #[test]
    fn inline_link_shows_text_only() {
        let (styled, width) = render_inline("[docs](https://x.io)", false);
        assert!(styled.contains("docs"));
        assert!(!styled.contains("https"), "url must be hidden: {styled:?}");
        assert_eq!(width, 4);
    }

    #[test]
    fn inline_strike_and_italic() {
        let (s1, w1) = render_inline("~~gone~~", false);
        assert!(!s1.contains('~'));
        assert_eq!(w1, 4);
        let (s2, w2) = render_inline("*it*", false);
        assert!(!s2.contains('*'));
        assert_eq!(w2, 2);
    }

    #[test]
    fn inline_unbalanced_markers_verbatim() {
        let (styled, width) = render_inline("**oops", false);
        assert_eq!(styled, "**oops");
        assert_eq!(width, 6);
    }

    #[test]
    fn inline_header_plain_is_bold() {
        let (styled, _) = render_inline("Name", true);
        assert!(styled.contains("\x1b["), "header cells get bold styling");
        assert!(styled.contains("Name"));
    }

    #[test]
    fn inline_width_ignores_ansi_in_source() {
        let (_, width) = render_inline("\x1b[31mred\x1b[0m", false);
        assert_eq!(width, 3);
    }

    #[test]
    fn inline_width_cjk() {
        let (_, width) = render_inline("日本", false);
        assert_eq!(width, 4);
    }

    #[test]
    fn inline_mixed_segments() {
        let (styled, width) = render_inline("use `x` **now**", false);
        assert!(!styled.contains('`'));
        assert!(!styled.contains("**"));
        // "use x now" = 9 visible chars
        assert_eq!(width, 9);
    }
}

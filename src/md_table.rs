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

/// A parsed GFM table awaiting rendering.
#[derive(Debug, Clone)]
pub struct Table {
    /// Cells in the header row.
    pub header: Vec<String>,
    /// Alignment for each column.
    pub aligns: Vec<Alignment>,
    /// Data rows; each inner Vec is a row of cells.
    pub rows: Vec<Vec<String>>,
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

/// True if `c` counts as a "word" character for emphasis-flanking checks:
/// letters, digits, and underscore.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// GFM-inspired flanking check for `_`/`__` emphasis: an underscore
/// delimiter must not be used intraword. Requires an explicit non-word
/// character immediately outside both delimiters -- a bare string edge does
/// not count, since a leading/trailing `_`/`__` in cell content is more
/// often part of a larger identifier (e.g. `__init__`) than intentional
/// emphasis.
fn underscore_flanks_ok(text: &str, start: usize, end: usize) -> bool {
    let before_ok = text[..start].chars().next_back().is_some_and(|c| !is_word_char(c));
    let after_ok = text[end..].chars().next().is_some_and(|c| !is_word_char(c));
    before_ok && after_ok
}

/// GFM flanking check for `*`/`**` emphasis: the inner text must not start
/// or end with whitespace (rejects e.g. `2 * 3 * 4`).
fn asterisk_inner_ok(inner: &str) -> bool {
    let starts_ws = inner.chars().next().is_some_and(|c| c.is_whitespace());
    let ends_ws = inner.chars().next_back().is_some_and(|c| c.is_whitespace());
    !starts_ws && !ends_ws
}

/// Render GFM inline emphasis, code spans, links, and strikethrough within a
/// single table cell as ANSI-styled text.
///
/// Markers are stripped and replaced with terminal styling (bold, italic,
/// strikethrough, colored code/link text). Matches that violate GFM
/// emphasis-flanking rules (intraword `_`/`__`, whitespace-flanked
/// `*`/`**`) are left verbatim rather than styled. Returns the styled string
/// together with its visible (Unicode) width, ignoring any ANSI codes
/// already present in `text` -- `header` bolds plain (unstyled) segments too.
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

        // `accepted` is `None` when GFM flanking rules reject this match;
        // the whole match then falls through to verbatim output below.
        let accepted: Option<(&str, Style)> = if let Some(m) = caps.name("code") {
            let s = m.as_str();
            Some((&s[1..s.len() - 1], base_style(header).truecolor(166, 227, 161)))
        } else if caps.name("link").is_some() {
            Some((
                caps.name("ltext").unwrap().as_str(),
                base_style(header).truecolor(137, 180, 250).underline(),
            ))
        } else if caps.name("bold").is_some() {
            let inner = caps.name("btext").unwrap().as_str();
            asterisk_inner_ok(inner).then(|| (inner, base_style(header).bold()))
        } else if caps.name("bold2").is_some() {
            let inner = caps.name("b2text").unwrap().as_str();
            underscore_flanks_ok(text, whole.start(), whole.end())
                .then(|| (inner, base_style(header).bold()))
        } else if caps.name("strike").is_some() {
            Some((
                caps.name("stext").unwrap().as_str(),
                base_style(header).strikethrough(),
            ))
        } else if caps.name("italic").is_some() {
            let inner = caps.name("itext").unwrap().as_str();
            asterisk_inner_ok(inner).then(|| (inner, base_style(header).italic()))
        } else {
            let inner = caps.name("i2text").unwrap().as_str();
            underscore_flanks_ok(text, whole.start(), whole.end())
                .then(|| (inner, base_style(header).italic()))
        };

        match accepted {
            Some((inner, style)) => {
                styled.push_str(&inner.style(style).to_string());
                plain.push_str(inner);
            }
            None => push_plain(&mut styled, &mut plain, whole.as_str()),
        }
        last = whole.end();
    }
    push_plain(&mut styled, &mut plain, &text[last..]);

    let width = UnicodeWidthStr::width(strip_ansi(&plain).as_str());
    (styled, width)
}

/// Streaming state machine: feed lines, get passthrough lines or rendered
/// tables. Holds at most one line of lookahead (a candidate header waiting
/// for its delimiter row).
pub struct TableAssembler {
    state: State,
}

enum State {
    Idle,
    Candidate(String),
    InTable(Table),
}

/// Outcome of feeding a line to the assembler.
#[derive(Debug)]
pub enum FeedResult {
    /// Line(s) passed through unchanged. Emitted in order when a candidate fails to become
    /// a table or when a table ends.
    Pass(Vec<String>),
    /// Line held for lookahead: it could be a header or a table row.
    Buffered,
    /// Table rendering complete; rendered lines and optional trailing line to pass to engine.
    Table {
        /// Box-drawn ANSI output lines.
        rendered: Vec<String>,
        /// The line that ended the table (blank line, non-table line, etc.), if any.
        trailing: Option<String>,
    },
}

/// Outcome of flushing buffered content at end-of-input.
#[derive(Debug)]
pub enum FlushResult {
    /// Nothing held.
    Nothing,
    /// Held candidate line that never formed a table.
    Raw(String),
    /// Buffered table rendered and returned.
    Table(Vec<String>),
}

impl TableAssembler {
    /// Create a new idle assembler with no buffered content.
    pub fn new() -> Self {
        Self { state: State::Idle }
    }

    /// Return true if a line is currently buffered (candidate header or in-table state).
    pub fn has_buffered(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    /// Feed a line to the assembler.
    ///
    /// - Returns `Pass(lines)` if the input and/or previously buffered content should be passed
    ///   through unchanged to the engine.
    /// - Returns `Buffered` if the line is held for lookahead (candidate or table row).
    /// - Returns `Table { rendered, trailing }` if the input ended the table; table is rendered,
    ///   and `trailing` is passed through separately (may contain empty string or actual content).
    pub fn feed(&mut self, line: &str) -> FeedResult {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Idle => {
                if looks_like_row(line) {
                    self.state = State::Candidate(line.to_string());
                    FeedResult::Buffered
                } else {
                    FeedResult::Pass(vec![line.to_string()])
                }
            }
            State::Candidate(held) => {
                if let Some(aligns) = is_delimiter_row(line) {
                    let header = split_cells(&held);
                    if header.len() == aligns.len() {
                        self.state = State::InTable(Table {
                            header,
                            aligns,
                            rows: vec![],
                        });
                        return FeedResult::Buffered;
                    }
                }
                // Not a confirming delimiter: release held line. The current
                // line may itself start a new candidate.
                if looks_like_row(line) {
                    self.state = State::Candidate(line.to_string());
                    FeedResult::Pass(vec![held])
                } else {
                    FeedResult::Pass(vec![held, line.to_string()])
                }
            }
            State::InTable(mut table) => {
                if looks_like_row(line) {
                    table.rows.push(split_cells(line));
                    self.state = State::InTable(table);
                    FeedResult::Buffered
                } else {
                    FeedResult::Table {
                        rendered: render_table(&table),
                        trailing: Some(line.to_string()),
                    }
                }
            }
        }
    }

    /// Flush buffered content at end-of-input.
    ///
    /// - If idle, returns `Nothing`.
    /// - If a candidate header is held, returns `Raw(line)`.
    /// - If a table is in progress, renders and returns `Table(lines)`.
    pub fn flush(&mut self) -> FlushResult {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Idle => FlushResult::Nothing,
            State::Candidate(held) => FlushResult::Raw(held),
            State::InTable(table) => FlushResult::Table(render_table(&table)),
        }
    }
}

impl Default for TableAssembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a parsed table as box-drawn ANSI lines with styled borders and header.
///
/// Returns one line per output row: top border, header, middle border (if data),
/// data rows, and bottom border. Borders are styled dimmed, header is bold,
/// and cell content respects alignment and inline styling.
pub fn render_table(table: &Table) -> Vec<String> {
    let dim = Style::new().dimmed();

    let ncols = table
        .rows
        .iter()
        .map(|r| r.len())
        .chain([table.header.len()])
        .max()
        .unwrap_or(0)
        .max(1);

    // Normalize: pad every row (and header/aligns) to ncols.
    let pad_row = |row: &[String]| -> Vec<String> {
        let mut r: Vec<String> = row.to_vec();
        r.resize(ncols, String::new());
        r
    };
    let header = pad_row(&table.header);
    let mut aligns = table.aligns.clone();
    aligns.resize(ncols, Alignment::Left);

    // Render all cells up front: (styled, width) per cell.
    let header_cells: Vec<(String, usize)> =
        header.iter().map(|c| render_inline(c, true)).collect();
    let body_cells: Vec<Vec<(String, usize)>> = table
        .rows
        .iter()
        .map(|r| pad_row(r).iter().map(|c| render_inline(c, false)).collect())
        .collect();

    // Column widths: max visible width, minimum 1.
    let mut widths = vec![1usize; ncols];
    for (i, (_, w)) in header_cells.iter().enumerate() {
        widths[i] = widths[i].max(*w);
    }
    for row in &body_cells {
        for (i, (_, w)) in row.iter().enumerate() {
            widths[i] = widths[i].max(*w);
        }
    }

    let border = |left: char, mid: char, right: char| -> String {
        let inner: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
        format!("{left}{}{right}", inner.join(&mid.to_string()))
            .style(dim)
            .to_string()
    };

    let content_row = |cells: &[(String, usize)]| -> String {
        let sep = "│".style(dim).to_string();
        let mut line = sep.clone();
        for (i, (styled, width)) in cells.iter().enumerate() {
            let fill = widths[i] - width;
            let (l, r) = match aligns[i] {
                Alignment::Left => (0, fill),
                Alignment::Right => (fill, 0),
                Alignment::Center => (fill / 2, fill - fill / 2),
            };
            line.push(' ');
            line.push_str(&" ".repeat(l));
            line.push_str(styled);
            line.push_str(&" ".repeat(r));
            line.push(' ');
            line.push_str(&sep);
        }
        line
    };

    let mut out = Vec::new();
    out.push(border('┌', '┬', '┐'));
    out.push(content_row(&header_cells));
    if !body_cells.is_empty() {
        out.push(border('├', '┼', '┤'));
        for row in &body_cells {
            out.push(content_row(row));
        }
    }
    out.push(border('└', '┴', '┘'));
    out
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
    fn inline_intraword_underscore_verbatim() {
        // GFM intraword emphasis: identifiers with underscores must survive
        // untouched -- no deletion, no accidental italics.
        let (styled, width) = render_inline("get_user_id", false);
        assert_eq!(styled, "get_user_id");
        assert_eq!(width, 11);
    }

    #[test]
    fn inline_intraword_dunder_verbatim() {
        let (styled, width) = render_inline("__init__", false);
        assert_eq!(styled, "__init__");
        assert_eq!(width, 8);
    }

    #[test]
    fn inline_asterisk_with_whitespace_flanking_verbatim() {
        // Inner text flanked by whitespace (e.g. multiplication) must not be
        // treated as emphasis.
        let (styled, width) = render_inline("2 * 3 * 4", false);
        assert_eq!(styled, "2 * 3 * 4");
        assert_eq!(width, 9);
    }

    #[test]
    fn inline_bold_asterisk_still_works_near_underscore_identifier() {
        let (styled, _) = render_inline("snake_case in **bold**", false);
        assert!(!styled.contains("**"), "markers must be stripped: {styled:?}");
        assert!(styled.contains("bold"));
        assert!(styled.contains("snake_case"), "identifier must survive: {styled:?}");
        assert!(styled.contains("\x1b["), "bold should carry ANSI styling: {styled:?}");
    }

    #[test]
    fn inline_underscore_emphasis_still_works_with_space_flanking() {
        let (styled, _) = render_inline("a _real_ emphasis", false);
        assert!(!styled.contains('_'), "markers must be stripped: {styled:?}");
        assert!(styled.contains("real"));
        assert!(styled.contains("\x1b["), "italic should carry ANSI styling: {styled:?}");
    }

    #[test]
    fn inline_mixed_segments() {
        let (styled, width) = render_inline("use `x` **now**", false);
        assert!(!styled.contains('`'));
        assert!(!styled.contains("**"));
        // "use x now" = 9 visible chars
        assert_eq!(width, 9);
    }

    fn table_2x2() -> Table {
        Table {
            header: vec!["Name".into(), "Value".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![
                vec!["foo".into(), "12".into()],
                vec!["barbaz".into(), "3".into()],
            ],
        }
    }

    #[test]
    fn render_box_layout() {
        let lines: Vec<String> = render_table(&table_2x2())
            .iter()
            .map(|l| strip_ansi(l))
            .collect();
        assert_eq!(
            lines,
            vec![
                "┌────────┬───────┐",
                "│ Name   │ Value │",
                "├────────┼───────┤",
                "│ foo    │ 12    │",
                "│ barbaz │ 3     │",
                "└────────┴───────┘",
            ]
        );
    }

    #[test]
    fn render_has_styling() {
        let lines = render_table(&table_2x2());
        assert!(lines[0].contains("\x1b["), "borders should be styled");
        assert!(lines[1].contains("\x1b["), "header should be styled");
    }

    #[test]
    fn render_alignment() {
        let t = Table {
            header: vec!["L".into(), "C".into(), "R".into()],
            aligns: vec![Alignment::Left, Alignment::Center, Alignment::Right],
            rows: vec![vec!["a".into(), "b".into(), "c".into()]],
        };
        let lines: Vec<String> = render_table(&t).iter().map(|l| strip_ansi(l)).collect();
        // widths are all 1 → alignment is invisible at width 1; use wider header
        let t2 = Table {
            header: vec!["Col1".into(), "Col2".into(), "Col3".into()],
            aligns: vec![Alignment::Left, Alignment::Center, Alignment::Right],
            rows: vec![vec!["a".into(), "b".into(), "c".into()]],
        };
        let lines2: Vec<String> = render_table(&t2).iter().map(|l| strip_ansi(l)).collect();
        assert_eq!(lines2[3], "│ a    │  b   │    c │");
        drop(lines);
    }

    #[test]
    fn render_short_row_padded() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec!["only".into()]],
        };
        let lines: Vec<String> = render_table(&t).iter().map(|l| strip_ansi(l)).collect();
        assert_eq!(lines[3], "│ only │   │");
    }

    #[test]
    fn render_extra_cells_widen_table() {
        let t = Table {
            header: vec!["A".into()],
            aligns: vec![Alignment::Left],
            rows: vec![vec!["x".into(), "extra".into()]],
        };
        let lines: Vec<String> = render_table(&t).iter().map(|l| strip_ansi(l)).collect();
        // 2 columns in every line; no content dropped
        assert!(lines[1].matches('│').count() == 3, "{}", lines[1]);
        assert!(lines[3].contains("extra"));
    }

    #[test]
    fn render_header_only_box() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![],
        };
        let lines: Vec<String> = render_table(&t).iter().map(|l| strip_ansi(l)).collect();
        assert_eq!(lines.len(), 3); // top, header, bottom
        assert!(lines[2].starts_with('└'));
    }

    #[test]
    fn render_inline_markup_in_cells_affects_width() {
        let t = Table {
            header: vec!["A".into()],
            aligns: vec![Alignment::Left],
            rows: vec![vec!["**hi**".into()]],
        };
        let lines: Vec<String> = render_table(&t).iter().map(|l| strip_ansi(l)).collect();
        // visible "hi" (2 wide) < "A"… column width is max(1, 2) = 2
        assert_eq!(lines[3], "│ hi │");
    }

    fn feed_all(a: &mut TableAssembler, lines: &[&str]) -> Vec<FeedResult> {
        lines.iter().map(|l| a.feed(l)).collect()
    }

    #[test]
    fn assembler_passes_plain_lines() {
        let mut a = TableAssembler::new();
        match a.feed("hello world") {
            FeedResult::Pass(v) => assert_eq!(v, vec!["hello world"]),
            other => panic!("expected Pass, got {other:?}"),
        }
        assert!(!a.has_buffered());
    }

    #[test]
    fn assembler_holds_candidate_then_releases() {
        let mut a = TableAssembler::new();
        assert!(matches!(a.feed("| a | b |"), FeedResult::Buffered));
        assert!(a.has_buffered());
        // next line is not a delimiter → both released in order
        match a.feed("plain") {
            FeedResult::Pass(v) => assert_eq!(v, vec!["| a | b |", "plain"]),
            other => panic!("expected Pass, got {other:?}"),
        }
        assert!(!a.has_buffered());
    }

    #[test]
    fn assembler_candidate_then_new_candidate() {
        let mut a = TableAssembler::new();
        assert!(matches!(a.feed("| a | b |"), FeedResult::Buffered));
        // pipe-y line that is not a delimiter → old released, new held
        match a.feed("| c | d |") {
            FeedResult::Pass(v) => assert_eq!(v, vec!["| a | b |"]),
            other => panic!("expected Pass, got {other:?}"),
        }
        assert!(a.has_buffered());
    }

    #[test]
    fn assembler_renders_table_on_end() {
        let mut a = TableAssembler::new();
        feed_all(&mut a, &["| a | b |", "|---|---|", "| 1 | 2 |"]);
        match a.feed("after") {
            FeedResult::Table { rendered, trailing } => {
                assert!(rendered[0].contains('┌'));
                assert_eq!(rendered.len(), 5); // top, header, sep, 1 row, bottom
                assert_eq!(trailing.as_deref(), Some("after"));
            }
            other => panic!("expected Table, got {other:?}"),
        }
        assert!(!a.has_buffered());
    }

    #[test]
    fn assembler_blank_line_ends_table() {
        let mut a = TableAssembler::new();
        feed_all(&mut a, &["| a |", "|---|", "| 1 |"]);
        match a.feed("") {
            FeedResult::Table { trailing, .. } => assert_eq!(trailing.as_deref(), Some("")),
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn assembler_delimiter_count_mismatch_not_a_table() {
        let mut a = TableAssembler::new();
        assert!(matches!(a.feed("| a | b |"), FeedResult::Buffered));
        // 3-column delimiter under a 2-column header: GFM says not a table.
        // The delimiter line is itself pipe-y → becomes the new candidate.
        match a.feed("|---|---|---|") {
            FeedResult::Pass(v) => assert_eq!(v, vec!["| a | b |"]),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn assembler_delimiter_without_header_passes_raw() {
        let mut a = TableAssembler::new();
        // no candidate held; a delimiter row is just a pipe-y candidate line
        assert!(matches!(a.feed("|---|---|"), FeedResult::Buffered));
        match a.feed("plain") {
            FeedResult::Pass(v) => assert_eq!(v, vec!["|---|---|", "plain"]),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn assembler_flush_mid_table_renders() {
        let mut a = TableAssembler::new();
        feed_all(&mut a, &["| a |", "|---|", "| 1 |"]);
        match a.flush() {
            FlushResult::Table(lines) => assert!(lines[0].contains('┌')),
            other => panic!("expected Table, got {other:?}"),
        }
        assert!(!a.has_buffered());
    }

    #[test]
    fn assembler_flush_held_candidate_raw() {
        let mut a = TableAssembler::new();
        a.feed("| a | b |");
        match a.flush() {
            FlushResult::Raw(l) => assert_eq!(l, "| a | b |"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }

    #[test]
    fn assembler_flush_idle_nothing() {
        let mut a = TableAssembler::new();
        assert!(matches!(a.flush(), FlushResult::Nothing));
    }

    #[test]
    fn assembler_header_only_table_at_flush() {
        let mut a = TableAssembler::new();
        feed_all(&mut a, &["| a | b |", "|---|---|"]);
        match a.flush() {
            FlushResult::Table(lines) => assert_eq!(lines.len(), 3), // header-only box
            other => panic!("expected Table, got {other:?}"),
        }
    }
}

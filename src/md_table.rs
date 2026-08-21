//! GFM table detection and box-drawn rendering for markdown content.

use owo_colors::{OwoColorize, Style};

use crate::md_inline::{inline_segments, render_markdown_line, visible, wrap_cell, Seg};

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

/// Shrink columns to make the full box fit within `max_width`. The widest
/// column is shaved first, one character at a time, until the table fits or
/// every column has reached its floor (3 columns, or its natural width if
/// smaller). If the floors still don't fit, the table overflows.
fn fit_widths(natural: &[usize], max_width: usize) -> Vec<usize> {
    let n = natural.len();
    // Per column: two padding spaces plus one border, plus the final border.
    let chrome = 3 * n + 1;
    let available = max_width.saturating_sub(chrome);
    let mut widths = natural.to_vec();
    let floors: Vec<usize> = natural.iter().map(|&w| w.min(3)).collect();
    while widths.iter().sum::<usize>() > available {
        let widest = (0..n)
            .filter(|&i| widths[i] > floors[i])
            .max_by_key(|&i| (widths[i], std::cmp::Reverse(i)));
        match widest {
            Some(i) => widths[i] -= 1,
            None => break,
        }
    }
    widths
}

/// Streaming state machine: feed lines, get passthrough lines or rendered
/// tables. Holds at most one line of lookahead (a candidate header waiting
/// for its delimiter row).
pub struct TableAssembler {
    state: State,
    /// True while inside a fenced code block (``` or ~~~); table detection
    /// is suspended and every line passes through verbatim.
    in_fence: bool,
}

/// True if the line's trimmed start opens or closes a fenced code block.
pub(crate) fn is_fence_toggle(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
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
        Self { state: State::Idle, in_fence: false }
    }

    /// Return true if a line is currently buffered (candidate header or in-table state).
    pub fn has_buffered(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    /// Feed a line to the assembler.
    ///
    /// - Returns `Pass(lines)` if the input and/or previously buffered content should be passed
    ///   through to the engine. Body lines get inline markdown formatting
    ///   (emphasis, code spans, clickable links); fenced code stays verbatim.
    /// - Returns `Buffered` if the line is held for lookahead (candidate or table row).
    /// - Returns `Table { rendered, trailing }` if the input ended the table; table is rendered,
    ///   and `trailing` is passed through separately (may contain empty string or actual content).
    pub fn feed(&mut self, line: &str) -> FeedResult {
        if self.in_fence {
            if is_fence_toggle(line) {
                self.in_fence = false;
            }
            return FeedResult::Pass(vec![line.to_string()]);
        }

        match std::mem::replace(&mut self.state, State::Idle) {
            State::Idle => {
                if is_fence_toggle(line) {
                    self.in_fence = true;
                    FeedResult::Pass(vec![line.to_string()])
                } else if looks_like_row(line) {
                    self.state = State::Candidate(line.to_string());
                    FeedResult::Buffered
                } else {
                    FeedResult::Pass(vec![render_markdown_line(line)])
                }
            }
            State::Candidate(held) => {
                if is_fence_toggle(line) {
                    self.in_fence = true;
                    return FeedResult::Pass(vec![render_markdown_line(&held), line.to_string()]);
                }
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
                    FeedResult::Pass(vec![render_markdown_line(&held)])
                } else {
                    FeedResult::Pass(vec![
                        render_markdown_line(&held),
                        render_markdown_line(line),
                    ])
                }
            }
            State::InTable(mut table) => {
                if is_fence_toggle(line) {
                    self.in_fence = true;
                    FeedResult::Table {
                        rendered: render_table(&table, detect_width()),
                        trailing: Some(line.to_string()),
                    }
                } else if looks_like_row(line) {
                    table.rows.push(split_cells(line));
                    self.state = State::InTable(table);
                    FeedResult::Buffered
                } else {
                    FeedResult::Table {
                        rendered: render_table(&table, detect_width()),
                        trailing: Some(render_markdown_line(line)),
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
            State::Candidate(held) => FlushResult::Raw(render_markdown_line(&held)),
            State::InTable(table) => FlushResult::Table(render_table(&table, detect_width())),
        }
    }
}

impl Default for TableAssembler {
    fn default() -> Self {
        Self::new()
    }
}

/// Detected terminal width in columns, falling back to 80 when the output
/// is not a terminal (same convention as the trigger separator).
fn detect_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

/// Render a parsed table as box-drawn ANSI lines with styled borders and header.
///
/// Returns one line per output row: top border, header, middle border (if data),
/// data rows, and bottom border. Borders are styled dimmed, header is bold,
/// and cell content respects alignment and inline styling. Columns are shrunk
/// and their content wrapped as needed so the box fits within `max_width`.
pub fn render_table(table: &Table, max_width: usize) -> Vec<String> {
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

    // Parse all cells up front into styled segments.
    let header_cells: Vec<Vec<Seg>> =
        header.iter().map(|c| inline_segments(c, true)).collect();
    let body_cells: Vec<Vec<Vec<Seg>>> = table
        .rows
        .iter()
        .map(|r| pad_row(r).iter().map(|c| inline_segments(c, false)).collect())
        .collect();

    let cell_width =
        |segs: &[Seg]| -> usize { segs.iter().map(|s| visible(&s.text)).sum() };

    // Natural column widths: max visible width, minimum 1.
    let mut natural = vec![1usize; ncols];
    for row in body_cells.iter().chain([&header_cells]) {
        for (i, segs) in row.iter().enumerate() {
            natural[i] = natural[i].max(cell_width(segs));
        }
    }
    let widths = fit_widths(&natural, max_width);

    let border = |left: char, mid: char, right: char| -> String {
        let inner: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
        format!("{left}{}{right}", inner.join(&mid.to_string()))
            .style(dim)
            .to_string()
    };

    // Render one logical row: cells wrap to their column width, the tallest
    // cell sets the physical line count, shorter cells pad with blanks.
    let content_rows = |cells: &[Vec<Seg>], out: &mut Vec<String>| {
        let sep = "│".style(dim).to_string();
        let wrapped: Vec<Vec<(String, usize)>> = cells
            .iter()
            .enumerate()
            .map(|(i, segs)| wrap_cell(segs, widths[i]))
            .collect();
        let height = wrapped.iter().map(|c| c.len()).max().unwrap_or(1);
        for li in 0..height {
            let mut line = sep.clone();
            for (i, cell) in wrapped.iter().enumerate() {
                let empty = (String::new(), 0);
                let (styled, width) = cell.get(li).unwrap_or(&empty);
                let fill = widths[i].saturating_sub(*width);
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
            out.push(line);
        }
    };

    let mut out = Vec::new();
    out.push(border('┌', '┬', '┐'));
    content_rows(&header_cells, &mut out);
    if !body_cells.is_empty() {
        out.push(border('├', '┼', '┤'));
        for row in &body_cells {
            content_rows(row, &mut out);
        }
    }
    out.push(border('└', '┴', '┘'));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    use crate::trigger::strip_ansi;

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
        let lines: Vec<String> = render_table(&table_2x2(), 500)
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
        let lines = render_table(&table_2x2(), 500);
        assert!(lines[0].contains("\x1b["), "borders should be styled");
        assert!(lines[1].contains("\x1b["), "header should be styled");
    }

    #[test]
    fn render_alignment() {
        // widths of 1 make alignment invisible, so use a wider header.
        let t = Table {
            header: vec!["Col1".into(), "Col2".into(), "Col3".into()],
            aligns: vec![Alignment::Left, Alignment::Center, Alignment::Right],
            rows: vec![vec!["a".into(), "b".into(), "c".into()]],
        };
        let lines: Vec<String> = render_table(&t, 500).iter().map(|l| strip_ansi(l)).collect();
        assert_eq!(lines[3], "│ a    │  b   │    c │");
    }

    #[test]
    fn render_short_row_padded() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec!["only".into()]],
        };
        let lines: Vec<String> = render_table(&t, 500).iter().map(|l| strip_ansi(l)).collect();
        assert_eq!(lines[3], "│ only │   │");
    }

    #[test]
    fn render_extra_cells_widen_table() {
        let t = Table {
            header: vec!["A".into()],
            aligns: vec![Alignment::Left],
            rows: vec![vec!["x".into(), "extra".into()]],
        };
        let lines: Vec<String> = render_table(&t, 500).iter().map(|l| strip_ansi(l)).collect();
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
        let lines: Vec<String> = render_table(&t, 500).iter().map(|l| strip_ansi(l)).collect();
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
        let lines: Vec<String> = render_table(&t, 500).iter().map(|l| strip_ansi(l)).collect();
        // visible "hi" (2 wide) < "A"… column width is max(1, 2) = 2
        assert_eq!(lines[3], "│ hi │");
    }

    /// Visible width of a rendered line (ANSI stripped), using the same
    /// emoji-width mode as the renderer.
    fn visible_width(line: &str) -> usize {
        visible(line)
    }

    #[test]
    fn render_fits_within_max_width() {
        let t = Table {
            header: vec!["Key".into(), "Description".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec![
                "alpha".into(),
                "a very long description that certainly exceeds the maximum table width".into(),
            ]],
        };
        let lines: Vec<String> = render_table(&t, 40).iter().map(|l| strip_ansi(l)).collect();
        for line in &lines {
            assert!(
                UnicodeWidthStr::width(line.as_str()) <= 40,
                "line exceeds max width: {line:?}"
            );
        }
        let all = lines.join("\n");
        assert!(all.contains("alpha"));
        assert!(all.contains("description"), "wrapped content must survive: {all}");
    }

    #[test]
    fn render_wraps_at_word_boundaries_exact_layout() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec!["x".into(), "one two three four".into()]],
        };
        let lines: Vec<String> = render_table(&t, 20).iter().map(|l| strip_ansi(l)).collect();
        assert_eq!(
            lines,
            vec![
                "┌───┬──────────────┐",
                "│ A │ B            │",
                "├───┼──────────────┤",
                "│ x │ one two      │",
                "│   │ three four   │",
                "└───┴──────────────┘",
            ]
        );
    }

    #[test]
    fn render_wrapped_link_reopens_hyperlink_per_line() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec![
                "x".into(),
                "[a very long link text wrapping](https://x.io)".into(),
            ]],
        };
        let rendered = render_table(&t, 20);
        let link_lines: Vec<&String> = rendered
            .iter()
            .filter(|l| l.contains("\x1b]8;;https://x.io\x1b\\"))
            .collect();
        assert!(link_lines.len() >= 2, "link should wrap across lines");
        // Every line that opens a hyperlink also closes it, so the link
        // never spans borders or padding.
        for line in link_lines {
            assert!(
                line.contains("\x1b]8;;\x1b\\"),
                "hyperlink must close on its own line: {line:?}"
            );
        }
        for line in &rendered {
            assert!(visible_width(line) <= 20, "line too wide: {line:?}");
        }
    }

    #[test]
    fn render_wrapped_emoji_sequence_keeps_borders_aligned() {
        // U+2764 U+FE0F is an emoji presentation sequence: width 2 at string
        // level. Char-by-char counting would see 1+0 and misalign borders on
        // wrapped rows.
        let t = Table {
            header: vec!["Tool".into(), "Verdict".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec![
                "delta".into(),
                "❤️ was already using it without realizing love it".into(),
            ]],
        };
        let lines: Vec<String> = render_table(&t, 40).iter().map(|l| strip_ansi(l)).collect();
        let widths: Vec<usize> = lines.iter().map(|l| visible_width(l)).collect();
        assert!(
            widths.iter().all(|&w| w == widths[0]),
            "all lines must be equally wide: {widths:?}\n{lines:#?}"
        );
    }

    #[test]
    fn render_hard_breaks_long_tokens() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec!["x".into(), "abcdefghijkl".into()]],
        };
        // chrome 7 + col A 1 → column B gets 6.
        let lines: Vec<String> = render_table(&t, 14).iter().map(|l| strip_ansi(l)).collect();
        assert!(lines.iter().any(|l| l.contains("abcdef")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("ghijkl")), "{lines:?}");
        for line in &lines {
            assert!(visible_width(line) <= 14, "line too wide: {line:?}");
        }
    }

    #[test]
    fn render_shaves_widest_column_first() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec![
                "short cell".into(),
                "a considerably longer cell with much more text in it".into(),
            ]],
        };
        let lines: Vec<String> = render_table(&t, 40).iter().map(|l| strip_ansi(l)).collect();
        // The short column keeps its natural width; only the wide one wraps.
        assert!(lines.iter().any(|l| l.contains(" short cell ")), "{lines:?}");
        for line in &lines {
            assert!(visible_width(line) <= 40, "line too wide: {line:?}");
        }
    }

    #[test]
    fn render_wrap_cjk_stays_within_width() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec!["x".into(), "日本語日本語".into()]],
        };
        // Column B gets 6 → wraps into width-4 slices of two CJK chars… any
        // split is fine as long as no line overflows and nothing is lost.
        let lines: Vec<String> = render_table(&t, 14).iter().map(|l| strip_ansi(l)).collect();
        for line in &lines {
            assert!(visible_width(line) <= 14, "line too wide: {line:?}");
        }
        let all = lines.join("");
        assert_eq!(all.matches('日').count(), 2, "{lines:?}");
        assert_eq!(all.matches('本').count(), 2, "{lines:?}");
        assert_eq!(all.matches('語').count(), 2, "{lines:?}");
    }

    #[test]
    fn render_wrap_ignores_source_ansi_in_widths() {
        let t = Table {
            header: vec!["A".into(), "B".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec![
                "x".into(),
                "\x1b[31mred words wrapping here nicely\x1b[0m".into(),
            ]],
        };
        let rendered = render_table(&t, 20);
        for line in &rendered {
            assert!(visible_width(line) <= 20, "line too wide: {line:?}");
        }
        // Escape sequences survive wrapping unsplit.
        let all = rendered.join("\n");
        assert!(all.contains("\x1b[31m"), "source ANSI must survive");
        let plain = strip_ansi(&all);
        assert!(plain.contains("red words"), "{plain}");
        assert!(plain.contains("nicely"), "{plain}");
    }

    #[test]
    fn render_floor_reached_overflows_gracefully() {
        let t = Table {
            header: vec!["AAAA".into(), "BBBB".into(), "CCCC".into()],
            aligns: vec![Alignment::Left; 3],
            rows: vec![vec!["aaaa".into(), "bbbb".into(), "cccc".into()]],
        };
        // chrome 10 + 3 floors of 3 = 19 minimum; max_width 12 can't be met.
        let lines: Vec<String> = render_table(&t, 12).iter().map(|l| strip_ansi(l)).collect();
        for line in &lines {
            assert_eq!(visible_width(line), 19, "floor width box: {line:?}");
        }
        // Content is wrapped, not dropped.
        let all = lines.join("");
        assert!(all.contains("aaa"));
        assert!(all.contains("ccc"));
    }

    #[test]
    fn render_wrapped_header_keeps_bold() {
        let t = Table {
            header: vec!["K".into(), "A Rather Long Header Title".into()],
            aligns: vec![Alignment::Left, Alignment::Left],
            rows: vec![vec!["v".into(), "x".into()]],
        };
        let rendered = render_table(&t, 18);
        let plain: Vec<String> = rendered.iter().map(|l| strip_ansi(l)).collect();
        for line in &plain {
            assert!(visible_width(line) <= 18, "line too wide: {line:?}");
        }
        // Header spans multiple lines, each carrying styling.
        let header_lines: Vec<&String> = rendered
            .iter()
            .filter(|l| {
                let p = strip_ansi(l);
                p.contains("Rather") || p.contains("Header")
            })
            .collect();
        assert!(header_lines.len() >= 2, "header should wrap: {plain:?}");
        for line in &header_lines {
            assert!(line.contains("\x1b["), "wrapped header line should be styled");
        }
    }

    #[test]
    fn render_alignment_applies_per_wrapped_line() {
        let t = Table {
            header: vec!["A".into(), "Num".into()],
            aligns: vec![Alignment::Left, Alignment::Right],
            rows: vec![vec!["x".into(), "aaa bb".into()]],
        };
        // Column B natural 6, shrink to 3 → two lines, right-aligned.
        let lines: Vec<String> = render_table(&t, 11).iter().map(|l| strip_ansi(l)).collect();
        assert!(lines.contains(&"│ x │ aaa │".to_string()), "{lines:?}");
        assert!(lines.contains(&"│   │  bb │".to_string()), "{lines:?}");
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

    #[test]
    fn assembler_formats_body_lines() {
        let mut a = TableAssembler::new();
        match a.feed("some **bold** text") {
            FeedResult::Pass(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(strip_ansi(&v[0]), "some bold text");
                assert!(v[0].contains("\x1b["), "should carry ANSI styling: {:?}", v[0]);
            }
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn assembler_formats_released_candidate() {
        let mut a = TableAssembler::new();
        assert!(matches!(a.feed("| **a** | b |"), FeedResult::Buffered));
        match a.feed("plain") {
            FeedResult::Pass(v) => {
                assert_eq!(strip_ansi(&v[0]), "| a | b |", "markers stripped: {:?}", v[0]);
                assert_eq!(v[1], "plain");
            }
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn assembler_formats_trailing_line_after_table() {
        let mut a = TableAssembler::new();
        feed_all(&mut a, &["| a |", "|---|", "| 1 |"]);
        match a.feed("after **bold**") {
            FeedResult::Table { trailing, .. } => {
                let t = trailing.unwrap();
                assert_eq!(strip_ansi(&t), "after bold");
                assert!(t.contains("\x1b["), "trailing should be formatted: {t:?}");
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn assembler_does_not_format_inside_fence() {
        let mut a = TableAssembler::new();
        assert!(matches!(a.feed("```"), FeedResult::Pass(_)));
        match a.feed("let x = **not bold**;") {
            FeedResult::Pass(v) => assert_eq!(v, vec!["let x = **not bold**;"]),
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn assembler_flush_formats_held_candidate() {
        let mut a = TableAssembler::new();
        a.feed("| **a** | b |");
        match a.flush() {
            FlushResult::Raw(l) => assert_eq!(strip_ansi(&l), "| a | b |"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }

    #[test]
    fn assembler_fenced_table_passes_through_raw() {
        let mut a = TableAssembler::new();
        let lines = ["```", "| a | b |", "|---|---|", "| 1 | 2 |", "```"];
        for line in lines {
            match a.feed(line) {
                FeedResult::Pass(v) => assert_eq!(v, vec![line]),
                other => panic!("expected Pass for {line:?}, got {other:?}"),
            }
        }
        assert!(!a.has_buffered());
    }

    #[test]
    fn assembler_fence_closing_then_real_table_still_renders() {
        let mut a = TableAssembler::new();
        for line in ["```", "code here", "```"] {
            assert!(matches!(a.feed(line), FeedResult::Pass(_)));
        }
        feed_all(&mut a, &["| a | b |", "|---|---|", "| 1 | 2 |"]);
        match a.feed("after") {
            FeedResult::Table { rendered, trailing } => {
                assert!(rendered[0].contains('┌'));
                assert_eq!(trailing.as_deref(), Some("after"));
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn assembler_fence_toggle_during_in_table_renders_table() {
        let mut a = TableAssembler::new();
        feed_all(&mut a, &["| a | b |", "|---|---|", "| 1 | 2 |"]);
        match a.feed("```") {
            FeedResult::Table { rendered, trailing } => {
                assert!(rendered[0].contains('┌'));
                assert_eq!(trailing.as_deref(), Some("```"));
            }
            other => panic!("expected Table, got {other:?}"),
        }
        // Assembler is now inside the fence: subsequent lines pass through
        // raw, even ones that look like table rows.
        match a.feed("| c | d |") {
            FeedResult::Pass(v) => assert_eq!(v, vec!["| c | d |"]),
            other => panic!("expected Pass, got {other:?}"),
        }
        assert!(!a.has_buffered());
    }

    #[test]
    fn assembler_tilde_fence_also_toggles() {
        let mut a = TableAssembler::new();
        assert!(matches!(a.feed("~~~"), FeedResult::Pass(_)));
        match a.feed("| a | b |") {
            FeedResult::Pass(v) => assert_eq!(v, vec!["| a | b |"]),
            other => panic!("expected Pass (still fenced), got {other:?}"),
        }
        assert!(matches!(a.feed("~~~"), FeedResult::Pass(_)));
        // Fence closed: normal candidate detection resumes.
        assert!(matches!(a.feed("| a | b |"), FeedResult::Buffered));
    }
}

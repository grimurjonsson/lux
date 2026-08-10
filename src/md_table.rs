//! GFM table detection and box-drawn rendering for markdown content.

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
}

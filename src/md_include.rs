//! Inline expansion of own-line `@file.md` references when viewing markdown.

use owo_colors::{OwoColorize, Style};

/// Truecolor cycle for include depths 1..: blue, magenta, cyan, yellow, green.
const DEPTH_COLORS: [(u8, u8, u8); 5] = [
    (137, 180, 250),
    (203, 166, 247),
    (148, 226, 213),
    (249, 226, 175),
    (166, 227, 161),
];

/// Total visible width of header/footer frame lines.
const FRAME_WIDTH: usize = 40;

/// Detect an own-line include reference: trimmed `@<path>` whose path ends
/// in `.md` or `.markdown` (case-insensitive). Returns the path as written.
pub fn detect_ref(line: &str) -> Option<&str> {
    let t = line.trim();
    let path = t.strip_prefix('@')?;
    if path.is_empty() || path != path.trim() {
        return None;
    }
    let lower = path.to_lowercase();
    let stem_ok = |ext: &str| lower.len() > ext.len() && lower.ends_with(ext);
    if stem_ok(".md") || stem_ok(".markdown") {
        Some(path)
    } else {
        None
    }
}

/// The gutter/frame color for an include depth (1-based). None without color.
pub(crate) fn depth_style(depth: usize, color_enabled: bool) -> Option<Style> {
    if !color_enabled {
        return None;
    }
    let (r, g, b) = DEPTH_COLORS[(depth.saturating_sub(1)) % DEPTH_COLORS.len()];
    Some(Style::new().truecolor(r, g, b))
}

fn styled(text: &str, style: Option<Style>) -> String {
    match style {
        Some(s) => text.style(s).to_string(),
        None => text.to_string(),
    }
}

/// `┌─ <path> ` padded with `─` to FRAME_WIDTH visible columns (min 3).
pub(crate) fn header_line(path: &str, depth: usize, color_enabled: bool) -> String {
    let style = depth_style(depth, color_enabled);
    let lead = "┌─ ";
    let used = lead.chars().count() + path.chars().count() + 1;
    let dashes = FRAME_WIDTH.saturating_sub(used).max(3);
    let name = if color_enabled {
        path.style(Style::new().dimmed()).to_string()
    } else {
        path.to_string()
    };
    format!(
        "{}{} {}",
        styled(lead, style),
        name,
        styled(&"─".repeat(dashes), style)
    )
}

/// `└` + `─` to FRAME_WIDTH visible columns.
pub(crate) fn footer_line(depth: usize, color_enabled: bool) -> String {
    let style = depth_style(depth, color_enabled);
    styled(&format!("└{}", "─".repeat(FRAME_WIDTH - 1)), style)
}

/// Single-line inline note: `┌─ <path> ─ <reason> ─` in dim red.
pub(crate) fn note_line(path: &str, reason: &str, _depth: usize, color_enabled: bool) -> String {
    let text = format!("┌─ {path} ─ {reason} ─");
    if color_enabled {
        text.style(Style::new().red().dimmed()).to_string()
    } else {
        text
    }
}

/// One gutter unit `│ ` in the depth's color.
pub(crate) fn gutter(depth: usize, color_enabled: bool) -> String {
    let style = depth_style(depth, color_enabled);
    format!("{} ", styled("│", style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::strip_ansi;

    #[test]
    fn ref_own_line() {
        assert_eq!(detect_ref("@setup.md"), Some("setup.md"));
        assert_eq!(detect_ref("  @docs/setup.md  "), Some("docs/setup.md"));
        assert_eq!(detect_ref("@My Notes.markdown"), Some("My Notes.markdown"));
        assert_eq!(detect_ref("@UPPER.MD"), Some("UPPER.MD"));
    }

    #[test]
    fn ref_rejects_non_refs() {
        assert_eq!(detect_ref("see @setup.md for details"), None);
        assert_eq!(detect_ref("@image.png"), None);
        assert_eq!(detect_ref("@"), None);
        assert_eq!(detect_ref("@.md"), None);
        assert_eq!(detect_ref("email@example.md extra"), None);
        assert_eq!(detect_ref("plain text"), None);
    }

    #[test]
    fn header_pads_to_40_visible_columns() {
        let h = strip_ansi(&header_line("setup.md", 1, true));
        assert!(h.starts_with("┌─ setup.md "));
        assert_eq!(h.chars().count(), 40);
        // Long path: minimum 3 trailing dashes even past 40
        let long = "a/very/long/path/that/overflows/forty/columns.md";
        let h2 = strip_ansi(&header_line(long, 1, true));
        assert!(h2.ends_with("───"));
        assert!(h2.chars().count() > 40);
    }

    #[test]
    fn footer_matches_header_width() {
        let f = strip_ansi(&footer_line(1, true));
        assert!(f.starts_with('└'));
        assert_eq!(f.chars().count(), 40);
        assert!(f[3..].chars().all(|c| c == '─' || c == '└'));
    }

    #[test]
    fn note_line_contains_reason() {
        let n = strip_ansi(&note_line("gone.md", "not found", 1, true));
        assert!(n.starts_with("┌─ gone.md ─ not found ─"));
    }

    #[test]
    fn gutter_is_bar_space() {
        assert_eq!(strip_ansi(&gutter(1, true)), "│ ");
        assert_eq!(gutter(1, false), "│ ");
        assert!(gutter(1, true).contains("\x1b["), "colored when enabled");
    }

    #[test]
    fn depth_colors_cycle() {
        // Compare rendered gutter strings (Style itself may not impl Eq/Debug)
        assert_eq!(gutter(1, true), gutter(6, true), "depth 6 wraps to depth 1's color");
        assert_ne!(gutter(1, true), gutter(2, true));
        assert!(depth_style(1, false).is_none());
    }

    #[test]
    fn fence_toggle_is_reusable() {
        assert!(crate::md_table::is_fence_toggle("```rust"));
        assert!(crate::md_table::is_fence_toggle("~~~"));
        assert!(!crate::md_table::is_fence_toggle("text"));
    }
}

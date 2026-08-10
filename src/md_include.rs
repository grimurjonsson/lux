//! Inline expansion of own-line `@file.md` references when viewing markdown.

use std::path::{Path, PathBuf};
use owo_colors::{OwoColorize, Style};
use crate::engine::Engine;
use crate::filter::LineFilter;
use crate::md_table::{is_fence_toggle, FeedResult, FlushResult, TableAssembler};

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

/// Maximum include nesting depth; deeper refs render a note instead.
pub const MAX_DEPTH: usize = 5;

/// Context shared across the include recursion.
pub struct IncludeCtx<'a> {
    pub color_enabled: bool,
    pub filter: &'a LineFilter,
}

/// Render the root file's lines with `@ref` expansion. `lines` are the
/// root's raw lines (already trimmed to any `-n` range by the caller).
pub fn render_root(
    lines: &[String],
    root_path: &Path,
    engine: &mut Engine,
    ctx: &IncludeCtx,
) -> Vec<String> {
    let base_dir = root_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut visited = vec![canonical(root_path)];
    render_lines(lines, &base_dir, 0, &mut visited, engine, ctx)
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Render one file's lines at `depth`. Returned lines are NOT yet gutter-
/// prefixed for `depth` — the caller prefixes when splicing (depth 0 = root,
/// no prefix). Nested includes below this level arrive already prefixed.
fn render_lines(
    lines: &[String],
    base_dir: &Path,
    depth: usize,
    visited: &mut Vec<PathBuf>,
    engine: &mut Engine,
    ctx: &IncludeCtx,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut table: Option<TableAssembler> =
        ctx.color_enabled.then(TableAssembler::new);
    let mut in_fence = false;

    let emit_engine = |out: &mut Vec<String>, engine: &mut Engine, raw: &str| {
        out.extend(engine.apply(raw).flatten());
    };
    let flush_table = |out: &mut Vec<String>, engine: &mut Engine, t: &mut TableAssembler| {
        match t.flush() {
            FlushResult::Nothing => {}
            FlushResult::Raw(raw) => out.extend(engine.apply(&raw).flatten()),
            FlushResult::Table(rendered) => out.extend(rendered),
        }
    };

    for line in lines {
        let raw = line.trim_end_matches('\r');

        let toggled = is_fence_toggle(raw);
        if toggled {
            in_fence = !in_fence;
        }

        // Ref detection comes BEFORE the line filter: an @ref line is a
        // structural directive, not content — filters must not suppress
        // expansion. Toggle lines can't be refs; inside fences refs are
        // literal text.
        if !in_fence && !toggled {
            if let Some(ref_path) = detect_ref(raw) {
                if let Some(t) = table.as_mut() {
                    flush_table(&mut out, engine, t);
                }
                out.extend(render_included(ref_path, base_dir, depth + 1, visited, engine, ctx));
                continue;
            }
        }

        if ctx.filter.is_active() && !ctx.filter.should_show(raw) {
            continue;
        }

        match table.as_mut() {
            None => emit_engine(&mut out, engine, raw),
            Some(t) => match t.feed(raw) {
                FeedResult::Pass(raws) => {
                    for r in raws {
                        emit_engine(&mut out, engine, &r);
                    }
                }
                FeedResult::Buffered => {}
                FeedResult::Table { rendered, trailing } => {
                    out.extend(rendered);
                    if let Some(r) = trailing {
                        emit_engine(&mut out, engine, &r);
                    }
                }
            },
        }
    }

    if let Some(t) = table.as_mut() {
        flush_table(&mut out, engine, t);
    }
    out
}

/// Render one `@ref`: checks (missing/cycle/depth) → note line, otherwise
/// header + gutter-prefixed child content + footer.
fn render_included(
    path_as_written: &str,
    base_dir: &Path,
    depth: usize,
    visited: &mut Vec<PathBuf>,
    engine: &mut Engine,
    ctx: &IncludeCtx,
) -> Vec<String> {
    let resolved = if Path::new(path_as_written).is_absolute() {
        PathBuf::from(path_as_written)
    } else {
        base_dir.join(path_as_written)
    };

    if depth > MAX_DEPTH {
        return vec![note_line(path_as_written, "skipped: max include depth", depth, ctx.color_enabled)];
    }
    let canon = canonical(&resolved);
    if visited.contains(&canon) {
        return vec![note_line(path_as_written, "skipped: already included above", depth, ctx.color_enabled)];
    }
    let content = match std::fs::read_to_string(&resolved) {
        Ok(c) => c,
        Err(_) => {
            return vec![note_line(path_as_written, "not found", depth, ctx.color_enabled)];
        }
    };

    visited.push(canon);
    let child_lines: Vec<String> = content.lines().map(String::from).collect();
    let child_base = resolved.parent().unwrap_or(Path::new(".")).to_path_buf();
    let rendered = render_lines(&child_lines, &child_base, depth, visited, engine, ctx);
    visited.pop();

    let g = gutter(depth, ctx.color_enabled);
    let mut out = Vec::with_capacity(rendered.len() + 2);
    out.push(header_line(path_as_written, depth, ctx.color_enabled));
    out.extend(rendered.into_iter().map(|l| format!("{g}{l}")));
    out.push(footer_line(depth, ctx.color_enabled));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::strip_ansi;
    use crate::engine::Engine;
    use crate::filter::LineFilter;
    use std::fs;
    use tempfile::TempDir;

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

    // Tests for render_root and recursive rendering
    fn ctx_filter() -> LineFilter {
        LineFilter::new(&[], &[], true).unwrap()
    }

    fn render(dir: &TempDir, root: &str, color: bool) -> Vec<String> {
        let filter = ctx_filter();
        let ctx = IncludeCtx { color_enabled: color, filter: &filter };
        let mut engine = Engine::new(vec![], color, None);
        let root_path = dir.path().join(root);
        let lines: Vec<String> = fs::read_to_string(&root_path)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        render_root(&lines, &root_path, &mut engine, &ctx)
            .iter()
            .map(|l| strip_ansi(l))
            .collect()
    }

    #[test]
    fn expands_simple_include() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("child.md"), "child line\n").unwrap();
        fs::write(dir.path().join("root.md"), "before\n@child.md\nafter\n").unwrap();
        let out = render(&dir, "root.md", false);
        assert_eq!(
            out,
            vec![
                "before",
                &format!("┌─ child.md {}", "─".repeat(40 - "┌─ child.md ".chars().count())),
                "│ child line",
                &format!("└{}", "─".repeat(39)),
                "after",
            ]
        );
    }

    #[test]
    fn nested_include_double_gutter() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("grand.md"), "deep\n").unwrap();
        fs::write(dir.path().join("child.md"), "mid\n@grand.md\n").unwrap();
        fs::write(dir.path().join("root.md"), "@child.md\n").unwrap();
        let out = render(&dir, "root.md", false);
        assert!(out.contains(&"│ mid".to_string()));
        assert!(out.contains(&"│ │ deep".to_string()));
        // nested header carries one gutter unit
        assert!(out.iter().any(|l| l.starts_with("│ ┌─ grand.md ")));
    }

    #[test]
    fn relative_paths_resolve_from_including_file() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/inner.md"), "inner\n").unwrap();
        fs::write(dir.path().join("sub/child.md"), "@inner.md\n").unwrap();
        fs::write(dir.path().join("root.md"), "@sub/child.md\n").unwrap();
        let out = render(&dir, "root.md", false);
        assert!(out.contains(&"│ │ inner".to_string()), "{out:?}");
    }

    #[test]
    fn missing_file_renders_note() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("root.md"), "@gone.md\nafter\n").unwrap();
        let out = render(&dir, "root.md", false);
        assert!(out.iter().any(|l| l.contains("gone.md") && l.contains("not found")));
        assert!(out.contains(&"after".to_string()));
        assert!(!out.iter().any(|l| l.starts_with('└')), "no footer for notes");
    }

    #[test]
    fn cycle_renders_note() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.md"), "in a\n@b.md\n").unwrap();
        fs::write(dir.path().join("b.md"), "in b\n@a.md\n").unwrap();
        let out = render(&dir, "a.md", false);
        assert!(out.contains(&"in a".to_string()));
        assert!(out.iter().any(|l| l.contains("│ │ in b") || l.contains("│ in b")));
        assert!(out.iter().any(|l| l.contains("already included above")));
    }

    #[test]
    fn depth_cap_renders_note() {
        let dir = TempDir::new().unwrap();
        // chain 0→1→…→6; depth cap 5 means f5 renders but its @f6 ref is capped
        for i in 0..7 {
            let body = if i < 6 { format!("lvl{i}\n@f{}.md\n", i + 1) } else { "lvl6\n".into() };
            fs::write(dir.path().join(format!("f{i}.md")), body).unwrap();
        }
        let out = render(&dir, "f0.md", false);
        assert!(out.iter().any(|l| l.contains("lvl5")), "depth 5 content renders");
        assert!(!out.iter().any(|l| l.contains("lvl6")), "depth 6 content capped");
        assert!(out.iter().any(|l| l.contains("max include depth")));
    }

    #[test]
    fn fenced_refs_stay_literal() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("child.md"), "child\n").unwrap();
        fs::write(
            dir.path().join("root.md"),
            "```\n@child.md\n```\n@child.md\n",
        )
        .unwrap();
        let out = render(&dir, "root.md", false);
        assert!(out.contains(&"@child.md".to_string()), "fenced ref literal");
        assert!(out.contains(&"│ child".to_string()), "unfenced ref expands");
    }

    #[test]
    fn tables_inside_include_box_drawn_with_color() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("t.md"), "| a | b |\n|---|---|\n| 1 | 2 |\n").unwrap();
        fs::write(dir.path().join("root.md"), "@t.md\n").unwrap();
        let out = render(&dir, "root.md", true);
        assert!(out.iter().any(|l| l.contains('┬')), "table box inside include: {out:?}");
        // every table line carries the gutter
        assert!(out.iter().filter(|l| l.contains('┼') || l.contains('┬')).all(|l| l.starts_with("│ ")));
    }

    #[test]
    fn empty_include_renders_header_footer_only() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("empty.md"), "").unwrap();
        fs::write(dir.path().join("root.md"), "@empty.md\n").unwrap();
        let out = render(&dir, "root.md", false);
        assert_eq!(out.len(), 2);
        assert!(out[0].starts_with("┌─ empty.md "));
        assert!(out[1].starts_with('└'));
    }

    #[test]
    fn filter_applies_to_content_not_frames() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("child.md"), "keep this\ndrop this\n").unwrap();
        fs::write(dir.path().join("root.md"), "@child.md\n").unwrap();
        let filter = LineFilter::new(&["keep".to_string()], &[], true).unwrap();
        let ctx = IncludeCtx { color_enabled: false, filter: &filter };
        let mut engine = Engine::new(vec![], false, None);
        let root_path = dir.path().join("root.md");
        let lines = vec!["@child.md".to_string()];
        let out: Vec<String> = render_root(&lines, &root_path, &mut engine, &ctx)
            .iter().map(|l| strip_ansi(l)).collect();
        assert!(out.contains(&"│ keep this".to_string()));
        assert!(!out.iter().any(|l| l.contains("drop this")));
        assert!(out[0].starts_with("┌─"), "frames unaffected by filter");
    }
}

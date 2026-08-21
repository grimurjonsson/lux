//! GFM inline rendering: emphasis, code spans, links, strikethrough — shared
//! by table cells and markdown body lines. Includes width-aware wrapping.

use std::sync::LazyLock;

use owo_colors::{OwoColorize, Style};
use regex::Regex;
use unicode_width::UnicodeWidthStr;

use crate::trigger::strip_ansi;

/// One alternation per inline construct. Order matters: code spans are
/// opaque, links before emphasis, `**bold**` before `*italic*`.
static INLINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?P<code>`[^`]+`)",
        r"|(?P<link>\[(?P<ltext>[^\]]*)\]\((?P<lurl>[^)]*)\))",
        r"|(?P<auto><(?P<aurl>https?://[^>\s]+)>)",
        r"|(?P<url>https?://\S+)",
        r"|(?P<bold>\*\*(?P<btext>.+?)\*\*)",
        r"|(?P<bold2>__(?P<b2text>.+?)__)",
        r"|(?P<strike>~~(?P<stext>.+?)~~)",
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

/// A run of text sharing one style. `style: None` means the text is emitted
/// verbatim (it may carry ANSI codes of its own from the source). A `link`
/// renders the run wrapped in an OSC 8 terminal hyperlink.
pub(crate) struct Seg {
    pub(crate) text: String,
    pub(crate) style: Option<Style>,
    pub(crate) link: Option<String>,
}

/// How many terminal cells an emoji presentation sequence (base char +
/// U+FE0F) occupies. Terminals disagree: xterm.js, Terminal.app, and default
/// iTerm2 advance one cell (`Narrow`); Ghostty, kitty, and WezTerm follow
/// Unicode and advance two (`Wide`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmojiWidth {
    Narrow,
    Wide,
}

static EMOJI_WIDTH: std::sync::OnceLock<EmojiWidth> = std::sync::OnceLock::new();

/// Pin the emoji width mode (from config). First caller wins; later calls
/// and the auto-detection fallback are ignored.
pub fn set_emoji_width(mode: EmojiWidth) {
    let _ = EMOJI_WIDTH.set(mode);
}

/// Detect the emoji width mode from terminal identity variables.
///
/// A recognized `TERM_PROGRAM` decides directly. An unrecognized one falls
/// through to `TERM`: wrappers embedding a real terminal advertise it there
/// (e.g. Supacode sets `TERM=xterm-ghostty` and renders with Ghostty).
/// Anything still unknown defaults to narrow.
fn detect_emoji_width_from(term_program: Option<&str>, term: Option<&str>) -> EmojiWidth {
    let says_wide = |v: &str| {
        let v = v.to_ascii_lowercase();
        v.contains("ghostty") || v.contains("wezterm") || v.contains("kitty")
    };
    let says_narrow = |v: &str| {
        let v = v.to_ascii_lowercase();
        v.contains("iterm")
            || v.contains("apple_terminal")
            || v.contains("tmux")
            || v.contains("vscode")
            || v.contains("hyper")
    };
    if let Some(tp) = term_program.filter(|v| !v.is_empty()) {
        if says_wide(tp) {
            return EmojiWidth::Wide;
        }
        if says_narrow(tp) {
            return EmojiWidth::Narrow;
        }
    }
    if term.filter(|v| !v.is_empty()).is_some_and(says_wide) {
        return EmojiWidth::Wide;
    }
    EmojiWidth::Narrow
}

/// The process-wide emoji width mode: config override if set, otherwise
/// detected from $TERM_PROGRAM / $TERM.
fn emoji_width() -> EmojiWidth {
    *EMOJI_WIDTH.get_or_init(|| {
        detect_emoji_width_from(
            std::env::var("TERM_PROGRAM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        )
    })
}

/// Width of ANSI-free text under a given emoji width mode. `Wide` uses
/// unicode-width's string measure (emoji presentation sequences count 2);
/// `Narrow` sums per-char widths, so VS16 adds nothing.
fn measure_mode(text: &str, mode: EmojiWidth) -> usize {
    match mode {
        EmojiWidth::Wide => UnicodeWidthStr::width(text),
        EmojiWidth::Narrow => text
            .chars()
            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
            .sum(),
    }
}

/// Visible width of a string in the active emoji mode, ignoring ANSI codes.
pub(crate) fn visible(text: &str) -> usize {
    measure_mode(strip_ansi(text).as_str(), emoji_width())
}

/// Parse GFM inline emphasis, code spans, links, and strikethrough into
/// styled segments.
///
/// Markers are stripped; each match becomes a segment carrying its terminal
/// style (bold, italic, strikethrough, colored code/link text). Matches that
/// violate GFM emphasis-flanking rules (intraword `_`/`__`, whitespace-flanked
/// `*`/`**`) are left verbatim rather than styled. `header` bolds plain
/// (unstyled) segments too.
pub(crate) fn inline_segments(text: &str, header: bool) -> Vec<Seg> {
    segments_with(text, base_style(header), header)
}

/// Recursive worker for [`inline_segments`]: `base` accumulates the styles of
/// enclosing emphasis spans (e.g. the inner of `**…**` parses with a bold
/// base, so `**[a](u)**` yields a bold clickable link). `style_plain` styles
/// text outside any construct with `base` too (header cells, emphasis inners).
fn segments_with(text: &str, base: Style, style_plain: bool) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut last = 0;

    let push_plain = |segs: &mut Vec<Seg>, seg: &str| {
        if seg.is_empty() {
            return;
        }
        segs.push(Seg {
            text: seg.to_string(),
            style: style_plain.then_some(base),
            link: None,
        });
    };
    // Emphasis recurses into its inner text so nested constructs keep
    // working; the emphasis attribute rides along via the base style.
    let nested =
        |segs: &mut Vec<Seg>, inner: &str, style: Style| segs.extend(segments_with(inner, style, true));

    for caps in INLINE_RE.captures_iter(text) {
        let whole = caps.get(0).unwrap();
        push_plain(&mut segs, &text[last..whole.start()]);

        // `leaf` is a construct whose text is taken verbatim (code, links);
        // `None` falls through to the emphasis arms below, which recurse.
        // Bare URLs exclude trailing punctuation, emitted verbatim after.
        let mut after_link = "";
        let link_style = base.truecolor(137, 180, 250).underline();
        let leaf: Option<(&str, Style, Option<String>)> = if let Some(m) = caps.name("code") {
            let s = m.as_str();
            Some((&s[1..s.len() - 1], base.truecolor(166, 227, 161), None))
        } else if caps.name("link").is_some() {
            let url = caps.name("lurl").unwrap().as_str();
            Some((
                caps.name("ltext").unwrap().as_str(),
                link_style,
                (!url.is_empty()).then(|| url.to_string()),
            ))
        } else if let Some(m) = caps.name("aurl") {
            Some((m.as_str(), link_style, Some(m.as_str().to_string())))
        } else if let Some(m) = caps.name("url") {
            let url = m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '"', '\'']);
            after_link = &m.as_str()[url.len()..];
            Some((url, link_style, Some(url.to_string())))
        } else {
            let emphasis: Option<(&str, Style)> = if caps.name("bold").is_some() {
                let inner = caps.name("btext").unwrap().as_str();
                asterisk_inner_ok(inner).then(|| (inner, base.bold()))
            } else if caps.name("bold2").is_some() {
                let inner = caps.name("b2text").unwrap().as_str();
                underscore_flanks_ok(text, whole.start(), whole.end())
                    .then(|| (inner, base.bold()))
            } else if caps.name("strike").is_some() {
                Some((caps.name("stext").unwrap().as_str(), base.strikethrough()))
            } else if caps.name("italic").is_some() {
                let inner = caps.name("itext").unwrap().as_str();
                asterisk_inner_ok(inner).then(|| (inner, base.italic()))
            } else {
                let inner = caps.name("i2text").unwrap().as_str();
                underscore_flanks_ok(text, whole.start(), whole.end())
                    .then(|| (inner, base.italic()))
            };
            match emphasis {
                Some((inner, style)) => nested(&mut segs, inner, style),
                None => push_plain(&mut segs, whole.as_str()),
            }
            last = whole.end();
            continue;
        };

        match leaf {
            Some((inner, style, link)) => {
                segs.push(Seg {
                    text: inner.to_string(),
                    style: Some(style),
                    link,
                });
                push_plain(&mut segs, after_link);
            }
            None => push_plain(&mut segs, whole.as_str()),
        }
        last = whole.end();
    }
    push_plain(&mut segs, &text[last..]);
    segs
}

/// Wrap already-styled text in an OSC 8 terminal hyperlink.
fn hyperlink(styled: &str, url: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{styled}\x1b]8;;\x1b\\")
}

/// Render segments to an ANSI string plus total visible width.
pub(crate) fn render_segs(segs: &[Seg]) -> (String, usize) {
    let mut styled = String::new();
    let mut width = 0;
    for seg in segs {
        let run = match seg.style {
            Some(style) => seg.text.style(style).to_string(),
            None => seg.text.clone(),
        };
        match &seg.link {
            Some(url) => styled.push_str(&hyperlink(&run, url)),
            None => styled.push_str(&run),
        }
        width += visible(&seg.text);
    }
    (styled, width)
}

/// Render GFM inline markup as ANSI-styled text. Returns the styled string
/// together with its visible (Unicode) width, ignoring any ANSI codes
/// already present in `text`.
pub fn render_inline(text: &str, header: bool) -> (String, usize) {
    render_segs(&inline_segments(text, header))
}

/// Render one markdown body line with inline formatting and clickable links.
///
/// Skipped verbatim: ATX heading lines (syntect owns their full-line style)
/// and indented code blocks (4 spaces or a tab). Lines without inline markup
/// come back unchanged.
pub fn render_markdown_line(line: &str) -> String {
    if line.trim_start().starts_with('#')
        || line.starts_with("    ")
        || line.starts_with('\t')
    {
        return line.to_string();
    }
    let segs = inline_segments(line, false);
    // All-plain segments mean no markup matched: return the input unchanged
    // rather than a reassembled copy.
    if segs.iter().all(|s| s.style.is_none() && s.link.is_none()) {
        return line.to_string();
    }
    render_segs(&segs).0
}

/// One indivisible unit of text for wrapping: a single character, or a
/// full ANSI escape sequence (width 0, glued to neighbors).
struct Atom {
    text: String,
    width: usize,
    /// Index into the segment list, for style lookup and merging.
    seg: usize,
    space: bool,
}

/// Flatten segments into atoms. ANSI escape sequences already present in the
/// source are kept as single zero-width atoms so wrapping never splits them.
/// Zero-width characters (variation selectors, ZWJ, combining marks) and the
/// character following a ZWJ glue onto the previous atom, whose width is then
/// recomputed at string level so emoji sequences (e.g. ❤️ = U+2764 U+FE0F,
/// width 2) count the way terminals render them.
fn tokenize(segs: &[Seg]) -> Vec<Atom> {
    let mode = emoji_width();
    let mut atoms: Vec<Atom> = Vec::new();
    for (si, seg) in segs.iter().enumerate() {
        let mut chars = seg.text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                let mut esc = String::from(c);
                for next in chars.by_ref() {
                    esc.push(next);
                    if next == 'm' {
                        break;
                    }
                }
                atoms.push(Atom { text: esc, width: 0, seg: si, space: false });
                continue;
            }
            let zero_width = measure_mode(c.to_string().as_str(), mode) == 0;
            let glue = atoms.last().is_some_and(|prev| {
                prev.seg == si
                    && !prev.text.starts_with('\x1b')
                    && (zero_width || prev.text.ends_with('\u{200d}'))
            });
            if glue {
                let prev = atoms.last_mut().unwrap();
                prev.text.push(c);
                prev.width = measure_mode(prev.text.as_str(), mode);
            } else {
                atoms.push(Atom {
                    text: c.to_string(),
                    width: measure_mode(c.to_string().as_str(), mode),
                    seg: si,
                    space: c.is_whitespace(),
                });
            }
        }
    }
    atoms
}

/// Build one output line from atoms: consecutive atoms of the same segment
/// merge into one styled run. Returns the ANSI string and its visible width.
fn line_from_atoms(atoms: &[Atom], segs: &[Seg]) -> (String, usize) {
    let mut out = String::new();
    let mut width = 0;
    let mut run = String::new();
    let mut run_seg = usize::MAX;
    let flush = |out: &mut String, run: &mut String, seg: usize| {
        if run.is_empty() {
            return;
        }
        let styled = match segs.get(seg).and_then(|s| s.style) {
            Some(style) => run.style(style).to_string(),
            None => run.clone(),
        };
        match segs.get(seg).and_then(|s| s.link.as_deref()) {
            Some(url) => out.push_str(&hyperlink(&styled, url)),
            None => out.push_str(&styled),
        }
        run.clear();
    };
    for atom in atoms {
        if atom.seg != run_seg {
            flush(&mut out, &mut run, run_seg);
            run_seg = atom.seg;
        }
        run.push_str(&atom.text);
        width += atom.width;
    }
    flush(&mut out, &mut run, run_seg);
    (out, width)
}

/// Wrap segments to `max` columns. Wraps at word boundaries, hard-breaking
/// words wider than `max`. Returns one `(styled, width)` pair per output
/// line; content that already fits comes back as a single line rendered
/// exactly as [`render_segs`] would.
pub(crate) fn wrap_cell(segs: &[Seg], max: usize) -> Vec<(String, usize)> {
    let (styled, width) = render_segs(segs);
    if width <= max {
        return vec![(styled, width)];
    }

    let atoms = tokenize(segs);
    // Group into words with the space atom (if any) that precedes each.
    let mut words: Vec<(Option<Atom>, Vec<Atom>)> = Vec::new();
    let mut gap: Option<Atom> = None;
    for atom in atoms {
        if atom.space {
            gap.get_or_insert(atom);
        } else {
            match words.last_mut() {
                Some((_, word)) if gap.is_none() => word.push(atom),
                _ => words.push((gap.take(), vec![atom])),
            }
        }
    }

    let mut lines: Vec<(String, usize)> = Vec::new();
    let mut cur: Vec<Atom> = Vec::new();
    let mut cur_w = 0;
    for (space, word) in words {
        let word_w: usize = word.iter().map(|a| a.width).sum();
        let sep_w = usize::from(space.is_some() && !cur.is_empty());
        if !cur.is_empty() && cur_w + sep_w + word_w > max {
            lines.push(line_from_atoms(&cur, segs));
            cur.clear();
            cur_w = 0;
        } else if let Some(sp) = space
            && !cur.is_empty()
        {
            cur_w += sp.width;
            cur.push(sp);
        }
        if word_w <= max {
            cur_w += word_w;
            cur.extend(word);
        } else {
            // Hard-break: fill lines atom by atom; zero-width atoms always
            // stay attached to the preceding character.
            for atom in word {
                if !cur.is_empty() && atom.width > 0 && cur_w + atom.width > max {
                    lines.push(line_from_atoms(&cur, segs));
                    cur.clear();
                    cur_w = 0;
                }
                cur_w += atom.width;
                cur.push(atom);
            }
        }
    }
    if !cur.is_empty() {
        lines.push(line_from_atoms(&cur, segs));
    }
    if lines.is_empty() {
        lines.push((String::new(), 0));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            !crate::trigger::strip_ansi(&styled).contains("https"),
            "url must not be visible: {styled:?}"
        );
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
    fn inline_link_is_osc8_hyperlink() {
        let (styled, width) = render_inline("[docs](https://x.io)", false);
        assert!(
            styled.contains("\x1b]8;;https://x.io\x1b\\"),
            "should open an OSC 8 hyperlink: {styled:?}"
        );
        assert!(
            styled.contains("\x1b]8;;\x1b\\"),
            "should close the OSC 8 hyperlink: {styled:?}"
        );
        assert_eq!(width, 4, "URL contributes no visible width");
        assert_eq!(crate::trigger::strip_ansi(&styled), "docs");
    }

    #[test]
    fn inline_autolink_clickable() {
        let (styled, width) = render_inline("<https://x.io>", false);
        assert!(styled.contains("\x1b]8;;https://x.io\x1b\\"), "{styled:?}");
        assert_eq!(crate::trigger::strip_ansi(&styled), "https://x.io");
        assert_eq!(width, 12);
    }

    #[test]
    fn inline_bare_url_clickable() {
        let (styled, width) = render_inline("see https://x.io now", false);
        assert!(styled.contains("\x1b]8;;https://x.io\x1b\\"), "{styled:?}");
        assert_eq!(crate::trigger::strip_ansi(&styled), "see https://x.io now");
        assert_eq!(width, 20);
    }

    #[test]
    fn inline_bare_url_trailing_punctuation_not_linked() {
        let (styled, _) = render_inline("visit https://x.io.", false);
        assert!(
            styled.contains("\x1b]8;;https://x.io\x1b\\"),
            "trailing period must stay outside the URL: {styled:?}"
        );
        assert_eq!(crate::trigger::strip_ansi(&styled), "visit https://x.io.");
    }

    #[test]
    fn inline_url_in_code_span_not_linked() {
        let (styled, _) = render_inline("`https://x.io`", false);
        assert!(
            !styled.contains("\x1b]8;;"),
            "code spans must stay unlinked: {styled:?}"
        );
    }

    #[test]
    fn body_line_formats_inline_markup() {
        let out = render_markdown_line("some **bold** and `code` here");
        let plain = crate::trigger::strip_ansi(&out);
        assert_eq!(plain, "some bold and code here");
        assert!(out.contains("\x1b["), "should carry ANSI styling: {out:?}");
    }

    #[test]
    fn body_line_links_clickable() {
        let out = render_markdown_line("see [docs](https://x.io) for more");
        assert!(out.contains("\x1b]8;;https://x.io\x1b\\"), "{out:?}");
        assert_eq!(crate::trigger::strip_ansi(&out), "see docs for more");
    }

    #[test]
    fn body_line_plain_text_unchanged() {
        assert_eq!(render_markdown_line("just plain text"), "just plain text");
        assert_eq!(render_markdown_line(""), "");
    }

    #[test]
    fn body_line_headings_left_verbatim() {
        assert_eq!(
            render_markdown_line("## A **heading** here"),
            "## A **heading** here"
        );
    }

    #[test]
    fn body_line_indented_code_left_verbatim() {
        assert_eq!(
            render_markdown_line("    let x = **not bold**;"),
            "    let x = **not bold**;"
        );
        assert_eq!(
            render_markdown_line("\tlet y = *nope*;"),
            "\tlet y = *nope*;"
        );
    }

    #[test]
    fn body_line_list_items_formatted() {
        let out = render_markdown_line("- item with **bold**");
        assert_eq!(crate::trigger::strip_ansi(&out), "- item with bold");
        assert!(out.contains("\x1b["), "{out:?}");
    }

    #[test]
    fn measure_mode_narrow_counts_vs16_emoji_as_one() {
        // Terminals like xterm.js, Terminal.app, and default iTerm2 advance
        // the cursor one cell for U+2764+VS16.
        assert_eq!(measure_mode("\u{2764}\u{fe0f}", EmojiWidth::Narrow), 1);
        assert_eq!(measure_mode("\u{2764}\u{fe0f} x", EmojiWidth::Narrow), 3);
    }

    #[test]
    fn measure_mode_wide_counts_vs16_emoji_as_two() {
        // Ghostty/kitty/WezTerm follow Unicode: emoji presentation is wide.
        assert_eq!(measure_mode("\u{2764}\u{fe0f}", EmojiWidth::Wide), 2);
    }

    #[test]
    fn measure_mode_agrees_on_plain_and_cjk() {
        for mode in [EmojiWidth::Narrow, EmojiWidth::Wide] {
            assert_eq!(measure_mode("hello", mode), 5);
            assert_eq!(measure_mode("日本", mode), 4);
        }
    }

    #[test]
    fn emoji_width_detection_by_terminal() {
        assert_eq!(detect_emoji_width_from(Some("ghostty"), None), EmojiWidth::Wide);
        assert_eq!(detect_emoji_width_from(Some("WezTerm"), None), EmojiWidth::Wide);
        assert_eq!(detect_emoji_width_from(None, Some("xterm-kitty")), EmojiWidth::Wide);
        assert_eq!(detect_emoji_width_from(Some("iTerm.app"), Some("xterm-256color")), EmojiWidth::Narrow);
        assert_eq!(detect_emoji_width_from(None, None), EmojiWidth::Narrow);
    }

    #[test]
    fn emoji_width_known_term_program_wins() {
        // A recognized TERM_PROGRAM decides directly, whatever $TERM says.
        assert_eq!(
            detect_emoji_width_from(Some("ghostty"), Some("xterm-256color")),
            EmojiWidth::Wide
        );
        assert_eq!(
            detect_emoji_width_from(Some("iTerm.app"), Some("xterm-ghostty")),
            EmojiWidth::Narrow
        );
        assert_eq!(
            detect_emoji_width_from(Some("vscode"), Some("xterm-256color")),
            EmojiWidth::Narrow
        );
    }

    #[test]
    fn emoji_width_unknown_term_program_falls_through_to_term() {
        // Wrappers embedding a real terminal advertise it via $TERM (Supacode
        // sets TERM=xterm-ghostty and renders with Ghostty: VS16 is wide).
        assert_eq!(
            detect_emoji_width_from(Some("supacode"), Some("xterm-ghostty")),
            EmojiWidth::Wide
        );
        assert_eq!(
            detect_emoji_width_from(Some("someterm"), Some("xterm-256color")),
            EmojiWidth::Narrow
        );
    }

    #[test]
    fn wrap_never_splits_emoji_sequences() {
        // Emoji presentation sequence (❤️) and ZWJ family sequence must ride
        // through hard-wrapping as indivisible units.
        let segs = inline_segments("❤️❤️❤️ and 👨\u{200d}👩\u{200d}👧 fam", false);
        let lines = wrap_cell(&segs, 4);
        let joined: Vec<String> = lines.iter().map(|(s, _)| s.clone()).collect();
        for line in &joined {
            assert!(
                !line.ends_with('\u{200d}'),
                "ZWJ must not dangle at a line break: {joined:?}"
            );
        }
        assert!(
            joined.iter().any(|l| l.contains("👨\u{200d}👩\u{200d}👧")),
            "family sequence must stay whole: {joined:?}"
        );
    }

    #[test]
    fn inline_link_nested_in_bold() {
        let (styled, width) = render_inline("**[lfk](https://github.com/janosmiko/lfk)**", false);
        let plain = crate::trigger::strip_ansi(&styled);
        assert_eq!(plain, "lfk", "markers and URL must be stripped: {styled:?}");
        assert!(
            styled.contains("\x1b]8;;https://github.com/janosmiko/lfk\x1b\\"),
            "nested link must be clickable: {styled:?}"
        );
        let has_bold = styled.split('\x1b').any(|chunk| {
            chunk.strip_prefix('[')
                .and_then(|c| c.strip_suffix('m').or(c.split('m').next()))
                .is_some_and(|params| params.split(';').any(|p| p == "1"))
        });
        assert!(has_bold, "should carry bold: {styled:?}");
        assert_eq!(width, 3);
    }

    #[test]
    fn inline_code_nested_in_bold() {
        let (styled, width) = render_inline("**use `jq` here**", false);
        assert_eq!(crate::trigger::strip_ansi(&styled), "use jq here");
        assert_eq!(width, 11);
    }

    #[test]
    fn inline_italic_nested_in_bold() {
        let (styled, width) = render_inline("**a *b* c**", false);
        assert_eq!(crate::trigger::strip_ansi(&styled), "a b c");
        assert_eq!(width, 5);
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

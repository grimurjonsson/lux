use std::collections::HashMap;
use std::io::Cursor;
use std::ops::Range;
use std::path::Path;
use std::sync::LazyLock;

use owo_colors::Style;
use syntect::easy::HighlightLines;
use syntect::highlighting::{self, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// Catppuccin Mocha theme embedded at compile time.
const CATPPUCCIN_MOCHA_TMTHEME: &[u8] = include_bytes!("../themes/catppuccin-mocha.tmTheme");

/// Lazily loaded syntax set using two-face's extended collection (bat's syntaxes).
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(|| {
    two_face::syntax::extra_newlines()
});

/// Lazily loaded theme set: syntect defaults + bundled Catppuccin Mocha.
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(|| {
    let mut ts = ThemeSet::load_defaults();
    if let Ok(theme) = ThemeSet::load_from_reader(&mut Cursor::new(CATPPUCCIN_MOCHA_TMTHEME)) {
        ts.themes.insert("Catppuccin Mocha".to_string(), theme);
    }
    ts
});

/// Default theme used when none is configured.
const DEFAULT_THEME: &str = "Catppuccin Mocha";

/// Built-in filename → syntax mappings for files not covered by syntect/two-face.
const BUILTIN_SYNTAX_MAP: &[(&str, &str)] = &[
    ("justfile", "Bourne Again Shell (bash)"),
    ("Justfile", "Bourne Again Shell (bash)"),
    ("Vagrantfile", "Ruby"),
    ("Gemfile", "Ruby"),
    ("Rakefile", "Ruby"),
    ("Brewfile", "Ruby"),
    (".envrc", "Bourne Again Shell (bash)"),
    (".env", "Bourne Again Shell (bash)"),
];

/// Wraps syntect to provide syntax highlighting as a base layer.
///
/// Produces `(Range<usize>, owo_colors::Style)` spans that the Engine
/// can merge with rule-based spans using priority ordering.
pub struct SyntaxHighlighter {
    syntax: &'static SyntaxReference,
    theme: &'static highlighting::Theme,
    /// Parser + highlighter state carried from one line to the next, so that
    /// multi-line constructs (fenced code blocks in Markdown, block comments,
    /// raw strings) keep their context.
    lines: HighlightLines<'static>,
}

impl SyntaxHighlighter {
    /// Create a highlighter for a file path.
    ///
    /// Lookup order:
    /// 1. Custom syntax_map entries (extension or filename → syntax name)
    /// 2. File extension via syntect
    /// 3. Full filename via syntect (handles Makefile, Dockerfile, etc.)
    ///
    /// Returns `None` if no syntax definition is found.
    pub fn for_file(
        path: &Path,
        theme_name: Option<&str>,
        syntax_map: Option<&HashMap<String, String>>,
    ) -> Option<Self> {
        let ss = &*SYNTAX_SET;

        // 1. Check custom syntax_map first
        if let Some(map) = syntax_map {
            // Try extension match
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if let Some(syntax_name) = map.get(ext) {
                    if let Some(syntax) = ss.find_syntax_by_name(syntax_name)
                        .or_else(|| ss.find_syntax_by_extension(syntax_name))
                    {
                        return Self::with_syntax(syntax, theme_name);
                    }
                }
            }
            // Try filename match
            if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
                if let Some(syntax_name) = map.get(fname) {
                    if let Some(syntax) = ss.find_syntax_by_name(syntax_name)
                        .or_else(|| ss.find_syntax_by_extension(syntax_name))
                    {
                        return Self::with_syntax(syntax, theme_name);
                    }
                }
            }
        }

        // 2. Try file extension via syntect
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(syntax) = ss.find_syntax_by_extension(ext) {
                return Self::with_syntax(syntax, theme_name);
            }
        }

        // 3. Try full filename via syntect (handles Makefile, Dockerfile, etc.)
        if let Some(fname) = path.file_name().and_then(|f| f.to_str()) {
            if let Some(syntax) = ss.find_syntax_by_extension(fname) {
                return Self::with_syntax(syntax, theme_name);
            }

            // 4. Built-in filename mappings (justfile → Makefile, etc.)
            for &(pattern, syntax_name) in BUILTIN_SYNTAX_MAP {
                if fname == pattern {
                    if let Some(syntax) = ss.find_syntax_by_name(syntax_name)
                        .or_else(|| ss.find_syntax_by_extension(syntax_name))
                    {
                        return Self::with_syntax(syntax, theme_name);
                    }
                }
            }
        }

        None
    }

    /// Create a highlighter for a syntect syntax identified by name or extension.
    ///
    /// Used when lux has no file path but still wants to apply a syntax — e.g.
    /// content-sniffing detects markdown on stdin.
    pub fn for_syntax_name(name: &str, theme_name: Option<&str>) -> Option<Self> {
        let ss = &*SYNTAX_SET;
        let syntax = ss
            .find_syntax_by_name(name)
            .or_else(|| ss.find_syntax_by_extension(name))?;
        Self::with_syntax(syntax, theme_name)
    }

    /// True when this highlighter is using the Markdown grammar — used to
    /// activate markdown-specific rendering (tables).
    pub fn is_markdown(&self) -> bool {
        self.syntax.name == "Markdown"
    }

    fn with_syntax(
        syntax: &'static SyntaxReference,
        theme_name: Option<&str>,
    ) -> Option<Self> {
        let requested = theme_name.unwrap_or(DEFAULT_THEME);
        let theme = THEME_SET
            .themes
            .get(requested)
            .or_else(|| THEME_SET.themes.get(DEFAULT_THEME))?;

        Some(Self {
            syntax,
            theme,
            lines: HighlightLines::new(syntax, theme),
        })
    }

    /// Highlight the next line of the document, returning byte-range spans with
    /// owo_colors styles.
    ///
    /// Lines must be fed in document order: parser state is carried across calls
    /// so that constructs spanning several lines (a ```lang fence in Markdown,
    /// a block comment) are highlighted correctly.
    ///
    /// Regions whose foreground matches the theme default foreground are skipped
    /// (they represent "no highlighting").
    pub fn highlight_line(&mut self, line: &str) -> Vec<(Range<usize>, Style)> {
        // The syntax set is compiled in "newlines" mode: grammars rely on a
        // literal '\n' to close line-scoped contexts such as '#' comments.
        // Callers hand us lines with the newline already stripped, so add it
        // back for parsing and clip the resulting spans to the original length.
        let with_newline;
        let parse_input: &str = if line.ends_with('\n') {
            line
        } else {
            with_newline = format!("{line}\n");
            &with_newline
        };
        let regions = match self.lines.highlight_line(parse_input, &SYNTAX_SET) {
            Ok(r) => r,
            Err(_) => return vec![],
        };

        let default_fg = self.theme.settings.foreground.unwrap_or(highlighting::Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        });
        let default_bg = self.theme.settings.background.unwrap_or(highlighting::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        });

        let mut spans = Vec::new();
        let mut byte_offset = 0usize;

        for (style, text) in &regions {
            let len = text.len();
            if len == 0 {
                continue;
            }

            let fg = style.foreground;
            let bg = style.background;

            // Skip regions that are just the default foreground on default background
            let fg_is_default = fg.r == default_fg.r && fg.g == default_fg.g && fg.b == default_fg.b;
            let bg_is_default = bg.r == default_bg.r && bg.g == default_bg.g && bg.b == default_bg.b;

            let start = byte_offset;
            let end = (byte_offset + len).min(line.len());
            byte_offset += len;

            if start >= end {
                continue;
            }

            if !fg_is_default || !bg_is_default {
                let mut owo_style = Style::new().truecolor(fg.r, fg.g, fg.b);
                if !bg_is_default {
                    owo_style = owo_style.on_truecolor(bg.r, bg.g, bg.b);
                }
                spans.push((start..end, owo_style));
            }
        }

        spans
    }

    /// Return a sorted list of available theme names from the default theme set.
    pub fn available_themes() -> Vec<String> {
        let mut names: Vec<String> = THEME_SET.themes.keys().cloned().collect();
        names.sort();
        names
    }

    /// Return a sorted list of available syntax names and their file extensions.
    pub fn available_syntaxes() -> Vec<(String, Vec<String>)> {
        let ss = &*SYNTAX_SET;
        let mut result: Vec<(String, Vec<String>)> = ss
            .syntaxes()
            .iter()
            .filter(|s| !s.file_extensions.is_empty())
            .map(|s| (s.name.clone(), s.file_extensions.clone()))
            .collect();
        result.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_file_known_extension() {
        let h = SyntaxHighlighter::for_file(Path::new("test.rs"), None, None);
        assert!(h.is_some(), "Expected syntect to recognize .rs files");
    }

    #[test]
    fn for_file_unknown_extension() {
        let h = SyntaxHighlighter::for_file(Path::new("test.zzz_nonexistent"), None, None);
        assert!(h.is_none());
    }

    #[test]
    fn for_file_toml_via_two_face() {
        let h = SyntaxHighlighter::for_file(Path::new("Cargo.toml"), None, None);
        assert!(h.is_some(), "Expected two-face to provide TOML syntax");
    }

    #[test]
    fn for_file_makefile_by_filename() {
        let h = SyntaxHighlighter::for_file(Path::new("Makefile"), None, None);
        assert!(h.is_some(), "Expected syntect to recognize Makefile by filename");
    }

    #[test]
    fn for_file_dockerfile_by_filename() {
        let h = SyntaxHighlighter::for_file(Path::new("Dockerfile"), None, None);
        assert!(h.is_some(), "Expected two-face to recognize Dockerfile");
    }

    #[test]
    fn for_file_custom_syntax_map_extension() {
        let mut map = HashMap::new();
        map.insert("just".to_string(), "Makefile".to_string());
        let h = SyntaxHighlighter::for_file(Path::new("build.just"), None, Some(&map));
        assert!(h.is_some(), "Custom extension mapping should resolve");
    }

    #[test]
    fn for_file_custom_syntax_map_filename() {
        let mut map = HashMap::new();
        map.insert("myweirdfile".to_string(), "Makefile".to_string());
        let h = SyntaxHighlighter::for_file(Path::new("myweirdfile"), None, Some(&map));
        assert!(h.is_some(), "Custom filename mapping should resolve");
    }

    #[test]
    fn for_file_justfile_builtin_mapping() {
        let h = SyntaxHighlighter::for_file(Path::new("justfile"), None, None);
        assert!(h.is_some(), "justfile should map to bash syntax via built-in map");
    }

    #[test]
    fn highlight_line_produces_spans_for_rust() {
        let mut h = SyntaxHighlighter::for_file(Path::new("test.rs"), None, None).unwrap();
        let spans = h.highlight_line("fn main() { let x = 42; }");
        assert!(!spans.is_empty(), "Expected non-empty spans for Rust code");
    }

    #[test]
    fn highlight_line_empty_input() {
        let mut h = SyntaxHighlighter::for_file(Path::new("test.rs"), None, None).unwrap();
        let spans = h.highlight_line("");
        assert!(spans.is_empty() || spans.iter().all(|(r, _)| r.is_empty()));
    }

    #[test]
    fn available_themes_non_empty() {
        let themes = SyntaxHighlighter::available_themes();
        assert!(!themes.is_empty(), "Expected at least one default theme");
        assert!(themes.contains(&"base16-ocean.dark".to_string()));
        assert!(themes.contains(&"Catppuccin Mocha".to_string()));
    }

    #[test]
    fn catppuccin_mocha_is_default() {
        let h = SyntaxHighlighter::for_file(Path::new("test.rs"), None, None);
        assert!(h.is_some(), "Default theme (Catppuccin Mocha) should work");
    }

    #[test]
    fn custom_theme_name_used() {
        let h = SyntaxHighlighter::for_file(Path::new("test.rs"), Some("Solarized (dark)"), None);
        assert!(h.is_some(), "Expected Solarized (dark) theme to be available");
    }

    #[test]
    fn unknown_theme_falls_back_to_default() {
        let h = SyntaxHighlighter::for_file(Path::new("test.rs"), Some("nonexistent-theme"), None);
        assert!(h.is_some(), "Should fall back to default theme");
    }

    #[test]
    fn available_syntaxes_includes_common_languages() {
        let syntaxes = SyntaxHighlighter::available_syntaxes();
        let names: Vec<&str> = syntaxes.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"Rust"), "Expected Rust in syntaxes");
        assert!(names.contains(&"Python"), "Expected Python in syntaxes");
        assert!(names.contains(&"Go"), "Expected Go in syntaxes");
    }

    #[test]
    fn highlight_line_keeps_fence_language_across_lines() {
        // A fenced code block's body is only highlightable if the parser
        // remembers (from the opening fence line) which language it is in.
        let mut h = SyntaxHighlighter::for_file(Path::new("notes.md"), None, None).unwrap();
        h.highlight_line("```csharp");
        let spans = h.highlight_line("public class Foo { }");
        assert!(
            !spans.is_empty(),
            "code inside a ```csharp fence should be highlighted with the C# grammar"
        );
    }

    #[test]
    fn highlight_line_fence_closes_and_returns_to_markdown() {
        let mut h = SyntaxHighlighter::for_file(Path::new("notes.md"), None, None).unwrap();
        h.highlight_line("```csharp");
        h.highlight_line("public class Foo { }");
        h.highlight_line("```");
        let after_fence = h.highlight_line("# Heading");
        // Once the fence closes, a heading must tokenise exactly as it would at
        // the top of a fresh document. If we were stuck inside the C# grammar
        // the spans would differ.
        let mut fresh = SyntaxHighlighter::for_file(Path::new("notes.md"), None, None).unwrap();
        let expected = fresh.highlight_line("# Heading");
        assert_eq!(after_fence, expected);
    }

    #[test]
    fn highlight_line_single_line_comment_does_not_leak_into_next_line() {
        // With parser state carried across lines, a line comment must still
        // end at its own line and never colour the code that follows it.
        let mut h = SyntaxHighlighter::for_file(Path::new("main.rs"), None, None).unwrap();
        h.highlight_line("// a comment");
        let spans = h.highlight_line("fn main() {}");
        assert!(
            spans.len() > 1,
            "expected keyword/ident spans on the line after a comment, got {spans:?}"
        );
    }

    #[test]
    fn highlight_line_hash_comment_does_not_leak_into_next_line() {
        // Bash/Python '#' comments only end at a literal '\n'. Callers pass
        // lines without one, so the highlighter must add it or the comment
        // colour bleeds into every following line.
        let mut h = SyntaxHighlighter::for_file(Path::new("run.sh"), None, None).unwrap();
        h.highlight_line("# a comment");
        let after_comment = h.highlight_line("ls -la");
        let mut fresh = SyntaxHighlighter::for_file(Path::new("run.sh"), None, None).unwrap();
        assert_eq!(after_comment, fresh.highlight_line("ls -la"));
    }

    #[test]
    fn highlight_line_spans_stay_within_line() {
        let mut h = SyntaxHighlighter::for_file(Path::new("run.sh"), None, None).unwrap();
        let line = "# a comment";
        let spans = h.highlight_line(line);
        assert!(!spans.is_empty());
        assert!(
            spans.iter().all(|(r, _)| r.end <= line.len()),
            "span ranges must not extend past the line: {spans:?}"
        );
    }

    #[test]
    fn is_markdown_for_md_file() {
        let h = SyntaxHighlighter::for_file(Path::new("notes.md"), None, None).unwrap();
        assert!(h.is_markdown());
        let h = SyntaxHighlighter::for_file(Path::new("main.rs"), None, None).unwrap();
        assert!(!h.is_markdown());
    }
}

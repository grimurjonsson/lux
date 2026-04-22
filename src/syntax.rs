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

    fn with_syntax(
        syntax: &'static SyntaxReference,
        theme_name: Option<&str>,
    ) -> Option<Self> {
        let requested = theme_name.unwrap_or(DEFAULT_THEME);
        let theme = THEME_SET
            .themes
            .get(requested)
            .or_else(|| THEME_SET.themes.get(DEFAULT_THEME))?;

        Some(Self { syntax, theme })
    }

    /// Highlight a single line, returning byte-range spans with owo_colors styles.
    ///
    /// Creates a fresh `HighlightLines` per call because Engine.apply takes `&self`.
    /// This is acceptable for a CLI tool; the syntax/theme lookups are the expensive
    /// part and those are cached in the statics.
    ///
    /// Regions whose foreground matches the theme default foreground are skipped
    /// (they represent "no highlighting").
    pub fn highlight_line(&self, line: &str) -> Vec<(Range<usize>, Style)> {
        let mut h = HighlightLines::new(self.syntax, self.theme);
        let regions = match h.highlight_line(line, &SYNTAX_SET) {
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

            if !fg_is_default || !bg_is_default {
                let mut owo_style = Style::new().truecolor(fg.r, fg.g, fg.b);
                if !bg_is_default {
                    owo_style = owo_style.on_truecolor(bg.r, bg.g, bg.b);
                }
                spans.push((byte_offset..byte_offset + len, owo_style));
            }

            byte_offset += len;
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
        let h = SyntaxHighlighter::for_file(Path::new("test.rs"), None, None).unwrap();
        let spans = h.highlight_line("fn main() { let x = 42; }");
        assert!(!spans.is_empty(), "Expected non-empty spans for Rust code");
    }

    #[test]
    fn highlight_line_empty_input() {
        let h = SyntaxHighlighter::for_file(Path::new("test.rs"), None, None).unwrap();
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
}

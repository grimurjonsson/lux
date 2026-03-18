---
phase: quick
plan: 260318-jvh
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - src/lib.rs
  - src/syntax.rs
  - src/engine.rs
  - src/config.rs
  - src/main.rs
  - src/rules.rs
  - src/wizard.rs
autonomous: true
must_haves:
  truths:
    - "Files with syntect-recognized extensions get syntax highlighting automatically"
    - "Custom rules (-r flags, profile rules) paint over syntect base layer"
    - "Theme is configurable via config.toml with a `theme` field"
    - "Built-in hand-crafted profiles (markdown, yaml, toml, shell) are removed"
    - "The 'logs' profile is preserved as a built-in"
    - "Syntect auto-detection is the fallback when no profile matches by extension"
  artifacts:
    - path: "src/syntax.rs"
      provides: "Syntect highlighting module"
      exports: ["SyntaxHighlighter"]
    - path: "Cargo.toml"
      provides: "syntect dependency"
      contains: "syntect"
  key_links:
    - from: "src/engine.rs"
      to: "src/syntax.rs"
      via: "Engine accepts optional SyntaxHighlighter to produce base layer spans"
      pattern: "syntax.*highlight"
    - from: "src/main.rs"
      to: "src/syntax.rs"
      via: "Creates SyntaxHighlighter from file extension and passes to Engine"
      pattern: "SyntaxHighlighter"
    - from: "src/config.rs"
      to: "theme"
      via: "Config struct has optional theme field, passed to SyntaxHighlighter"
      pattern: "theme"
---

<objective>
Add syntect-based syntax highlighting as a base layer underneath custom rules, with configurable themes. Remove the hand-crafted regex profiles (markdown, yaml, toml, shell) that syntect covers better. Keep the "logs" profile.

Purpose: Professional syntax highlighting for source code files without hand-crafted regex, while preserving the rule overlay system.
Output: Working syntect integration with theme config and cleaned-up built-in profiles.
</objective>

<execution_context>
@/Users/gimmi/.claude/get-shit-done/workflows/execute-plan.md
@/Users/gimmi/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@src/engine.rs
@src/config.rs
@src/rules.rs
@src/main.rs
@src/cli.rs
@src/lib.rs
@Cargo.toml

<interfaces>
<!-- Key types and contracts the executor needs -->

From src/engine.rs:
```rust
// The Engine uses a per-byte style map where lowest priority wins.
// style_map: Vec<Option<(usize, Style)>> — None means unstyled.
// Spans are sorted by priority ascending; first writer to a byte position wins.
pub struct Engine {
    rules: Vec<Rule>,
    color_enabled: bool,
}
impl Engine {
    pub fn new(rules: Vec<Rule>, color_enabled: bool) -> Self;
    pub fn apply(&self, line: &str) -> String;
}
```

From src/config.rs:
```rust
#[derive(Deserialize, Debug, Default)]
pub struct Config {
    pub rules: Vec<RuleConfig>,
    pub profiles: HashMap<String, ProfileConfig>,
    pub default_profile: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ProfileConfig {
    pub rules: Vec<RuleConfig>,
    pub trigger: Vec<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub lines: Option<String>,
    pub extensions: Vec<String>,
    pub filenames: Vec<String>,
}

pub fn builtin_profiles() -> HashMap<String, ProfileConfig>;
pub fn find_profile_by_extension(extension: &str, merged_profiles: &HashMap<String, ProfileConfig>) -> Option<String>;
pub fn find_profile_by_filename(filename: &str, merged_profiles: &HashMap<String, ProfileConfig>) -> Option<String>;
```

From src/rules.rs:
```rust
pub struct Rule {
    pub pattern: Regex,
    pub style: Style,
    pub scope: MatchScope,
    pub priority: usize,
}
```
</interfaces>
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add syntect dependency, create syntax.rs module, add theme config</name>
  <files>Cargo.toml, src/lib.rs, src/syntax.rs, src/config.rs</files>
  <action>
1. **Cargo.toml**: Add `syntect = { version = "5", default-features = false, features = ["default-fancy"] }` to `[dependencies]`. Per user decision: pure Rust, no C deps.

2. **src/syntax.rs**: Create a new module `syntax` that wraps syntect:
   - `pub struct SyntaxHighlighter` holding a reference to `SyntaxSet`, `Theme`, and optionally the `SyntaxReference` for the detected file type.
   - Constructor: `pub fn new(extension: &str, theme_name: Option<&str>) -> Option<Self>` — loads `SyntaxSet::load_defaults_newlines()` and `ThemeSet::load_defaults()`. Looks up the syntax by extension via `ss.find_syntax_by_extension(extension)`. Returns `None` if no syntax found. Uses the provided theme name (falling back to "base16-ocean.dark" or similar reasonable default from the default set).
   - `pub fn highlight_line(&self, line: &str) -> Vec<(std::ops::Range<usize>, owo_colors::Style)>` — Uses `syntect::easy::HighlightLines` to highlight a single line. Converts each `(syntect::highlighting::Style, &str)` region into a byte range + `owo_colors::Style`. The conversion maps `syntect::highlighting::Color { r, g, b, a }` to `owo_colors::Style::new().truecolor(r, g, b)`. If the syntect color is the default foreground of the theme (i.e., essentially "no color"), skip that region (return no span for it). Also handle the background color from syntect style — if it differs from the theme default background, apply it via `.on_truecolor(r, g, b)`.
   - `pub fn available_themes() -> Vec<String>` — returns sorted list of theme names from `ThemeSet::load_defaults()`.
   - IMPORTANT: `SyntaxSet` and `ThemeSet` are expensive to load. Use `std::sync::LazyLock` statics for both so they are loaded once per process.
   - IMPORTANT: `HighlightLines` is stateful (tracks multi-line highlighting state). For ctail's line-at-a-time processing, create a fresh `HighlightLines` per call to `highlight_line` OR store it mutably. Since Engine.apply takes `&self`, the simplest approach: `SyntaxHighlighter` stores the syntax reference and theme, and `highlight_line` creates a temporary `HighlightLines` each call. This is fine for a CLI tool.

3. **src/config.rs**:
   - Add `pub theme: Option<String>` field to `Config` struct (with `#[serde(default)]`).
   - This allows `theme = "Solarized (dark)"` in config.toml top-level.

4. **src/lib.rs**: Add `pub mod syntax;` to the module list.

Do NOT touch engine.rs or main.rs yet — that's Task 2.
  </action>
  <verify>
    <automated>cd /Users/gimmi/Documents/Sources/rust/ctail && cargo test --lib 2>&1 | tail -20</automated>
  </verify>
  <done>
    - `src/syntax.rs` exists with SyntaxHighlighter struct and highlight_line method
    - `Config` has a `theme` field
    - `cargo test --lib` passes (existing tests unbroken)
    - `cargo check` succeeds with syntect compiled in
  </done>
</task>

<task type="auto">
  <name>Task 2: Integrate syntect base layer into Engine and wire up in main.rs</name>
  <files>src/engine.rs, src/main.rs</files>
  <action>
1. **src/engine.rs**: Modify the Engine to optionally accept a `SyntaxHighlighter`:
   - Add `syntax: Option<crate::syntax::SyntaxHighlighter>` field to `Engine`.
   - Update constructor: `pub fn new(rules: Vec<Rule>, color_enabled: bool, syntax: Option<crate::syntax::SyntaxHighlighter>) -> Self`.
   - In `apply()`, after collecting spans from rules, if `self.syntax` is Some, call `self.syntax.as_ref().unwrap().highlight_line(line)` to get syntect base-layer regions. Convert each region into a `Span` with `priority: usize::MAX` (lowest priority — any rule span wins over syntect). Then proceed with the normal style_map merge. Since priority is `usize::MAX`, syntect spans only fill bytes that no rule claimed.
   - The key insight: syntect fills the style_map LAST (highest priority number = lowest visual priority). The existing `style_map[pos].is_none()` check means rule spans (which have lower priority numbers and are processed first due to sort) always win.
   - Actually, looking at the existing code more carefully: spans are sorted by priority ascending and the first writer wins (`is_none()` check). So syntect spans with `usize::MAX` will be sorted LAST and only fill unoccupied bytes. This is exactly the desired behavior.

2. **src/main.rs**: Wire up syntect:
   - After determining `active_profile_name` and before building rules, detect whether to use syntect:
     - If the file has an extension, try `crate::syntax::SyntaxHighlighter::new(ext, theme_name)` where `theme_name` comes from `config.as_ref().and_then(|(c, _)| c.theme.as_deref())`.
     - Create the Engine with the optional SyntaxHighlighter: `Engine::new(rules, color_mode.color_enabled(), syntax_highlighter)`.
   - For stdin mode (no file), do NOT use syntect (no extension to detect from). Pass `None`.
   - The auto-detection flow becomes: (1) check for explicit --profile, (2) check extension against profile extensions, (3) if no profile matched, check if syntect recognizes the extension. Steps 1-2 are existing logic. Step 3 is the new syntect fallback. Syntect works ALONGSIDE profiles — if a profile is active, its rules overlay on top of syntect. If no profile matches but syntect recognizes the extension, syntect provides base highlighting with just the default log-level rules on top.

3. Update ALL existing `Engine::new(rules, color_enabled)` call sites to `Engine::new(rules, color_enabled, None)` for now. There are call sites in:
   - `src/main.rs` (the primary one — this gets the real SyntaxHighlighter)
   - `src/engine.rs` tests (pass `None`)
   - Any integration tests

Do NOT use `highlight_line` with `&mut self`. Keep it `&self` by creating a fresh HighlightLines internally each call.
  </action>
  <verify>
    <automated>cd /Users/gimmi/Documents/Sources/rust/ctail && cargo test 2>&1 | tail -20</automated>
  </verify>
  <done>
    - Engine accepts optional SyntaxHighlighter
    - `ctail src/main.rs` shows syntax-highlighted Rust code
    - `echo "hello" | ctail` still works (no syntect for stdin)
    - `ctail -r 'fn:yellow' src/main.rs` shows yellow `fn` keywords painted over syntect base
    - All existing tests pass
  </done>
</task>

<task type="auto">
  <name>Task 3: Remove hand-crafted profiles (keep logs), remove filenames field, update auto-detect</name>
  <files>src/config.rs, src/main.rs, src/wizard.rs</files>
  <action>
1. **src/config.rs — `builtin_profiles()`**: Remove the `"markdown"`, `"yaml"`, `"toml"`, and `"shell"` profile entries from `builtin_profiles()`. Keep ONLY `"logs"`. Per user decision: syntect covers these languages better than hand-crafted regex.

2. **src/config.rs — `ProfileConfig`**: Remove the `filenames: Vec<String>` field entirely. Per user decision: filenames field should be removed along with `find_profile_by_filename()` since the hand-crafted profiles that used it are being removed. Update the `Deserialize` impl — since it's derived, just remove the field. Add `#[serde(default)]` is not needed since the field is gone. BUT: to avoid breaking existing user config files that might have `filenames = [...]`, keep the field with `#[serde(default)]` but just ignore it. Actually — removing it will cause a deserialization error if someone has it in their config. Better approach: keep the field in the struct with `#[serde(default)]` for backward compat but remove `find_profile_by_filename()` and all call sites. OR use `#[serde(deny_unknown_fields)]` is NOT set (it's not), so unknown fields in TOML tables are silently ignored by default with serde. Wait — actually it's the opposite: by default serde Deserialize IGNORES unknown fields. So removing the field from the struct means TOML files with `filenames = [...]` will just have that field silently ignored during deserialization. This is fine. Remove the field.

3. **src/config.rs**: Remove `find_profile_by_filename()` function entirely.

4. **src/config.rs — `print_profile_entry()`**: Remove the block that prints filenames in the profile listing (the `if !profile.filenames.is_empty()` section). Update it to only show extensions.

5. **src/main.rs**: Remove the `.or_else()` call to `find_profile_by_filename()` in the auto-detection chain. The chain becomes: explicit --profile > extension auto-select (against profile extensions) > default_profile > None. Syntect handles the "no profile but known extension" case via the SyntaxHighlighter created in Task 2.

6. **src/config.rs tests**: Update/remove tests that reference the removed profiles or filenames:
   - Remove `builtin_profiles_contains_shell` test
   - Remove `find_profile_by_extension_sh` test
   - Remove all `find_profile_by_filename_*` tests
   - Update `print_profiles_no_config_file` to check for "logs" instead of "markdown"
   - Update `builtin_profiles_contains_markdown` — remove it or replace with `builtin_profiles_contains_logs`
   - Update `user_profile_overrides_builtin` — change the test to override "logs" instead of "markdown"
   - Remove `find_profile_by_extension_md` and `find_profile_by_extension_markdown` tests
   - Keep `find_profile_by_extension_unknown` test (still valid)
   - Keep `find_profile_by_extension` function itself — it still works for user-defined profiles and the "logs" built-in

7. **src/wizard.rs**: If `wizard.rs` references `filenames` in ProfileConfig construction, remove those references. Check for any `filenames:` field initialization and remove.

8. **src/config.rs — `print_profile_entry()`**: The auto line now only shows extensions. Update the condition: `if !profile.extensions.is_empty()` (remove the `|| !profile.filenames.is_empty()` part). Remove `parts.extend(profile.filenames.iter().cloned())`.
  </action>
  <verify>
    <automated>cd /Users/gimmi/Documents/Sources/rust/ctail && cargo test 2>&1 | tail -30</automated>
  </verify>
  <done>
    - `builtin_profiles()` returns only "logs"
    - `filenames` field removed from ProfileConfig
    - `find_profile_by_filename()` removed
    - All tests pass
    - `ctail --list-profiles` shows "logs" as the only built-in
    - `ctail src/main.rs` uses syntect (not a removed profile)
    - `ctail -p logs -f app.log` still works
  </done>
</task>

</tasks>

<verification>
- `cargo test` — all tests pass
- `cargo build` — compiles without warnings
- `ctail src/main.rs` — shows syntax-highlighted Rust code via syntect
- `ctail -r 'fn:bold+yellow' src/main.rs` — "fn" keywords show as bold+yellow, rest is syntect base
- `echo "hello world" | ctail` — works without syntect (plain stdin)
- `ctail --list-profiles` — shows only "logs" as built-in
- `ctail Cargo.toml` — shows syntect-highlighted TOML (not the removed profile)
- Config with `theme = "Solarized (dark)"` changes the color scheme
</verification>

<success_criteria>
- Syntect highlights files with recognized extensions as a base layer
- Custom rules and profile rules overlay on top of syntect with higher priority
- Theme is configurable via `theme` in config.toml
- Only "logs" remains as a built-in profile
- All existing tests pass (with updates for removed profiles)
- No regression in stdin pipe mode
</success_criteria>

<output>
After completion, create `.planning/quick/260318-jvh-add-syntect-syntax-highlighting-with-the/260318-jvh-SUMMARY.md`
</output>

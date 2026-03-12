---
phase: quick-2
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - src/wizard.rs
  - tests/integration.rs
autonomous: true
requirements: [QUICK-2]

must_haves:
  truths:
    - "User is walked through pattern, fg color, bg color, effects, and scope step-by-step when adding a rule"
    - "User sees a live-rendered preview of their styled text before confirming the rule"
    - "Interactive rule wizard is used in both profile new and profile edit flows"
    - "User can still enter raw PATTERN:STYLE:SCOPE format as a shortcut"
  artifacts:
    - path: "src/wizard.rs"
      provides: "Interactive rule builder function ask_rule_interactive"
      contains: "fn ask_rule_interactive"
  key_links:
    - from: "src/wizard.rs ask_rule_interactive"
      to: "src/color.rs parse_style"
      via: "style validation and preview rendering"
      pattern: "parse_style"
    - from: "src/wizard.rs run_wizard"
      to: "src/wizard.rs ask_rule_interactive"
      via: "rule collection loop calls interactive builder"
      pattern: "ask_rule_interactive"
---

<objective>
Replace the raw text-based rule entry in the profile wizard with an interactive step-by-step flow that guides users through pattern, foreground color, background color, text effects, and scope selection, with a live-styled preview before confirmation.

Purpose: Make rule creation discoverable and error-proof -- users see available options and preview results instead of memorizing PATTERN:STYLE:SCOPE syntax.
Output: Updated src/wizard.rs with interactive rule builder used in both new and edit profile flows.
</objective>

<execution_context>
@/Users/gimmi/.claude/get-shit-done/workflows/execute-plan.md
@/Users/gimmi/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@src/wizard.rs
@src/color.rs
@src/rules.rs
@src/config.rs

<interfaces>
From src/color.rs:
```rust
pub fn parse_style(spec: &str) -> Result<Style>;
// Named ANSI: red, green, blue, yellow, cyan, magenta, white, black
// Bright: bright-red, bright-green, etc.
// Extended: orange, pink, grey/gray, purple, teal, lime, brown, olive, coral, lavender
// Hex: #rrggbb, 0xrrggbb
// 256-color: 0-255
// Background: bg:COLOR
// Effects: bold, dim, italic, underline
// Combined: bold+red+bg:white
```

From src/rules.rs:
```rust
pub fn parse_rule(input: &str, priority: usize) -> Result<Rule>;
pub fn parse_scope(s: &str) -> Option<MatchScope>;
pub enum MatchScope { Line, Match, Capture(usize) }
```

From src/config.rs:
```rust
pub struct RuleConfig { pub pattern: String, pub style: String, pub scope: String }
```

From src/wizard.rs:
```rust
fn ask(reader: &mut impl BufRead, out: &mut impl Write, prompt: &str, default: Option<&str>) -> Result<String>;
fn build_toml(name: &str, triggers: &[String], before: &Option<String>, after: &Option<String>, lines: &Option<String>, rules: &[(String, String, String)]) -> String;
```
</interfaces>
</context>

<tasks>

<task type="auto">
  <name>Task 1: Build interactive rule wizard function</name>
  <files>src/wizard.rs</files>
  <action>
Create a new function `ask_rule_interactive` in src/wizard.rs that walks the user through building a coloring rule step-by-step. Signature:

```rust
fn ask_rule_interactive(
    reader: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<Option<(String, String, String)>>
```

Returns `Ok(None)` if the user enters empty pattern (skip/finish signal), or `Ok(Some((pattern, style_spec, scope)))`.

The flow:

1. **Pattern**: Ask "Regex pattern (empty to finish)". Validate with `regex::Regex::new()`. On invalid, print error and re-ask. On empty, return `Ok(None)`.

2. **Foreground color**: Show a numbered menu of common colors in two columns. Use `owo_colors` to render each color name IN that color so the user can see what they look like. The menu:
   ```
   Foreground color (number, name, hex, or empty to skip):
     1. red         2. green       3. blue        4. yellow
     5. cyan        6. magenta     7. white       8. orange
     9. pink       10. purple     11. teal        12. lime
   ```
   Each color name should be rendered styled (e.g., "red" displayed in red). Accept: number (1-12), any valid color name, hex (#rrggbb), 256-color number, or empty to skip. Validate with `crate::color::parse_style()`.

3. **Background color**: Ask "Background color (empty to skip):" with same numbered menu but labeled "Background". If user picks one, prefix with "bg:" internally.

4. **Text effects**: Show "Effects (comma-separated, empty to skip):" with options: `1. bold  2. dim  3. italic  4. underline`. Accept numbers (e.g., "1,3") or names (e.g., "bold,italic") or a mix. Multiple selections allowed.

5. **Scope**: Ask "Scope:" with options `1. line (whole line)  2. match (only match)` defaulting to "line" (press Enter). Accept number or name.

6. **Build style string**: Combine fg + bg + effects with "+" separator (e.g., "bold+red+bg:blue"). Use `crate::color::parse_style()` to validate the combined style.

7. **Live preview**: Show "Preview:" with a sample line like `2024-01-15 ERROR: Connection failed` where the pattern match (or whole line) is rendered using `owo_colors::OwoColorize::style()` with the parsed Style. Show the preview in the appropriate scope context -- if scope is "match", only color the matched portion; if "line", color the entire sample line.

8. **Confirm**: Ask "Keep this rule? (Y/n)" -- if "n", restart from step 1 (but pre-fill the same pattern). If "Y" or Enter, return the tuple.

Also create a helper `ask_rule_or_raw` that first asks "Add rule: (w)izard or (r)aw format? [w]" -- if "r", use the existing raw parse_rule flow; if "w" or Enter, call `ask_rule_interactive`. This provides a shortcut for power users.

Then replace the rule-collection loops in BOTH `run_wizard` (lines ~148-187) and `run_edit` (lines ~514-548, the "Add rule" loop only -- existing rules still edited via raw format with defaults). In both places, replace the "Add rule" / "Rule" prompt loop to call `ask_rule_or_raw` instead.

For the edit flow's existing-rule display (lines ~459-512), keep the current behavior (show current value as default, accept "-" to remove, accept raw format to replace). Only the "add new rule" loop at the end should use the interactive wizard.

IMPORTANT: The `ask_rule_interactive` function must accept `reader: &mut impl BufRead` and `out: &mut impl Write` (not stdin/stdout directly) to maintain testability via the existing cursor-based test pattern.
  </action>
  <verify>
    <automated>cd /Users/gimmi/Documents/Sources/rust/ctail && cargo build 2>&1</automated>
  </verify>
  <done>Interactive rule wizard compiles, is called from both run_wizard and run_edit "add rule" loops, renders color menu with actual colors, shows live preview, allows raw format shortcut</done>
</task>

<task type="auto">
  <name>Task 2: Add tests for interactive rule wizard</name>
  <files>src/wizard.rs</files>
  <action>
Add tests to the existing `mod tests` block in src/wizard.rs:

1. `test_ask_rule_interactive_empty_skips` -- empty input returns None
2. `test_ask_rule_interactive_basic_flow` -- simulate: pattern "ERROR", pick color "1" (red), empty bg, empty effects, default scope, confirm "Y". Assert returns Some(("ERROR", "red", "line")).
3. `test_ask_rule_interactive_full_style` -- pattern "WARN", color "yellow", bg "blue", effects "1,3" (bold+italic), scope "2" (match), confirm. Assert returns Some(("WARN", "bold+italic+yellow+bg:blue", "match")).
4. `test_ask_rule_interactive_invalid_regex_retries` -- provide invalid regex "(bad", then valid "ERROR", then complete the flow. Assert the output contains "invalid regex" and the result is Ok.
5. `test_ask_rule_interactive_redo` -- complete a rule, then answer "n" to keep, re-enter pattern, complete again, answer "Y". Assert returns the second rule's values.
6. `test_wizard_with_interactive_rule` -- full wizard flow using the interactive path: profile name, skip triggers, skip lines, then wizard rule entry (pattern + color + skip bg + skip effects + scope + confirm), finish rules, confirm save. Assert output contains "Saved" and the TOML preview contains the expected pattern and style.
7. `test_raw_format_shortcut` -- test that entering "r" at the wizard/raw prompt and then a raw rule like "ERROR:red:match" works correctly.

Use the same `io::Cursor` pattern as existing tests. For multi-step input, concatenate the answers with "\n".
  </action>
  <verify>
    <automated>cd /Users/gimmi/Documents/Sources/rust/ctail && cargo test wizard -- --nocapture 2>&1</automated>
  </verify>
  <done>All wizard tests pass including new interactive rule wizard tests, existing wizard tests still pass</done>
</task>

</tasks>

<verification>
```bash
cd /Users/gimmi/Documents/Sources/rust/ctail && cargo test 2>&1
cd /Users/gimmi/Documents/Sources/rust/ctail && cargo clippy 2>&1
```
</verification>

<success_criteria>
- `cargo build` succeeds with no errors
- `cargo test` passes all existing and new tests
- `cargo clippy` has no warnings
- Interactive rule wizard is reachable from both `ctail profile new` and `ctail profile edit`
- Color menu items are rendered in their actual colors
- Live preview shows the styled sample text
- Raw format shortcut ("r") still works for power users
- Existing edit-flow behavior (show defaults, accept "-" to remove) is preserved
</success_criteria>

<output>
After completion, create `.planning/quick/2-interactive-style-rule-wizard-with-color/2-SUMMARY.md`
</output>

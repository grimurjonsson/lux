# Pager (less) Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `less`-like interactive pager mode to lux for browsing files with colorized output, keyboard/mouse navigation, and a status bar.

**Architecture:** New `pager.rs` module with ratatui+crossterm TUI. The existing coloring pipeline (Engine, LineFilter, TriggerFilter) runs unchanged—ANSI output is converted to ratatui Lines via `Line::from_ansi()`. Mode dispatch in `main.rs` routes to the pager based on `--less` flag or config default.

**Tech Stack:** ratatui (with `crossterm` + `ansi` features), crossterm

**Spec:** `docs/superpowers/specs/2026-03-19-pager-less-mode-design.md`

**Spec deviation:** The spec defines `pager::run()` as reading the file internally. This plan reads the file in `main.rs` and passes lines to the pager. This is better for testability and consistency with how cat/follow modes already work—they read lines in `main.rs` too.

---

### File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `Cargo.toml` | Modify | Add ratatui + crossterm deps |
| `src/cli.rs` | Modify | Add `--less`, `--cat` flags + `Config` subcommand + `ConfigAction` enum |
| `src/config.rs` | Modify | Add `default_file_mode` field to `Config` struct |
| `src/engine.rs` | Modify | Add `rule_count()` method |
| `src/main.rs` | Modify | Mode resolution logic, pager dispatch, help guard, config subcommand handler |
| `src/pager.rs` | Create | Pager module: terminal setup, event loop, rendering, drop guard |
| `src/lib.rs` | Modify | Register `pub mod pager;` |
| `tests/integration.rs` | Modify | Tests for `--cat`, flag conflicts, config subcommand |

---

### Task 1: Add ratatui and crossterm dependencies

**Files:**
- Modify: `Cargo.toml:7-19`

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add after the existing `toml` dependency (line 19):

```toml
ratatui = { version = "0.29", features = ["crossterm", "ansi"] }
crossterm = "0.28"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles successfully with new dependencies

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat(pager): add ratatui and crossterm dependencies"
```

---

### Task 2: Add `--less` and `--cat` CLI flags

**Files:**
- Modify: `src/cli.rs:7-85` (Cli struct)
- Modify: `src/cli.rs:167-419` (tests)

- [ ] **Step 1: Write failing tests for new flags**

Add to the `#[cfg(test)] mod tests` in `src/cli.rs`:

```rust
#[test]
fn test_less_flag() {
    let cli = Cli::try_parse_from(["lux", "--less", "app.log"]).unwrap();
    assert!(cli.less);
    assert_eq!(cli.file.as_deref(), Some("app.log"));
}

#[test]
fn test_cat_flag() {
    let cli = Cli::try_parse_from(["lux", "--cat", "app.log"]).unwrap();
    assert!(cli.cat);
    assert_eq!(cli.file.as_deref(), Some("app.log"));
}

#[test]
fn test_less_conflicts_with_follow_descriptor() {
    let result = Cli::try_parse_from(["lux", "--less", "-f", "app.log"]);
    assert!(result.is_err());
}

#[test]
fn test_less_conflicts_with_follow_name() {
    let result = Cli::try_parse_from(["lux", "--less", "-F", "app.log"]);
    assert!(result.is_err());
}

#[test]
fn test_less_conflicts_with_cat() {
    let result = Cli::try_parse_from(["lux", "--less", "--cat", "app.log"]);
    assert!(result.is_err());
}

#[test]
fn test_cat_conflicts_with_follow_descriptor() {
    let result = Cli::try_parse_from(["lux", "--cat", "-f", "app.log"]);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib cli::tests`
Expected: FAIL — `less` and `cat` fields don't exist on `Cli`

- [ ] **Step 3: Add the flags to Cli struct**

In `src/cli.rs`, add these fields to the `Cli` struct after the `follow_name` field (after line 53):

```rust
    /// Open file in interactive pager mode (like less)
    #[arg(long, conflicts_with_all = ["follow_descriptor", "follow_name", "cat"])]
    pub less: bool,

    /// Print file and exit (non-interactive, overrides default pager mode)
    #[arg(long, conflicts_with_all = ["follow_descriptor", "follow_name", "less"])]
    pub cat: bool,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib cli::tests`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs
git commit -m "feat(pager): add --less and --cat CLI flags with conflict enforcement"
```

---

### Task 3: Add `default_file_mode` to Config and `config` subcommand

**Files:**
- Modify: `src/config.rs:46-67` (Config struct)
- Modify: `src/cli.rs:98-112` (Command enum)
- Modify: `src/main.rs:24-67` (subcommand handling)
- Modify: `src/config.rs:548+` (tests)
- Modify: `src/cli.rs:167+` (tests)

- [ ] **Step 1: Write failing test for Config deserialization**

Add to `src/config.rs` tests:

```rust
#[test]
fn config_default_file_mode_deserialized() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    std::fs::write(&config_path, "default_file_mode = \"less\"\n").unwrap();

    let (config, _) = load_config(Some(&config_path)).unwrap().unwrap();
    assert_eq!(config.default_file_mode.as_deref(), Some("less"));
}

#[test]
fn config_default_file_mode_none_when_omitted() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    std::fs::write(&config_path, "[[rules]]\npattern = \"x\"\nstyle = \"red\"\n").unwrap();

    let (config, _) = load_config(Some(&config_path)).unwrap().unwrap();
    assert!(config.default_file_mode.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib config::tests::config_default_file_mode`
Expected: FAIL — field doesn't exist

- [ ] **Step 3: Add `default_file_mode` field to Config struct**

In `src/config.rs`, add to the `Config` struct after `update_mode` (after line 66):

```rust
    /// Default file mode: "less" (pager) or "cat" (print-and-exit). None = "less".
    #[serde(default)]
    pub default_file_mode: Option<String>,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib config::tests::config_default_file_mode`
Expected: PASS

- [ ] **Step 5: Write test for `config` CLI subcommand parsing**

Add to `src/cli.rs` tests:

```rust
#[test]
fn test_config_default_file_mode_subcommand() {
    let cli = Cli::try_parse_from(["lux", "config", "default-file-mode", "less"]).unwrap();
    match cli.command {
        Some(Command::Config { action: ConfigAction::DefaultFileMode { value } }) => {
            assert_eq!(value, "less");
        }
        _ => panic!("expected Config DefaultFileMode"),
    }
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo test --lib cli::tests::test_config_default_file_mode_subcommand`
Expected: FAIL — `Command::Config` doesn't exist

- [ ] **Step 7: Add Config subcommand to Command enum**

In `src/cli.rs`, add to the `Command` enum (after `Update` at line 111):

```rust
    /// Manage lux configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
```

Add a new enum after `ProfileAction`:

```rust
#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set the default file mode (less = pager, cat = print-and-exit)
    DefaultFileMode {
        /// Mode: "less" or "cat"
        value: String,
    },
}
```

- [ ] **Step 8: Run CLI test to verify it passes**

Run: `cargo test --lib cli::tests::test_config_default_file_mode_subcommand`
Expected: PASS

- [ ] **Step 9: Add subcommand handler in main.rs**

In `src/main.rs`, update the import on line 7 to include the new types:

```rust
use lux::cli::{Cli, Command, ConfigAction, ProfileAction, StripAnsi};
```

Add a new match arm in the subcommand handler (after the `Command::Profile` arm, before the closing `}` of the match on line 66):

```rust
            Command::Config { action } => {
                match action {
                    ConfigAction::DefaultFileMode { value } => {
                        let valid = ["less", "cat"];
                        if !valid.contains(&value.as_str()) {
                            eprintln!("lux: invalid mode '{}'. Valid options: less, cat", value);
                            std::process::exit(1);
                        }
                        config::set_config_field("default_file_mode", Some(&value))?;
                        eprintln!("Default file mode set to: {value}");
                    }
                }
                std::process::exit(0);
            }
```

Note: `set_config_field` always writes to the default XDG config path. The `--config` flag is not passed through to this subcommand, matching the behavior of a global preference setter.

- [ ] **Step 10: Verify it compiles**

Run: `cargo check`
Expected: compiles successfully

- [ ] **Step 11: Commit**

```bash
git add src/cli.rs src/config.rs src/main.rs
git commit -m "feat(pager): add default_file_mode config and 'lux config' subcommand"
```

---

### Task 4: Add mode resolution logic in main.rs

**Files:**
- Modify: `src/main.rs:70-87` (help guard)
- Modify: `src/main.rs:237-302` (file mode dispatch)
- Modify: `src/engine.rs` (add `rule_count()`)

- [ ] **Step 1: Update the help guard to include new flags**

In `src/main.rs`, update the help guard (lines 70-82) to add checks for the new flags. Add `&& !cli.less && !cli.cat` to the condition:

```rust
    if cli.file.is_none()
        && !cli.list_colors
        && !cli.list_profiles
        && !cli.list_themes
        && !cli.list_syntaxes
        && cli.rules.is_empty()
        && cli.profile.is_none()
        && cli.trigger.is_empty()
        && cli.include.is_empty()
        && cli.exclude.is_empty()
        && !cli.follow_descriptor
        && !cli.follow_name
        && !cli.less
        && !cli.cat
        && !stdin_is_pipe()
    {
```

- [ ] **Step 2: Add mode resolution and pager dispatch**

Replace the file mode detection block in `src/main.rs` (lines 237-242). The current code:

```rust
        let has_follow_flag = cli.follow_descriptor || cli.follow_name;

        // Two-mode detection:
        // 1. file + -f or -F = FOLLOW
        // 2. otherwise = PRINT AND EXIT (show last N lines)
        let is_print_and_exit = !has_follow_flag;
```

Replace with:

```rust
        let has_follow_flag = cli.follow_descriptor || cli.follow_name;

        // Mode resolution: --less > --cat > -f/-F > config default > "less"
        let use_pager = if cli.less {
            true
        } else if cli.cat || has_follow_flag {
            false
        } else {
            // Check config default_file_mode
            config.as_ref()
                .and_then(|(c, _)| c.default_file_mode.as_deref())
                .unwrap_or("less") == "less"
        };

        let is_print_and_exit = !has_follow_flag && !use_pager;
```

- [ ] **Step 3: Move `line_spec` computation to only non-pager paths**

The current code computes `line_spec` on lines 244-250 for all file modes. Since pager mode always reads the full file, move this computation to only run when NOT in pager mode. Replace lines 244-251:

Current:
```rust
        // Resolve line count: explicit CLI -n > profile lines > default
        // Print-and-exit shows the whole file by default; follow modes show last 20
        let default_lines = if is_print_and_exit { "+1" } else { "20" };
        let lines_str = cli.lines.as_deref()
            .or(profile_lines.as_deref())
            .unwrap_or(default_lines);
        let line_spec = tail::parse_line_spec(lines_str)?;
        let path = std::path::Path::new(file_path);
```

Replace with:
```rust
        let path = std::path::Path::new(file_path);
```

Then, compute `line_spec` only inside the non-pager branches. In the `else if cli.follow_descriptor` and `else if is_print_and_exit` and `else` (follow_name) branches, add at the top of each:

```rust
            let default_lines = if is_print_and_exit { "+1" } else { "20" };
            let lines_str = cli.lines.as_deref()
                .or(profile_lines.as_deref())
                .unwrap_or(default_lines);
            let line_spec = tail::parse_line_spec(lines_str)?;
```

This avoids computing unused variables in pager mode. To keep it DRY, you can instead compute it once in a closure or just keep it as-is with a comment — the key point is the pager branch bypasses `line_spec` entirely.

Alternatively, keep the existing `line_spec` computation in place and just add a comment:
```rust
        // line_spec is computed for follow/cat modes; pager mode ignores it and reads the full file.
        let default_lines = if is_print_and_exit { "+1" } else { "20" };
        let lines_str = cli.lines.as_deref()
            .or(profile_lines.as_deref())
            .unwrap_or(default_lines);
        let line_spec = tail::parse_line_spec(lines_str)?;
        let path = std::path::Path::new(file_path);
```

Either approach works; pick whichever is cleaner.

- [ ] **Step 4: Add pager dispatch branch**

Insert the pager branch between `let path = ...` (line 251) and `if cli.follow_descriptor {` (line 253). The insertion point is specifically after `path` is defined:

```rust
        if use_pager {
            let mut file = std::fs::File::open(path)
                .with_context(|| format!("cannot open '{file_path}'"))?;
            let lines = tail::read_from_line(&mut file, 1)?;
            let rule_count = engine.rule_count();
            lux::pager::run(
                path,
                &engine,
                &filter,
                &mut trigger_filter,
                active_profile_name.as_deref(),
                rule_count,
                &lines,
            )?;
        } else if cli.follow_descriptor {
```

The rest of the existing branches (`else if is_print_and_exit`, `else`) remain unchanged, just chained as `else if` after the new pager block.

- [ ] **Step 5: Add `rule_count()` method to Engine**

In `src/engine.rs`, add a public method to `impl Engine` (after the `new` method):

```rust
    /// Return the number of rules in this engine.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
```

- [ ] **Step 6: Verify it compiles (will fail until pager module exists)**

Run: `cargo check`
Expected: FAIL — `lux::pager` module doesn't exist yet. This is expected; it will pass after Task 5.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/engine.rs
git commit -m "feat(pager): add mode resolution logic and pager dispatch in main"
```

---

### Task 5: Implement the pager module

**Files:**
- Create: `src/pager.rs`
- Modify: `src/lib.rs` (add `pub mod pager;`)

This is the core of the feature. The pager:
1. Takes pre-read file lines
2. Colorizes them through the existing pipeline
3. Converts ANSI → ratatui Lines
4. Enters alternate screen with raw mode + mouse capture
5. Runs an event loop (render + handle input)
6. Cleans up on quit via a drop guard

- [ ] **Step 1: Register the pager module**

In `src/lib.rs`, add after `pub mod output;` (line 7):

```rust
pub mod pager;
```

- [ ] **Step 2: Create `src/pager.rs` with the full implementation**

Note on `Line::from_ansi()`: In ratatui 0.29 with the `ansi` feature, this returns `Line` directly (not `Option`). If the API changes, adjust accordingly — check `cargo doc --open` for the exact signature after adding the dependency.

```rust
use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Terminal;

use crate::engine::Engine;
use crate::filter::LineFilter;
use crate::trigger::{OutputDecision, TriggerFilter};

/// Drop guard that ensures terminal state is restored even on panic.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}

/// Run the interactive pager for the given file content.
///
/// `raw_lines` should contain the full file content (all lines).
/// Lines are filtered, colorized, and displayed interactively.
pub fn run(
    file_path: &Path,
    engine: &Engine,
    filter: &LineFilter,
    trigger: &mut TriggerFilter,
    profile_name: Option<&str>,
    rule_count: usize,
    raw_lines: &[String],
) -> Result<()> {
    // Apply filter + engine + trigger to produce colorized lines
    let colored_lines = colorize_lines(raw_lines, engine, filter, trigger);

    // Convert ANSI strings to ratatui Lines
    let tui_lines: Vec<Line<'static>> = colored_lines
        .iter()
        .map(|s| Line::from_ansi(s))
        .collect();

    if tui_lines.is_empty() {
        return Ok(());
    }

    // Setup terminal
    terminal::enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("failed to enter alternate screen")?;

    // Drop guard ensures cleanup even on panic
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    // Run the event loop
    let result = event_loop(
        &mut terminal,
        &tui_lines,
        file_path,
        profile_name,
        rule_count,
    );

    // Explicit cleanup (guard also handles panic case)
    drop(_guard);

    result
}

/// Colorize lines through the existing Engine/Filter/Trigger pipeline.
fn colorize_lines(
    raw_lines: &[String],
    engine: &Engine,
    filter: &LineFilter,
    trigger: &mut TriggerFilter,
) -> Vec<String> {
    let mut result = Vec::new();

    if trigger.is_active() {
        for line in raw_lines {
            if filter.is_active() && !filter.should_show(line) {
                continue;
            }
            let colored = engine.apply(line);
            match trigger.process_line(line, colored) {
                OutputDecision::Pass(s) => result.push(s),
                OutputDecision::Flush(lines) => result.extend(lines),
                OutputDecision::Suppress => {}
            }
        }
    } else {
        for line in raw_lines {
            if filter.is_active() && !filter.should_show(line) {
                continue;
            }
            result.push(engine.apply(line));
        }
    }

    result
}

/// Main event loop: render and handle input.
fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    lines: &[Line<'static>],
    file_path: &Path,
    profile_name: Option<&str>,
    rule_count: usize,
) -> Result<()> {
    let mut offset: usize = 0;
    let total_lines = lines.len();
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");

    loop {
        // Draw
        terminal.draw(|frame| {
            let area = frame.area();
            let [content_area, status_area] = Layout::vertical([
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .areas(area);

            let content_height = content_area.height as usize;

            // Clamp offset
            if total_lines > content_height {
                if offset > total_lines - content_height {
                    offset = total_lines - content_height;
                }
            } else {
                offset = 0;
            }

            // Render content
            let end = (offset + content_height).min(total_lines);
            let visible = &lines[offset..end];
            let content = Paragraph::new(visible.to_vec());
            frame.render_widget(content, content_area);

            // Render status bar
            let last_visible = end;
            let first_visible = offset + 1;
            let pct = if total_lines == 0 {
                0
            } else {
                (last_visible * 100) / total_lines
            };

            let left = match profile_name {
                Some(name) => format!(
                    " {} | profile: {} | {} rules",
                    file_name, name, rule_count
                ),
                None => format!(" {} | {} rules", file_name, rule_count),
            };
            let right = format!(
                "{}-{}/{} ({}%) q:quit Space:next b:prev ",
                first_visible, last_visible, total_lines, pct
            );

            let status_width = status_area.width as usize;
            let left_len = left.len();
            let right_len = right.len();
            let padding = if status_width > left_len + right_len {
                status_width - left_len - right_len
            } else {
                1
            };
            let status_text = format!(
                "{}{:>width$}{}",
                left,
                "",
                right,
                width = padding
            );

            let status = Paragraph::new(Line::raw(status_text)).style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
            frame.render_widget(status, status_area);
        })?;

        // Handle input — single read, match all event types
        let content_height = terminal.size()?.height.saturating_sub(1) as usize;
        match event::read()? {
            Event::Key(key) => match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) => break,
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                (KeyCode::Char(' '), _) => {
                    offset = offset.saturating_add(content_height);
                }
                (KeyCode::Char('b'), _) => {
                    offset = offset.saturating_sub(content_height);
                }
                (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
                    offset = offset.saturating_add(1);
                }
                (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
                    offset = offset.saturating_sub(1);
                }
                (KeyCode::Char('g'), _) => {
                    offset = 0;
                }
                (KeyCode::Char('G'), _) => {
                    offset = total_lines; // clamped in draw
                }
                _ => {}
            },
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => {
                    offset = offset.saturating_add(3);
                }
                MouseEventKind::ScrollUp => {
                    offset = offset.saturating_sub(3);
                }
                _ => {}
            },
            Event::Resize(_, _) => {
                // Next draw picks up new dimensions automatically
            }
            _ => {}
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles successfully. If `Line::from_ansi()` returns `Option<Line>` instead of `Line`, change the map to: `.map(|s| Line::from_ansi(s).unwrap_or_else(|| Line::raw(s)))`.

- [ ] **Step 4: Manual smoke test**

Create a test file and verify the pager works:

```bash
# Create a test file with enough lines to page
seq 1 200 | while read n; do echo "2024-03-19 $(if [ $((n % 5)) -eq 0 ]; then echo ERROR; elif [ $((n % 3)) -eq 0 ]; then echo WARN; else echo INFO; fi) Line $n: some log message here"; done > /tmp/pager-test.log

# Test pager mode (should enter interactive pager)
cargo run -- --less /tmp/pager-test.log

# Test: Space pages down, b pages up, j/k scroll, g/G jump, q quits
# Test: mouse wheel scrolls
# Test: Ctrl-C quits

# Test cat mode override
cargo run -- --cat /tmp/pager-test.log
# Should print and exit immediately

# Test default (should use pager since default is "less")
cargo run -- /tmp/pager-test.log
```

- [ ] **Step 5: Commit**

```bash
git add src/pager.rs src/lib.rs
git commit -m "feat(pager): implement interactive pager with ratatui"
```

---

### Task 6: Integration tests

**Files:**
- Modify: `tests/integration.rs`

Note: Integration tests for the pager are limited since it enters alternate screen and requires a TTY. We test the non-interactive paths and flag behavior. Important: the default behavior for bare `lux file.log` has changed to pager mode, so any existing integration tests that expect print-and-exit for bare file arguments may need `--cat` added.

- [ ] **Step 1: Check existing integration tests for bare file arguments**

Run: `grep -n 'file.*to_str\|\.arg.*\.log\|\.arg.*\.txt' tests/integration.rs`

Look for any tests that open a file without `--cat` and expect stdout output. These tests will now enter pager mode (which requires a TTY and will fail in CI). Add `--cat` to those tests to preserve the print-and-exit behavior.

- [ ] **Step 2: Add integration tests for --cat flag and conflicts**

Add to `tests/integration.rs`:

```rust
#[test]
fn cat_flag_prints_file_and_exits() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test.log");
    std::fs::write(&file, "line 1\nline 2\nline 3\n").unwrap();

    lux()
        .arg("--cat")
        .arg("--color")
        .arg("never")
        .arg(file.to_str().unwrap())
        .assert()
        .success()
        .stdout("line 1\nline 2\nline 3\n");
}

#[test]
fn less_and_cat_conflict() {
    lux()
        .arg("--less")
        .arg("--cat")
        .arg("test.log")
        .assert()
        .failure();
}

#[test]
fn less_and_follow_conflict() {
    lux()
        .arg("--less")
        .arg("-f")
        .arg("test.log")
        .assert()
        .failure();
}

#[test]
fn cat_and_follow_conflict() {
    lux()
        .arg("--cat")
        .arg("-f")
        .arg("test.log")
        .assert()
        .failure();
}

#[test]
fn config_subcommand_help() {
    lux()
        .arg("config")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("default-file-mode"));
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test --test integration`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add tests/integration.rs
git commit -m "test(pager): add integration tests for --cat, --less, flag conflicts"
```

---

### Task 7: Final verification and cleanup

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: all tests PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Verify help text includes new flags**

Run: `cargo run -- --help`
Expected: output includes `--less`, `--cat`, and `config` subcommand

- [ ] **Step 4: Verify `lux config --help`**

Run: `cargo run -- config --help`
Expected: shows `default-file-mode` subcommand

- [ ] **Step 5: End-to-end manual test with profiles and rules**

```bash
# Test with a profile
cargo run -- --less -p logs /tmp/pager-test.log
# Should show pager with log coloring

# Test with custom rules
cargo run -- --less -r 'ERROR:bold+red' /tmp/pager-test.log
# Should show pager with ERROR lines in bold red

# Test with syntax highlighting
cargo run -- --less src/main.rs
# Should show pager with Rust syntax highlighting

# Test --cat override
cargo run -- --cat src/main.rs
# Should print and exit immediately
```

- [ ] **Step 6: Commit any fixes from verification**

Only if needed.

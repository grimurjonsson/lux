# Pager (less) Mode for Lux

## Overview

Add an interactive pager mode to lux that displays files one screen at a time with `less`-style navigation. The pager reuses lux's existing coloring pipeline (rules, filters, triggers, syntax highlighting, profiles) and renders interactively via ratatui + crossterm.

**Breaking change:** This changes the default behavior when opening files. Previously `lux myfile.log` would print and exit (cat mode). Now it opens the interactive pager. Users who prefer the old behavior can run `lux config default-file-mode cat` or use `--cat`.

## Scope

- **In scope:** File-based paging, keyboard/mouse navigation, configurable default mode, status bar, alternate screen, CLI subcommand for config
- **Out of scope:** Stdin paging, in-file search, horizontal scrolling, line wrapping configuration

## Activation & Mode Resolution

### CLI Flags

| Flag | Effect |
|------|--------|
| `--less` | Force pager mode |
| `--cat` | Force print-and-exit mode |
| `-f` | Follow file by descriptor (existing) |
| `-F` | Follow file by name (existing) |

All four flags conflict with each other (enforced by clap).

### Mode Resolution Order

1. Piped stdin → always stream (no pager, same as today)
2. `--less` flag → pager
3. `--cat` flag → print-and-exit
4. `-f` / `-F` flag → follow
5. Config `default_file_mode` → `"less"` or `"cat"`
6. No config → `"less"` (pager is the default)

### Configuration

**Config file** (`~/.config/lux/config.toml`):

```toml
default_file_mode = "less"  # or "cat"
```

**CLI subcommand:**

```
lux config default-file-mode less
lux config default-file-mode cat
```

Writes the setting to the config file, creating it if needed. Same pattern as `lux profile set-default`.

**Implementation note:** A new `Command::Config` variant with a `DefaultFileMode` action needs to be added to the `Command` enum in `cli.rs`. The `Config` struct in `config.rs` needs a new `default_file_mode: Option<String>` field.

### Interaction with `-n` Flag

The `-n` flag is ignored in pager mode. The entire file is always loaded, and the pager starts at the top. (In cat mode, `-n` continues to work as before.)

## Architecture

### New Module: `pager.rs`

A single new module containing the pager state and event loop. Entry point:

```rust
pub fn run(
    file: &Path,
    engine: &Engine,
    filter: &LineFilter,
    trigger: &mut TriggerFilter,
    profile_name: Option<&str>,
    rule_count: usize,
) -> Result<()>
```

### Data Flow

1. Read all lines from the file
2. Pass lines through the existing pipeline: `LineFilter` → `Engine::apply()` → `TriggerFilter`
3. Convert ANSI-colored strings to ratatui `Line` objects via `Line::from_ansi()`
4. Enter alternate screen, enable raw mode, enable mouse capture
5. Run event loop: render visible slice + status bar, handle input
6. On quit: disable mouse capture, leave alternate screen, disable raw mode

When triggers are active (`-t`), the pager shows only the trigger context windows (same filtering as cat mode). The user scrolls through the already-filtered output.

### Integration with Existing Code

The pager reuses the same `Engine`, `LineFilter`, `TriggerFilter`, and profile/rule resolution that cat and follow modes use. The shared pipeline in `main.rs` (config loading, profile detection, rule merging) is unchanged. The only change is that after building the pipeline, `main.rs` dispatches to `pager::run()` instead of the print-and-exit path.

The no-args help guard in `main.rs` needs updating to account for `--less` and `--cat` flags (running `lux --less` without a file should show an error, not hang).

## Navigation

### Key Bindings

| Key | Action |
|-----|--------|
| `Space` | Page down |
| `b` | Page up |
| `j` / `Down Arrow` | Scroll down 1 line |
| `k` / `Up Arrow` | Scroll up 1 line |
| `g` | Go to top |
| `G` | Go to bottom |
| `q` | Quit |

### Mouse

| Input | Action |
|-------|--------|
| Scroll wheel down | Scroll down 3 lines |
| Scroll wheel up | Scroll up 3 lines |

### Terminal Resize

The pager listens for `Resize` events and updates the content area dimensions. The visible line count adjusts automatically.

## Rendering

### Screen Layout

Two-region vertical layout using ratatui's `Layout`:

- **Content area** (`Constraint::Min(1)`): fills all available space. No border, no `Block` wrapper. A bare `Paragraph` widget with the visible slice of colorized lines.
- **Status bar** (`Constraint::Length(1)`): exactly 1 row at the bottom. Background color only (no border). Contains file info and key hints.

### Line Wrapping

Lines wrap at terminal width (ratatui `Paragraph` default behavior). Note: when lines wrap, the visual line count exceeds the logical line count. Page-down/page-up scroll by logical lines. The status bar reports logical line numbers.

### ANSI Integration

Lux's `Engine::apply()` returns ANSI-escaped strings. These are converted to ratatui `Line` objects using `Line::from_ansi()`. This requires the `ansi` feature flag on the ratatui crate.

- The Engine's coloring logic is completely unchanged
- All color modes (TrueColor, 256, 16) work as-is
- Syntax highlighting layers work as-is
- Rule priority and span-based overlapping work as-is

Color mode is resolved before the pager starts. Since the pager is only active when stdout is a terminal, auto-detection works correctly. `--color never` disables coloring as expected (the Engine produces unstyled output, which the pager displays as plain text).

Lines are colorized once on load and cached as `Vec<Line>`. The draw loop renders only the visible slice.

### Status Bar

Format:

```
 filename | profile: name | N rules    L1-L2/TOTAL (P%) q:quit Space:next b:prev
```

Left side: filename, active profile name, rule count.
Right side: visible line range, total lines, scroll percentage, key hints.

Styled with a background color (dark gray) to visually separate from content. Profile name and filename use accent colors.

### Alternate Screen

The pager enters crossterm's alternate screen on start and restores the original screen on quit. The user's terminal scrollback is untouched after exiting the pager.

## Dependencies

### New

| Crate | Purpose | Features |
|-------|---------|----------|
| `ratatui` | TUI framework (rendering, layout, widgets) | `crossterm`, `ansi` |
| `crossterm` | Terminal I/O (raw mode, events, alternate screen) | — |

Both are new direct dependencies.

### Existing (unchanged)

Engine, rules, filters, triggers, syntax highlighting, config loading — all reused as-is.

## Error Handling

- If the terminal doesn't support alternate screen or raw mode, fall back to cat mode with a warning
- Ensure cleanup (leave alternate screen, disable raw mode) runs even on panic via a drop guard
- File read errors surface as `anyhow::Result` before entering the pager

## Future Extensions (not in this spec)

- Stdin paging via temp-file buffer
- In-file search (`/pattern`, `n`/`N`)
- Horizontal scrolling for long lines
- Interactive filter toggling
- Line number gutter

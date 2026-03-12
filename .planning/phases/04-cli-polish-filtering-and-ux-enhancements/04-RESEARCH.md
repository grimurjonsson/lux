# Phase 4: CLI Polish, Filtering, and UX Enhancements - Research

**Researched:** 2026-03-13
**Domain:** CLI UX patterns, line filtering, shell integration, error messages
**Confidence:** HIGH

## Summary

Phase 4 is a polish phase. All v1 requirements (INP-01 through INP-04, CLR-01 through CLR-09, CFG-01 through CFG-05) are complete, plus two quick tasks (context buffer with trigger patterns, interactive style rule wizard). The tool works end-to-end for all documented use cases. What remains is the "fit and finish" that separates a working tool from a pleasant tool.

Research identifies four concrete areas where ctail can be improved: (1) line filtering to suppress uninteresting output, (2) shell completions for discoverability, (3) better version/help output, and (4) ANSI passthrough handling for input that already contains color codes. These are drawn from studying what similar Rust CLI tools (ripgrep, bat, fd) provide and what the v2 requirements list mentions.

**Primary recommendation:** Focus on grep-style line filtering (`--include`/`--exclude`/`--invert-match`), shell completions via `clap_complete`, `--version` with useful metadata, and ANSI-stripping for clean pattern matching -- these deliver the highest UX value with moderate implementation effort.

## Standard Stack

### Core (already in project)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| clap | 4.5.x | CLI argument parsing | Already used; derive API |
| regex | 1.12.x | Pattern matching | Already used for rules/triggers |
| owo-colors | 4.3.x | Terminal coloring | Already used for output |
| anyhow | 1.0.x | Error handling | Already used throughout |

### New Dependencies for Phase 4
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| clap_complete | 4.6.x | Shell completion generation | `ctail completions <shell>` subcommand |

### No New Dependencies Needed For
| Feature | Why No Dep |
|---------|-----------|
| Line filtering (--include/--exclude) | Already have `regex` crate |
| ANSI stripping | Already have a `strip_ansi` function in `trigger.rs` |
| Version info | clap `#[command(version)]` built-in |
| Better error messages | anyhow already provides this |

## Architecture Patterns

### Recommended Scope for Phase 4

Based on the codebase analysis and similar CLI tools, here are the features that belong in this phase:

#### 1. Line Filtering (HIGH value)
Like `grep -v`, `grep --include`, `grep --exclude` but for ctail's streaming output:
- `--include PATTERN` -- only show lines matching PATTERN (like grep)
- `--exclude PATTERN` -- hide lines matching PATTERN (like grep -v)
- Multiple `--include`/`--exclude` allowed (like `-r` rules)
- Filtering happens BEFORE coloring (saves work, matches user intent)
- Works with both stdin pipe and file following modes

**Why this matters:** Users tailing verbose logs often want to suppress DEBUG/TRACE noise without removing color rules. Currently they must pipe through `grep -v`, which strips colors.

#### 2. Shell Completions (HIGH value, low effort)
- Add `ctail completions <shell>` subcommand
- Support bash, zsh, fish, powershell
- Use `clap_complete` crate with the existing clap derive setup

#### 3. Version and Help Polish (MEDIUM value, low effort)
- Add `--version` / `-V` with version from Cargo.toml
- Improve help text organization with `#[command(after_help = "...")]` for examples
- Add long help (`--help`) with usage examples vs short help (`-h`)

#### 4. ANSI Passthrough / Stripping (MEDIUM value, from v2 req INP-06)
- Strip existing ANSI codes from input before pattern matching
- Option to preserve or strip them in output
- `--strip-ansi` flag (default: auto-detect)
- Addresses v2 requirement INP-06: "Existing ANSI codes in input are stripped before matching and optionally preserved"

#### 5. NO_COLOR / FORCE_COLOR Polish (LOW effort, from v2 reqs TERM-01, TERM-03)
- NO_COLOR is already handled in `detect_color_mode`
- Add FORCE_COLOR support: force color output even when piping
- These are v2 requirements TERM-01 and TERM-03 but trivial to implement

### What NOT to Include (Out of Scope)
- Multi-file tailing (INP-05) -- too large for polish phase
- Terminal color capability detection with fallback chain (TERM-02) -- complex, separate phase
- Line numbers in output -- ctail is a colorizer, not grep; users can pipe from `nl`
- TUI/interactive mode -- explicitly out of scope in REQUIREMENTS.md

### Recommended Project Structure Changes

No new modules needed. Changes fit existing structure:

```
src/
  cli.rs        # Add --include, --exclude, --strip-ansi flags, completions subcommand
  main.rs       # Wire up filtering before engine.apply(), FORCE_COLOR support
  trigger.rs    # Extract strip_ansi to shared utility (or move to new filter.rs)
  engine.rs     # Optional: strip ANSI from input before matching
```

If filtering logic grows beyond a few lines, extract to `src/filter.rs`:
```
src/
  filter.rs     # LineFilter struct: include/exclude pattern matching
```

### Pattern: Pre-Engine Line Filter

```rust
// Filtering happens BEFORE coloring and trigger processing
pub struct LineFilter {
    include: Vec<Regex>,
    exclude: Vec<Regex>,
}

impl LineFilter {
    pub fn should_show(&self, line: &str) -> bool {
        // If includes are specified, line must match at least one
        let pass_include = self.include.is_empty()
            || self.include.iter().any(|r| r.is_match(line));
        // If excludes are specified, line must not match any
        let pass_exclude = self.exclude.iter().all(|r| !r.is_match(line));
        pass_include && pass_exclude
    }
}
```

### Pattern: Shell Completions Subcommand

```rust
use clap_complete::{generate, Shell};

#[derive(Subcommand)]
pub enum Command {
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
    // ... existing Profile subcommand
}

// In main.rs:
Command::Completions { shell } => {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "ctail", &mut std::io::stdout());
}
```

### Pattern: ANSI Stripping Before Match

The `strip_ansi` function already exists in `trigger.rs`. It should be:
1. Moved to a shared location (e.g., top-level utility or `engine.rs`)
2. Applied to input lines before regex matching when input contains ANSI codes
3. Controlled by `--strip-ansi auto|always|never` flag

```rust
// In the main loop, before engine.apply():
let raw_line = if strip_ansi_enabled {
    strip_ansi(&line)
} else {
    line.clone()
};
// Use raw_line for matching, but output the original (preserving input ANSI)
// OR strip and re-color (replacing input ANSI with ctail's colors)
```

### Anti-Patterns to Avoid
- **Don't add filtering inside Engine::apply:** Filtering is a separate concern. The engine colors; the filter decides visibility. Keep them separate.
- **Don't add filtering after coloring:** Wastes cycles coloring lines that will be suppressed.
- **Don't build custom completion generators:** Use `clap_complete` -- it handles shell syntax quirks correctly.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Shell completions | Custom completion scripts | `clap_complete` 4.6.x | Shell completion syntax is complex and differs per shell; clap_complete generates correct scripts from your clap Command definition |
| Version info | Manual version string | `clap #[command(version)]` | Automatically picks up version from Cargo.toml |
| ANSI code stripping | Complex regex | Existing `strip_ansi` in trigger.rs (or `strip-ansi-escapes` crate) | The existing char-by-char parser handles SGR sequences; for full ANSI (CSI, OSC) a crate may be needed but SGR-only is fine for log output |

## Common Pitfalls

### Pitfall 1: Filter ordering with triggers
**What goes wrong:** Filtering and trigger patterns interact. If a line matches a trigger but is excluded by `--exclude`, should the trigger fire?
**Why it happens:** Two filtering systems (trigger and include/exclude) operating on the same stream.
**How to avoid:** Define clear precedence: include/exclude runs FIRST, then trigger processes remaining lines. An excluded line never triggers. Document this.
**Warning signs:** Tests where trigger fires on excluded lines.

### Pitfall 2: ANSI stripping breaks regex captures
**What goes wrong:** ANSI escape codes in input shift byte offsets, so regex match positions on stripped text don't correspond to positions in the original text.
**Why it happens:** Stripping ANSI changes string length; capture group byte ranges become invalid for the original string.
**How to avoid:** Two approaches: (a) strip and re-color (discard original ANSI, apply ctail colors), or (b) build an offset mapping. Approach (a) is simpler and aligns with ctail's purpose. Recommend approach (a) as default.
**Warning signs:** Garbled output when input has ANSI codes and match/capture scopes are used.

### Pitfall 3: Shell completions subcommand conflicts
**What goes wrong:** Adding a `completions` subcommand changes CLI parsing behavior -- bare `ctail` or `ctail <file>` might be interpreted differently.
**Why it happens:** clap treats the first positional arg as a potential subcommand.
**How to avoid:** The current code uses `#[command(subcommand)] pub command: Option<Command>` which handles this correctly -- subcommands are only matched by name, not by position. Verify with tests.
**Warning signs:** `ctail completions.log` being parsed as the completions subcommand instead of a file argument.

### Pitfall 4: FORCE_COLOR interaction with --color
**What goes wrong:** User sets FORCE_COLOR=1 but also passes --color never. Which wins?
**Why it happens:** Multiple sources of truth for color control.
**How to avoid:** Define precedence: explicit --color flag > FORCE_COLOR env > NO_COLOR env > auto-detect. Document this.
**Warning signs:** Inconsistent behavior between env vars and flags.

### Pitfall 5: Include/exclude with trigger before-context
**What goes wrong:** Lines in the trigger before-context buffer were excluded by --exclude, so the context window has gaps.
**Why it happens:** Excluded lines never enter the trigger buffer.
**How to avoid:** This is actually correct behavior -- if a line is excluded, it shouldn't appear in context either. But document it clearly.

## Code Examples

### Adding --include/--exclude to CLI

```rust
// In cli.rs, add to Cli struct:

/// Only show lines matching PATTERN (can be repeated)
#[arg(long = "include", action = clap::ArgAction::Append)]
pub include: Vec<String>,

/// Hide lines matching PATTERN (can be repeated)
#[arg(long = "exclude", action = clap::ArgAction::Append)]
pub exclude: Vec<String>,
```

### Adding version to clap

```rust
// In cli.rs, change the derive:
#[derive(Parser)]
#[command(name = "ctail", version, about = "Colored tail - instantly readable log output")]
pub struct Cli {
    // ... existing fields
}
```

### Adding completions subcommand

```rust
// In cli.rs:
use clap_complete::Shell;

#[derive(Subcommand)]
pub enum Command {
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },
    /// Manage config profiles
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
}
```

### FORCE_COLOR support

```rust
// In output.rs, modify detect_color_mode:
ColorChoice::Auto => {
    // Explicit --color flag takes priority (handled by match arm above)

    // NO_COLOR takes priority over FORCE_COLOR
    if std::env::var_os("NO_COLOR").is_some() {
        return ColorMode::NoColor;
    }

    // FORCE_COLOR forces color even when not a terminal
    if std::env::var_os("FORCE_COLOR").is_some() {
        return ColorMode::TrueColor;
    }

    // ... rest of auto-detection
}
```

### Main loop with filtering

```rust
// In main.rs stdin pipe mode:
let filter = LineFilter::new(&cli.include, &cli.exclude)?;

for line in stdin.lines() {
    let line = line?;
    if !filter.should_show(&line) {
        continue;
    }
    // ... existing trigger/engine logic
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual completion scripts | `clap_complete` auto-generation | clap 3.x (2022+) | No maintenance burden for completions |
| `structopt` for CLI | `clap` 4.x derive | clap 4.0 (2022) | Already using current approach |
| Custom version strings | `#[command(version)]` from Cargo.toml | clap 4.x | Zero-maintenance version info |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + assert_cmd 2.2.0 + predicates 3.1.4 |
| Config file | Cargo.toml [dev-dependencies] |
| Quick run command | `cargo test` |
| Full suite command | `cargo test` |

### Phase Requirements -> Test Map

Since no formal requirement IDs are mapped yet, here are the feature-level test requirements:

| Feature | Behavior | Test Type | Automated Command | File Exists? |
|---------|----------|-----------|-------------------|-------------|
| --include | Only matching lines shown | integration | `cargo test --test integration filter_include` | Wave 0 |
| --exclude | Matching lines hidden | integration | `cargo test --test integration filter_exclude` | Wave 0 |
| --include + --exclude combo | Both applied correctly | integration | `cargo test --test integration filter_combo` | Wave 0 |
| Filter + trigger interaction | Excluded lines don't trigger | integration | `cargo test --test integration filter_trigger` | Wave 0 |
| Filter + file follow | Filtering works in follow mode | integration | `cargo test --test integration filter_follow` | Wave 0 |
| completions subcommand | Generates valid output | integration | `cargo test --test integration completions` | Wave 0 |
| --version | Shows version from Cargo.toml | integration | `cargo test --test integration version_output` | Wave 0 |
| FORCE_COLOR | Forces color when piping | unit | `cargo test output::tests::force_color` | Wave 0 |
| ANSI stripping | Patterns match through ANSI codes | unit | `cargo test engine::tests::ansi_strip` | Wave 0 |
| LineFilter unit | should_show logic | unit | `cargo test filter::tests` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src/filter.rs` -- LineFilter struct with unit tests (new module)
- [ ] Integration tests for --include/--exclude in `tests/integration.rs`
- [ ] Integration test for completions subcommand
- [ ] Integration test for --version output
- [ ] Unit test for FORCE_COLOR in `src/output.rs`

## Open Questions

1. **Should --include/--exclude use the same regex flavor as -r rules?**
   - What we know: Both use the `regex` crate, so they naturally share the same flavor.
   - What's unclear: Should include/exclude support the same PATTERN:STYLE syntax or just bare regex?
   - Recommendation: Bare regex only (no style component). These are visibility filters, not coloring rules.

2. **Should ANSI stripping be default-on or default-off?**
   - What we know: Most log output doesn't contain ANSI codes. When it does (e.g., piped from another tool with `--color=always`), pattern matching breaks.
   - What's unclear: Performance cost of checking every line for ANSI presence.
   - Recommendation: Default `auto` -- check if input contains ANSI escape and strip if found. The strip_ansi function is O(n) and log lines are short, so overhead is negligible.

3. **Should `ctail completions` be a hidden subcommand?**
   - What we know: Tools like ripgrep and fd use `--generate complete-bash` or similar. Others use `completions <shell>` as a visible subcommand.
   - Recommendation: Visible subcommand `ctail completions <shell>` -- it's standard and discoverable.

## Sources

### Primary (HIGH confidence)
- Codebase analysis: all source files in src/ and tests/integration.rs
- clap_complete docs (https://docs.rs/clap_complete/latest/clap_complete/) - completion generation API
- REQUIREMENTS.md v2 requirements - INP-06, TERM-01, TERM-03

### Secondary (MEDIUM confidence)
- ripgrep filtering patterns (https://github.com/BurntSushi/ripgrep) - -v/--invert-match, include/exclude patterns
- Rust CLI patterns blog posts (2025-2026) - shell completions, version info, error handling best practices

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - already established in project, only adding clap_complete
- Architecture: HIGH - patterns are straightforward extensions of existing code
- Pitfalls: HIGH - identified through codebase analysis of actual interaction points
- Filtering design: MEDIUM - interaction between filter/trigger/ANSI needs validation during implementation

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (stable domain, no fast-moving dependencies)

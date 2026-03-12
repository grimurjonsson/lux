# Phase 1: Core Pipeline - Research

**Researched:** 2026-03-12
**Domain:** Rust CLI streaming line colorizer (stdin pipe mode)
**Confidence:** HIGH

## Summary

Phase 1 builds the core stdin-to-stdout colorizing pipeline: read lines from stdin, apply color rules (both built-in log level defaults and user-defined regex-based rules via `-r` flags), and write styled output to stdout. The phase must handle SIGPIPE cleanly, respect `NO_COLOR`, and support the full color specification (ANSI 16, 256-color, RGB hex, named colors, and styles like bold/dim/italic/underline).

The technology stack is well-established. `clap` (derive mode) handles CLI parsing with repeatable `-r` flags. `regex` provides pattern matching with `RegexSet` for efficient multi-pattern checking. `owo-colors` provides zero-allocation color output with runtime `Style` building that supports all three color depths plus text effects. `supports-color` handles terminal capability detection and `NO_COLOR`/`FORCE_COLOR` respect. All libraries are mature, well-documented, and widely used in the Rust CLI ecosystem.

The main architectural concern is designing the span-based coloring model correctly from the start. Even though Phase 1 only requires whole-line coloring (the `line` scope), the `StyledSpan` data structure must be designed to support `match` and `capture` scopes in Phase 2 without rewrite. The STATE.md explicitly calls this out as a key decision.

**Primary recommendation:** Build the pipeline as four clean modules -- cli, rules, engine, output -- with a `Style`-based color abstraction that decouples color specification from ANSI code generation. Use `owo_colors::Style` as the runtime style representation throughout.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- Rule syntax separator is colon: `PATTERN:color` (not equals)
- Scope is optional suffix: `PATTERN:color:scope` where scope is `line`, `match`, or `cap1`/`cap2`/etc.
- Default scope when omitted: `line` (whole line colored)
- Multiple styles combined with `+`: `bold+red+underline`
- Background colors with `bg:` prefix: `red+bg:white`
- Full syntax: `ctail -r 'PATTERN:style1+style2+bg:color:scope'`
- Invalid rules cause error and exit (fail fast)
- Built-in log level rules always active (no --no-defaults flag)
- Case-insensitive matching for default log level rules
- Substring match (no word boundaries)
- Scope: whole line colored for defaults
- Default levels: FATAL/CRITICAL=bold+red, ERROR=red, WARN/WARNING=yellow, INFO=uncolored, DEBUG=grey/dim, TRACE=very dim
- Extended named color set: standard 16 ANSI + intuitive names (orange, pink, grey, purple, teal, lime)
- Hex colors: accept `#ff0000` and `0xff0000` formats
- 256-color codes: plain numeric (e.g., `196`)
- Background colors: `bg:` prefix
- Styles: bold, italic, underline, dim
- Unmatched lines: pass through unchanged by default
- Binary input: pass through without detection or warning
- SIGPIPE: exit cleanly, no panic

### Claude's Discretion
- Exact extended color name set (which extras beyond ANSI 16)
- Internal regex compilation strategy
- CLI help text and error message wording
- How to disambiguate plain number (256-color code) from a regex pattern in edge cases

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INP-01 | User can pipe stdin to ctail (`some-command \| ctail`) | BufReader<Stdin> with line-by-line reading; clap detects stdin mode when no file args given |
| INP-04 | ctail exits cleanly on broken pipe (SIGPIPE handling) | Catch `ErrorKind::BrokenPipe` on stdout writes and `std::process::exit(0)` -- stable approach on all Rust versions |
| CLR-01 | Lines matching ERROR are colored red by default | Built-in rule: case-insensitive substring match "error" -> `Style::new().red()` |
| CLR-02 | Lines matching WARN/WARNING are colored yellow by default | Built-in rule: case-insensitive substring match "warn" -> `Style::new().yellow()` |
| CLR-03 | Lines matching DEBUG are colored grey/dim by default | Built-in rule: case-insensitive substring match "debug" -> `Style::new().dimmed()` |
| CLR-04 | User can define rules with regex patterns | `regex::Regex` compilation at startup; `-r` flag parsed into Rule structs |
| CLR-07 | Full RGB/hex color support (`#ff0000`) | Parse hex strings to `owo_colors::Rgb(r, g, b)` used with `Style::new().truecolor(r, g, b)` |
| CLR-08 | ANSI 16-color and 256-color support | `owo_colors::AnsiColors` enum for 16 colors; `owo_colors::XtermColor` for 256-color index |
| CLR-09 | Text styles: bold, italic, underline, dim | `owo_colors::Style` methods: `.bold()`, `.italic()`, `.underline()`, `.dimmed()` |
| CFG-01 | User can define rules via CLI flags (`-r 'PATTERN:color'`) | clap derive with `Vec<String>` for repeatable `-r` flag; custom parser splits on colon separator |

</phase_requirements>

## Standard Stack

### Core (Phase 1 dependencies only)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| clap | 4.5.x | CLI argument parsing | Industry standard for Rust CLIs. Derive macro gives type-safe args. Handles repeatable `-r` flags as `Vec<String>`. |
| regex | 1.12.x | Pattern matching | Guaranteed linear-time matching. `RegexSet` for multi-pattern efficiency. Used by ripgrep. |
| owo-colors | 4.2.x | Color output | Zero-allocation color formatting. Runtime `Style` builder supports ANSI 16, 256, and truecolor. `DynColor` trait for runtime color selection. |
| supports-color | 3.0.x | Terminal capability detection | Detects truecolor/256/16/none. Respects `NO_COLOR` and `FORCE_COLOR` env vars automatically. |
| anyhow | 1.0.x | Error handling | Ergonomic `?` operator with `.context()` for CLI error messages. |

### Not needed in Phase 1
| Library | Phase | Reason |
|---------|-------|--------|
| serde + toml | Phase 2 | Config file parsing -- Phase 1 is CLI-only |
| notify | Phase 3 | File watching -- Phase 1 is stdin-only |
| dirs | Phase 2 | Config file paths -- Phase 1 has no config files |

### Installation (Phase 1 only)

```bash
cargo add clap --features derive
cargo add regex
cargo add owo-colors --features supports-colors
cargo add supports-color
cargo add anyhow
# Dev dependencies
cargo add --dev assert_cmd
cargo add --dev predicates
```

## Architecture Patterns

### Recommended Project Structure

```
src/
├── main.rs          # Entry point: CLI parsing, SIGPIPE setup, main loop
├── cli.rs           # clap derive struct, argument definitions
├── rules.rs         # Rule struct, rule parsing (colon syntax), built-in defaults
├── color.rs         # Color/style parsing ("red", "#ff0000", "196", "bold+red+bg:white")
├── engine.rs        # Color engine: apply rules to a line, produce styled spans
└── output.rs        # Renderer: terminal detection, Style->ANSI output, BufWriter management
```

### Pattern 1: Rule Parsing Pipeline

**What:** Parse the `-r 'PATTERN:style:scope'` string into a compiled `Rule` struct at startup.
**When:** During CLI argument processing, before the main loop.
**Example:**

```rust
// Rule struct
struct Rule {
    pattern: Regex,
    style: owo_colors::Style,
    scope: MatchScope,
    priority: usize,
}

enum MatchScope {
    Line,              // whole line gets styled
    Match,             // only regex match region (Phase 2)
    Capture(usize),    // specific capture group (Phase 2)
}

// Parsing flow:
// "ERROR:bold+red:line" -> split on ':' -> ("ERROR", "bold+red", "line")
//   -> pattern: Regex::new("(?i)ERROR")  (note: case handling per rule type)
//   -> style: parse_style("bold+red") -> Style::new().bold().red()
//   -> scope: MatchScope::Line
```

### Pattern 2: Style String Parser

**What:** Parse the user's style specification (`"bold+red+bg:white"`) into an `owo_colors::Style`.
**When:** At rule compilation time (startup).
**Example:**

```rust
fn parse_style(spec: &str) -> Result<owo_colors::Style> {
    let mut style = owo_colors::Style::new();
    for token in spec.split('+') {
        style = match token {
            // Text effects
            "bold" => style.bold(),
            "dim" => style.dimmed(),
            "italic" => style.italic(),
            "underline" => style.underline(),
            // Named ANSI colors (foreground)
            "red" => style.red(),
            "yellow" => style.yellow(),
            "green" => style.green(),
            "blue" => style.blue(),
            "cyan" => style.cyan(),
            "magenta" => style.magenta(),
            "white" => style.white(),
            "black" => style.black(),
            // Bright variants
            "bright-red" => style.bright_red(),
            // ... etc
            // Background colors
            s if s.starts_with("bg:") => parse_bg_color(style, &s[3..])?,
            // Hex colors: #ff0000 or 0xff0000
            s if s.starts_with('#') || s.starts_with("0x") => {
                let (r, g, b) = parse_hex(s)?;
                style.truecolor(r, g, b)
            }
            // 256-color numeric
            s if s.parse::<u8>().is_ok() => {
                // Use XtermColor for 256-color index
                style.color(owo_colors::XtermColor::from(s.parse::<u8>()?))
            }
            // Extended named colors (orange, pink, etc.)
            s => parse_extended_color(style, s)?,
        };
    }
    Ok(style)
}
```

### Pattern 3: owo-colors Runtime Style Building

**What:** Build `owo_colors::Style` objects at runtime using the builder pattern.
**When:** When compiling rules from user input or defaults.

The key API surface on `Style`:
```rust
use owo_colors::Style;

// Create and chain (all methods are const and return Self)
let style = Style::new()
    .bold()
    .red()           // foreground
    .on_white()      // background (named)
    .italic();

// Apply to text
let styled = style.style("some text");
println!("{}", styled);  // outputs ANSI-colored text

// For RGB colors at runtime:
let style = Style::new().truecolor(255, 0, 0);      // fg
let style = Style::new().on_truecolor(255, 0, 0);    // bg

// For runtime-determined colors (DynColor trait):
use owo_colors::{AnsiColors, XtermColor};
let style = Style::new().color(AnsiColors::Red);              // 16-color
let style = Style::new().color(XtermColor::from(196u8));      // 256-color
```

**Source:** [owo-colors Style docs](https://docs.rs/owo-colors/latest/owo_colors/struct.Style.html)

### Pattern 4: SIGPIPE Handling (Stable Rust)

**What:** Catch `BrokenPipe` errors on stdout writes and exit cleanly.
**When:** Every write to stdout in the main loop.
**Why:** Rust ignores SIGPIPE by default. `#[unix_sigpipe]` attribute is still unstable as of early 2026.

```rust
use std::io::{self, Write, BufWriter, BufRead, ErrorKind};

fn main() {
    let result = run();
    if let Err(e) = result {
        // Don't print error on broken pipe
        if e.downcast_ref::<io::Error>()
            .map_or(false, |e| e.kind() == ErrorKind::BrokenPipe) {
            std::process::exit(0);
        }
        eprintln!("ctail: {e}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    // ... setup ...
    let stdout = io::stdout().lock();
    let mut writer = BufWriter::new(stdout);
    let stdin = io::stdin().lock();

    for line in stdin.lines() {
        let line = line?;
        // ... apply rules ...
        writeln!(writer, "{}", styled_line)?;
    }
    Ok(())
}
```

**Key insight:** Wrapping the main logic in a `run()` function that returns `Result` allows catching `BrokenPipe` at the top level without panicking. The `anyhow::Error` can be downcast to check for `io::Error`.

**Source:** [Rust SIGPIPE issue #46016](https://github.com/rust-lang/rust/issues/46016)

### Pattern 5: Terminal Detection and NO_COLOR

**What:** Detect terminal color capabilities and respect `NO_COLOR` env var.
**When:** Once at startup, stored in app state.

```rust
use std::io::IsTerminal;  // stable since Rust 1.70

enum ColorMode {
    TrueColor,
    Color256,
    Color16,
    NoColor,
}

fn detect_color_mode() -> ColorMode {
    // NO_COLOR takes precedence (supports-color handles this too)
    if std::env::var("NO_COLOR").is_ok() {
        return ColorMode::NoColor;
    }

    // Not a terminal -> no color (unless forced)
    if !std::io::stdout().is_terminal() {
        return ColorMode::NoColor;
    }

    // Use supports-color for detection
    match supports_color::on(supports_color::Stream::Stdout) {
        Some(level) if level.has_16m => ColorMode::TrueColor,
        Some(level) if level.has_256 => ColorMode::Color256,
        Some(level) if level.has_basic => ColorMode::Color16,
        _ => ColorMode::NoColor,
    }
}
```

**Source:** [supports-color docs](https://docs.rs/supports-color/latest/supports_color/), [IsTerminal docs](https://doc.rust-lang.org/std/io/trait.IsTerminal.html)

### Pattern 6: Clap Derive with Repeatable Flag

**What:** Define `-r` as a repeatable flag collecting into `Vec<String>`.
**When:** CLI definition.

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "ctail", about = "Colored tail - instantly readable log output")]
struct Cli {
    /// Color rule: PATTERN:STYLE[:SCOPE]
    #[arg(short = 'r', long = "rule", action = clap::ArgAction::Append)]
    rules: Vec<String>,

    /// When to use colors: auto, always, never
    #[arg(long, default_value = "auto")]
    color: ColorChoice,
}

#[derive(Clone, clap::ValueEnum)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}
```

### Anti-Patterns to Avoid

- **Re-compiling regex per line:** Compile all patterns at startup into `Rule` structs. Never store patterns as strings.
- **Hardcoded ANSI escape codes:** Always use `owo_colors::Style` -- never write `\x1b[31m` directly. This ensures proper terminal capability handling.
- **Allocating per-line output strings:** Write directly to `BufWriter<Stdout>` using the `Styled` display impl from owo-colors.
- **Using `println!` in the hot loop:** Use `writeln!` to `BufWriter<Stdout>` instead. `println!` panics on broken pipe; `writeln!` returns `Result`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| CLI argument parsing | Custom arg parser | clap (derive) | Edge cases: Unicode, quoting, `--`, help generation |
| ANSI color codes | Manual escape sequences | owo-colors `Style` | Color depth negotiation, reset handling, correctness |
| Terminal detection | `$TERM` string matching | supports-color | Cross-platform, handles `NO_COLOR`, `FORCE_COLOR`, `COLORTERM` |
| Regex matching | Custom pattern engine | regex crate | Linear-time guarantee, Unicode support, `RegexSet` |
| Hex color parsing | Manual string slicing | `u8::from_str_radix` + validation | Off-by-one errors, format variations (`#`, `0x`, no prefix) |
| Error context | Custom error types | anyhow | `.context()` for clear CLI error messages without boilerplate |

## Common Pitfalls

### Pitfall 1: SIGPIPE Panic
**What goes wrong:** `ctail | head -5` prints "broken pipe" error and exits with non-zero code.
**Why it happens:** Rust ignores SIGPIPE by default since 2014.
**How to avoid:** Never use `println!` in the hot path. Use `writeln!` to a `BufWriter<Stdout>`, catch `BrokenPipe` at the top level, exit with code 0.
**Warning signs:** Any `println!` call in the main processing loop is a bug waiting to happen.

### Pitfall 2: Color Output When Piped
**What goes wrong:** `ctail > output.txt` writes ANSI escape codes to the file.
**Why it happens:** Not checking `is_terminal()` or `NO_COLOR`.
**How to avoid:** Check terminal and env vars at startup. When `ColorMode::NoColor`, pass lines through unmodified.
**Warning signs:** Escape sequences visible in redirected output.

### Pitfall 3: Colon Separator Ambiguity in Rule Parsing
**What goes wrong:** Regex patterns containing colons (e.g., `\d{2}:\d{2}:\d{2}`) get split incorrectly.
**Why it happens:** Naive `split(':')` breaks on colons inside the pattern.
**How to avoid:** Parse from the right: the last colon-separated token is scope (if valid), the second-to-last is style, everything before is the pattern. Or use a smarter parser that respects escaping.
**Warning signs:** Rules with time patterns (`HH:MM:SS`) fail to compile or match.

### Pitfall 4: Forgetting ANSI Reset at End of Line
**What goes wrong:** Colors "bleed" across line boundaries.
**Why it happens:** Applying a style to the whole line without resetting at the end.
**How to avoid:** Always emit a reset after each styled line. `owo_colors::Style::style()` handles this automatically (the `Styled` Display impl emits reset), but verify in tests.
**Warning signs:** Subsequent unmatched lines appear colored.

### Pitfall 5: Case-Insensitive Regex Construction
**What goes wrong:** Default log level rules don't match mixed-case input like "Error" or "WARNING".
**Why it happens:** Using `Regex::new("ERROR")` instead of `Regex::new("(?i)ERROR")`.
**How to avoid:** For built-in defaults, prepend `(?i)` to the pattern. For user-defined rules, case sensitivity is the default (user can add `(?i)` themselves).
**Warning signs:** `Error:` lines not colored but `ERROR:` lines are.

### Pitfall 6: Style Building Order
**What goes wrong:** `Style::new().red().bold()` vs `Style::new().bold().red()` -- both work, but background+foreground order matters for clarity.
**Why it happens:** `owo_colors::Style` is a builder; each call returns a new Style with that property set.
**How to avoid:** Apply styles in a consistent order in the parser. The builder pattern makes order irrelevant for correctness (each method sets an independent field), but consistent ordering aids readability.

## Code Examples

### Complete Rule Parsing Example

```rust
use anyhow::{bail, Context, Result};
use owo_colors::Style;
use regex::Regex;

struct Rule {
    pattern: Regex,
    style: Style,
    scope: MatchScope,
    priority: usize,
}

enum MatchScope {
    Line,
    Match,
    Capture(usize),
}

fn parse_rule(input: &str, priority: usize) -> Result<Rule> {
    // Parse from right to left to handle colons in patterns
    let parts: Vec<&str> = input.rsplitn(3, ':').collect();
    let (pattern_str, style_str, scope) = match parts.len() {
        1 => bail!("rule must have at least PATTERN:STYLE"),
        2 => (parts[1], parts[0], MatchScope::Line),
        3 => {
            let maybe_scope = parse_scope(parts[0]);
            match maybe_scope {
                Some(scope) => (parts[2], parts[1], scope),
                None => {
                    // Third part is not a valid scope, treat as pattern:style
                    // with the "scope" being part of the pattern
                    let combined = format!("{}:{}", parts[2], parts[1]);
                    // Hmm, this gets tricky. Simpler: try scope parse first.
                    bail!("invalid scope '{}' -- use line, match, or cap1/cap2/etc.", parts[0]);
                }
            }
        }
        _ => unreachable!(),
    };

    let pattern = Regex::new(pattern_str)
        .with_context(|| format!("invalid regex pattern: {}", pattern_str))?;
    let style = parse_style(style_str)
        .with_context(|| format!("invalid style: {}", style_str))?;

    Ok(Rule { pattern, style, scope, priority })
}

fn parse_scope(s: &str) -> Option<MatchScope> {
    match s {
        "line" => Some(MatchScope::Line),
        "match" => Some(MatchScope::Match),
        s if s.starts_with("cap") => {
            s[3..].parse::<usize>().ok().map(MatchScope::Capture)
        }
        _ => None,
    }
}
```

### Built-in Default Rules

```rust
fn default_rules() -> Vec<Rule> {
    vec![
        Rule {
            pattern: Regex::new("(?i)fatal|(?i)critical").unwrap(),
            style: Style::new().bold().red(),
            scope: MatchScope::Line,
            priority: 0,
        },
        Rule {
            pattern: Regex::new("(?i)error").unwrap(),
            style: Style::new().red(),
            scope: MatchScope::Line,
            priority: 1,
        },
        Rule {
            pattern: Regex::new("(?i)warn").unwrap(),  // matches WARN and WARNING
            style: Style::new().yellow(),
            scope: MatchScope::Line,
            priority: 2,
        },
        // INFO: uncolored -- no rule needed (pass through)
        Rule {
            pattern: Regex::new("(?i)debug").unwrap(),
            style: Style::new().dimmed(),
            scope: MatchScope::Line,
            priority: 3,
        },
        Rule {
            pattern: Regex::new("(?i)trace").unwrap(),
            style: Style::new().dimmed(),  // "very dim" -- dimmed is the closest
            scope: MatchScope::Line,
            priority: 4,
        },
    ]
}
```

### Main Loop with SIGPIPE Handling

```rust
use std::io::{self, BufRead, BufWriter, Write, ErrorKind};
use anyhow::Result;

fn run() -> Result<()> {
    let cli = Cli::parse();
    let color_mode = detect_color_mode(&cli);
    let rules = build_rules(&cli)?;

    let stdin = io::stdin().lock();
    let stdout = io::stdout().lock();
    let mut writer = BufWriter::new(stdout);

    for line_result in stdin.lines() {
        let line = line_result?;

        if matches!(color_mode, ColorMode::NoColor) {
            writeln!(writer, "{}", line)?;
            continue;
        }

        // Find first matching rule (by priority)
        if let Some(rule) = rules.iter().find(|r| r.pattern.is_match(&line)) {
            match rule.scope {
                MatchScope::Line => {
                    writeln!(writer, "{}", rule.style.style(&line))?;
                }
                _ => {
                    // Phase 2: match/capture scoping
                    writeln!(writer, "{}", rule.style.style(&line))?;
                }
            }
        } else {
            writeln!(writer, "{}", line)?;
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        if let Some(io_err) = e.downcast_ref::<io::Error>() {
            if io_err.kind() == ErrorKind::BrokenPipe {
                std::process::exit(0);
            }
        }
        eprintln!("ctail: {e:#}");
        std::process::exit(1);
    }
}
```

### Extended Color Names (Discretion Area)

Recommended extended color set beyond ANSI 16, mapped to nearest 256-color index:

| Name | XtermColor Index | Visual |
|------|-----------------|--------|
| orange | 208 | orange |
| pink | 213 | pink |
| grey / gray | 245 | medium grey |
| purple | 129 | purple |
| teal | 30 | teal |
| lime | 118 | lime green |
| brown | 130 | brown |
| olive | 142 | olive |
| coral | 209 | coral |
| lavender | 183 | lavender |

These map to `XtermColor::from(index)` and degrade gracefully to nearest ANSI 16 on limited terminals.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `colored` crate (alloc per call) | `owo-colors` (zero-alloc) | 2022+ | Better performance for high-throughput streams |
| `atty` crate for TTY detection | `std::io::IsTerminal` (stdlib) | Rust 1.70 (2023) | No external dependency needed for basic TTY check |
| Custom SIGPIPE signal handler | Catch `BrokenPipe` in `Result` | Ongoing (stable pattern) | `#[unix_sigpipe]` still unstable; explicit error handling is the stable approach |
| `termcolor` for Windows console | ANSI codes everywhere | Windows 10+ | Modern Windows supports ANSI natively; `termcolor`'s Win32 Console API path is deprecated |

## Open Questions

1. **rsplitn parsing ambiguity with colons in regex**
   - What we know: User-provided regex can contain colons (e.g., time patterns `\d{2}:\d{2}:\d{2}`)
   - What's unclear: `rsplitn(3, ':')` with max 3 parts works for the simple case but breaks if the style token itself contains a colon (unlikely given our spec)
   - Recommendation: Use `rsplitn(3, ':')` parsing from the right. The scope is always the rightmost token (if valid), style is second-from-right, and everything left of that is the pattern. This handles colons in patterns correctly because `rsplitn` stops after N-1 splits.

2. **"Very dim" for TRACE level**
   - What we know: owo-colors has `.dimmed()` but no "very dim" or double-dim
   - What's unclear: Whether terminals support stacking dim effects
   - Recommendation: Use `.dimmed()` for both DEBUG and TRACE. Alternatively, use a dark grey foreground for TRACE (`XtermColor::from(240)`) to differentiate from DEBUG's `.dimmed()`.

3. **Style builder and the `color()` method with DynColor**
   - What we know: `Style::new().color(AnsiColors::Red)` and `Style::new().color(XtermColor::from(196))` both work via the `DynColor` trait
   - What's unclear: Whether a single `Style` can mix static methods (`.bold()`) with dynamic color methods (`.color()`) seamlessly
   - Recommendation: Test this combination early. If it doesn't work, use `.truecolor(r, g, b)` for all runtime colors by converting named colors to their RGB values.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + assert_cmd + predicates |
| Config file | None -- Wave 0 must create test infrastructure |
| Quick run command | `cargo test` |
| Full suite command | `cargo test -- --include-ignored` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INP-01 | Pipe stdin to ctail | integration | `cargo test --test integration pipe_stdin` | No -- Wave 0 |
| INP-04 | Clean exit on broken pipe | integration | `cargo test --test integration sigpipe` | No -- Wave 0 |
| CLR-01 | ERROR lines colored red | unit | `cargo test rules::test_default_error` | No -- Wave 0 |
| CLR-02 | WARN lines colored yellow | unit | `cargo test rules::test_default_warn` | No -- Wave 0 |
| CLR-03 | DEBUG lines dimmed | unit | `cargo test rules::test_default_debug` | No -- Wave 0 |
| CLR-04 | User-defined regex rules | unit + integration | `cargo test rules::test_custom_rule` | No -- Wave 0 |
| CLR-07 | RGB/hex color support | unit | `cargo test color::test_hex_parsing` | No -- Wave 0 |
| CLR-08 | ANSI 16 and 256-color | unit | `cargo test color::test_ansi_colors` | No -- Wave 0 |
| CLR-09 | Bold, italic, underline, dim | unit | `cargo test color::test_styles` | No -- Wave 0 |
| CFG-01 | Define rules via -r flags | integration | `cargo test --test integration cli_rules` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test`
- **Per wave merge:** `cargo test -- --include-ignored`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/integration.rs` -- integration tests for CLI (pipe, SIGPIPE, `-r` flags)
- [ ] Unit tests in `src/color.rs` -- color/style parsing
- [ ] Unit tests in `src/rules.rs` -- rule parsing, default rules
- [ ] `Cargo.toml` dev-dependencies: `assert_cmd`, `predicates`

## Sources

### Primary (HIGH confidence)
- [owo-colors Style API](https://docs.rs/owo-colors/latest/owo_colors/struct.Style.html) -- runtime Style building, method signatures
- [owo-colors AnsiColors enum](https://docs.rs/owo-colors/latest/owo_colors/enum.AnsiColors.html) -- 17 variants (16 colors + Default)
- [owo-colors DynColor trait](https://docs.rs/owo-colors/latest/owo_colors/trait.DynColor.html) -- runtime color trait for Rgb, XtermColor, AnsiColors
- [supports-color API](https://docs.rs/supports-color/latest/supports_color/) -- `on(Stream::Stdout)` returns `Option<ColorLevel>` with `has_16m`, `has_256`, `has_basic`
- [std::io::IsTerminal](https://doc.rust-lang.org/std/io/trait.IsTerminal.html) -- stable since Rust 1.70
- [Rust SIGPIPE issue #46016](https://github.com/rust-lang/rust/issues/46016) -- BrokenPipe handling
- [clap derive tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html) -- repeatable args with Vec

### Secondary (MEDIUM confidence)
- [Rain's Rust CLI color recommendations](https://rust-cli-recommendations.sunshowers.io/managing-colors-in-rust.html) -- owo-colors recommendation
- [NO_COLOR standard](https://no-color.org/) -- env var convention

### Tertiary (LOW confidence)
- Extended color name set and XtermColor index mappings -- based on standard XTerm 256-color palette knowledge; verify exact indices during implementation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries verified on docs.rs, versions confirmed, APIs checked
- Architecture: HIGH -- follows patterns from project-level ARCHITECTURE.md research, validated against library APIs
- Pitfalls: HIGH -- SIGPIPE and NO_COLOR are well-documented issues with clear solutions; colon parsing is a design concern flagged for careful implementation

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 (stable ecosystem, 30-day validity)

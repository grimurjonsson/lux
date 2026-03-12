# Technology Stack

**Project:** ctail (Colored Tail)
**Researched:** 2026-03-12

## Recommended Stack

### Core Language

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Rust | 2024 edition | Language | Already chosen. Ideal for single-binary CLI with high-throughput stream processing. |

### CLI Framework

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| clap | 4.5.60 | Argument parsing | Industry standard for Rust CLIs. Derive macro API gives type-safe args with minimal boilerplate. Powers ripgrep, bat, fd. Handles `-f`, `-F`, `--profile`, `-r` flags cleanly. | HIGH |

### Terminal Colors

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| owo-colors | 4.2.3 | Color output | Zero-allocation, zero-cost color formatting. Supports ANSI 16, 256-color, and truecolor (RGB). Runtime color selection via `.color(Color::...)` for dynamic rule-based coloring. No-std compatible, drop-in replacement for `colored` with better performance. | HIGH |
| supports-color | 3.0.2 | Terminal capability detection | Detects whether terminal supports truecolor, 256, 16, or no color. Respects `NO_COLOR` and `FORCE_COLOR` env vars. Needed for graceful degradation per project requirements. | HIGH |

### Regex

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| regex | 1.12.3 | Pattern matching | The Rust regex crate. Guarantees linear-time matching (no catastrophic backtracking). Named capture groups for per-rule scope matching. Used by ripgrep itself. | HIGH |

### Config Parsing

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| serde | 1.0.228 | Serialization framework | De-facto standard for Rust (de)serialization. Derive macros make config struct definitions trivial. | HIGH |
| toml | 1.0.6 | TOML parser | Official TOML crate for Rust. Integrates with serde. Specified in project constraints. Handles nested rule sets and profile definitions cleanly. | HIGH |

### File Following

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| notify | 8.2.0 | Filesystem watching | Cross-platform fs event notification. Uses kqueue on macOS, inotify on Linux. Needed for `-F` (follow by name / handle rotation). Used by rust-analyzer, zed, watchexec. | HIGH |

### Platform / Paths

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| dirs | 6.0.0 | XDG / platform config paths | Returns `~/.config` on Linux, `~/Library/Application Support` on macOS. Tiny, well-maintained. Used for locating `~/.config/ctail/`. | HIGH |

### Error Handling

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| anyhow | 1.0.102 | Application error handling | Lightweight, ergonomic error handling for applications. `?` operator works everywhere with context via `.context()`. Simpler than eyre for a CLI that mostly prints errors to stderr and exits. | HIGH |

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| CLI parsing | clap (derive) | argh, lexopt | argh has smaller binary but far less ecosystem adoption. lexopt is too low-level for a tool with profiles, flags, and subcommands. |
| Colors | owo-colors | colored, termcolor, crossterm | `colored` allocates per call -- owo-colors is zero-alloc. `termcolor` targets deprecated Windows Console APIs. `crossterm` is a full terminal manipulation library -- overkill when you only need color output on a stream. |
| Error handling | anyhow | eyre + color-eyre | eyre shines when error presentation is core UX. ctail's errors are "file not found" or "bad regex" -- anyhow's `.context()` is sufficient. Less dependency weight. |
| Config paths | dirs | directories, xdg | `directories` provides more than needed (cache, data, runtime dirs). `xdg` crate is Linux-only. `dirs` is minimal and cross-platform. |
| File watching | notify | polling, inotify | `inotify` is Linux-only. Manual polling wastes CPU. `notify` abstracts platform differences and is battle-tested. |
| TOML | toml | toml_edit, config-rs | `toml_edit` preserves formatting (useful for editors, not readers). `config-rs` adds unnecessary abstraction layers when you only need TOML. |

## What NOT to Use

| Library | Why Not |
|---------|---------|
| termcolor | Targets deprecated Windows Console APIs. Use ANSI codes with `owo-colors` instead. |
| colored | Allocates a new `ColoredString` per styling call. In a high-throughput log tail, this creates unnecessary heap pressure. |
| crossterm | Full terminal manipulation (cursor, input, screen). ctail writes to stdout -- it does not need terminal control. Pulls in far more than needed. |
| tokio / async runtime | ctail reads lines and prints them. A synchronous loop with `notify` for file events is simpler, easier to debug, and avoids async complexity. The tool is I/O bound on a single stream -- async buys nothing here. |
| config-rs | Layered config abstraction supporting JSON/YAML/TOML/env. Overkill -- ctail only reads TOML. Direct `toml` + `serde` is simpler and more transparent. |

## Installation

```bash
# Core dependencies
cargo add clap --features derive
cargo add regex
cargo add serde --features derive
cargo add toml
cargo add owo-colors --features supports-colors
cargo add supports-color
cargo add notify
cargo add dirs
cargo add anyhow

# Dev dependencies (testing)
cargo add --dev assert_cmd
cargo add --dev predicates
```

## Architecture Notes for Stack

**Color application pattern:** owo-colors supports both compile-time generics (`"text".red()`) and runtime values (`"text".color(color_value)`). ctail needs runtime color selection since rules are loaded from config -- use the runtime `OwoColorize::color()` / `.on_color()` / `.style()` methods with `owo_colors::Style`.

**Regex compilation:** Compile regex patterns once at startup into a `Vec<CompiledRule>`. The `regex` crate's `Regex::new()` returns a compiled automaton -- do not re-compile per line.

**Terminal detection flow:**
1. At startup, call `supports_color::on(Stream::Stdout)` to get color support level
2. Store the level (None / Basic / Ansi256 / Truecolor) in app state
3. When applying rules, downgrade RGB colors to nearest 256 or 16 color based on detected level
4. Respect `--color=always|auto|never` CLI flag (override detection)

**File following:** For `-f` (follow fd), use a simple read loop with `BufRead` -- seek to end, read new lines as they appear. For `-F` (follow name), additionally use `notify` to detect file rename/delete/create events and re-open the file.

## Sources

- clap: [crates.io](https://crates.io/crates/clap) -- v4.5.60 verified 2026-03-12
- owo-colors: [crates.io](https://crates.io/crates/owo-colors) -- v4.2.3, [docs.rs](https://docs.rs/owo-colors)
- supports-color: [crates.io](https://crates.io/crates/supports-color) -- v3.0.2
- regex: [crates.io](https://crates.io/crates/regex) -- v1.12.3 verified 2026-03-12
- serde: [crates.io](https://crates.io/crates/serde) -- v1.0.228
- toml: [crates.io](https://crates.io/crates/toml) -- v1.0.6 verified 2026-03-12
- notify: [crates.io](https://crates.io/crates/notify) -- v8.2.0, [GitHub](https://github.com/notify-rs/notify)
- dirs: [crates.io](https://crates.io/crates/dirs) -- v6.0.0
- anyhow: [crates.io](https://crates.io/crates/anyhow) -- v1.0.102
- Color library recommendation: [Rain's Rust CLI recommendations](https://rust-cli-recommendations.sunshowers.io/managing-colors-in-rust.html)
- CLI parser recommendation: [Rain's Rust CLI recommendations](https://rust-cli-recommendations.sunshowers.io/cli-parser.html)

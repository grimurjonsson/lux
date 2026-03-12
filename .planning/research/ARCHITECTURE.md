# Architecture Patterns

**Domain:** CLI log colorizer / streaming text processor
**Researched:** 2026-03-12

## Recommended Architecture

ctail is a **streaming line processor** with a classic Unix filter architecture: read lines from an input source, transform them (apply colors), write to stdout. The key architectural insight from existing tools (grc, ccze, logalize) is that the colorizer itself is a **pure filter** -- input source selection is a separate concern from the colorization pipeline.

```
┌─────────────────┐    ┌──────────────┐    ┌─────────────────┐    ┌────────────┐
│  Input Source    │───>│  Line Reader  │───>│  Color Engine   │───>│  Renderer  │
│  (stdin / file)  │    │  (buffered)   │    │  (rule matcher) │    │  (stdout)  │
└─────────────────┘    └──────────────┘    └─────────────────┘    └────────────┘
        │                                          │
        │                                   ┌──────┴──────┐
   ┌────┴────┐                              │  Rule Set   │
   │  File   │                              │  (config +  │
   │  Watcher│                              │   CLI args) │
   └─────────┘                              └─────────────┘
```

### Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| **CLI Parser** | Parse args, flags, file paths, inline rules | Config Loader, Input Source, main |
| **Config Loader** | Read TOML config, merge with CLI rules, resolve profiles | CLI Parser, Rule Engine |
| **Rule Engine** | Compile regex patterns, define match scopes, store ordered rule list | Config Loader, Color Engine |
| **Input Source** | Abstract over stdin vs file-follow vs file-follow-by-name | CLI Parser, Line Reader |
| **File Watcher** | Monitor file for appends; handle rotation (rename/truncate) for `-F` mode | Input Source |
| **Line Reader** | Buffered line-by-line reading from input source | Input Source, Color Engine |
| **Color Engine** | Apply all matching rules to a line, produce styled output segments | Rule Engine, Renderer |
| **Renderer** | Detect terminal capabilities, emit correct ANSI escape codes, handle NO_COLOR | Color Engine, stdout |

### Data Flow

**Startup path:**
1. CLI Parser reads arguments -> determines mode (stdin pipe vs file follow) and any inline rules
2. Config Loader reads `~/.config/ctail/config.toml` (if exists), merges default rules + profile rules + CLI rules
3. Rule Engine compiles all patterns into a `RegexSet` (for fast multi-match) plus individual `Regex` objects (for capture groups)
4. Renderer detects terminal color capability (truecolor / 256 / 16 / none)

**Hot path (per line):**
1. Input Source yields a line (blocking read from stdin, or watcher-driven read from file)
2. Color Engine runs `RegexSet::matches()` for a single-pass check of which rules match
3. For each matching rule (in priority order), determine styled spans:
   - **line scope**: entire line gets that rule's style
   - **match scope**: only the matched substring gets styled
   - **capture scope**: only the named/numbered capture group gets styled
4. Merge overlapping spans (later rules win on overlap, or first-match-wins -- a design decision)
5. Renderer converts styled spans to ANSI escape sequences appropriate for detected terminal capability
6. Write completed line to stdout (with `BufWriter` for throughput)

**File rotation path (`-F` mode):**
1. File Watcher detects rename/delete of watched path via `notify` crate
2. File Watcher polls or watches for new file at same path
3. Input Source reopens file, seeks to beginning of new file
4. Line Reader continues as normal

## Component Details

### CLI Parser
Use `clap` (derive mode). Responsible for:
- `-f <file>` -- follow file by descriptor
- `-F <file>` -- follow file by name (handles rotation)
- `-r 'PATTERN=COLOR'` -- inline rule definition (repeatable)
- `--profile <name>` -- named config profile
- `--no-color` -- disable colorization
- Positional args or stdin detection via `atty`/`is-terminal`

### Config Loader
TOML-based configuration using `toml` crate with `serde` deserialization.

```
~/.config/ctail/config.toml
├── [defaults]          # default rules applied always
│   └── [[defaults.rules]]
├── [profiles.spring]   # named profile
│   └── [[profiles.spring.rules]]
└── [settings]          # global settings (theme, color mode override)
```

**Rule merge order (later wins on conflict):**
1. Built-in defaults (ERROR=red, WARN=yellow, DEBUG=dim, etc.)
2. Config file `[defaults]` section
3. Config file selected profile
4. CLI `-r` inline rules

### Rule Engine
Each rule is:
```rust
struct Rule {
    pattern: Regex,          // compiled regex
    style: Style,            // color + text attributes
    scope: MatchScope,       // Line | Match | Capture(usize)
    priority: usize,         // for overlap resolution
}
```

For performance, also maintain a `RegexSet` compiled from all rule patterns. Use `RegexSet::matches()` to determine which rules fire in a single pass, then only run individual `Regex::find()` / `Regex::captures()` for the rules that matched. This is the standard pattern recommended by the `regex` crate docs.

### Input Source
An enum-based abstraction:
```rust
enum InputSource {
    Stdin,
    FollowFd(File),          // -f: keep file descriptor open
    FollowName(PathBuf),     // -F: reopen on rotation
}
```

For stdin: just `BufReader<Stdin>`.
For file follow: use `notify` crate for filesystem events. The `poll-tail` crate pattern is also worth studying -- it uses polling for robustness against rotation. However, rolling our own on top of `notify` gives more control.

Key decision: **synchronous with blocking reads** for v1. Async (tokio) adds complexity without clear benefit for a single-file CLI tool. If multiplexing multiple files becomes a feature later, async would justify itself then.

### Color Engine
The core transform. Takes a line + rule set, produces a `Vec<StyledSpan>`:
```rust
struct StyledSpan {
    start: usize,       // byte offset in line
    end: usize,         // byte offset in line
    style: Style,       // color + attributes
}
```

Algorithm:
1. Run `RegexSet::matches(&line)` -- O(n) single pass
2. For each matched rule index, get match positions via individual `Regex`
3. Build span list, resolve overlaps (last-rule-wins or priority-based)
4. Return ordered, non-overlapping spans

### Renderer
Responsible for terminal capability detection and ANSI code generation.

```rust
enum ColorSupport {
    TrueColor,    // 24-bit: \x1b[38;2;R;G;Bm
    Color256,     // 8-bit:  \x1b[38;5;Nm
    Color16,      // Basic:  \x1b[31m etc
    NoColor,      // No escape codes
}
```

Detection order:
1. `--no-color` flag or `NO_COLOR` env var -> NoColor
2. `COLORTERM=truecolor` or `COLORTERM=24bit` -> TrueColor
3. `TERM` contains `256color` -> Color256
4. Is a TTY -> Color16
5. Not a TTY (piped output) -> NoColor (unless `--force-color`)

Use raw ANSI escape code generation rather than a styling crate -- ctail needs precise control over color downgrading (mapping RGB to nearest 256-color or 16-color). The `anstyle` crate is a good lightweight option if we want a helper.

## Patterns to Follow

### Pattern 1: RegexSet + Individual Regex Two-Pass
**What:** Use `RegexSet` for fast multi-pattern matching, then individual `Regex` only for patterns that matched.
**When:** Any time you have multiple regex patterns to check against the same input.
**Why:** `RegexSet` uses a single DFA pass. Without it, N rules means N separate regex scans per line.
**Example:**
```rust
let set = RegexSet::new(rules.iter().map(|r| r.pattern.as_str()))?;
let matches = set.matches(&line);
for idx in matches.iter() {
    let captures = rules[idx].pattern.captures(&line);
    // build styled spans from captures
}
```

### Pattern 2: Span Merge for Overlapping Matches
**What:** When multiple rules match overlapping regions of a line, resolve to non-overlapping styled spans.
**When:** Multi-rule matching with different scopes (line vs match vs capture).
**Why:** Terminal escape codes are linear -- you cannot nest arbitrary colors. Must flatten to a sequence of styled segments.
**Example approach:** Build an interval list, split overlapping intervals, assign the highest-priority style to each resulting segment.

### Pattern 3: BufWriter for Output Throughput
**What:** Wrap stdout in `BufWriter` to batch write syscalls.
**When:** Always, for the hot path.
**Why:** Line-by-line `write()` syscalls are expensive. High-throughput log streams (thousands of lines/sec) will bottleneck on syscalls without buffering. Use `BufWriter` with explicit flush on a timer or line count threshold to balance latency vs throughput.

### Pattern 4: Enum-Based Input Abstraction
**What:** Use a trait or enum to abstract input sources behind a common `Iterator<Item = String>` (or `BufRead`) interface.
**When:** When supporting multiple input modes (stdin, file follow, file follow-by-name).
**Why:** The color engine should not know or care where lines come from. Clean separation enables testing the color engine with simple string inputs.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Async for Single-File Follow
**What:** Using tokio/async-std for a simple single-file tail.
**Why bad:** Adds compile time, binary size, and conceptual complexity. A blocking `BufReader::read_line()` loop with `notify` events or a simple `sleep` poll is simpler, correct, and fast enough.
**Instead:** Use synchronous I/O. Reserve async for a future multi-file multiplexing feature.

### Anti-Pattern 2: Re-compiling Regex Per Line
**What:** Creating `Regex` objects inside the per-line processing loop.
**Why bad:** Regex compilation is expensive (milliseconds). On high-throughput streams this becomes the bottleneck.
**Instead:** Compile all patterns once at startup, store in the Rule Engine.

### Anti-Pattern 3: Allocating Strings Per Span
**What:** Building the colorized output by concatenating `String` fragments.
**Why bad:** Many small allocations per line under high throughput.
**Instead:** Write directly to a `BufWriter` or reuse a `String` buffer that gets cleared each line.

### Anti-Pattern 4: Hardcoded ANSI Codes Throughout
**What:** Scattering `\x1b[31m` literals across the codebase.
**Why bad:** Makes color fallback impossible. Cannot gracefully degrade from truecolor to 256 to 16.
**Instead:** Centralize all ANSI code generation in the Renderer. The Color Engine produces abstract `Style` values; the Renderer converts to terminal-appropriate escape sequences.

## Suggested Build Order

Based on component dependencies, the recommended implementation order is:

```
Phase 1: Core Pipeline (stdin only)
  CLI Parser (minimal) -> Line Reader (stdin) -> Color Engine (hardcoded rules) -> Renderer (truecolor only)

Phase 2: Rule System
  Rule Engine (regex patterns + scopes) -> Config Loader (TOML) -> merge with built-in defaults

Phase 3: File Following
  Input Source abstraction -> File Watcher (-f mode) -> File rotation (-F mode)

Phase 4: Polish
  Terminal capability detection + color fallback -> Named profiles -> Edge cases
```

**Rationale:** The core pipeline (read line, color it, print it) is the skeleton everything hangs on. Get stdin working with basic rules first -- this validates the Color Engine design before adding file-watching complexity. Rule system comes second because the Color Engine needs real rules to exercise properly. File following is independent of the coloring logic and can be added once the pipeline works.

## Scalability Considerations

| Concern | Normal use (< 100 lines/sec) | Heavy logging (1K-10K lines/sec) | Stress test (100K+ lines/sec) |
|---------|------|------|------|
| Regex matching | No concern | RegexSet essential | Consider compiled DFA caching, avoid captures when scope=line |
| Output buffering | Line-buffered fine | BufWriter needed | BufWriter + batch flush every N lines or M milliseconds |
| Memory | No concern | No concern (line-at-a-time) | Ensure no line accumulation; watch for very long lines |
| Startup time | No concern | No concern | Regex compilation time for large rule sets (pre-compile, lazy_static) |

## Sources

- [RegexSet documentation](https://docs.rs/regex/latest/regex/struct.RegexSet.html) - Official regex crate docs on multi-pattern matching
- [notify crate](https://crates.io/crates/notify) - Cross-platform filesystem notification for Rust
- [poll-tail crate](https://crates.io/crates/poll-tail) - Polling-based file tailer with rotation handling
- [grc (Generic Coloriser)](https://github.com/garabik/grc) - Reference architecture: filter-based design with config-driven rules
- [logalize](https://github.com/deponian/logalize) - Modern log colorizer with formats/patterns/words matching hierarchy
- [linemux](https://github.com/jmagnuson/linemux) - Async multiplexed file tailing library
- [Rain's Rust CLI recommendations on color management](https://rust-cli-recommendations.sunshowers.io/managing-colors-in-rust.html)
- [term_color_support crate](https://crates.io/crates/term_color_support) - Terminal color capability detection

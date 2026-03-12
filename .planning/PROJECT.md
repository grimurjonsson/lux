# ctail — Colored Tail

## What This Is

A Rust CLI tool that works like `tail -f`/`tail -F` with configurable, rule-based colorization of output. Users define pattern-matching rules (regex or literal) that apply colors and styles to log output, with sensible defaults for common log levels. It also reads from stdin via pipes.

## Core Value

Instantly readable log output — the right colors applied to the right patterns, with zero configuration needed for common cases and full customization when you want it.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Follow files with `-f` (open file descriptor) and `-F` (follow by name, handles rotation)
- [ ] Read from stdin when piped (e.g. `some-command | ctail`)
- [ ] Built-in default rules for common log levels: ERROR (red), WARN (yellow), INFO (normal), DEBUG (grey)
- [ ] User-defined rules via CLI flags (e.g. `ctail -r 'ERROR=red' -f app.log`)
- [ ] User-defined rules via TOML config file (~/.config/ctail/ or similar)
- [ ] Named rule profiles in config (e.g. `ctail --profile spring-boot -f app.log`)
- [ ] Per-rule match scope: color the whole line, just the matched text, or a regex capture group
- [ ] Multiple rules can match the same line — all apply (different parts get different colors)
- [ ] Full RGB color support (#ff0000 style), plus ANSI 16 and 256 as fallback
- [ ] Text styles: bold, italic, underline, dim
- [ ] Regex pattern support in rules

### Out of Scope

- Full `tail` feature parity (-n, -c, --pid, --bytes, etc.) — focus on follow and pipe modes
- GUI or TUI interface — this is a pure CLI stream processor
- Log parsing/structured output — ctail colors lines, it doesn't parse them
- Remote file tailing — local files and stdin only

## Context

- Written in Rust for performance and easy single-binary distribution
- Targets Unix-like systems primarily (macOS, Linux)
- Should handle high-throughput log streams without becoming a bottleneck
- Existing tools in this space: `ccze`, `grc`, `lnav` — ctail aims to be simpler and more composable (pipe-friendly)

## Constraints

- **Language**: Rust (2024 edition, already initialized)
- **Config format**: TOML for rule definitions and profiles
- **Terminal compatibility**: Graceful degradation — detect color support and fall back (truecolor → 256 → 16 → no color)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| TOML for config | Rust ecosystem standard, readable, good for nested rule sets | — Pending |
| All-rules-apply on multi-match | Allows rich coloring where different parts of a line get different colors | — Pending |
| Per-rule scope (line/match/capture) | Maximum flexibility without complexity for simple cases | — Pending |
| Named profiles | Lets users switch between rule sets for different log formats | — Pending |

---
*Last updated: 2026-03-12 after initialization*

# Phase 1: Core Pipeline - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Working stdin-to-stdout colorizer with built-in log level defaults, user-defined regex rules via CLI flags, and full color/style support. No config files, no file following — pure pipe-based coloring.

</domain>

<decisions>
## Implementation Decisions

### Rule syntax
- Separator is colon: `PATTERN:color` (not equals, avoids regex ambiguity)
- Scope is optional suffix: `PATTERN:color:scope` where scope is `line`, `match`, or `cap1`/`cap2`/etc.
- Default scope when omitted: `line` (whole line colored)
- Multiple styles combined with `+`: `bold+red+underline`
- Background colors with `bg:` prefix: `red+bg:white`
- Full syntax: `ctail -r 'PATTERN:style1+style2+bg:color:scope'`
- Example: `ctail -r 'ERROR:bold+red:line' -r 'status=(\d+):blue:cap1'`
- Invalid rules cause error and exit (fail fast)

### Default log level rules
- Built-in rules always active (no --no-defaults flag)
- Case-insensitive matching
- Substring match (no word boundaries — matches "Errors:" too)
- Scope: whole line colored
- Levels and colors:
  - FATAL/CRITICAL: bold + red
  - ERROR: red
  - WARN/WARNING: yellow
  - INFO: normal/uncolored
  - DEBUG: grey/dim
  - TRACE: very dim

### Color specification
- Extended named color set: standard 16 ANSI (red, bright-red, etc.) plus intuitive names (orange, pink, grey, purple, teal, lime)
- Hex colors: accept both `#ff0000` and `0xff0000` formats
- 256-color codes: plain numeric (e.g., `196` for bright red)
- Background colors: `bg:` prefix (e.g., `bg:red`, `bg:#ff0000`)
- Styles: bold, italic, underline, dim

### Edge cases
- Unmatched lines: pass through unchanged by default, configurable (flag to dim unmatched lines)
- Binary input: pass through without detection or warning
- SIGPIPE: exit cleanly, no panic

### Claude's Discretion
- Exact extended color name set (which extras beyond ANSI 16)
- Internal regex compilation strategy
- CLI help text and error message wording
- How to disambiguate plain number (256-color code) from a regex pattern in edge cases

</decisions>

<specifics>
## Specific Ideas

- "I want it like tail -f but with colors" — the primary mental model is tail, not a standalone colorizer
- Rule syntax should be memorable for quick one-offs: `ctail -r 'ERROR:red' -r 'WARN:yellow'`
- The tool should feel transparent when no rules match — just passes data through

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- None — fresh `cargo init` scaffold (hello world main.rs, empty Cargo.toml)

### Established Patterns
- Rust 2024 edition
- No dependencies yet

### Integration Points
- main.rs is the entry point, everything builds from scratch

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-core-pipeline*
*Context gathered: 2026-03-12*

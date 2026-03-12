---
phase: 03-file-following
plan: 01
subsystem: cli
tags: [clap, tail, file-io, backward-seek]

# Dependency graph
requires:
  - phase: 01-core-pipeline
    provides: Engine, BufWriter output pattern, BrokenPipe handling
  - phase: 02-advanced-rules-config
    provides: build_rules_with_config, detect_color_mode, discovery commands
provides:
  - tail module with backward-seeking last-N-lines reader
  - CLI flags for file following (-f, -F, -n, positional file arg)
  - Three-mode input branching (stdin, print-and-exit, follow stub)
  - LineSpec parsing for line number specifications
affects: [03-02-file-following]

# Tech tracking
tech-stack:
  added: [notify 8.2 (dependency only, used in Plan 02)]
  patterns: [backward-seeking 8KB chunk reader, three-mode input branching]

key-files:
  created: [src/tail.rs]
  modified: [src/cli.rs, src/main.rs, src/lib.rs, Cargo.toml]

key-decisions:
  - "lines field is Option<String> with no default -- None vs Some distinguishes follow vs print-and-exit mode"
  - "Backward-seeking algorithm uses 8KB chunks from EOF for efficient last-N-lines"
  - "notify dependency added in Plan 01 to avoid Cargo.toml merge conflicts with Plan 02"

patterns-established:
  - "Three-mode branching: stdin pipe / print-and-exit / follow based on CLI args"
  - "LineSpec enum for flexible line selection (Last(n), FromLine(n))"

requirements-completed: [INP-02, INP-03]

# Metrics
duration: 3min
completed: 2026-03-12
---

# Phase 3 Plan 01: CLI & Tail Reader Summary

**CLI flags (-f, -F, -n, positional file), backward-seeking last-N-lines reader, and three-mode input branching (stdin/print-and-exit/follow)**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-12T17:06:53Z
- **Completed:** 2026-03-12T17:10:22Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Backward-seeking last-N-lines reader using 8KB chunks from EOF, avoiding full file load
- CLI extended with -f (follow descriptor), -F (follow name), -n (line count), and positional file argument
- Three-mode input branching in main.rs: stdin pipe, print-and-exit (file + -n), follow stub (file + -f/-F or bare)
- Print-and-exit mode fully functional: `ctail -n 20 app.log` prints last 20 colored lines and exits
- stdin+file conflict detection and error reporting

## Task Commits

Each task was committed atomically:

1. **Task 1: CLI flags and tail module with last-N-lines reader** - `c8658de` (feat)
2. **Task 2: Input mode branching with three modes in main.rs** - `9147724` (feat)

_Note: Task 1 was TDD -- tests and implementation in single commit._

## Files Created/Modified
- `src/tail.rs` - New module: LineSpec enum, parse_line_spec, read_last_n_lines (backward-seeking), read_from_line
- `src/cli.rs` - Extended Cli struct with follow_descriptor, follow_name, lines (Option), file fields + 11 new tests
- `src/main.rs` - Three-mode input branching: stdin pipe, print-and-exit, follow stub
- `src/lib.rs` - Added `pub mod tail`
- `Cargo.toml` - Added notify 8.0 dependency

## Decisions Made
- lines field is Option<String> with no default_value -- distinguishing None (bare file, implies follow) from Some("10") (explicit -n, implies print-and-exit)
- Backward-seeking uses 8KB chunks reading from EOF, counting newlines to find last N lines efficiently
- notify dependency added early to avoid Cargo.toml conflicts when Plan 02 implements the follow loop

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Follow stub in main.rs ready for Plan 02 to implement the event loop
- File position left at EOF after initial line read, ready for follow continuation
- notify crate already in Cargo.toml for Plan 02 to use

---
*Phase: 03-file-following*
*Completed: 2026-03-12*

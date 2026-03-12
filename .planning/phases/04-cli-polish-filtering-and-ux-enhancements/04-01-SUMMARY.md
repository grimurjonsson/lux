---
phase: 04-cli-polish-filtering-and-ux-enhancements
plan: 01
subsystem: cli
tags: [regex, filtering, ansi, clap]

requires:
  - phase: 01-core-pipeline
    provides: Engine coloring pipeline, span-based model
  - phase: 03-file-following
    provides: follow::run and follow::run_waiting for file tailing
provides:
  - LineFilter module with regex include/exclude and ANSI stripping
  - --include, --exclude, --strip-ansi CLI flags
  - Filter wiring in all input modes (stdin, file, follow)
affects: [04-02]

tech-stack:
  added: []
  patterns: [filter-before-color pipeline stage, ANSI-aware pattern matching]

key-files:
  created: [src/filter.rs]
  modified: [src/cli.rs, src/main.rs, src/follow.rs, src/lib.rs, src/trigger.rs, tests/integration.rs]

key-decisions:
  - "strip_ansi defaults to auto (always strip) for safety -- matching through ANSI codes is the common case"
  - "Filter applied before engine.apply() and trigger processing so excluded lines never fire triggers"
  - "pub(crate) strip_ansi in trigger.rs rather than extracting to separate module -- minimal change"

patterns-established:
  - "Filter-before-color: line filtering happens before engine coloring in all code paths"

requirements-completed: [FILT-01, INP-06]

duration: 4min
completed: 2026-03-13
---

# Phase 04 Plan 01: Line Filtering Summary

**Regex-based --include/--exclude line filtering with ANSI-aware matching across stdin, file, and follow modes**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-13T12:16:54Z
- **Completed:** 2026-03-13T12:20:58Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- LineFilter module with include/exclude regex patterns and optional ANSI stripping
- CLI flags --include, --exclude, --strip-ansi wired into all input modes
- Excluded lines never fire triggers -- filter applied before coloring and trigger processing
- 11 unit tests + 7 integration tests covering all filter behaviors

## Task Commits

Each task was committed atomically:

1. **Task 1: Create LineFilter module with ANSI-aware matching** - `63e16e6` (feat)
2. **Task 2: Wire filtering into CLI and main loop with integration tests** - `1812fea` (feat)

## Files Created/Modified
- `src/filter.rs` - LineFilter struct with include/exclude regex matching and ANSI stripping
- `src/cli.rs` - Added --include, --exclude, --strip-ansi flags and StripAnsi enum
- `src/main.rs` - Filter creation and wiring in stdin pipe and file modes
- `src/follow.rs` - Filter parameter added to read_new_lines, run, and run_waiting
- `src/lib.rs` - Added pub mod filter
- `src/trigger.rs` - Made strip_ansi pub(crate) for shared use
- `tests/integration.rs` - 7 filter integration tests

## Decisions Made
- strip_ansi defaults to auto (always strip) for safety -- matching through ANSI codes is the common case
- Filter applied before engine.apply() and trigger processing so excluded lines never fire triggers
- Reused strip_ansi from trigger.rs as pub(crate) rather than extracting to a separate module

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Added Debug derive to LineFilter**
- **Found during:** Task 1
- **Issue:** Tests using unwrap_err() require Debug trait on the Ok type
- **Fix:** Added #[derive(Debug)] to LineFilter struct
- **Files modified:** src/filter.rs
- **Verification:** All tests compile and pass
- **Committed in:** 63e16e6 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Minor derive addition for test ergonomics. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Filter infrastructure ready for Plan 02 to build on
- All tests green (212 unit + 42 integration)

---
*Phase: 04-cli-polish-filtering-and-ux-enhancements*
*Completed: 2026-03-13*

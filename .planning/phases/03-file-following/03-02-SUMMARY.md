---
phase: 03-file-following
plan: 02
subsystem: file-io
tags: [notify, kqueue, inotify, file-follow, log-rotation, inode, copytruncate]

# Dependency graph
requires:
  - phase: 03-file-following
    provides: CLI flags (-f, -F, -n), tail reader, three-mode input branching, notify dependency
  - phase: 01-core-pipeline
    provides: Engine, BufWriter output pattern, BrokenPipe handling
provides:
  - follow module with Descriptor and Name modes
  - notify-based event loop with filesystem watching
  - Rotation detection via inode identity comparison
  - Copytruncate detection via file size comparison
  - run_waiting for missing file polling
  - Full three-mode wiring in main.rs (pipe, print-and-exit, follow)
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: [notify recommended_watcher with mpsc channel, inode-based rotation detection, FIFO-based stdin pipe detection]

key-files:
  created: [src/follow.rs]
  modified: [src/main.rs, src/lib.rs, tests/integration.rs]

key-decisions:
  - "stdin_is_pipe uses /dev/stdin fstat FIFO check instead of is_terminal -- avoids false positives with /dev/null stdin"
  - "Descriptor mode exits after 20 consecutive timeouts (~5s) with deleted file and no new data"
  - "Name mode watches parent directory (not file) to detect recreation after rotation"
  - "Event debouncing via try_recv drain before reading handles macOS kqueue duplicate events"

patterns-established:
  - "read_new_lines uses read_line loop (not lines() iterator) for following past EOF"
  - "StdCommand with Stdio::null() for file-mode integration tests to avoid stdin pipe conflict"

requirements-completed: [INP-02, INP-03]

# Metrics
duration: 13min
completed: 2026-03-12
---

# Phase 3 Plan 02: File Following Summary

**notify-based file following with -f (descriptor) and -F (name) modes, inode rotation detection, copytruncate handling, and 8 new integration tests**

## Performance

- **Duration:** 13 min
- **Started:** 2026-03-12T17:13:22Z
- **Completed:** 2026-03-12T17:26:27Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- follow.rs module with FollowMode::Descriptor and FollowMode::Name, notify event loop with 250ms timeout fallback
- Rotation detection via FileIdentity (dev + ino) comparison, copytruncate detection via file size comparison
- main.rs fully wired: pipe mode, print-and-exit mode, and follow mode all functional
- 8 new integration tests covering follow scenarios (new lines, rotation, copytruncate, bare file, missing file, stdin conflict)

## Task Commits

Each task was committed atomically:

1. **Task 1: Follow module with descriptor and name modes** - `cedad6d` (feat)
2. **Task 2: Wire follow into main.rs and add integration tests** - `bb2d02f` (feat)

## Files Created/Modified
- `src/follow.rs` - New module: FollowMode enum, FileIdentity struct, run/run_waiting functions, notify event loop
- `src/main.rs` - Replaced follow stub with follow::run calls, added stdin_is_pipe, read_initial, print_lines helpers
- `src/lib.rs` - Added `pub mod follow`
- `tests/integration.rs` - 8 new tests: print_and_exit_last_n, missing_file_f_errors, file_stdin_conflict, follow_reads_new_lines, bare_file_implies_follow, n_with_follow_flag, follow_name_rotation, follow_copytruncate

## Decisions Made
- Changed stdin conflict detection from `is_terminal()` to FIFO-specific check via `/dev/stdin` stat -- `is_terminal()` returned false for `/dev/null` stdin causing false positives in test contexts and daemon invocations
- Descriptor mode uses consecutive timeout counter (20 timeouts = ~5s) to detect file deletion and clean exit
- Name mode watches parent directory to catch file recreation after rotation (watching the file directly follows the inode, missing the new file)
- Event debouncing: drain all pending events via `try_recv()` before reading to handle macOS kqueue double-fire

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] stdin_is_pipe instead of is_terminal for conflict detection**
- **Found during:** Task 2 (integration tests)
- **Issue:** `is_terminal()` returns false for both piped stdin and /dev/null, causing file-mode tests to fail with "cannot use file argument with piped input"
- **Fix:** Replaced with FIFO-specific check using `std::os::unix::fs::FileTypeExt::is_fifo()` on `/dev/stdin`
- **Files modified:** src/main.rs
- **Verification:** All 30 integration tests pass including both file-mode and pipe-conflict tests
- **Committed in:** bb2d02f (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Essential for correct behavior in non-terminal contexts. No scope creep.

## Issues Encountered
None beyond the stdin detection fix documented above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All Phase 3 plans complete -- file following fully functional
- `ctail -f`, `ctail -F`, and bare `ctail app.log` all work with colorized output
- Log rotation (rename/create and copytruncate) handled seamlessly
- 145 unit tests + 30 integration tests all passing

---
*Phase: 03-file-following*
*Completed: 2026-03-12*

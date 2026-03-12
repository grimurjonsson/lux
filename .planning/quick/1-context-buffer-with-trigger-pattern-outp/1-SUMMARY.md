---
phase: quick
plan: 1
subsystem: trigger-filter
tags: [cli, trigger, state-machine, filtering]
dependency_graph:
  requires: [engine, follow, cli]
  provides: [trigger-filter, context-window-output]
  affects: [main, follow, cli]
tech_stack:
  added: []
  patterns: [state-machine, rolling-buffer, output-gating]
key_files:
  created:
    - src/trigger.rs
  modified:
    - src/cli.rs
    - src/main.rs
    - src/follow.rs
    - src/lib.rs
    - src/config.rs
    - tests/integration.rs
decisions:
  - "after=0 skips Emitting state entirely (stays in Suppressing)"
  - "before=0 disables buffer push to avoid off-by-one"
  - "Separator '--' only between non-first trigger groups (matches grep -C behavior)"
metrics:
  duration: 6min
  completed: 2026-03-13
---

# Quick Task 1: Context Buffer with Trigger-Pattern Output Summary

TriggerFilter state machine with rolling VecDeque buffer gating output through --trigger/--before/--after CLI flags, integrated into stdin, file, and follow pipelines.

## What Was Built

### Task 1: CLI Flags and TriggerFilter Module
- Added `--trigger PATTERN` (multiple via Append), `--before N` (default 20), `--after N` (default 20) to Cli struct
- Created `src/trigger.rs` with `TriggerFilter` state machine:
  - `OutputDecision` enum: Pass, Flush, Suppress
  - Suppressing state: lines buffered in rolling VecDeque of capacity `before`
  - Emitting state: countdown `after` lines, then transition back to Suppressing
  - Re-trigger during Emitting resets the after counter
  - Separator "--" between trigger groups (like grep -C)
  - Raw line used for pattern matching (avoids ANSI escape code interference)
- 11 unit tests covering all state transitions and edge cases

### Task 2: Pipeline Integration
- Stdin mode: trigger filter wraps engine output, handling Pass/Flush/Suppress decisions
- File mode: `print_lines_filtered` processes initial lines through trigger filter
- Follow mode: `Option<TriggerFilter>` passed through `run` and `run_waiting` via `trigger.as_mut()`
- Zero overhead when trigger inactive: `is_active()` check preserves original code paths
- 5 integration tests: basic, before/after window sizes, passthrough, multiple OR patterns, separator

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed before=0 buffer edge case**
- **Found during:** Task 2 (integration test `trigger_multiple` and `trigger_separator`)
- **Issue:** With `before=0`, the rolling buffer still accumulated 1 line because `buffer.len() >= 0` was always true, causing push after pop
- **Fix:** Added `if self.before > 0` guard around buffer push logic
- **Files modified:** src/trigger.rs
- **Commit:** 8473193

**2. [Rule 1 - Bug] Fixed after=0 state transition**
- **Found during:** Task 1 (unit test `multiple_patterns_or`)
- **Issue:** With `after=0`, state transitioned to `Emitting { remaining: 0 }` which never naturally exited
- **Fix:** When after=0, stay in Suppressing state after Flush (skip Emitting entirely)
- **Files modified:** src/trigger.rs
- **Commit:** f4b618d

**3. [Rule 3 - Blocking] Fixed clippy warnings for verification to pass**
- **Found during:** Task 2 (clippy verification step)
- **Issue:** Pre-existing `collapsible_if` (config.rs, main.rs) and `seek_from_current` (follow.rs) warnings failed `cargo clippy -- -D warnings`
- **Fix:** Collapsed nested if-let chains, replaced `seek(SeekFrom::Current(0))` with `stream_position()`
- **Files modified:** src/config.rs, src/main.rs, src/follow.rs
- **Commit:** 8473193

## Verification Results

1. `cargo test --no-fail-fast` -- 195 tests pass (160 lib + 35 integration)
2. `cargo clippy -- -D warnings` -- clean, no warnings
3. Smoke test: `echo "a\nb\nc\nERROR boom\nd\ne\nf" | ctail --trigger ERROR --before 2 --after 1 --color never` outputs "b", "c", "ERROR boom", "d"
4. Passthrough test: `echo "hello\nworld" | ctail --color never` outputs "hello", "world"

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1 | f4b618d | feat(quick-1): add --trigger/--before/--after CLI flags and TriggerFilter module |
| 2 | 8473193 | feat(quick-1): integrate TriggerFilter into main and follow pipelines |

## Self-Check: PASSED

---
phase: 03-file-following
verified: 2026-03-12T18:00:00Z
status: passed
score: 17/17 must-haves verified
re_verification: false
gaps: []
human_verification:
  - test: "Manual smoke test of ctail -f with a real live log"
    expected: "New log lines appear colored in terminal as they are written"
    why_human: "Integration tests use piped I/O and kill signals; actual tty color + real-time display requires human observation"
  - test: "Ctrl+C exits cleanly during follow mode"
    expected: "Process exits with code 0 and no error message on Ctrl+C"
    why_human: "Signal handling behavior in a real terminal cannot be reliably asserted programmatically"
---

# Phase 3: File Following Verification Report

**Phase Goal:** Users can tail log files directly with ctail, including files that get rotated by logrotate
**Verified:** 2026-03-12T18:00:00Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

All 17 truths from both plan must_haves sections were verified.

#### Truths from Plan 01

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `ctail -n 20 app.log` prints last 20 colored lines and exits (print-and-exit mode) | VERIFIED | `print_and_exit_last_n` integration test passes; `is_print_and_exit = has_explicit_n && !has_follow_flag` logic in main.rs lines 63-77 |
| 2 | `ctail app.log` with no flags implies -n 10 -F behavior (follow mode) | VERIFIED | `bare_file_implies_follow` integration test passes; `lines_str = cli.lines.as_deref().unwrap_or("10")` with Name mode path in main.rs |
| 3 | `ctail -f app.log` starts by showing last 10 lines then follows (follow mode) | VERIFIED | `follow_reads_new_lines` integration test passes; descriptor mode branch in main.rs lines 70-76 |
| 4 | `ctail -n 5 -f app.log` shows last 5 lines then follows (follow mode) | VERIFIED | `n_with_follow_flag` integration test passes; has_follow_flag check prevents print-and-exit when -f present |
| 5 | `ctail app.log` with piped stdin produces an error | VERIFIED | `file_stdin_conflict` integration test passes; `stdin_is_pipe()` FIFO check in main.rs lines 52-53 |
| 6 | -n is silently ignored when reading from stdin pipe | VERIFIED | Stdin branch (else block, main.rs lines 103-111) reads stdin lines without consulting `cli.lines` |
| 7 | Last-N-lines reads efficiently from end of file without loading entire file | VERIFIED | `read_last_n_lines` in tail.rs uses backward-seeking 8KB chunks from EOF (lines 45-76) |

#### Truths from Plan 02

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 8 | User can follow a file with `ctail -f app.log` and see new lines colored as they are appended | VERIFIED | `follow_reads_new_lines` integration test passes; notify event loop in follow.rs with `engine.apply()` call |
| 9 | User can follow a file by name with `ctail -F app.log` and continue seeing output after log rotation | VERIFIED | `follow_name_rotation` integration test passes; inode identity check and file reopen in follow.rs lines 124-136 |
| 10 | ctail handles rename/create rotation without losing lines or hanging | VERIFIED | Rotation logic drains old fd before opening new file (follow.rs lines 128-135); both event-driven and timeout-based paths handle this |
| 11 | ctail handles copytruncate rotation by detecting file shrinkage and re-reading from beginning | VERIFIED | `follow_copytruncate` integration test passes; `current_size < last_pos` check seeks to 0 (follow.rs lines 141-148) |
| 12 | -f mode: file must exist at startup or error | VERIFIED | `missing_file_f_errors` integration test passes; `File::open` with `with_context` error in main.rs lines 72-73 |
| 13 | -f mode: file deleted mid-follow reads until EOF then exits | VERIFIED | Descriptor mode: consecutive_timeouts > 20 + !path.exists() triggers clean exit after final read (follow.rs lines 215-222) |
| 14 | -F mode: file missing at start prints warning and polls until it appears | VERIFIED | NotFound branch in main.rs lines 92-97 calls `run_waiting` which polls every 1s (follow.rs lines 269-286) |
| 15 | -F mode: file disappears mid-follow prints warning and polls until it reappears | VERIFIED | Remove event handler in follow.rs lines 164-193 prints stderr warning and polls in a 1s loop |
| 16 | Bare file arg (`ctail app.log`) behaves like -F with -n 10 | VERIFIED | `bare_file_implies_follow` test passes; bare file falls into the Name-mode branch in main.rs line 90 |
| 17 | `ctail -n 5 -f app.log` shows last 5 then follows (follow mode, not print-and-exit) | VERIFIED | `n_with_follow_flag` test passes; `has_follow_flag = true` means `is_print_and_exit = false` |

**Score:** 17/17 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/tail.rs` | Backward-seeking last-N-lines reader | VERIFIED | 211 lines; exports `read_last_n_lines`, `parse_line_spec`, `LineSpec`; backward seek with 8KB chunks (Level 1+2+3) |
| `src/cli.rs` | Extended CLI with -f, -F, -n flags and positional file arg | VERIFIED | Contains `follow_descriptor`, `follow_name`, `lines: Option<String>` (no default), `file: Option<String>`; 11 new tests (Level 1+2+3) |
| `src/main.rs` | Input mode branching: stdin vs print-and-exit vs follow | VERIFIED | Contains `is_terminal`-equivalent `stdin_is_pipe`, calls `tail::read_last_n_lines`, `follow::run`, three-mode branching (Level 1+2+3) |
| `src/follow.rs` | File following with -f and -F modes, notify-based event loop, rotation detection | VERIFIED | 343 lines; exports `run`, `run_waiting`, `FollowMode`; notify event loop + inode identity (Level 1+2+3) |
| `tests/integration.rs` | Integration tests for file following scenarios | VERIFIED | 8 new file-following tests: `print_and_exit_last_n`, `missing_file_f_errors`, `file_stdin_conflict`, `follow_reads_new_lines`, `bare_file_implies_follow`, `n_with_follow_flag`, `follow_name_rotation`, `follow_copytruncate` (Level 1+2+3) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/main.rs` | `src/tail.rs` | `tail::read_last_n_lines` call for initial content | WIRED | `read_initial` in main.rs calls `tail::read_last_n_lines` (line 127); import `ctail::tail::{self, LineSpec}` at line 13 |
| `src/main.rs` | `src/cli.rs` | CLI arg parsing for file/follow/lines | WIRED | `cli.file`, `cli.follow_descriptor`, `cli.follow_name`, `cli.lines` all consumed in main.rs branching logic |
| `src/follow.rs` | `notify` crate | `notify::recommended_watcher` with mpsc channel | WIRED | `notify::recommended_watcher(tx)?` at follow.rs line 91; `use notify::{EventKind, RecursiveMode, Watcher}` at line 8 |
| `src/follow.rs` | `src/engine.rs` | `Engine::apply` for colorizing each new line | WIRED | `engine.apply(trimmed)` in `read_new_lines` at follow.rs line 63; `use crate::engine::Engine` at line 10 |
| `src/follow.rs` | `std::os::unix::fs::MetadataExt` | inode/device identity for rotation detection | WIRED | `use std::os::unix::fs::MetadataExt` at follow.rs line 3; `.dev()` and `.ino()` used in `FileIdentity::from_metadata` |
| `src/main.rs` | `src/follow.rs` | `follow::run` call from file mode branch | WIRED | `follow::run(path, follow::FollowMode::Descriptor, file, ...)` at main.rs line 76; `follow::run(path, follow::FollowMode::Name, ...)` at line 90; `use ctail::follow` at line 10 |

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| INP-02 | 03-01, 03-02 | User can follow a file by descriptor (`ctail -f app.log`) | SATISFIED | `follow_reads_new_lines` and `n_with_follow_flag` tests pass; Descriptor mode in follow.rs fully implemented; main.rs wires `-f` flag to `FollowMode::Descriptor` |
| INP-03 | 03-01, 03-02 | User can follow a file by name with rotation handling (`ctail -F app.log`) | SATISFIED | `follow_name_rotation` and `follow_copytruncate` tests pass; Name mode with inode-based rotation detection and copytruncate detection fully implemented; `run_waiting` handles missing file at startup |

No orphaned requirements: REQUIREMENTS.md traceability table maps only INP-02 and INP-03 to Phase 3, both claimed by plans 03-01 and 03-02.

### Anti-Patterns Found

None. Scan of `src/tail.rs`, `src/follow.rs`, `src/main.rs`, and `src/cli.rs` found:
- Zero TODO/FIXME/HACK/PLACEHOLDER comments
- Zero `unimplemented!()` or `todo!()` calls
- Zero stub return patterns (`return null`, `return {}`, empty closures)
- All handlers process real data (no `console.log`-equivalent stubs)

### Human Verification Required

#### 1. Live tty color output during follow

**Test:** Run `ctail --color always -f /var/log/system.log` (or any actively written log) in a terminal
**Expected:** New log lines appear in the terminal with ANSI colors applied immediately as they are written to the log file
**Why human:** Integration tests run without a tty and use process kill; actual tty rendering and real-time display responsiveness requires human observation

#### 2. Ctrl+C clean exit

**Test:** Start `ctail -F somefile.log` in a terminal, press Ctrl+C
**Expected:** Process exits immediately with no error message and returns exit code 0
**Why human:** Signal handling under SIGINT in a real terminal context cannot be reliably asserted in automated tests

### Gaps Summary

No gaps. All 17 observable truths are verified, all 5 required artifacts pass all three levels (exists, substantive, wired), all 6 key links are confirmed wired, and both requirements (INP-02 and INP-03) are fully satisfied. The full test suite (30 integration tests + 145 unit tests) passes with 0 failures.

---

_Verified: 2026-03-12T18:00:00Z_
_Verifier: Claude (gsd-verifier)_

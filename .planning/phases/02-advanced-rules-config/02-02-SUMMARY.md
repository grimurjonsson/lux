---
phase: 02-advanced-rules-config
plan: 02
subsystem: config
tags: [toml, serde, xdg, profiles, config-loading]

requires:
  - phase: 02-advanced-rules-config
    plan: 01
    provides: "Span-based engine, rule_from_config, parse_scope"
provides:
  - "TOML config file loading with XDG path discovery"
  - "Named profile support with --profile flag"
  - "build_rules_with_config merging CLI > global config > profile > defaults"
  - "--config path override for custom config locations"
affects: [02-advanced-rules-config]

tech-stack:
  added: [toml, serde]
  patterns: [xdg-config-discovery, priority-layered-rule-merging]

key-files:
  created:
    - src/config.rs
  modified:
    - Cargo.toml
    - src/lib.rs
    - src/cli.rs
    - src/rules.rs
    - src/main.rs
    - tests/integration.rs

key-decisions:
  - "Unsafe blocks for env var manipulation in tests (Rust 2024 edition requirement)"
  - "XDG path uses HOME/.config directly, not dirs crate (per locked decision)"
  - "Profile error lists available profiles sorted alphabetically"

patterns-established:
  - "Config struct pattern: top-level Config with rules vec and profiles HashMap"
  - "Priority layering: CLI (0+) > global config > profile > defaults"

requirements-completed: [CFG-02, CFG-03]

duration: 6min
completed: 2026-03-12
---

# Phase 2 Plan 2: TOML Config and Named Profiles Summary

**TOML config file support with XDG discovery, named profiles via --profile, and priority-layered rule merging**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-12T16:10:28Z
- **Completed:** 2026-03-12T16:16:15Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Created config module with TOML deserialization for rules and named profiles
- XDG config path discovery without dirs crate (uses ~/.config on macOS per locked decision)
- Extended CLI with --profile, --config, --list-profiles, --list-colors flags
- build_rules_with_config merges CLI > global config > profile > defaults with correct priority
- Error messages list available profiles when unknown profile requested

## Task Commits

Each task was committed atomically:

1. **Task 1: Create config module with TOML loading and profile merging** - `104a58d` (feat)
2. **Task 2: Extend CLI, update rule building, wire config into main** - `4424eb1` (feat)

## Files Created/Modified
- `src/config.rs` - TOML config loading, XDG path resolution, Config/RuleConfig/ProfileConfig structs
- `Cargo.toml` - Added toml, serde dependencies; tempfile dev-dependency
- `src/lib.rs` - Added pub mod config
- `src/cli.rs` - Added --profile, --config, --list-profiles, --list-colors flags
- `src/rules.rs` - Added build_rules_with_config with priority layering
- `src/main.rs` - Wired config loading, early-exit stubs for discovery commands
- `tests/integration.rs` - 5 new integration tests for config/profile functionality

## Decisions Made
- Used unsafe blocks for env var set/remove in tests (Rust 2024 edition made set_var unsafe)
- Profile available list sorted alphabetically for consistent error output
- list-profiles and list-colors are stubs printing "Not yet implemented" (Plan 03)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Rust 2024 edition unsafe env var APIs**
- **Found during:** Task 1 (config tests)
- **Issue:** std::env::set_var and remove_var are unsafe in Rust 2024 edition
- **Fix:** Wrapped all env var mutations in unsafe blocks in test code
- **Files modified:** src/config.rs
- **Verification:** All 10 config tests pass with --test-threads=1
- **Committed in:** 104a58d (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Required for compilation under Rust 2024 edition. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Config loading and profile selection fully functional
- Discovery commands (--list-profiles, --list-colors) stubbed for Plan 03
- All 112 unit tests and 16 integration tests pass

---
*Phase: 02-advanced-rules-config*
*Completed: 2026-03-12*

## Self-Check: PASSED

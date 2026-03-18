---
phase: quick
plan: 260318-jvh
subsystem: syntax-highlighting
tags: [syntect, syntax-highlighting, themes, owo-colors]

# Dependency graph
requires: []
provides:
  - "Syntect-based syntax highlighting as base layer under rule overlays"
  - "Configurable theme via config.toml theme field"
  - "Cleaned up built-in profiles (only logs remains)"
affects: [config, engine, profiles]

# Tech tracking
tech-stack:
  added: [syntect 5 (default-fancy features, pure Rust)]
  patterns: [LazyLock statics for expensive syntect SyntaxSet/ThemeSet loading, base-layer highlighting with usize::MAX priority]

key-files:
  created: [src/syntax.rs]
  modified: [Cargo.toml, src/lib.rs, src/config.rs, src/engine.rs, src/main.rs, src/rules.rs, tests/integration.rs]

key-decisions:
  - "LazyLock statics for SyntaxSet and ThemeSet to avoid repeated expensive loading"
  - "Fresh HighlightLines per call (stateless) since Engine.apply takes &self"
  - "Syntect spans use usize::MAX priority so any rule span wins over syntect base layer"
  - "Removed markdown, yaml, toml, shell built-in profiles -- syntect covers them better"
  - "Removed filenames field from ProfileConfig since profiles that used it were removed"
  - "Default theme: base16-ocean.dark with fallback if configured theme not found"

patterns-established:
  - "Base-layer highlighting: syntect fills unstyled bytes after all rule spans applied"
  - "Theme configuration: top-level theme field in config.toml"

requirements-completed: []

# Metrics
duration: 9min
completed: 2026-03-18
---

# Quick Task 260318-jvh: Syntect Syntax Highlighting Summary

**Syntect-based syntax highlighting as base layer with configurable themes, replacing hand-crafted regex profiles for markdown/yaml/toml/shell**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-18T14:22:01Z
- **Completed:** 2026-03-18T14:30:42Z
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments
- Added syntect 5 as a dependency for professional syntax highlighting of source code files
- Created src/syntax.rs module wrapping syntect with LazyLock statics for SyntaxSet/ThemeSet
- Integrated syntect base layer into Engine with lowest priority (rules always win over syntect)
- Removed 4 hand-crafted built-in profiles (markdown, yaml, toml, shell) -- syntect covers these better
- Added configurable theme support via config.toml `theme` field
- Preserved "logs" as the only built-in profile

## Task Commits

Each task was committed atomically:

1. **Task 1: Add syntect dependency, create syntax.rs module, add theme config** - `9b3bb8c` (feat)
2. **Task 2: Integrate syntect base layer into Engine and wire up in main.rs** - `01586e1` (feat)
3. **Task 3: Remove hand-crafted profiles, keep logs, remove filenames field** - `57480c2` (feat)

## Files Created/Modified
- `Cargo.toml` - Added syntect 5 dependency with default-fancy features
- `src/syntax.rs` - New module: SyntaxHighlighter wrapping syntect with LazyLock statics
- `src/lib.rs` - Added pub mod syntax
- `src/config.rs` - Added theme field to Config, removed 4 profiles, removed filenames field and find_profile_by_filename()
- `src/engine.rs` - Engine accepts optional SyntaxHighlighter, syntect spans at usize::MAX priority
- `src/main.rs` - Creates SyntaxHighlighter from file extension, passes to Engine
- `src/rules.rs` - Updated test Config constructors for new theme field and removed filenames
- `tests/integration.rs` - Updated tests for removed profiles, added syntect highlighting tests

## Decisions Made
- Used LazyLock statics for SyntaxSet and ThemeSet (loaded once per process, not per file)
- Fresh HighlightLines created per highlight_line call since Engine.apply takes &self (acceptable for CLI)
- Syntect spans use usize::MAX priority -- sorted last in the style_map merge, only filling unoccupied bytes
- base16-ocean.dark as default theme with graceful fallback for unknown theme names
- Removed filenames field entirely from ProfileConfig (serde silently ignores unknown fields in existing configs)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed missing `theme` field in test Config constructors**
- **Found during:** Task 1
- **Issue:** Adding `theme: Option<String>` to Config broke 4 test constructors that weren't using `..Default::default()`
- **Fix:** Added `theme: None` to all Config constructors in config.rs and rules.rs tests
- **Files modified:** src/config.rs, src/rules.rs
- **Verification:** cargo test --lib passes
- **Committed in:** 9b3bb8c (Task 1 commit)

**2. [Rule 1 - Bug] Fixed integration tests referencing removed profiles**
- **Found during:** Task 3
- **Issue:** Integration tests checked for "markdown" and "shell" in --list-profiles output and tested filename-based auto-selection
- **Fix:** Updated tests to check for "logs", replaced profile-specific tests with syntect highlighting tests
- **Files modified:** tests/integration.rs
- **Verification:** cargo test passes (57 tests + 1 ignored)
- **Committed in:** 57480c2 (Task 3 commit)

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Both auto-fixes necessary for test correctness after struct/profile changes. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Steps
- Users can configure `theme = "Solarized (dark)"` (or any syntect theme) in their config.toml
- Users can still create custom profiles in config.toml for any language (overlays on top of syntect)
- The `ctail --list-profiles` command now shows only "logs" as built-in

## Self-Check: PASSED

- All key files exist (src/syntax.rs, Cargo.toml, src/lib.rs, src/config.rs, src/engine.rs, src/main.rs)
- All task commits verified (9b3bb8c, 01586e1, 57480c2)
- SUMMARY.md exists at expected path

---
*Quick task: 260318-jvh*
*Completed: 2026-03-18*

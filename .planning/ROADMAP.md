# Roadmap: ctail

## Overview

ctail goes from zero to a working colored-tail CLI in three phases: first, a complete stdin-to-stdout color pipeline with built-in defaults and CLI rules (the core value proposition); second, persistent configuration via TOML files with named profiles and advanced rule features like match scopes and multi-rule overlap; third, file following with `-f` and `-F` including log rotation handling. Each phase delivers a usable tool -- Phase 1 alone is a useful `cmd | ctail` colorizer.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Core Pipeline** - Working stdin colorizer with built-in defaults, CLI rules, and full color/style support (completed 2026-03-12)
- [ ] **Phase 2: Advanced Rules + Config** - Match scopes, multi-rule overlap, TOML config files, named profiles, and discovery commands
- [x] **Phase 3: File Following** - Follow files by descriptor (-f) and by name (-F) with log rotation handling (completed 2026-03-12)

## Phase Details

### Phase 1: Core Pipeline
**Goal**: Users can pipe any command through ctail and get instantly readable, colored log output with zero configuration -- and customize it with CLI flags when needed
**Depends on**: Nothing (first phase)
**Requirements**: INP-01, INP-04, CLR-01, CLR-02, CLR-03, CLR-04, CLR-07, CLR-08, CLR-09, CFG-01
**Success Criteria** (what must be TRUE):
  1. User can pipe command output through ctail and see ERROR lines in red, WARN lines in yellow, and DEBUG lines dimmed -- with no flags or config
  2. User can define custom color rules via CLI flags (e.g., `ctail -r 'PATTERN=red'`) using regex patterns, and those rules apply to matching text
  3. User can specify colors as named ANSI colors, 256-color codes, or RGB hex values (`#ff0000`), and apply styles (bold, italic, underline, dim)
  4. ctail exits cleanly when used in pipelines (e.g., `ctail | head -5` does not panic or print errors)
  5. ctail suppresses color output when stdout is not a terminal or when `NO_COLOR` is set
**Plans**: 3 plans

Plans:
- [ ] 01-01-PLAN.md -- Project setup and color/style parsing module
- [ ] 01-02-PLAN.md -- CLI parsing, rule definitions, and color engine
- [ ] 01-03-PLAN.md -- Main loop, output, SIGPIPE handling, and integration tests

### Phase 2: Advanced Rules + Config
**Goal**: Users can persist rule sets in config files, switch between named profiles for different log formats, and use fine-grained match scoping with multiple rules applying to the same line
**Depends on**: Phase 1
**Requirements**: CLR-05, CLR-06, CFG-02, CFG-03, CFG-04, CFG-05
**Success Criteria** (what must be TRUE):
  1. User can set per-rule match scope to color the whole line, just the matched text, or a regex capture group -- and the correct region is colored
  2. Multiple rules can match the same line and each applies to its respective region without conflicts (e.g., timestamp in blue AND error level in red on the same line)
  3. User can define rules in `~/.config/ctail/config.toml` and they apply automatically without CLI flags
  4. User can create named profiles in config and activate one with `--profile name`
  5. User can discover available profiles with `--list-profiles` and available colors/styles with `--list-colors`
**Plans**: 3 plans

Plans:
- [ ] 02-01-PLAN.md -- Span-based engine rewrite for match scopes and multi-rule overlap
- [ ] 02-02-PLAN.md -- TOML config module, CLI extensions, profile merging
- [ ] 02-03-PLAN.md -- Discovery commands (--list-profiles, --list-colors)

### Phase 3: File Following
**Goal**: Users can tail log files directly with ctail, including files that get rotated by logrotate
**Depends on**: Phase 1
**Requirements**: INP-02, INP-03
**Success Criteria** (what must be TRUE):
  1. User can follow a file with `ctail -f app.log` and see new lines colored as they are appended
  2. User can follow a file by name with `ctail -F app.log` and continue seeing output after log rotation (file renamed and new file created at same path)
  3. ctail handles both common rotation strategies (rename/create and copytruncate) without losing lines or hanging
**Plans**: 2 plans

Plans:
- [ ] 03-01-PLAN.md -- CLI flags (-f, -F, -n), tail module (last-N-lines reader), input mode branching
- [ ] 03-02-PLAN.md -- Follow module (notify-based event loop, -f/-F modes, rotation detection), integration tests

### Phase 4: CLI polish, filtering, and UX enhancements

**Goal:** Users get line filtering to suppress noisy log output, shell completions for discoverability, version info, and FORCE_COLOR support for piped workflows -- the polish that makes ctail pleasant to use daily
**Depends on:** Phase 3
**Requirements**: FILT-01, INP-06, UX-01, UX-02, TERM-03
**Success Criteria** (what must be TRUE):
  1. User can filter log output with --include/--exclude patterns to suppress noise without losing colors
  2. User can generate shell completions with `ctail completions <shell>` for bash, zsh, and fish
  3. User can check version with `ctail --version`
  4. Setting FORCE_COLOR=1 forces colored output even when piping to another tool
  5. Pattern matching works correctly on input that contains ANSI escape codes
**Plans**: 2 plans

Plans:
- [ ] 04-01-PLAN.md -- Line filtering (--include/--exclude) and ANSI input stripping
- [ ] 04-02-PLAN.md -- Shell completions, --version flag, FORCE_COLOR env var support

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Core Pipeline | 1/3 | Complete    | 2026-03-12 |
| 2. Advanced Rules + Config | 2/3 | In Progress|  |
| 3. File Following | 2/2 | Complete   | 2026-03-12 |
| 4. CLI Polish + Filtering | 2/2 | Complete | 2026-03-13 |

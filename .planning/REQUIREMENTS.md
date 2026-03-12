# Requirements: ctail

**Defined:** 2026-03-12
**Core Value:** Instantly readable log output — the right colors applied to the right patterns, with zero configuration needed for common cases and full customization when you want it.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Input

- [x] **INP-01**: User can pipe stdin to ctail (`some-command | ctail`)
- [x] **INP-02**: User can follow a file by descriptor (`ctail -f app.log`)
- [x] **INP-03**: User can follow a file by name with rotation handling (`ctail -F app.log`)
- [x] **INP-04**: ctail exits cleanly on broken pipe (SIGPIPE handling)

### Coloring

- [x] **CLR-01**: Lines matching ERROR are colored red by default (no config needed)
- [x] **CLR-02**: Lines matching WARN/WARNING are colored yellow by default
- [x] **CLR-03**: Lines matching DEBUG are colored grey/dim by default
- [x] **CLR-04**: User can define rules with regex patterns
- [x] **CLR-05**: Per-rule match scope: color whole line, just the match, or a capture group
- [x] **CLR-06**: Multiple rules can match the same line — all apply to their respective regions
- [x] **CLR-07**: Full RGB/hex color support (`#ff0000`)
- [x] **CLR-08**: ANSI 16-color and 256-color support
- [x] **CLR-09**: Text styles: bold, italic, underline, dim

### Configuration

- [x] **CFG-01**: User can define rules via CLI flags (`-r 'PATTERN=color'`)
- [x] **CFG-02**: User can define rules in a TOML config file
- [x] **CFG-03**: User can create named profiles in config and select with `--profile`
- [x] **CFG-04**: User can list available profiles with `--list-profiles`
- [x] **CFG-05**: User can list available colors/styles with `--list-colors`

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Input

- **INP-05**: User can tail multiple files simultaneously with filename headers
- **INP-06**: Existing ANSI codes in input are stripped before matching and optionally preserved

### Terminal Compatibility

- **TERM-01**: Colors suppressed when NO_COLOR env var is set
- **TERM-02**: Automatic terminal color capability detection with fallback chain (truecolor → 256 → 16 → none)
- **TERM-03**: FORCE_COLOR env var forces color output even when piping

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Log parsing / structured field extraction | lnav does this; ctail colors lines, doesn't parse them |
| TUI / interactive mode | lnav owns this space; ctail pipes to `less -R` for scrollback |
| Format-specific parsers (syslog, nginx, etc.) | Maintenance burden; regex rules + example profiles instead |
| Remote file tailing | Massive complexity; users can `ssh host tail -f /var/log/x \| ctail` |
| Full `tail` feature parity (-n, -c, --pid, --bytes) | ctail is a colorizer, not a tail replacement; pipe from tail for those |
| HTML/CSS output | Niche; users can pipe through `ansi2html` |
| Log file decompression | lnav does this; users can `zcat file.gz \| ctail` |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| INP-01 | Phase 1 | Complete |
| INP-02 | Phase 3 | Complete |
| INP-03 | Phase 3 | Complete |
| INP-04 | Phase 1 | Complete |
| CLR-01 | Phase 1 | Complete |
| CLR-02 | Phase 1 | Complete |
| CLR-03 | Phase 1 | Complete |
| CLR-04 | Phase 1 | Complete |
| CLR-05 | Phase 2 | Complete |
| CLR-06 | Phase 2 | Complete |
| CLR-07 | Phase 1 | Complete |
| CLR-08 | Phase 1 | Complete |
| CLR-09 | Phase 1 | Complete |
| CFG-01 | Phase 1 | Complete |
| CFG-02 | Phase 2 | Complete |
| CFG-03 | Phase 2 | Complete |
| CFG-04 | Phase 2 | Complete |
| CFG-05 | Phase 2 | Complete |

**Coverage:**
- v1 requirements: 18 total
- Mapped to phases: 18
- Unmapped: 0

---
*Requirements defined: 2026-03-12*
*Last updated: 2026-03-12 after roadmap creation*

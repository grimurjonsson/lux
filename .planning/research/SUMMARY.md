# Project Research Summary

**Project:** ctail (Colored Tail)
**Domain:** CLI log colorizer / streaming text processor (Rust)
**Researched:** 2026-03-12
**Confidence:** HIGH

## Executive Summary

ctail is a Rust CLI tool in a well-understood domain: streaming log colorizers. The competitive landscape (tailspin, grc, ccze, logalize, colortail) is mature, which is an advantage — established patterns exist for every component, and clear gaps define where ctail can win. The recommended approach is a classic Unix filter architecture: an input source abstraction feeds lines through a regex-based color engine to stdout, with all complexity isolated at component boundaries. The entire stack can be assembled from well-vetted crates (clap, regex, owo-colors, serde/toml, notify, dirs, anyhow) — no major architectural decisions are left open.

The recommended build strategy is incremental: get a working stdin-to-stdout color pipeline first, then layer the rule system on top, then add file following, then polish. This order respects component dependencies — the color engine must exist before rules can exercise it, and the input source abstraction can be added independently once the pipeline is proven. The single most important architectural decision is the span-based coloring model (each line produces a flat list of non-overlapping styled regions), which must be designed upfront to avoid a painful rewrite when multi-rule matches arrive.

The key risks are practical rather than architectural: SIGPIPE panics break pipeline composability if not addressed on day one; log rotation race conditions cause silent data loss in `-F` mode if the file-watcher strategy is not designed carefully; and pre-colored ANSI input in the pipe breaks regex matching if the matching layer does not strip escape codes first. All three are well-understood problems with standard solutions — the risk is simply forgetting them, not not knowing how to solve them.

## Key Findings

### Recommended Stack

All recommended crates are mature, widely adopted, and well-suited to ctail's requirements. No experimental dependencies are needed. The stack is intentionally minimal — async runtimes (tokio), full terminal control libraries (crossterm), and layered config abstractions (config-rs) are explicitly excluded. See `.planning/research/STACK.md` for full rationale and version pins.

**Core technologies:**
- **clap 4.5.60** (derive mode): Argument parsing — industry standard, powers ripgrep/bat/fd, handles all required flags cleanly
- **regex 1.12.3 + RegexSet**: Pattern matching — guaranteed linear-time, named capture groups, `RegexSet` enables single-pass multi-rule matching
- **owo-colors 4.2.3**: Color output — zero-allocation, supports ANSI 16/256/truecolor, runtime color selection for config-driven rules
- **supports-color 3.0.2**: Terminal capability detection — respects `NO_COLOR`/`FORCE_COLOR`, detects truecolor/256/16/none
- **serde 1.0.228 + toml 1.0.6**: Config parsing — de-facto standard, TOML specified in project constraints
- **notify 8.2.0**: Filesystem watching — cross-platform (kqueue/inotify), handles file rotation detection for `-F` mode
- **dirs 6.0.0**: XDG/platform config paths — resolves `~/.config/ctail/` correctly across Linux/macOS
- **anyhow 1.0.102**: Error handling — lightweight, ergonomic for a CLI that mostly exits on errors

### Expected Features

The feature landscape is well-researched with 6 competitors analyzed. ctail's competitive position is: simpler than lnav, more configurable than tailspin, faster than grc, and the only tool combining file-following-with-rotation with rich rule configuration. See `.planning/research/FEATURES.md` for full competitor matrix.

**Must have (table stakes):**
- Stdin pipe support (`cmd | ctail`) — primary usage mode; auto-detect tty vs pipe
- Follow file (`-f`) — core "tail" functionality users expect
- Follow by name (`-F`) with rotation detection — log rotation is universal in production
- Built-in log level coloring (ERROR=red, WARN=yellow, DEBUG=dim) — zero-config instant value
- Regex-based pattern matching — using Rust `regex` crate with capture group support
- User-defined color rules (CLI `-r 'PATTERN=color'` and TOML config) — both inline and persistent
- ANSI 16-color baseline output — works everywhere
- NO_COLOR support — informal standard adopted by 500+ tools
- Graceful EPIPE/SIGPIPE exit — must exit cleanly in pipelines like `ctail | head`
- Multiple file support — users expect `ctail file1 file2`

**Should have (differentiators):**
- Per-rule match scope (line/match/capture) — nobody offers explicit three-way scope control; real gap in competitors
- Named profiles (`--profile spring-boot`) — switching log format rule sets is painful today; no competitor has this cleanly
- Multiple rules applying to the same line — apply ALL matching rules, not first-match-wins
- RGB/hex color support (`#ff0000`) — more intuitive than ANSI color names
- Automatic terminal color capability detection with graceful fallback (truecolor -> 256 -> 16)
- FORCE_COLOR support — complement to NO_COLOR for `ctail | less -R` usage

**Defer (v2+):**
- ANSI passthrough/preservation — high complexity; strip-and-recolorize is the v1 default
- `--list-profiles` and `--list-colors` introspection commands
- Example profile configs for common formats (nginx, syslog, etc.)
- Partial line / no-newline timeout flush — noticeable in practice but not MVP-blocking

**Anti-features (never build):**
- Log parsing / structured field extraction (lnav owns this)
- TUI / interactive mode
- Full `tail` feature parity beyond `-f`, `-F`, and maybe `-n`
- Remote file tailing / network support

### Architecture Approach

ctail follows a classic Unix filter architecture with clean component separation. The critical design insight: the Color Engine is a pure function (line in, styled segments out) that knows nothing about input sources. This separation makes the color engine independently testable and lets file-following complexity be added without touching the coloring logic. See `.planning/research/ARCHITECTURE.md` for full component specs, data flow diagrams, and Rust struct/enum sketches.

**Major components:**
1. **CLI Parser** (clap derive) — parses flags, file paths, inline rules; determines mode
2. **Config Loader** (serde/toml) — reads `~/.config/ctail/config.toml`, merges defaults + profile + CLI rules in priority order
3. **Rule Engine** — compiles regex patterns once at startup into `RegexSet` + individual `Regex` objects; stores ordered `Vec<Rule>` with scope and priority
4. **Input Source** (enum abstraction) — `Stdin | FollowFd(File) | FollowName(PathBuf)`; shields Color Engine from I/O source details
5. **File Watcher** (notify) — filesystem events for `-F` mode; inode-based rotation detection with polling fallback
6. **Color Engine** — runs `RegexSet::matches()` then individual `Regex` for matched rules; produces `Vec<StyledSpan>` with overlap resolution
7. **Renderer** — detects `ColorSupport` level at startup; converts abstract `Style` values to correct ANSI escape sequences; centralizes all `\x1b[` emission

**Key patterns to follow:**
- `RegexSet` + individual `Regex` two-pass: single-pass check for which rules fire, then targeted captures only for matches
- Span-merge algorithm: flatten all colored regions to non-overlapping segments before emitting any ANSI codes
- `BufWriter` on stdout with line-flush for interactive tailing; batch-flush for high-throughput piped mode
- Synchronous I/O throughout v1 — no async/tokio; a blocking read loop + `notify` events is sufficient

### Critical Pitfalls

Top pitfalls from research — all have known solutions; the risk is omission not complexity. See `.planning/research/PITFALLS.md` for full details and detection strategies.

1. **SIGPIPE panic breaks pipelines** — Rust ignores SIGPIPE by default; catch `BrokenPipe` errors on all stdout writes and call `std::process::exit(0)`. Must be in Phase 1.
2. **Log rotation race conditions (`-F` mode)** — poll path periodically, compare inodes; when inode changes, close old handle and open new file from the start. Test with both `copytruncate` and `rename/create` logrotate strategies.
3. **Overlapping multi-rule color conflicts** — design the span-based coloring model upfront; flatten all spans to non-overlapping segments before emitting escape codes; always emit `\x1b[0m` reset after every colored span and at end of line.
4. **Pre-colored ANSI input breaks regex matching** — strip ANSI escape codes from each line before running regex matches; strip-then-recolorize is the correct v1 default.
5. **Color output when not on a TTY** — check `stdout().is_terminal()` + `NO_COLOR` env var from day one in the Renderer; bake this in before any color output works.

## Implications for Roadmap

Based on combined research, component dependencies, and pitfall phase warnings, the recommended phase structure is:

### Phase 1: Core Pipeline (stdin + basic coloring)

**Rationale:** The color engine is the core of the entire product. Everything else — file following, config, profiles — hangs on it. Validate the pipeline design (input -> line reader -> color engine -> renderer -> stdout) with the simplest possible input source (stdin) before adding complexity. All critical I/O pitfalls (SIGPIPE, NO_COLOR, output buffering) must be addressed here or they become expensive to retrofit.

**Delivers:** A working `cmd | ctail` tool with built-in log level coloring and inline `-r` rules. Users can immediately get value from this.

**Addresses features:** Stdin pipe support, built-in log level coloring, regex-based user rules via `-r` flag, ANSI 16-color output, NO_COLOR support, graceful SIGPIPE/EPIPE exit, FORCE_COLOR support.

**Avoids:** SIGPIPE panic (Pitfall 1), color output when not on TTY (Pitfall 5), unbuffered I/O latency (Pitfall 8), end-of-line reset bleeding (Pitfall 11).

**Architecture work:** CLI Parser (minimal), Color Engine (span-based model — design this correctly here), Renderer (color detection + ANSI emission), Line Reader (stdin only).

### Phase 2: Rule System + Config

**Rationale:** Once the pipeline is proven with hardcoded rules, the rule system formalizes it. TOML config, user-defined rules with full scope control, and named profiles are all config-system features that share the same infrastructure (Config Loader + Rule Engine). Building them together avoids multiple passes through the config layer. The overlapping-match and pre-colored ANSI pitfalls must be addressed here as the rule engine is built.

**Delivers:** Persistent rule configuration, named profiles, per-rule match scope (line/match/capture), multiple rules per line, config versioning.

**Addresses features:** TOML config file at `~/.config/ctail/config.toml`, named profiles (`--profile`), per-rule match scope, multiple rules applying to same line, RGB/hex color support.

**Avoids:** Overlapping multi-rule color conflicts (Pitfall 3), pre-colored ANSI input breaking matches (Pitfall 4), non-standard config paths (Pitfall 9), unversioned config schema (Pitfall 12).

**Stack used:** serde + toml, regex RegexSet two-pass pattern, dirs crate for XDG paths, owo-colors runtime color selection.

### Phase 3: File Following

**Rationale:** File following (`-f` and `-F`) is architecturally independent of the color engine — once the Input Source abstraction is in place, following a file is a different "plug" into the same pipeline. Doing this after the rule system means file-following complexity does not entangle with coloring logic. `-f` (follow by descriptor) is simpler and should precede `-F` (follow by name with rotation) within this phase.

**Delivers:** `ctail -f app.log` and `ctail -F app.log` with log rotation handling, multiple file support.

**Addresses features:** Follow file (`-f`), follow by name (`-F`) with rotation detection, multiple files support.

**Avoids:** Log rotation race conditions (Pitfall 2) — inode comparison, both `copytruncate` and `rename/create` rotation strategies.

**Stack used:** notify crate (kqueue/inotify), Input Source enum abstraction, File Watcher component.

### Phase 4: Polish + Terminal Capability

**Rationale:** Full terminal color capability detection (truecolor -> 256 -> 16 fallback), `--color-mode` override flag, and partial line handling are polish items. Starting with ANSI 16-color in Phase 1 avoids early bikeshedding; upgrading the Renderer here is low-risk since color generation is already centralized in the Renderer component.

**Delivers:** Correct color output on any terminal (iTerm2, Terminal.app, tmux, SSH, xterm, CI), partial line/no-newline timeout flush, `--list-profiles`/`--list-colors` discovery UX.

**Addresses features:** Automatic terminal color capability detection, `--color-mode` override, partial line handling, introspection commands.

**Avoids:** Color capability detection fragility (Pitfall 7), partial line hangs (Pitfall 10).

**Stack used:** supports-color crate, owo-colors truecolor/256 APIs.

### Phase Ordering Rationale

- The span-based Color Engine model is the architectural bet that enables phases 2-4 — it must be designed correctly in Phase 1, not retrofitted.
- Phase 2 (Rule System) depends on Phase 1 (Color Engine) existing; it cannot be built independently.
- Phase 3 (File Following) is independent of Phase 2 but benefits from having a tested pipeline to plug into.
- Phase 4 (Polish) is genuinely separable and can be deferred without breaking earlier phases.
- All four critical pitfalls that require architectural decisions (SIGPIPE, span overlap, ANSI stripping, color detection) are addressed in Phases 1-2 per the PITFALLS.md phase-warning table.

### Research Flags

Phases with standard patterns (skip research-phase):
- **Phase 1:** stdin pipeline + clap CLI is extremely well-documented for Rust; no research needed
- **Phase 2:** serde/toml config loading is a solved problem; regex crate is comprehensively documented
- **Phase 4:** terminal color detection is well-covered by supports-color and owo-colors crate docs

Phases likely needing deeper research during planning:
- **Phase 3:** log rotation behavior differences between macOS (kqueue) and Linux (inotify) via `notify` crate, particularly with `copytruncate` logrotate. The `notify` crate has known limitations with FSEvents file ownership and symlinks — plan for polling fallback complexity.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crates verified on crates.io 2026-03-12; version pins confirmed; rationale from authoritative sources (Rain's Rust CLI guide, official crate docs) |
| Features | HIGH | Six competitors analyzed with detailed feature matrix; competitive gaps clearly identified |
| Architecture | HIGH | Standard Unix filter pattern with multiple reference implementations (grc, logalize); component boundaries and data flow fully specified with Rust types |
| Pitfalls | HIGH | Each pitfall has documented root cause, real-world consequences, concrete prevention, and detection strategy; sources include Rust issue tracker and production incident reports |

**Overall confidence:** HIGH

### Gaps to Address

- **Span overlap resolution policy:** The research recommends a priority-based approach but explicitly notes "last-rule-wins or first-rule-wins — pick one and document it." This design decision should be made explicit early in Phase 2 planning and documented in the rule engine spec.
- **`notify` reliability on macOS under log rotation:** The PITFALLS research flags that `notify` has limitations with FSEvents and recommends polling as fallback, but the exact failure modes were not fully enumerated. Phase 3 planning should include a spike to validate behavior.
- **Partial line buffer flushing strategy:** The 200ms timeout recommendation in PITFALLS.md is a reasonable starting point but is untested. Validate during Phase 4.

## Sources

### Primary (HIGH confidence)
- [Rain's Rust CLI recommendations](https://rust-cli-recommendations.sunshowers.io/) — color management, CLI parser selection
- [crates.io](https://crates.io/) — all crate versions verified 2026-03-12
- [regex crate docs (RegexSet)](https://docs.rs/regex/latest/regex/struct.RegexSet.html) — multi-pattern matching patterns
- [Rust SIGPIPE issue #46016](https://github.com/rust-lang/rust/issues/46016) — SIGPIPE behavior and workarounds
- [NO_COLOR standard](https://no-color.org/) / [FORCE_COLOR standard](https://force-color.org/)
- [tailspin GitHub](https://github.com/bensadeh/tailspin) — competitor reference
- [grc GitHub](https://github.com/garabik/grc) — competitor reference architecture
- [logalize GitHub](https://github.com/deponian/logalize) — competitor with ANSI stripping approach
- [notify-rs GitHub](https://github.com/notify-rs/notify) — filesystem watching

### Secondary (MEDIUM confidence)
- [Terminal color detection pitfalls](https://marvinh.dev/blog/terminal-colors/) — color capability detection fragility
- [Terminal color standards](https://github.com/termstandard/colors) — truecolor/256/16 detection signals
- [Logrotate copytruncate race condition](https://incoherency.co.uk/blog/stories/logrotate-copytruncate-race-condition.html) — rotation race condition details
- [Rust I/O performance book](https://nnethercote.github.io/perf-book/io.html) — BufWriter/BufReader guidance

### Tertiary (LOW confidence)
- Partial line flush timeout of 200ms — reasonable estimate, needs empirical validation

---
*Research completed: 2026-03-12*
*Ready for roadmap: yes*

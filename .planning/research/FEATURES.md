# Feature Landscape

**Domain:** CLI log colorizer / colored tail tool
**Researched:** 2026-03-12
**Competitors analyzed:** tailspin (tspin), grc, ccze, lnav, colortail, logalize

## Table Stakes

Features users expect from a log colorizer. Missing any of these and users will pick tailspin or grc instead.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Stdin pipe support (`cmd \| ctail`) | Every colorizer supports this; it's the primary usage mode for composability | Low | Must auto-detect when stdin is a pipe vs terminal |
| Follow file (`-f`) | Core "tail" functionality; the reason users reach for this tool | Medium | Follow by file descriptor, like `tail -f` |
| Follow by name (`-F`) | Log rotation is universal in production; `-F` handles it | Medium | Detect file rename/recreate, reopen automatically. Equivalent to `--follow=name --retry` |
| Built-in log level coloring | tailspin, ccze, grc all do this out of the box; zero-config value is the #1 draw | Low | ERROR=red, WARN=yellow, DEBUG=dim/grey at minimum. INFO stays default/uncolored |
| Regex-based pattern matching | Every serious colorizer uses regex for rules; literal-only would be a non-starter | Medium | Use Rust `regex` crate. Capture groups are important for scoped coloring |
| User-defined color rules | grc, tailspin, logalize all support custom rules; users need to color app-specific patterns | Medium | Both CLI flags (`-r 'PATTERN=color'`) and config file |
| Config file for rules | Every tool except the most trivial supports persistent rule definitions | Low | TOML at `~/.config/ctail/config.toml` per XDG convention |
| NO_COLOR support | Informal standard (no-color.org) adopted by 500+ CLI tools; expected in 2026 | Low | When `NO_COLOR` env var is set and non-empty, suppress all color output |
| ANSI color codes (16-color) | Baseline terminal compatibility; works everywhere | Low | The 8 standard + 8 bright colors plus bold/dim |
| Graceful exit on broken pipe | When downstream closes (e.g., `ctail \| head`), must exit cleanly, not panic with EPIPE | Low | Handle `SIGPIPE` / `BrokenPipe` error |
| Multiple files | `tail` supports multiple files; users expect `ctail file1 file2` | Low | Show filename headers when tailing multiple files |

## Differentiators

Features that set ctail apart. Not expected by default, but create competitive advantage.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Per-rule match scope (line/match/capture) | tailspin only colors matched text; grc has "count" modes but no explicit scope control. ctail lets you say "color the whole line red when ERROR appears" OR "only color the word ERROR red" OR "color capture group 1" | Medium | Three modes: `line` (whole line), `match` (matched text only), `capture` (specific group). This is a real gap in competitors |
| Named profiles (`--profile spring-boot`) | grc has per-command configs but no user-facing "profile" concept; tailspin has no profiles. Switching between log formats is painful today | Low | Profile = named set of rules in config. Simple TOML section: `[profiles.spring-boot]` |
| Multiple rules match same line | grc's `stop` count mode halts after first match; ccze uses fixed plugins. ctail applying ALL matching rules means a single line can have timestamp=blue + level=red + IP=cyan | Medium | Rules apply in order; each colors its matched region. Need conflict resolution (last-writer-wins for overlapping regions) |
| RGB/hex color support (`#ff0000`) | tailspin supports hex colors in themes; grc only supports named ANSI colors + raw escape codes. Hex colors are more intuitive | Low | Parse `#rrggbb`, convert to truecolor escape sequences |
| Automatic terminal color capability detection | Most colorizers either require truecolor or silently break on limited terminals. Detecting and falling back gracefully is rare | Medium | Detect via `COLORTERM`, `TERM`, terminfo. Fallback chain: truecolor -> 256-color -> 16-color -> no color |
| FORCE_COLOR support | Complement to NO_COLOR; force color output even when piping (useful for `ctail \| less -R`) | Low | Check `FORCE_COLOR` env var per force-color.org spec |
| Fast startup, zero allocation overhead | tailspin is already fast (Rust); ctail can match or beat it by being simpler (no built-in pattern library for dates/UUIDs/etc.) | Low | Compile regexes once at startup, stream-process with minimal allocation |
| Existing ANSI passthrough | When input already contains ANSI colors (e.g., from app that outputs color), strip them before matching but optionally preserve them | High | logalize strips ANSI before matching then re-applies. This is the right approach but adds complexity |
| Rule priority / ordering control | logalize has explicit priority; grc uses file order. Explicit priority prevents surprises when rules overlap | Low | Rules apply in config-file order by default; optional `priority` field |
| `--list-profiles` and `--list-colors` | Discovery UX: let users see what's available without reading docs | Low | Simple introspection commands |

## Anti-Features

Features to explicitly NOT build. These would pull ctail away from its core value proposition.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Log parsing / structured field extraction | lnav already does this and does it extremely well (SQL queries over log fields). Attempting this makes ctail complex and slow. ctail colors lines, it does not understand them | Stay a stream colorizer. Users who need structured log analysis should use lnav |
| TUI / interactive mode | lnav owns this space with scrollback, filtering, SQL. Adding a TUI means maintaining a UI framework, handling resize, input modes, etc. | Output to stdout; let users pipe to `less -R` for scrollback/search |
| Built-in format-aware parsers (syslog, nginx, etc.) | ccze's plugin model for specific log formats is a maintenance burden and makes the tool rigid. tailspin avoids this too | Use regex rules instead. Ship example profile configs for common formats (syslog, nginx, etc.) as files users can copy |
| Remote file tailing / network support | Out of scope per PROJECT.md; adds massive complexity (SSH, auth, reconnection) | Users can `ssh host tail -f /var/log/x \| ctail` |
| Full `tail` feature parity (`-n`, `-c`, `--pid`, `--bytes`) | ctail is a colorizer that happens to follow files, not a tail replacement. Adding all tail flags dilutes focus | Support `-f`, `-F`, and maybe `-n` for convenience. For everything else, pipe: `tail -n 100 file \| ctail` |
| HTML/CSS output mode | ccze supports HTML output. Niche use case that adds complexity | If needed, users can pipe through `ansi2html` from the `colorized-logs` package |
| Log file decompression (gzip, bzip2) | lnav does this; it's a nice feature but adds dependencies and complexity for a stream colorizer | Users can `zcat file.gz \| ctail` |
| Word stemming / inflection matching | logalize matches "complete" -> "completed", "completing". Clever but fragile and hard to reason about | Use regex: `complet(e|ed|ing)` is explicit and predictable |

## Feature Dependencies

```
Regex engine ─────────────> User-defined rules ──────> Config file parsing
                                    │                         │
                                    │                         v
                                    │                  Named profiles
                                    v
                            Per-rule match scope
                            (line/match/capture)

Terminal capability ──────> Color output system ──────> RGB/hex support
detection                         │
                                  v
                          NO_COLOR / FORCE_COLOR

File following (-f) ──────> Follow by name (-F) ──────> Log rotation detection
                                                              │
                                                              v
                                                    Multiple file support

Stdin detection ──────────> Pipe mode (auto-detect tty vs pipe)
```

## MVP Recommendation

**Prioritize (Phase 1 - core value):**
1. Stdin pipe support -- the most common usage pattern (`some-cmd | ctail`)
2. Built-in log level coloring (ERROR/WARN/DEBUG) -- zero-config instant value
3. Regex-based user rules via CLI flag (`-r 'PATTERN=color'`)
4. Basic ANSI 16-color output
5. NO_COLOR support

**Prioritize (Phase 2 - file following):**
1. Follow file (`-f`)
2. Follow by name (`-F`) with rotation detection
3. Multiple file support

**Prioritize (Phase 3 - configuration):**
1. TOML config file with rule definitions
2. Named profiles
3. Per-rule match scope (line/match/capture)
4. RGB/hex color + terminal capability detection with fallback

**Defer:**
- ANSI passthrough: High complexity, handle in a later phase
- `--list-profiles` / `--list-colors`: Nice UX but not critical for launch
- Example profile configs for common formats: Community-driven, ship after core is solid

## Competitor Feature Matrix

| Feature | ctail (planned) | tailspin | grc | ccze | logalize | colortail |
|---------|----------------|----------|-----|------|----------|-----------|
| Zero-config log levels | Yes | Yes (auto-detect many patterns) | No (needs config) | Yes (plugin-based) | Yes | No |
| Stdin pipe | Yes | Yes | Yes (grcat) | Yes | Yes | No |
| Follow file (-f) | Yes | Yes | No | No | No | Yes |
| Follow by name (-F) | Yes | No | No | No | No | No |
| User regex rules | Yes | Yes (theme.toml) | Yes (config files) | No (plugin only) | Yes (yaml) | Yes |
| Config file | TOML | TOML | Custom format | No | YAML | Custom |
| Named profiles | Yes | No | Per-command configs | No | No | Per-file configs |
| Match scope control | line/match/capture | match only | count modes | N/A | format/pattern/word tiers | match only |
| RGB/hex colors | Yes | Yes | No (ANSI only + raw) | No | Yes | No |
| Color capability detection | Yes (planned) | Unknown | No | No | Unknown | No |
| NO_COLOR support | Yes | Yes | No | No | Unknown | No |
| Multiple rules per line | Yes | Yes | Yes (with stop option) | N/A | Yes | Unknown |
| Performance | Rust, minimal | Rust, good | Python, slower | C, fast | Go, fast | C, fast |
| ANSI passthrough | Later | Strips + re-applies | No | No | Yes (strips first) | No |

## Key Insight

The biggest gap in the current landscape is the combination of:
1. **File following with rotation support** (-F) -- tailspin has -f but not -F; grc/ccze/logalize don't follow files at all
2. **Per-rule match scope** -- nobody offers explicit line/match/capture control per rule
3. **Named profiles** -- no tool has a clean "switch your entire rule set" UX

ctail's competitive position is: **simpler than lnav, more configurable than tailspin, faster than grc, and the only tool that combines file following with rich rule configuration.**

## Sources

- [tailspin GitHub](https://github.com/bensadeh/tailspin)
- [grc GitHub](https://github.com/garabik/grc)
- [ccze man page](https://linux.die.net/man/1/ccze)
- [lnav features](https://lnav.org/features)
- [logalize GitHub](https://github.com/deponian/logalize)
- [colortail man page](https://manpages.ubuntu.com/manpages/bionic/man1/colortail.1.html)
- [NO_COLOR standard](https://no-color.org/)
- [FORCE_COLOR standard](https://force-color.org/)
- [tail(1) man page](https://man7.org/linux/man-pages/man1/tail.1.html)

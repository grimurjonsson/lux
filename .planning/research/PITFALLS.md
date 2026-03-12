# Domain Pitfalls

**Domain:** Log colorizer / CLI tail tool (Rust)
**Researched:** 2026-03-12

## Critical Pitfalls

Mistakes that cause rewrites or major issues.

### Pitfall 1: SIGPIPE Panic on Pipe Close

**What goes wrong:** Rust ignores SIGPIPE by default (since 2014). When a user runs `ctail -f app.log | head -20`, the program panics with a "broken pipe" error instead of exiting silently like every other Unix tool. This makes the tool feel broken and unusable in pipelines.
**Why it happens:** Rust sets SIGPIPE to SIG_IGN before `fn main()` runs. When stdout is closed by the downstream process, write calls return `ErrorKind::BrokenPipe` instead of killing the process.
**Consequences:** Ugly error messages on stderr, non-zero exit codes, users lose trust in the tool immediately.
**Prevention:** Handle `BrokenPipe` errors on all stdout writes and exit cleanly with code 0. Alternatively, use the `#[unix_sigpipe = "sig_dfl"]` attribute (nightly only as of early 2026) or reset the signal handler manually at startup. The stable approach is to catch `BrokenPipe` on every write and call `std::process::exit(0)`.
**Detection:** Test with `ctail -f somefile | head -1` early. If you see "broken pipe" on stderr, this is the issue.
**Phase:** Must be addressed in Phase 1 (core I/O pipeline). Retrofitting is easy but embarrassing if shipped without it.

### Pitfall 2: Log Rotation Race Conditions (tail -F)

**What goes wrong:** When following a file by name (`-F`), log rotation creates a window where lines are lost. With `copytruncate` rotation, data written between the copy and truncate is silently dropped. With `rename/create` rotation, the tool may keep reading the renamed (old) file and never notice the new file.
**Why it happens:** File rotation is not atomic. There is always a gap between the rotation action and the tool detecting it. Inode-based following (`-f`) tracks the old file descriptor after rename; name-based following (`-F`) must detect the new file.
**Consequences:** Silent data loss. Users tailing production logs miss critical error messages during rotation events -- exactly when they need them most.
**Prevention:**
- For `-F` (follow by name): poll the file path periodically, compare inode numbers. When inode changes, close old handle and open new file from the beginning.
- For `-f` with truncation: detect file size decrease (current size < last known position) and seek back to the beginning.
- Use the `notify` crate for filesystem events (kqueue on macOS, inotify on Linux) but always have polling as a fallback -- `notify` has known limitations with FSEvents file ownership and symlinks.
- Test with both `copytruncate` and `rename/create` logrotate strategies.
**Detection:** Set up a test with `logrotate` running every few seconds while tailing. Count lines in and lines colorized -- any mismatch is this bug.
**Phase:** Phase 1-2. The `-f` (descriptor follow) is simpler; `-F` (name follow with rotation) should be a separate phase given its complexity.

### Pitfall 3: Overlapping Multi-Rule Color Conflicts

**What goes wrong:** When multiple rules match the same line and their match regions overlap, naive implementations produce garbled ANSI output -- nested/overlapping escape sequences that terminals render unpredictably. For example, rule A colors bytes 10-30 red, rule B colors bytes 20-40 blue. What color is bytes 20-30?
**Why it happens:** ANSI escape codes are stateful -- once you emit a color code, it persists until reset. Overlapping regions require careful tracking of which code is "active" at each byte position. Simply concatenating escape codes creates invalid sequences.
**Consequences:** Garbled terminal output, colors "bleeding" past intended regions, visual noise that defeats the entire purpose of the tool.
**Prevention:**
- Use a position-based approach: for each line, build an array of "color spans" (start, end, style). Resolve overlaps with a priority system (last rule wins, or first rule wins -- pick one and document it).
- Flatten all spans into non-overlapping segments before emitting any ANSI codes.
- Each segment gets exactly one style applied. Reset at the end of each segment before starting the next.
- Always emit a reset (`\x1b[0m`) at end of line.
**Detection:** Write test cases with intentionally overlapping regex rules. Visually inspect output and also assert on the raw byte output.
**Phase:** Phase 2-3 (rule engine). This is an architectural decision that must be designed upfront even if implemented incrementally. Retrofit is painful.

### Pitfall 4: Input Already Contains ANSI Escape Codes

**What goes wrong:** Many programs emit colored output. When piped through ctail, the existing ANSI codes interfere with regex matching (the escape bytes are part of the string) and with ctail's own colorization (double-colorize produces garbled output).
**Why it happens:** Regex patterns match against raw bytes including invisible escape sequences. A rule matching "ERROR" will fail if the input is actually `\x1b[31mERROR\x1b[0m`. Additionally, ctail's color codes interleave with existing codes unpredictably.
**Consequences:** Rules silently fail to match. Colors are wrong or garbled. Users blame ctail when the real issue is upstream colored output.
**Prevention:**
- Strip ANSI escape codes from each line before running regex matches. The standard regex for this: `\x1b\[[0-9;]*[a-zA-Z]` (or use the `strip-ansi-escapes` Rust crate).
- Decide on a policy: (a) strip existing colors and re-colorize, (b) preserve existing colors and only colorize uncolored regions, or (c) offer both via a flag (`--strip-ansi`).
- Option (a) is simplest and most predictable. Option (b) is complex but "nice." Recommend starting with (a) as default.
**Detection:** Pipe the output of a tool that emits colors (e.g., `grep --color=always "ERROR" app.log | ctail`) and check if rules still match.
**Phase:** Phase 2. Must be addressed when building the regex matching engine. Adding it later means changing the core matching pipeline.

## Moderate Pitfalls

### Pitfall 5: Ignoring NO_COLOR and Terminal Detection

**What goes wrong:** The tool emits ANSI codes when piped to a file or when the user has set `NO_COLOR=1`, corrupting output with escape sequences.
**Why it happens:** Not checking `isatty()` on stdout, not respecting the `NO_COLOR` environment variable, not handling `CLICOLOR_FORCE`.
**Prevention:**
- Check `std::io::stdout().is_terminal()` (stable since Rust 1.70).
- Respect `NO_COLOR` env var: when present and non-empty, emit no color codes.
- Respect `CLICOLOR_FORCE`: when present, emit colors even to non-TTY.
- Provide `--color=always|never|auto` CLI flag that overrides everything.
- Precedence order: CLI flag > `NO_COLOR` / `CLICOLOR_FORCE` > TTY detection.
- Use `owo-colors` crate (recommended by Rain's Rust CLI guide) -- it handles much of this.
**Detection:** Run `ctail -f app.log > output.txt` and check if output.txt contains escape sequences. It should not (unless `--color=always`).
**Phase:** Phase 1. Bake this into the output layer from day one. Retrofitting color control into an app that assumed "always color" is tedious.

### Pitfall 6: Regex Compilation on Every Line

**What goes wrong:** Compiling regex patterns inside the hot loop (per-line) instead of once at startup. Performance degrades from microseconds to milliseconds per line, making the tool a bottleneck on high-throughput log streams.
**Why it happens:** Seems obvious in hindsight, but easy to do accidentally when rules are loaded from config and passed around as strings rather than pre-compiled `Regex` objects.
**Prevention:**
- Compile all regex patterns once at startup into a `Vec<Regex>` or similar structure.
- Consider using `RegexSet` from the `regex` crate when you only need to know which patterns match (not where). `RegexSet` can test all patterns simultaneously in a single pass.
- Use `is_match()` when you only need a boolean, `find()` when you need the position, and `captures()` only when you need capture groups. Each is progressively more expensive.
**Detection:** Profile with a large log file (100K+ lines). If ctail is slower than `grep --color`, something is wrong.
**Phase:** Phase 1-2. Design the rule storage as compiled regexes from the start.

### Pitfall 7: Color Capability Detection is Fragile

**What goes wrong:** Assuming truecolor support when the terminal only supports 256 or 16 colors, producing invisible or garbled output. Or being too conservative and only using 16 colors when truecolor is available.
**Why it happens:** Terminal color capability detection is notoriously unreliable. `COLORTERM=truecolor` is the most reliable signal but is not forwarded through SSH or sudo. `TERM` ending in `256color` is a hint but not definitive. CI environments often report as dumb terminals.
**Prevention:**
- Detection priority: `COLORTERM` env var (truecolor/24bit) > `TERM` value (256color suffix) > assume 16 colors.
- Always support graceful degradation: define colors as truecolor values internally but map down to 256/16 approximations at output time.
- Provide `--color-mode=truecolor|256|16` override flag for when detection is wrong.
- Do NOT attempt terminal query sequences (sending escape codes and reading responses) -- they are unreliable and can cause hangs with non-interactive terminals.
**Detection:** Test in multiple terminals: iTerm2, Terminal.app, tmux, screen, basic xterm, and over SSH.
**Phase:** Phase 2-3. Start with 16-color ANSI codes (universally supported), add truecolor in a later phase. This avoids early bikeshedding.

### Pitfall 8: Unbuffered or Wrong-Sized I/O

**What goes wrong:** Using unbuffered reads from stdin or files causes excessive system calls. Alternatively, buffering too aggressively causes latency -- lines appear to "lag" behind real-time, defeating the purpose of `tail -f`.
**Why it happens:** `BufReader` defaults to 8KB buffer. For file tailing, you want line-buffered output (flush after each line) but efficient input reads. The tension between throughput and latency is real.
**Prevention:**
- Input: Use `BufReader` with default or slightly larger buffer for file reads. For stdin, the default 8KB is fine.
- Output: Use `BufWriter` for stdout but flush after every line (or after every N lines with a max latency). For interactive tailing, `stdout().lock()` with explicit line-by-line flushing is the right tradeoff.
- For high-throughput mode (piped input, piped output), consider larger buffers and batch flushes.
**Detection:** Tail a file that gets one line per second. If lines don't appear immediately, buffering is wrong.
**Phase:** Phase 1. Get the I/O pipeline right from the start.

## Minor Pitfalls

### Pitfall 9: Config File Location Non-Standard

**What goes wrong:** Putting config in `~/.ctail.toml` or `~/.ctail/config.toml` instead of following XDG conventions. Users on Linux expect `~/.config/ctail/`, macOS users expect `~/Library/Application Support/` or at least `~/.config/`.
**Prevention:** Use the `dirs` or `directories` crate to resolve platform-appropriate config paths. Support `XDG_CONFIG_HOME` on Linux. Also support a local `.ctail.toml` in the current directory for project-specific profiles.
**Phase:** Phase 2 (config file support).

### Pitfall 10: Not Handling Partial Lines / Lines Without Newlines

**What goes wrong:** Log files sometimes have partial writes (line not yet terminated with newline). `read_line()` blocks until a newline appears, causing the tool to "hang" on the last partial line until more data arrives.
**Prevention:** Use `read_until(b'\n')` or non-blocking reads with a timeout. Display partial lines and update them when the rest arrives. At minimum, flush partial lines after a short timeout (e.g., 200ms).
**Phase:** Phase 2. Not critical for MVP but noticeable in practice.

### Pitfall 11: Forgetting End-of-Line Reset

**What goes wrong:** After colorizing a match in the middle of a line, the color bleeds into the rest of the line (or even subsequent lines) because no reset code was emitted.
**Prevention:** Always emit `\x1b[0m` after every colored span. Emit a final reset at the end of every line. This is slightly wasteful but prevents any color bleeding.
**Phase:** Phase 1. Part of the core colorization output.

### Pitfall 12: Profile/Config TOML Schema Not Versioned

**What goes wrong:** Changing the config format in a later version breaks existing user configs silently or with cryptic errors.
**Prevention:** Include a `version = 1` field in config files from the start. Validate and provide clear error messages on unknown fields. Use `serde`'s `deny_unknown_fields` judiciously.
**Phase:** Phase 2 (config file support).

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Core I/O (Phase 1) | SIGPIPE panic, buffering latency | Handle BrokenPipe from first write; line-buffered output |
| Core I/O (Phase 1) | Color output when not on a TTY | Check `is_terminal()` + `NO_COLOR` from day one |
| File Following (Phase 1-2) | Log rotation data loss | Test with both copytruncate and rename rotation strategies |
| Rule Engine (Phase 2) | Overlapping match garble | Design span-based coloring architecture upfront |
| Rule Engine (Phase 2) | Pre-colored input breaks matching | Strip ANSI before matching, decide on policy early |
| Regex Performance (Phase 2) | Per-line regex compilation | Store compiled `Regex`/`RegexSet`, never strings |
| Config System (Phase 2-3) | Non-standard paths, no schema version | Use `dirs` crate, version config from v1 |
| Color Support (Phase 2-3) | Wrong color mode detection | Graceful degradation truecolor -> 256 -> 16; provide override flag |
| Polish (Phase 3+) | Partial line handling | Timeout-based flush for unterminated lines |

## Sources

- [Rust SIGPIPE issue #46016](https://github.com/rust-lang/rust/issues/46016)
- [Tracking issue for unix_sigpipe #97889](https://github.com/rust-lang/rust/issues/97889)
- [Rain's Rust CLI - Managing Colors](https://rust-cli-recommendations.sunshowers.io/managing-colors-in-rust.html)
- [NO_COLOR standard](https://no-color.org/)
- [Terminal color detection pitfalls](https://marvinh.dev/blog/terminal-colors/)
- [Terminal color standards](https://github.com/termstandard/colors)
- [Logrotate copytruncate race condition](https://incoherency.co.uk/blog/stories/logrotate-copytruncate-race-condition.html)
- [Rust regex performance discussion #960](https://github.com/rust-lang/regex/discussions/960)
- [Rust I/O performance book](https://nnethercote.github.io/perf-book/io.html)
- [notify-rs crate](https://github.com/notify-rs/notify)
- [logalize - ccze alternative](https://github.com/deponian/logalize)
- [Fluent Bit log rotation issues](https://github.com/fluent/fluent-bit/issues/2110)

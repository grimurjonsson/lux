# Phase 3: File Following - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Follow files by descriptor (`-f`) and by name (`-F`) with log rotation handling. Supports configurable initial line count (`-n`), both rename/create and copytruncate rotation strategies. Single file only — multiple files are v2.

</domain>

<decisions>
## Implementation Decisions

### Initial content on start
- Default behavior: show last 10 lines then follow (matches tail -f)
- `-n N` flag: show last N lines (`-n 0` for new-only)
- `-n +N` syntax: start from line N (show from beginning with `-n +1`)
- `-n` works standalone: `ctail -n 20 app.log` prints last 20 colored lines and exits
- `-n` works with `-f`/`-F`: show last N lines then follow
- `-n` silently ignored in pipe mode (stdin)

### Rotation feedback
- Normal rotation (rename/create, copytruncate): silent — no stderr messages, seamless continuation
- Abnormal events (file disappeared, permission error, initial missing file): stderr warnings
- After rotation, read new file from beginning (don't skip existing content in new file)
- Copytruncate detection: if file size < last known position, assume truncation, seek to beginning

### Missing file behavior
- `-f` (follow descriptor): file must exist at startup, error if missing
- `-F` (follow name): if file doesn't exist, print stderr warning and wait/poll until it appears
- `-f` file deleted mid-follow: keep reading until EOF on existing fd, then exit (no warning)
- `-F` file disappeared: stderr warning ("disappeared, waiting..."), poll until reappears, stderr notice on reappear
- `-F` errors (permission denied on new file): stderr warnings, keep trying

### Input mode switching
- File arg is positional: `ctail app.log`, `ctail -f app.log`, `ctail -F app.log`
- `ctail app.log` (no -f/-F flag) implies `-n 10 -F` — show last 10 lines, follow by name
- No file arg: read stdin (current pipe behavior, unchanged)
- File arg + piped stdin: error and exit ("cannot use file argument with piped input")
- `-f` and `-F` are boolean flags, not value flags — file is always positional

### Claude's Discretion
- Poll interval for file following (e.g., 100ms, 250ms, 1s)
- Whether to use `notify` crate (inotify/kqueue) vs pure polling
- Internal buffering strategy for file reads
- How to efficiently seek to last N lines without reading entire file
- Exact stderr warning message wording

</decisions>

<specifics>
## Specific Ideas

- "Like tail -f but with colors" — the primary mental model (carried from Phase 1)
- `ctail app.log` should just work with no flags — the most common case should be the simplest invocation
- Bare file arg implying -F (not -f) means rotation handling by default — the safer choice for log files

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Engine::apply(&str) -> String`: works on any line regardless of source — no changes needed for file input
- `build_rules_with_config()`: full rule assembly with config/profile/CLI priority — reusable as-is
- `detect_color_mode()`: color detection — works for file output too
- `BufWriter` + `writeln!` pattern: established output approach
- SIGPIPE/BrokenPipe handling in `main()`: keeps working for file mode

### Established Patterns
- `clap::Parser` derive for CLI — add `-f`, `-F`, `-n`, positional file arg
- `anyhow::Result` error handling throughout
- Early-exit pattern for discovery commands (main.rs:16-23) — model for print-and-exit with `-n` standalone

### Integration Points
- `main.rs` input loop: currently `stdin.lines()` — needs to branch: stdin vs file-follow based on args
- `Cli` struct (cli.rs): needs `-f`, `-F`, `-n` flags and optional positional file arg
- `main()` error handling: needs to distinguish file errors from pipe errors

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 03-file-following*
*Context gathered: 2026-03-12*

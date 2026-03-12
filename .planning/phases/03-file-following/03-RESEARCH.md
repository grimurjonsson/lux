# Phase 3: File Following - Research

**Researched:** 2026-03-12
**Domain:** File I/O, filesystem watching, log rotation detection (Rust)
**Confidence:** HIGH

## Summary

This phase adds file-following capability to ctail, making it a direct replacement for `tail -f`/`tail -F` with colorized output. The core challenge is three-fold: (1) efficiently reading the last N lines from a file without loading it entirely, (2) detecting new content appended to a file, and (3) handling log rotation (both rename/create and copytruncate strategies) seamlessly.

The recommended approach uses the `notify` crate (v8.2.0) for OS-native filesystem events (kqueue on macOS, inotify on Linux) with a polling fallback, combined with inode/device identity checks via `std::os::unix::fs::MetadataExt` for rotation detection. The `notify` crate is the de facto standard in the Rust ecosystem, used by rust-analyzer, cargo-watch, deno, and alacritty. For the "last N lines" feature, a backward-seeking algorithm on `BufReader` avoids reading the entire file.

**Primary recommendation:** Use `notify` 8.x for filesystem events, `std::os::unix::fs::MetadataExt` for rotation identity checks, and pure stdlib `BufReader`/`Seek` for file reading. Keep polling as the fallback strategy (not primary). Structure the new code as a `follow` module separate from the existing stdin pipeline.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Default behavior: show last 10 lines then follow (matches tail -f)
- `-n N` flag: show last N lines (`-n 0` for new-only)
- `-n +N` syntax: start from line N (show from beginning with `-n +1`)
- `-n` works standalone: `ctail -n 20 app.log` prints last 20 colored lines and exits
- `-n` works with `-f`/`-F`: show last N lines then follow
- `-n` silently ignored in pipe mode (stdin)
- Normal rotation (rename/create, copytruncate): silent -- no stderr messages, seamless continuation
- Abnormal events (file disappeared, permission error, initial missing file): stderr warnings
- After rotation, read new file from beginning (don't skip existing content in new file)
- Copytruncate detection: if file size < last known position, assume truncation, seek to beginning
- `-f` (follow descriptor): file must exist at startup, error if missing
- `-F` (follow name): if file doesn't exist, print stderr warning and wait/poll until it appears
- `-f` file deleted mid-follow: keep reading until EOF on existing fd, then exit (no warning)
- `-F` file disappeared: stderr warning ("disappeared, waiting..."), poll until reappears, stderr notice on reappear
- `-F` errors (permission denied on new file): stderr warnings, keep trying
- File arg is positional: `ctail app.log`, `ctail -f app.log`, `ctail -F app.log`
- `ctail app.log` (no -f/-F flag) implies `-n 10 -F` -- show last 10 lines, follow by name
- No file arg: read stdin (current pipe behavior, unchanged)
- File arg + piped stdin: error and exit ("cannot use file argument with piped input")
- `-f` and `-F` are boolean flags, not value flags -- file is always positional

### Claude's Discretion
- Poll interval for file following (e.g., 100ms, 250ms, 1s)
- Whether to use `notify` crate (inotify/kqueue) vs pure polling
- Internal buffering strategy for file reads
- How to efficiently seek to last N lines without reading entire file
- Exact stderr warning message wording

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INP-02 | User can follow a file by descriptor (`ctail -f app.log`) | `notify` crate for modify events + `BufReader` with seek-based reading; `-f` follows the open fd, stops at EOF after delete |
| INP-03 | User can follow a file by name with rotation handling (`ctail -F app.log`) | inode/dev identity check via `MetadataExt` to detect rotation; watch parent directory for create events; reopen file on rotation |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| notify | 8.2.0 | Cross-platform filesystem event notifications | De facto standard; used by rust-analyzer, cargo-watch, deno; wraps kqueue/inotify/FSEvents natively |
| std::os::unix::fs::MetadataExt | stdlib | inode/device identity for rotation detection | No external dep needed; provides `.ino()` and `.dev()` |
| std::io::BufReader + Seek | stdlib | Efficient file reading with seek | Standard approach for tail-like reading |
| std::fs::File | stdlib | File handle management | Standard; needed for fd-based following |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tempfile | 3.27.0 | Temporary files in tests | Already in dev-dependencies; use for rotation tests |
| assert_cmd | 2.2.0 | CLI integration testing | Already in dev-dependencies |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| notify crate | Pure polling (sleep + stat) | Simpler, no dependency, but higher latency or CPU; notify gives instant detection |
| notify crate | libc inotify/kqueue directly | More control but much more code; notify abstracts cross-platform differences |

**Installation:**
```bash
cargo add notify@8.2
```

## Architecture Patterns

### Recommended Project Structure
```
src/
├── cli.rs           # Add -f, -F, -n flags and positional file arg
├── follow.rs        # NEW: file following logic (core of this phase)
├── tail.rs          # NEW: last-N-lines reading (seek-from-end algorithm)
├── main.rs          # Branch: stdin mode vs file mode based on args
├── engine.rs        # Unchanged -- apply() works on any &str
├── color.rs         # Unchanged
├── config.rs        # Unchanged
├── output.rs        # Unchanged
├── rules.rs         # Unchanged
└── lib.rs           # Add pub mod follow; pub mod tail;
```

### Pattern 1: Input Mode Branching
**What:** main.rs decides between stdin-pipe mode and file-follow mode based on CLI args.
**When to use:** At startup, after parsing CLI args.
**Example:**
```rust
// In run() after CLI parsing and engine setup:
if let Some(ref file_path) = cli.file {
    // Check for conflicting piped stdin
    if atty::isnt(atty::Stream::Stdin) {
        anyhow::bail!("cannot use file argument with piped input");
    }
    // Determine follow mode
    let mode = if cli.follow_descriptor {
        FollowMode::Descriptor
    } else {
        FollowMode::Name // default when bare file arg or -F
    };
    follow::run(file_path, mode, cli.lines, &engine, &mut writer)?;
} else {
    // Existing stdin pipe mode
    let stdin = io::stdin().lock();
    for line in stdin.lines() { /* ... */ }
}
```

### Pattern 2: Seek-from-End for Last N Lines
**What:** Read last N lines without loading entire file by seeking backward from EOF in chunks.
**When to use:** On initial file open, before following begins.
**Example:**
```rust
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

/// Read last `n` lines from a seekable reader.
/// Algorithm: seek to end, read backward in chunks, count newlines.
fn read_last_n_lines(file: &mut std::fs::File, n: usize) -> anyhow::Result<Vec<String>> {
    let file_len = file.seek(SeekFrom::End(0))?;
    if file_len == 0 || n == 0 {
        return Ok(vec![]);
    }

    let chunk_size: u64 = 8192;
    let mut lines_found = Vec::new();
    let mut remaining = file_len;
    let mut buf = Vec::new();

    while lines_found.len() <= n && remaining > 0 {
        let read_size = chunk_size.min(remaining);
        remaining -= read_size;
        file.seek(SeekFrom::Start(remaining))?;

        let mut chunk = vec![0u8; read_size as usize];
        file.read_exact(&mut chunk)?;
        chunk.append(&mut buf);
        buf = chunk;

        // Count newlines in accumulated buffer
        lines_found = buf.split(|&b| b == b'\n')
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
    }

    // Take last n lines (skip empty trailing element from final newline)
    let result: Vec<String> = lines_found.into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    let start = result.len().saturating_sub(n);
    Ok(result[start..].to_vec())
}
```

### Pattern 3: Rotation Detection via Inode Identity
**What:** Compare inode+device of the path vs the currently-open file to detect rotation.
**When to use:** In `-F` (follow-name) mode, after receiving filesystem events or periodically.
**Example:**
```rust
use std::os::unix::fs::MetadataExt;

struct FileIdentity {
    dev: u64,
    ino: u64,
}

impl FileIdentity {
    fn from_metadata(meta: &std::fs::Metadata) -> Self {
        Self {
            dev: meta.dev(),
            ino: meta.ino(),
        }
    }

    fn matches(&self, other: &FileIdentity) -> bool {
        self.dev == other.dev && self.ino == other.ino
    }
}

// During follow-name loop:
// 1. Get identity of currently open fd: fstat via file.metadata()
// 2. Get identity of path: stat via std::fs::metadata(path)
// 3. If they differ -> rotation happened -> drain old fd, open new file from beginning
```

### Pattern 4: Notify Watcher with Channel
**What:** Use notify's `recommended_watcher` with `mpsc::channel` for non-blocking event reception.
**When to use:** Primary event loop for detecting file modifications.
**Example:**
```rust
use notify::{Event, RecursiveMode, Watcher, EventKind};
use std::sync::mpsc;
use std::time::Duration;

let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
let mut watcher = notify::recommended_watcher(tx)?;

// For -F mode: watch parent directory to catch file creation after rotation
let parent = path.parent().unwrap_or(Path::new("."));
watcher.watch(parent, RecursiveMode::NonRecursive)?;

// Event loop with timeout for periodic checks
loop {
    match rx.recv_timeout(Duration::from_millis(250)) {
        Ok(Ok(event)) => {
            match event.kind {
                EventKind::Modify(_) => { /* read new data */ }
                EventKind::Create(_) => { /* check if our file was recreated */ }
                EventKind::Remove(_) => { /* file deleted */ }
                _ => {}
            }
        }
        Ok(Err(e)) => eprintln!("ctail: watch error: {e}"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Periodic fallback: check file size/identity
            // Handles cases where events are missed
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => break,
    }
}
```

### Anti-Patterns to Avoid
- **Busy-polling without sleep:** Never `loop { read(); }` -- use notify events with polling fallback.
- **Watching the file directly (not parent) for -F mode:** After rotation the old file is watched, not the new one. Watch the parent directory to detect creation of the new file at the same path.
- **Using `lines()` iterator for following:** The `BufRead::lines()` iterator returns `None` at EOF and terminates. For following, use `read_line()` in a loop and handle the zero-bytes-read case (EOF, wait for more).
- **Reopening file on every read:** Keep the fd open and seek. Only reopen on rotation.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Cross-platform filesystem events | Custom inotify/kqueue bindings | `notify` 8.2 crate | Handles macOS/Linux/Windows differences, tested extensively |
| Temporary files in tests | Manual file creation/cleanup | `tempfile` crate | Already a dev-dep; handles cleanup, unique names |
| Piped-stdin detection | Manual fd checks | `std::io::stdin().is_terminal()` (Rust 1.70+ `IsTerminal` trait) | Stable stdlib, no dep needed |

**Key insight:** The notify crate abstracts significant platform complexity (kqueue on macOS, inotify on Linux, FSEvents as alternative macOS backend). Hand-rolling filesystem watching would be a multi-week effort to get right across platforms.

## Common Pitfalls

### Pitfall 1: BufRead::lines() Terminates at EOF
**What goes wrong:** Using `stdin.lines()` pattern for file following causes immediate exit when reaching end of file.
**Why it happens:** `lines()` returns `None` when the underlying reader returns 0 bytes, which is normal EOF for files.
**How to avoid:** Use `read_line()` in a loop. When it returns `Ok(0)`, that means EOF -- wait for events, then try again.
**Warning signs:** File follow exits immediately after printing existing content.

### Pitfall 2: Watching the File Instead of Parent Directory for -F Mode
**What goes wrong:** After log rotation, the watcher is still watching the old (renamed) file, missing the new file created at the original path.
**Why it happens:** `notify` watches by inode, not by path. When the file is renamed, the watch follows the inode.
**How to avoid:** Watch the parent directory with `RecursiveMode::NonRecursive`. Filter events by matching the filename.
**Warning signs:** `-F` mode stops producing output after rotation.

### Pitfall 3: Race Condition Between Rotation and Reading
**What goes wrong:** Lines written between rotation detection and reopening the new file are lost.
**Why it happens:** The old file may have had lines appended after the last read but before rotation was detected.
**How to avoid:** On rotation detection: (1) drain remaining content from old fd, (2) then open new file from beginning. This is the order GNU tail uses.
**Warning signs:** Missing lines around rotation boundaries.

### Pitfall 4: Copytruncate Losing Position
**What goes wrong:** After copytruncate rotation, reads return empty or garbage because the read position is beyond the new (smaller) file size.
**Why it happens:** Copytruncate copies the file then truncates the original. The fd remains valid but the file is now smaller than the seek position.
**How to avoid:** Before each read, check `file.metadata()?.len()` against last known position. If smaller, seek to 0 (or to beginning) and read from there.
**Warning signs:** No output after copytruncate rotation, or "invalid seek" errors.

### Pitfall 5: notify on macOS kqueue -- Duplicate Events
**What goes wrong:** A single file write triggers two modify events.
**Why it happens:** kqueue reports both NOTE_WRITE and NOTE_EXTEND, which notify surfaces as two events.
**How to avoid:** Debounce: after receiving a modify event, drain all pending events (with zero timeout) before reading. Or simply: on any modify event, read all available data regardless.
**Warning signs:** Lines printed twice, or unnecessary wake-ups.

### Pitfall 6: Stdin Detection for Conflict Check
**What goes wrong:** `ctail app.log` while also receiving piped stdin silently ignores one input.
**Why it happens:** Without checking, both stdin and file could be active.
**How to avoid:** Use `std::io::stdin().is_terminal()` (stable since Rust 1.70) to detect whether stdin is a pipe. If stdin is piped AND a file arg is given, error out.
**Warning signs:** Confusing behavior where piped input is silently dropped.

## Code Examples

### CLI Additions (clap derive)
```rust
/// Colored tail - instantly readable log output
#[derive(Parser)]
#[command(name = "ctail")]
pub struct Cli {
    /// File to read (if omitted, reads stdin)
    pub file: Option<String>,

    /// Follow file by descriptor (stop if file is deleted)
    #[arg(short = 'f')]
    pub follow_descriptor: bool,

    /// Follow file by name (handle log rotation)
    #[arg(short = 'F', conflicts_with = "follow_descriptor")]
    pub follow_name: bool,

    /// Number of lines to show initially (default: 10)
    /// Use +N to start from line N
    #[arg(short = 'n', default_value = "10")]
    pub lines: String,  // String to support +N syntax

    // ... existing fields ...
}
```

### Detecting Piped Stdin
```rust
use std::io::IsTerminal;

if cli.file.is_some() && !std::io::stdin().is_terminal() {
    anyhow::bail!("cannot use file argument with piped input");
}
```

### Reading New Data After Event
```rust
/// Read any new data appended since our last position.
fn read_new_lines(
    reader: &mut BufReader<File>,
    engine: &Engine,
    writer: &mut BufWriter<impl Write>,
) -> anyhow::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break; // EOF -- no more data right now
        }
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        let output = engine.apply(trimmed);
        writeln!(writer, "{output}")?;
    }
    writer.flush()?;
    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Pure polling (sleep + stat) | OS event APIs (inotify/kqueue) + polling fallback | notify crate since 2014, mature by v5+ | Sub-second latency for new lines without CPU waste |
| `atty` crate for terminal detection | `std::io::IsTerminal` (stdlib) | Rust 1.70 (June 2023) | No need for `atty` crate; use stdlib `is_terminal()` |
| notify 6.x with backend issues | notify 8.2.0 | 2025 | More reliable cross-platform events |

**Deprecated/outdated:**
- `atty` crate: replaced by stdlib `IsTerminal` trait (Rust 1.70+)
- notify 5.x/6.x: use 8.2.0 for current APIs

## Open Questions

1. **notify vs pure polling recommendation**
   - What we know: notify gives instant event delivery; polling introduces latency proportional to interval. For a colorizing tail tool, sub-second response is expected.
   - Recommendation: Use notify as primary, with a 250ms `recv_timeout` as periodic fallback (handles missed events, copytruncate size checks). This gives best UX with safety net.

2. **Poll interval for -F mode when file is missing**
   - What we know: GNU tail uses 1 second default for checking disappeared files.
   - Recommendation: 1 second poll when file is missing/disappeared (matches user expectation from tail). 250ms timeout in normal following for event-based responsiveness.

3. **`-n +N` parsing**
   - What we know: Needs to parse both "10" (last 10) and "+10" (from line 10). clap doesn't natively support this.
   - Recommendation: Accept `-n` as `String`, parse manually: if starts with `+`, interpret as from-line-N; otherwise parse as usize for last-N.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + assert_cmd 2.2.0 + tempfile 3.27.0 |
| Config file | Cargo.toml `[dev-dependencies]` |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INP-02 | Follow file by descriptor, see new lines colored | integration | `cargo test --test integration follow_descriptor` | No -- Wave 0 |
| INP-02 | `-f` errors if file missing at startup | integration | `cargo test --test integration follow_missing_file` | No -- Wave 0 |
| INP-02 | `-f` exits at EOF after file deleted | integration | `cargo test --test integration follow_descriptor_deleted` | No -- Wave 0 |
| INP-03 | Follow file by name, see new lines after rotation | integration | `cargo test --test integration follow_name_rotation` | No -- Wave 0 |
| INP-03 | Copytruncate rotation detection | integration | `cargo test --test integration follow_copytruncate` | No -- Wave 0 |
| INP-03 | `-F` waits for file to appear | integration | `cargo test --test integration follow_name_missing` | No -- Wave 0 |
| INP-02/03 | `-n N` shows last N lines | unit | `cargo test --lib tail::tests` | No -- Wave 0 |
| INP-02/03 | `-n +N` shows from line N | unit | `cargo test --lib tail::tests::from_line` | No -- Wave 0 |
| INP-02/03 | `-n` standalone prints and exits | integration | `cargo test --test integration n_standalone` | No -- Wave 0 |
| INP-02/03 | Bare file arg implies -F | integration | `cargo test --test integration bare_file_follows` | No -- Wave 0 |
| INP-02/03 | File + piped stdin errors | integration | `cargo test --test integration file_stdin_conflict` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src/tail.rs` -- last-N-lines reading with unit tests
- [ ] `src/follow.rs` -- file following logic with unit tests
- [ ] Integration tests for file following in `tests/integration.rs`
- [ ] Note: `tempfile` and `assert_cmd` already in dev-dependencies -- no framework install needed

## Sources

### Primary (HIGH confidence)
- [notify crate docs.rs](https://docs.rs/notify/latest/notify/) -- API, version 8.2.0, usage patterns
- [std::os::unix::fs::MetadataExt](https://doc.rust-lang.org/std/os/unix/fs/trait.MetadataExt.html) -- inode/dev methods
- [std::io::BufReader](https://doc.rust-lang.org/std/io/struct.BufReader.html) -- seek behavior with internal buffer
- [GNU coreutils tail invocation](https://www.gnu.org/software/coreutils/manual/html_node/tail-invocation.html) -- reference behavior for -f/-F semantics

### Secondary (MEDIUM confidence)
- [notify-rs/notify GitHub](https://github.com/notify-rs/notify) -- kqueue issues, event patterns
- [uutils/coreutils](https://github.com/uutils/coreutils) -- Rust tail implementation reference
- [rsanden/logwatch](https://github.com/rsanden/logwatch) -- tail -F clone in Rust, rotation handling patterns

### Tertiary (LOW confidence)
- [notify kqueue macOS issues](https://github.com/notify-rs/notify/issues/596) -- duplicate events on macOS; mitigated by read-all-available-data pattern

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- notify is well-established, stdlib for file I/O is definitive
- Architecture: HIGH -- patterns derived from GNU tail behavior docs and existing Rust implementations
- Pitfalls: HIGH -- documented issues in notify GitHub, well-known tail implementation challenges

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 (stable domain, slow-moving)

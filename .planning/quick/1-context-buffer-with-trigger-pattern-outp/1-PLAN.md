---
phase: quick
plan: 1
type: execute
wave: 1
depends_on: []
files_modified:
  - src/cli.rs
  - src/trigger.rs
  - src/lib.rs
  - src/main.rs
  - src/follow.rs
  - tests/integration.rs
autonomous: true
requirements: [TRIGGER-01]
must_haves:
  truths:
    - "Without --trigger, ctail behavior is completely unchanged"
    - "With --trigger PATTERN, output is suppressed until a trigger match"
    - "On trigger match, previous N lines from rolling buffer are output (--before N, default 20)"
    - "After trigger match, next N lines are output (--after N, default 20)"
    - "New trigger during after-window resets the after counter"
    - "Multiple --trigger flags are allowed and OR'd together"
  artifacts:
    - path: "src/trigger.rs"
      provides: "TriggerFilter with rolling buffer and state machine"
    - path: "src/cli.rs"
      provides: "--trigger, --before, --after CLI flags"
  key_links:
    - from: "src/main.rs"
      to: "src/trigger.rs"
      via: "TriggerFilter wraps engine output in stdin and file modes"
      pattern: "trigger_filter\\.process_line"
    - from: "src/follow.rs"
      to: "src/trigger.rs"
      via: "TriggerFilter passed to read_new_lines for follow mode"
      pattern: "TriggerFilter"
---

<objective>
Add context buffer with trigger-based output gating to ctail.

Purpose: Allow users to suppress noisy log output and only see lines surrounding trigger matches (like grep -B/-A but integrated into the live tail+color pipeline).

Output: Working --trigger PATTERN --before N --after N flags that gate output through a rolling buffer state machine.
</objective>

<execution_context>
@/Users/gimmi/.claude/get-shit-done/workflows/execute-plan.md
@/Users/gimmi/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@src/cli.rs
@src/main.rs
@src/follow.rs
@src/engine.rs
@src/lib.rs
@tests/integration.rs

<interfaces>
From src/engine.rs:
```rust
pub struct Engine {
    rules: Vec<Rule>,
    color_enabled: bool,
}
impl Engine {
    pub fn new(rules: Vec<Rule>, color_enabled: bool) -> Self;
    pub fn apply(&self, line: &str) -> String;
}
```

From src/follow.rs:
```rust
fn read_new_lines(
    reader: &mut BufReader<File>,
    engine: &Engine,
    writer: &mut BufWriter<impl Write>,
) -> anyhow::Result<usize>;

pub fn run(path: &Path, mode: FollowMode, file: File, engine: &Engine, writer: &mut BufWriter<impl Write>) -> anyhow::Result<()>;
pub fn run_waiting(path: &Path, engine: &Engine, writer: &mut BufWriter<impl Write>) -> anyhow::Result<()>;
```

From src/main.rs:
```rust
fn print_lines(lines: &[String], engine: &Engine, writer: &mut BufWriter<impl Write>) -> anyhow::Result<()>;
```
</interfaces>
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Add CLI flags and TriggerFilter module</name>
  <files>src/cli.rs, src/trigger.rs, src/lib.rs</files>
  <behavior>
    - CLI: `--trigger PATTERN` accepts multiple values via Append action, stored as Vec&lt;String&gt;
    - CLI: `--before N` defaults to 20, stored as usize
    - CLI: `--after N` defaults to 20, stored as usize
    - CLI: parsing "--trigger ERROR --trigger WARN --before 5 --after 10" produces correct struct
    - CLI: without --trigger, trigger vec is empty (no behavior change)
    - TriggerFilter::new(patterns, before, after) compiles patterns as regex::Regex
    - TriggerFilter with empty patterns: process_line always returns OutputDecision::Pass(line)
    - TriggerFilter state machine — Suppressing state: lines go into rolling VecDeque buffer of capacity `before`, process_line returns OutputDecision::Suppress
    - TriggerFilter state machine — on trigger match while Suppressing: flush buffer contents via OutputDecision::Flush(vec_of_buffered_lines), then transition to Emitting state with after_remaining = after + 1 (include trigger line)
    - TriggerFilter state machine — Emitting state: process_line returns OutputDecision::Pass(line), decrements after_remaining, transitions to Suppressing when after_remaining hits 0
    - TriggerFilter — new trigger during Emitting state: resets after_remaining to after + 1 (include re-trigger line)
    - TriggerFilter — trigger match checks raw (uncolored) line against compiled regex patterns
    - Separator: when flushing buffer after being in Suppressing state (not the very first trigger), include a "--" separator line before the buffer contents to visually separate context groups (like grep -C does)
  </behavior>
  <action>
    1. In src/cli.rs: Add three new fields to Cli struct:
       - `trigger: Vec<String>` with `#[arg(long = "trigger", action = clap::ArgAction::Append)]`
       - `before: usize` with `#[arg(long, default_value = "20")]`
       - `after: usize` with `#[arg(long, default_value = "20")]`
       Add unit tests for new CLI parsing.

    2. Create src/trigger.rs with:
       - `enum OutputDecision { Pass(String), Flush(Vec<String>), Suppress }` — the colored string or vec of colored strings
       - `enum State { Suppressing, Emitting { remaining: usize } }`
       - `struct TriggerFilter { patterns: Vec<regex::Regex>, buffer: VecDeque<String>, before: usize, after: usize, state: State, has_emitted: bool }`
       - `TriggerFilter::new(patterns: &[String], before: usize, after: usize) -> Result<Self>` — compiles patterns, returns error on bad regex
       - `TriggerFilter::is_active(&self) -> bool` — true if patterns is non-empty (used to skip filter entirely)
       - `fn process_line(&mut self, raw_line: &str, colored_line: String) -> OutputDecision`:
         - If patterns is empty: return Pass(colored_line) always
         - Check if any pattern matches raw_line
         - In Suppressing state with no match: push colored_line to buffer (evict front if over capacity), return Suppress
         - In Suppressing state with match: build flush vec (optionally prefixed with "--" separator if has_emitted), drain buffer into it, append colored_line, set state to Emitting { remaining: after }, set has_emitted = true, return Flush(flush_vec)
         - In Emitting state with no match: decrement remaining, if remaining hits 0 transition to Suppressing, return Pass(colored_line)
         - In Emitting state with match: reset remaining to after, return Pass(colored_line)

    3. In src/lib.rs: Add `pub mod trigger;`

    The regex crate is already a transitive dependency through owo-colors/rules, but verify it's in Cargo.toml directly. If not, add `regex` to [dependencies].

    IMPORTANT: process_line takes raw_line (pre-coloring) for pattern matching but colored_line (post-engine) for output. This avoids matching against ANSI escape codes.
  </action>
  <verify>
    <automated>cargo test --lib -- trigger:: cli::tests --no-fail-fast</automated>
  </verify>
  <done>TriggerFilter passes all unit tests: empty patterns passthrough, buffer filling, trigger flush with separator, after-window countdown, re-trigger reset. CLI parses --trigger/--before/--after correctly.</done>
</task>

<task type="auto">
  <name>Task 2: Integrate TriggerFilter into main and follow pipelines</name>
  <files>src/main.rs, src/follow.rs, tests/integration.rs</files>
  <action>
    1. In src/main.rs:
       - Import TriggerFilter from ctail::trigger
       - After building Engine, construct TriggerFilter: `TriggerFilter::new(&cli.trigger, cli.before, cli.after)?`
       - If trigger_filter.is_active() is false, all existing code paths remain EXACTLY unchanged (no performance overhead)
       - If trigger_filter.is_active() is true:
         a. Stdin pipe mode: for each line, call `engine.apply(&line)` to get colored, then `trigger_filter.process_line(&line, colored)`. On Pass(s), writeln. On Flush(vec), write each line. On Suppress, skip.
         b. print_lines (initial file lines): run them through trigger_filter too, so initial context is also gated
         c. For follow mode: pass trigger_filter into follow::run and follow::run_waiting

    2. In src/follow.rs:
       - Change `read_new_lines` signature to accept `Option<&mut TriggerFilter>`:
         `fn read_new_lines(reader: &mut BufReader<File>, engine: &Engine, writer: &mut BufWriter<impl Write>, trigger: Option<&mut TriggerFilter>) -> anyhow::Result<usize>`
       - When trigger is Some: for each line read, apply engine, then process through trigger filter. Handle OutputDecision same as stdin mode.
       - When trigger is None: existing behavior unchanged (backward compatible)
       - Update `run` and `run_waiting` signatures to accept `Option<TriggerFilter>` (owned, passed by value, stored as mutable local). Pass `trigger.as_mut()` to read_new_lines.
       - Update ALL call sites of read_new_lines in follow.rs (there are ~8 calls across run and run_waiting)

    3. In tests/integration.rs: Add integration tests:
       - `trigger_basic`: echo lines through ctail with --trigger, verify only matching context appears
       - `trigger_before_after`: verify --before and --after control window sizes
       - `trigger_passthrough`: without --trigger flag, output is identical to normal mode
       - `trigger_multiple`: multiple --trigger flags, verify OR behavior
       - `trigger_separator`: verify "--" separator between trigger groups

       Pattern for integration tests (from existing tests): spawn ctail process, pipe input via stdin, capture stdout, assert on output lines.
  </action>
  <verify>
    <automated>cargo test --no-fail-fast 2>&1</automated>
  </verify>
  <done>All existing tests pass (no regression). New integration tests pass. `echo -e "line1\nline2\nERROR here\nline3" | cargo run -- --trigger ERROR --before 1 --after 1 --color never` outputs separator + "line2" + "ERROR here" + "line3". Without --trigger, all lines appear.</done>
</task>

</tasks>

<verification>
1. `cargo test --no-fail-fast` — all unit + integration tests pass
2. `cargo clippy -- -D warnings` — no warnings
3. Manual smoke test: `echo -e "a\nb\nc\nERROR boom\nd\ne\nf" | cargo run -- --trigger ERROR --before 2 --after 1 --color never` outputs "b", "c", "ERROR boom", "d"
4. Manual passthrough test: `echo -e "hello\nworld" | cargo run -- --color never` outputs "hello", "world" (unchanged)
</verification>

<success_criteria>
- --trigger PATTERN suppresses output until trigger match, then shows before+after context window
- --before N and --after N control window sizes (default 20)
- Multiple --trigger flags OR together
- New trigger during after-window resets the after counter
- Without --trigger, ctail works identically to before (zero overhead path)
- All existing tests continue to pass
- New unit and integration tests cover trigger behavior
</success_criteria>

<output>
After completion, create `.planning/quick/1-context-buffer-with-trigger-pattern-outp/1-SUMMARY.md`
</output>

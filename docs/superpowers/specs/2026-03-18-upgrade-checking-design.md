# Upgrade Checking Design

## Goal

Add background version checking and a `lux update` subcommand so users know when a new version is available and can upgrade easily — without impacting normal operation.

## Constraints

- **stdout is sacred** — all update messaging goes to stderr only, never corrupting piped output
- **No new Rust dependencies** — shells out to `curl` (already required by install script)
- **Minimal latency impact** — background thread for passive checks, synchronous only for `lux update`
- **First-class disable** — configurable interval, explicit disable option

## Architecture

A new `update` module (`src/update.rs`) owns all upgrade-checking logic. It manages a state file separate from the user's config.

### State file: `~/.config/lux/update.toml`

Machine-managed state file. Sits alongside `config.toml`. May be edited by the user but is primarily written by the app.

```toml
last_checked = "2026-03-18T14:30:00Z"
latest_version = "0.2.0"
skipped_versions = ["0.1.2"]
```

Fields:
- `last_checked` — ISO 8601 timestamp of last successful API check
- `latest_version` — cached version from last check (avoids re-checking within interval)
- `skipped_versions` — versions the user explicitly chose to skip

A missing `update.toml` is treated as `{ last_checked: never, latest_version: none, skipped_versions: [] }`. The file is only created/written after the first successful API check or preference save.

Concurrent writes from multiple lux instances are benign (worst case: an extra API call). No file locking.

### Config integration

User preferences for update behavior live in `config.toml`:

```toml
# How often to check for updates (default: 7, set to 0 to disable)
update_check_interval_days = 7

# What to do when an update is found: "notify" | "auto" | "disabled"
# Not set by default — first detection triggers interactive prompt
update_mode = "notify"
```

- `update_check_interval_days` — days between API checks. Default 7. Set to 0 to disable.
- `update_mode` — user's preference, initially unset:
  - (unset) — first detection triggers interactive prompt
  - `"notify"` — print one-line stderr message after output completes
  - `"auto"` — download and replace binary automatically after output completes
  - `"disabled"` — no checks, no messages

### CLI surface

New subcommand (consistent with existing `profile` and `completions` subcommands):

```
lux update    Check for updates and upgrade interactively
```

When run, executes synchronously (no background thread), hits the API, and shows the interactive prompt if a new version is found. Bypasses saved `update_mode` and interval — the user explicitly asked.

The `main.rs` no-args guard clause must include `update` in its check to avoid printing help when `lux update` is invoked.

### Background check flow (normal runs)

On every normal invocation:

1. **Early in `main()`**, after CLI parsing and config loading, spawn a background thread
2. Thread reads `update.toml` state file and `update_check_interval_days` from config
3. If `update_mode = "disabled"` or `update_check_interval_days = 0` → thread exits immediately
4. Checks if `update_check_interval_days` have elapsed since `last_checked`
   - If not elapsed and `latest_version` is cached → use cached value, skip API call
   - If elapsed → shell out to `curl --connect-timeout 5 --max-time 10 -s https://api.github.com/repos/grimurjonsson/lux/releases/latest`
5. Parse `tag_name` from JSON response using regex: `"tag_name"\s*:\s*"v?([^"]+)"` (strips `v` prefix)
6. Compare remote version to compiled-in version (`env!("CARGO_PKG_VERSION")`) using simple numeric semver comparison (split on `.`, compare major/minor/patch as integers)
7. If newer and not in `skipped_versions` → return `Some(version)` from thread
8. Write updated `last_checked` and `latest_version` back to `update.toml`
9. Thread exits

Back in main, **after all normal output is done** (after the pipeline finishes, after writer is flushed):

1. Check if the background thread has finished (`JoinHandle::is_finished()`) — **never block**
2. If the thread is still running → let it die with the process. The next run will use the cached `latest_version` from `update.toml` (written by whatever check last completed).
3. If the thread has finished, join it and read the result. If it returned a new version:
   - If `update_mode` is not yet set and stderr is a TTY → show interactive prompt
   - If `update_mode` is not yet set and stderr is not a TTY → print one-line notification, don't save preference
   - If `update_mode = "notify"` → print: `lux vX.Y.Z available — run 'lux update' to upgrade`
   - If `update_mode = "auto"` → perform direct binary download and replace (see below)

This means short-lived commands (e.g. `echo foo | lux`) never hang waiting for the update check. If the check didn't finish in time, the result is cached in `update.toml` and picked up on the next run — the thread writes to `update.toml` before returning, so even if the main process exits while curl is still running, the next invocation that completes within interval will use the cached version.

### `lux update` flow (synchronous, explicit)

When `lux update` is invoked:

1. Hit the GitHub API (no interval check, always fresh)
2. If current version is latest → print "lux vX.Y.Z is up to date" to stderr, exit 0
3. If newer version found → show interactive prompt:

```
lux v0.2.0 is available (you have v0.1.1)

  1) Update now
  2) Always notify me (don't ask again)
  3) Always auto-update (don't ask again)
  4) Skip this version
  5) Disable update checks

Choose [1-5]:
```

Actions:
- **Update now** — performs direct binary download (see below). Does not change saved mode.
- **Always notify** — saves `update_mode = "notify"` to `config.toml`, prints the one-line notification format going forward.
- **Always auto-update** — saves `update_mode = "auto"` to `config.toml`. Future background checks will download and replace automatically.
- **Skip this version** — adds version to `skipped_versions` in `update.toml`. Won't notify again for this specific version.
- **Disable** — saves `update_mode = "disabled"` to `config.toml`. No future checks.

### Direct binary download (for "Update now" and "auto" mode)

Instead of invoking the interactive install script, the update module performs a simple direct download:

1. Detect platform/arch (same logic as install script: `uname -s`, `uname -m`)
2. Download: `curl -L -f --connect-timeout 10 --max-time 120 -o /tmp/lux-update.tar.gz https://github.com/grimurjonsson/lux/releases/download/vX.Y.Z/lux-{target}.tar.gz`
3. Extract: `tar -xzf /tmp/lux-update.tar.gz -C /tmp/lux-update/`
4. Get current binary path via `std::env::current_exe()`
5. Replace: `mv /tmp/lux-update/lux <current_binary_path>` (atomic on same filesystem)
6. Clean up temp files
7. Print "Updated to vX.Y.Z" to stderr

This is non-interactive and safe for both `lux update` and background auto-update. If the binary is in a directory requiring sudo, print an error suggesting `lux update` or the install script.

### First-time experience

When a new version is detected and `update_mode` is not yet set in `config.toml`:
- If stderr is a TTY → show the full interactive prompt (same as `lux update`)
- If stderr is not a TTY → fall back to one-line notification on stderr, don't save a preference

### Version comparison

Simple numeric semver comparison: strip optional `v` prefix, split on `.`, compare major/minor/patch as integers. No need for a semver crate — versions are always `X.Y.Z`.

### Error handling

All update-check errors are **silently swallowed** during background checks — a failed curl, missing state file, parse error, or network timeout must never affect the user's actual work. Errors during `lux update` (explicit) are reported to stderr.

### Thread communication

The background thread communicates the result via `JoinHandle<Option<String>>` where `Some(version)` means a newer version was found. No shared mutable state needed — just join and read the return value.

## Files

| File | Purpose |
|------|---------|
| `src/update.rs` | All upgrade-checking logic: state file I/O, API check, version comparison, interactive prompt, direct binary download |
| `src/cli.rs` | Add `Update` variant to `Command` enum |
| `src/main.rs` | Spawn background thread, join after output, handle `lux update` early exit |
| `src/config.rs` | Add `update_check_interval_days` and `update_mode` fields to `Config` |
| `src/lib.rs` | Add `pub mod update;` |

## Non-goals

- Self-replacement via Rust crate (e.g. `self_update`) — direct curl + mv is simpler
- Windows support — lux is Unix-only
- Checking for pre-release versions
- Downloading changelog in background (only `lux update` shows version info)

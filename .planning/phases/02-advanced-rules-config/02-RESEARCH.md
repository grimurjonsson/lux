# Phase 2: Advanced Rules + Config - Research

**Researched:** 2026-03-12
**Domain:** TOML config parsing, span-based text coloring, XDG config discovery
**Confidence:** HIGH

## Summary

Phase 2 adds three major capabilities: (1) span-based coloring so multiple rules can style different regions of the same line, (2) TOML config file with named profiles, and (3) discovery commands (`--list-profiles`, `--list-colors`). The existing codebase already has `MatchScope` enum variants (`Match`, `Capture(usize)`) stubbed out, `parse_scope()` working, and a priority system in place. The main engineering challenge is replacing the first-match-wins `Engine::apply()` with a span-based model that resolves overlapping regions by priority.

For config, the `toml` crate (v1.0.6) with serde derive is the standard Rust approach. For XDG config path resolution, a manual `$XDG_CONFIG_HOME` check with `~/.config` fallback is simplest and matches the user's explicit decision (the `dirs` crate returns `~/Library/Application Support` on macOS, which conflicts with the locked XDG decision).

**Primary recommendation:** Use `toml` + `serde` for config deserialization, manual XDG path resolution (3 lines of code), and a `Vec<(Range<usize>, Style)>` span model for multi-rule coloring.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- All rules evaluate against every line (not first-match-wins for the whole line)
- First rule wins per character: if two rules want the same character, higher-priority rule keeps it
- Line-scoped rule acts as a floor/base layer -- colors the whole line, then match/capture rules override specific regions on top
- Priority order: CLI -r flags > global config rules > profile rules > built-in defaults
- Missing capture group (e.g., cap2 when regex has 1 group): silently skip, no error or warning
- Rules defined as `[[rules]]` array of tables with `pattern`, `style`, and optional `scope` fields
- Named profiles as `[profiles.name]` sections with `[[profiles.name.rules]]` entries
- Global rules (outside profiles) always active
- When a profile is activated, global rules + profile rules both apply (global first, then profile)
- Default log level rules cannot be disabled; users override by defining higher-priority rules for same patterns
- Single location: `$XDG_CONFIG_HOME/ctail/config.toml` (defaults to `~/.config/ctail/config.toml`)
- No project-local config files
- Missing config file: silently continue with CLI rules + defaults only
- Malformed config file: error and exit with clear parse error message
- `--config path` flag to specify custom config file path (overrides XDG discovery; error if file doesn't exist)
- `--profile name` with missing config or undefined profile: error and exit, listing available profiles if config exists
- `--list-profiles`: shows profile name + rule count, plus config file path at bottom
- `--list-colors`: shows each named color rendered in that color with sample text, plus styles section, plus note about hex/numeric formats
- `--list-colors` forces color output even when piped
- Both discovery commands print and exit immediately (like --help); other flags ignored
- Rule syntax in TOML reuses the same style spec format as CLI (`bold+red`, `bg:white`, etc.)

### Claude's Discretion
- Span-based coloring algorithm internals (how to split line into non-overlapping styled regions)
- TOML deserialization approach (serde vs manual)
- Config file creation/templating (whether to include an `--init` command)
- Exact color sample rendering format

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CLR-05 | Per-rule match scope: color whole line, just the match, or a capture group | Span-based coloring algorithm; existing `MatchScope` enum already has all variants |
| CLR-06 | Multiple rules can match the same line -- all apply to their respective regions | Span collection + priority-based conflict resolution; `owo_colors::Style::style()` works on arbitrary `&str` slices |
| CFG-02 | User can define rules in a TOML config file | `toml` crate v1.0.6 with serde derive; config struct design documented below |
| CFG-03 | User can create named profiles in config and select with `--profile` | TOML `[profiles.name]` sections with `HashMap<String, ProfileConfig>` deserialization |
| CFG-04 | User can list available profiles with `--list-profiles` | Early-exit pattern in main; read config, enumerate profile keys |
| CFG-05 | User can list available colors/styles with `--list-colors` | Hardcoded color catalog rendered with `owo_colors`; force color mode |

</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| toml | 1.0.6 | TOML config file parsing | De facto Rust TOML crate; serde-native; used by Cargo itself |
| serde | 1.x | Deserialization derive macros | Required by `toml` for struct-based deserialization |

### Already in Cargo.toml (no change needed)
| Library | Version | Purpose |
|---------|---------|---------|
| clap | 4.5.60 | CLI argument parsing (add `--profile`, `--config`, `--list-*` flags) |
| owo-colors | 4.3.0 | Per-segment styling via `Style::style(&str)` |
| regex | 1.12.3 | Pattern matching with capture group support |
| anyhow | 1.0.102 | Error handling |

### NOT Needed
| Library | Reason Not to Use |
|---------|-------------------|
| dirs | `config_dir()` returns `~/Library/Application Support` on macOS; user locked XDG (`~/.config`) |
| xdg | Overkill for single path lookup; 3 lines of manual code suffice |
| etcetera | Same as above; unnecessary dependency |

**Installation:**
```bash
cargo add toml serde --features serde/derive
```

## Architecture Patterns

### Recommended Project Structure
```
src/
├── cli.rs           # Extended with --profile, --config, --list-profiles, --list-colors
├── color.rs         # Unchanged (parse_style reused for TOML rule styles)
├── config.rs        # NEW: TOML config loading, XDG path resolution, profile merging
├── engine.rs        # REWRITTEN: span-based multi-rule coloring
├── main.rs          # Extended with early-exit for discovery commands, config loading
├── output.rs        # Unchanged
├── rules.rs         # Extended: build_rules accepts config rules + CLI rules
└── lib.rs           # Add pub mod config
```

### Pattern 1: Span-Based Coloring Algorithm
**What:** Convert all rule matches into styled character spans, resolve overlaps by priority, then render.
**When to use:** Every call to `Engine::apply()`.

**Algorithm:**
```rust
use std::ops::Range;
use owo_colors::Style;

/// A styled region of a line.
struct Span {
    range: Range<usize>,
    style: Style,
    priority: usize,
}

/// Core algorithm: collect spans, resolve overlaps, render.
fn apply(line: &str, rules: &[Rule]) -> String {
    // 1. Collect all spans from all matching rules
    let mut spans: Vec<Span> = Vec::new();
    for rule in rules {
        if let Some(m) = rule.pattern.find(line) {
            match &rule.scope {
                MatchScope::Line => {
                    spans.push(Span {
                        range: 0..line.len(),
                        style: rule.style,
                        priority: rule.priority,
                    });
                }
                MatchScope::Match => {
                    spans.push(Span {
                        range: m.start()..m.end(),
                        style: rule.style,
                        priority: rule.priority,
                    });
                }
                MatchScope::Capture(n) => {
                    // Use captures() to get the nth group
                    if let Some(caps) = rule.pattern.captures(line) {
                        if let Some(cap) = caps.get(*n) {
                            spans.push(Span {
                                range: cap.start()..cap.end(),
                                style: rule.style,
                                priority: rule.priority,
                            });
                        }
                        // Missing capture group: silently skip (locked decision)
                    }
                }
            }
        }
    }

    if spans.is_empty() {
        return line.to_string();
    }

    // 2. Build per-character style map (lowest priority number wins)
    //    - Initialize with None for each byte position
    //    - For each span (sorted by priority ascending = highest priority first),
    //      fill positions that are still None
    // 3. Coalesce consecutive chars with same style into segments
    // 4. Render each segment with its style or plain
    // ... (implementation detail left to planner)
}
```

**Key insight:** Priority is numeric where lower number = higher priority. Sort spans by priority ascending, then iterate: first writer to a character position wins. This naturally implements "CLI > global config > profile > defaults" since CLI rules get priority 0, 1, 2... and defaults get the highest numbers.

### Pattern 2: TOML Config Struct Design
**What:** Serde-derived structs mirroring the locked TOML structure.

```rust
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Default)]
pub struct Config {
    /// Global rules (always active)
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    /// Named profiles
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

#[derive(Deserialize)]
pub struct RuleConfig {
    pub pattern: String,
    pub style: String,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "line".to_string()
}

#[derive(Deserialize)]
pub struct ProfileConfig {
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}
```

**Example TOML:**
```toml
[[rules]]
pattern = "\\d{4}-\\d{2}-\\d{2}"
style = "blue"
scope = "match"

[profiles.spring-boot]
[[profiles.spring-boot.rules]]
pattern = "(?i)springframework"
style = "green"
scope = "match"
```

### Pattern 3: XDG Config Path Resolution (Manual)
**What:** Resolve config file path without external crate.

```rust
fn default_config_path() -> Option<std::path::PathBuf> {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            // home_dir is deprecated but fine here; alternatively use $HOME
            std::env::var("HOME").ok().map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(config_home.join("ctail").join("config.toml"))
}
```

### Pattern 4: Priority Layering for Rule Merging
**What:** Merge rules from multiple sources with correct priority ordering.

```rust
fn build_rules_with_config(
    cli_rules: &[String],
    config: Option<&Config>,
    profile_name: Option<&str>,
) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();
    let mut priority = 0;

    // Layer 1: CLI rules (highest priority = lowest number)
    for rule_str in cli_rules {
        rules.push(parse_rule(rule_str, priority)?);
        priority += 1;
    }

    // Layer 2: Global config rules
    if let Some(config) = config {
        for rc in &config.rules {
            rules.push(rule_from_config(rc, priority)?);
            priority += 1;
        }

        // Layer 3: Profile rules
        if let Some(name) = profile_name {
            if let Some(profile) = config.profiles.get(name) {
                for rc in &profile.rules {
                    rules.push(rule_from_config(rc, priority)?);
                    priority += 1;
                }
            }
        }
    }

    // Layer 4: Built-in defaults (lowest priority)
    rules.extend(default_rules(priority));

    Ok(rules)
}
```

### Pattern 5: Early-Exit Discovery Commands
**What:** Handle `--list-profiles` and `--list-colors` before entering the main processing loop.

```rust
// In main.rs, after CLI parsing:
if cli.list_colors {
    print_colors();  // forces color output
    std::process::exit(0);
}
if cli.list_profiles {
    print_profiles(&config_path)?;
    std::process::exit(0);
}
```

### Anti-Patterns to Avoid
- **Byte-level span operations on multi-byte UTF-8:** Always use `char_indices()` or ensure ranges align to UTF-8 boundaries. Regex crate guarantees byte-aligned match boundaries on valid UTF-8 strings, so this is safe as long as we don't split within regex match boundaries.
- **Styling each character individually:** Instead of applying ANSI codes per character, coalesce adjacent characters with the same style into segments. One ANSI sequence per styled segment, not per character.
- **Using `regex::Regex::find()` for capture scopes:** Must use `captures()` instead when the scope is `Capture(n)`, since `find()` only returns the overall match.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TOML parsing | Custom parser | `toml` crate with serde | TOML spec has many edge cases (multiline strings, inline tables, dotted keys) |
| Style spec parsing | New parser for TOML styles | Existing `color::parse_style()` | Already handles `bold+red+bg:white`, hex, 256-color -- reuse for TOML `style` field |
| Regex compilation | Custom pattern engine | `regex` crate (already used) | Capture groups, Unicode, performance already handled |
| Scope parsing | New scope parser for TOML | Existing `parse_scope()` | Already handles `line`, `match`, `cap1`, `cap2`, etc. |

**Key insight:** Most parsing infrastructure already exists from Phase 1. The TOML config just feeds strings into existing `parse_style()` and `parse_scope()` functions. Don't create parallel parsing paths.

## Common Pitfalls

### Pitfall 1: UTF-8 Boundary Corruption in Span Splitting
**What goes wrong:** Splitting a line at arbitrary byte positions can land in the middle of a multi-byte UTF-8 character, causing panics on `&line[start..end]`.
**Why it happens:** Spans use byte ranges from regex matches, which are safe, but if you try to split between spans at non-character boundaries, you get invalid slices.
**How to avoid:** Only split at positions that come from regex match boundaries or at position 0/line.len(). The regex crate guarantees match boundaries are valid UTF-8 positions. When building the per-position style map, iterate by byte position but only emit segment boundaries where styles change -- these will always align with character boundaries since they originate from regex matches or position 0/end.
**Warning signs:** Panics on lines with emoji or non-ASCII characters.

### Pitfall 2: Quadratic Per-Character Style Resolution
**What goes wrong:** For each character position, iterating through all spans to find which one applies is O(chars * spans).
**Why it happens:** Naive implementation checks every span for every position.
**How to avoid:** Sort spans by priority, then for each span, mark positions in a `Vec<Option<Style>>` (only if position is still None). This is O(chars + total_span_length). For typical log lines (< 1000 chars, < 20 rules), even the naive approach is fine, but the fill approach is cleaner.
**Warning signs:** Noticeable slowdown on very wide lines with many rules.

### Pitfall 3: `dirs::config_dir()` Returns Wrong Path on macOS
**What goes wrong:** Using `dirs::config_dir()` returns `~/Library/Application Support/ctail/config.toml` instead of the XDG-compliant `~/.config/ctail/config.toml`.
**Why it happens:** The `dirs` crate v6 follows Apple's Standard Directory guidelines on macOS.
**How to avoid:** Do NOT use the `dirs` crate. Manually check `$XDG_CONFIG_HOME`, fall back to `$HOME/.config`. This matches the locked user decision.
**Warning signs:** Config file not found on macOS despite existing at `~/.config/ctail/config.toml`.

### Pitfall 4: TOML Deserialization Error Messages
**What goes wrong:** Default `toml::from_str()` error messages reference byte offsets, which are unhelpful to users.
**Why it happens:** The toml crate includes line/column info but the raw error can be verbose.
**How to avoid:** Wrap the error with context: `toml::from_str(&contents).map_err(|e| anyhow!("failed to parse {}: {e}", path.display()))`. The `toml` crate's error type already includes line/column, so this gives good messages.
**Warning signs:** Users seeing cryptic parse errors without file path context.

### Pitfall 5: Profile Name Typos Silently Ignored
**What goes wrong:** User types `--profile sprin-boot` (typo), and if you just check `HashMap::get()` returning None, the user gets no rules applied and doesn't know why.
**How to avoid:** When profile not found, error with available profile names. The locked decision already specifies this: "error and exit, listing available profiles if config exists."
**Warning signs:** Silent missing rules.

### Pitfall 6: Line-Scope Rule Interactions with Match/Capture Rules
**What goes wrong:** A line-scope rule (priority 5) and a match-scope rule (priority 3) both match. If you just paint the whole line with priority 5 first, the higher-priority match-scope rule (priority 3) should override those specific characters.
**Why it happens:** Confusion about "first writer wins" vs "last writer wins" in the span model.
**How to avoid:** The locked decision says "first rule wins per character" where first = lowest priority number. Process spans in priority order (ascending). For each span, only fill positions that are still unstyled. A match-scope rule with priority 3 writes its positions first; then the line-scope rule with priority 5 fills the remaining positions. Result: match region gets priority-3 style, rest of line gets priority-5 style.

## Code Examples

### Loading and Validating Config
```rust
// Source: Pattern derived from toml crate docs + locked decisions
use std::fs;
use std::path::{Path, PathBuf};

pub fn load_config(explicit_path: Option<&Path>) -> Result<Option<(Config, PathBuf)>> {
    let path = match explicit_path {
        Some(p) => {
            if !p.exists() {
                bail!("config file not found: {}", p.display());
            }
            p.to_path_buf()
        }
        None => {
            match default_config_path() {
                Some(p) if p.exists() => p,
                _ => return Ok(None),  // Missing config: silently continue
            }
        }
    };

    let contents = fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    let config: Config = toml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {e}", path.display()))?;

    Ok(Some((config, path)))
}
```

### Converting Config Rule to Compiled Rule
```rust
// Reuses existing parse_style and parse_scope from Phase 1
fn rule_from_config(rc: &RuleConfig, priority: usize) -> Result<Rule> {
    let pattern = Regex::new(&rc.pattern)
        .map_err(|e| anyhow::anyhow!("invalid regex in config '{}': {e}", rc.pattern))?;
    let style = color::parse_style(&rc.style)?;
    let scope = parse_scope(&rc.scope)
        .ok_or_else(|| anyhow::anyhow!("invalid scope '{}' in config", rc.scope))?;
    Ok(Rule { pattern, style, scope, priority })
}
```

### Span Coalescing and Rendering
```rust
// Coalesce: convert per-position styles into contiguous segments
fn render_styled_line(line: &str, styles: &[Option<Style>]) -> String {
    let mut result = String::with_capacity(line.len() + 64);
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let current_style = &styles[i];
        let start = i;
        // Find run of same style
        while i < bytes.len() && &styles[i] == current_style {
            i += 1;
        }
        let segment = &line[start..i];
        match current_style {
            Some(style) => {
                use owo_colors::OwoColorize;
                result.push_str(&segment.style(*style).to_string());
            }
            None => result.push_str(segment),
        }
    }
    result
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `toml` 0.5/0.7/0.8 | `toml` 1.0.6 (stable API) | Late 2024 | Clean serde integration, TOML spec 1.1 |
| `dirs` for all platforms | Manual XDG or platform-aware crate | dirs 6.0 changed macOS behavior | Must choose: XDG everywhere vs platform-native |
| First-match-wins coloring | Span-based multi-rule coloring | Design evolution | More expressive, handles CLR-05/CLR-06 |

**Note:** `toml` 1.0 is built on `toml_edit` internally. The API is stable and well-maintained.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + assert_cmd 2.2.0 |
| Config file | None needed -- `#[cfg(test)]` and `tests/integration.rs` |
| Quick run command | `cargo test` |
| Full suite command | `cargo test --all-targets` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CLR-05 | Match scope colors only matched text; capture scope colors only capture group | unit | `cargo test --lib engine::tests::test_match_scope -x` | Wave 0 |
| CLR-05 | Line scope colors entire line (regression) | unit | `cargo test --lib engine::tests::test_line_scope -x` | Wave 0 |
| CLR-06 | Two rules match same line, each colors its region | unit | `cargo test --lib engine::tests::test_multi_rule_overlap -x` | Wave 0 |
| CLR-06 | Higher-priority rule wins on character conflict | unit | `cargo test --lib engine::tests::test_priority_conflict -x` | Wave 0 |
| CFG-02 | Config file parsed into rules that apply to input | integration | `cargo test --test integration config_file_rules -x` | Wave 0 |
| CFG-03 | --profile selects named profile rules | integration | `cargo test --test integration profile_selection -x` | Wave 0 |
| CFG-04 | --list-profiles shows profile names and exits | integration | `cargo test --test integration list_profiles -x` | Wave 0 |
| CFG-05 | --list-colors shows color catalog and exits | integration | `cargo test --test integration list_colors -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test`
- **Per wave merge:** `cargo test --all-targets`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `src/config.rs` -- new module (no test infrastructure yet)
- [ ] `src/engine.rs` tests -- existing tests use first-match model, need rewrite for span model
- [ ] `tests/integration.rs` -- needs config-related integration tests (temp dir with config file)
- [ ] `tempfile` dev-dependency -- needed for config file integration tests

## Open Questions

1. **Style equality for coalescing**
   - What we know: `owo_colors::Style` may not implement `PartialEq`. We need to compare styles for span coalescing.
   - What's unclear: Whether `Style` derives `PartialEq` or if we need a wrapper.
   - Recommendation: Check at implementation time. If not, store style index (rule priority) alongside style and compare by index instead.

2. **`--init` command for creating default config**
   - What we know: Marked as Claude's discretion.
   - Recommendation: Skip for now. Users can create the file manually. Avoids scope creep.

## Sources

### Primary (HIGH confidence)
- [toml crate docs](https://docs.rs/toml/latest/toml/) -- v1.0.6, serde deserialization API
- [owo-colors Style docs](https://docs.rs/owo-colors/latest/owo_colors/struct.Style.html) -- `style()` method for arbitrary string styling
- Existing codebase: `src/rules.rs`, `src/engine.rs`, `src/color.rs`, `src/cli.rs`

### Secondary (MEDIUM confidence)
- [dirs crate](https://crates.io/crates/dirs) -- v6.0.0, confirmed macOS returns Application Support (not XDG)
- [Dirs Rust Guide](https://generalistprogrammer.com/tutorials/dirs-rust-crate-guide) -- platform behavior documentation

### Tertiary (LOW confidence)
- None -- all findings verified with official sources

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- `toml` 1.0 is universally used in Rust for TOML; manual XDG is trivial
- Architecture: HIGH -- span-based coloring is a well-understood algorithm; existing code has clear integration points
- Pitfalls: HIGH -- identified through direct code analysis and crate documentation review

**Research date:** 2026-03-12
**Valid until:** 2026-04-12 (stable domain, no fast-moving dependencies)

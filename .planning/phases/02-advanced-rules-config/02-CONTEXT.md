# Phase 2: Advanced Rules + Config - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Implement match scopes (match, capture) and multi-rule overlap so multiple rules apply to their respective regions on the same line. Add TOML config file support with named profiles and discovery commands (--list-profiles, --list-colors). No file following — that's Phase 3.

</domain>

<decisions>
## Implementation Decisions

### Multi-rule overlap
- All rules evaluate against every line (not first-match-wins for the whole line)
- First rule wins per character: if two rules want the same character, higher-priority rule keeps it
- Line-scoped rule acts as a floor/base layer — colors the whole line, then match/capture rules override specific regions on top
- Priority order: CLI -r flags > global config rules > profile rules > built-in defaults
- Missing capture group (e.g., cap2 when regex has 1 group): silently skip, no error or warning

### TOML config structure
- Rules defined as `[[rules]]` array of tables with `pattern`, `style`, and optional `scope` fields
- Named profiles as `[profiles.name]` sections with `[[profiles.name.rules]]` entries
- Global rules (outside profiles) always active
- When a profile is activated, global rules + profile rules both apply (global first, then profile)
- Default log level rules cannot be disabled (consistent with Phase 1); users override by defining higher-priority rules for same patterns

### Config file discovery
- Single location: `$XDG_CONFIG_HOME/ctail/config.toml` (defaults to `~/.config/ctail/config.toml`)
- No project-local config files
- Missing config file: silently continue with CLI rules + defaults only (no error, no warning)
- Malformed config file: error and exit (fail fast) with clear parse error message
- `--config path` flag to specify custom config file path (overrides XDG discovery; error if file doesn't exist)
- `--profile name` with missing config or undefined profile: error and exit, listing available profiles if config exists

### Discovery commands
- `--list-profiles`: shows profile name + rule count, plus config file path at bottom
- `--list-colors`: shows each named color rendered in that color with sample text, plus styles section, plus note about hex/numeric formats
- `--list-colors` forces color output even when piped (the whole point is to see colors)
- Both commands print and exit immediately (like --help); other flags ignored

### Claude's Discretion
- Span-based coloring algorithm internals (how to split line into non-overlapping styled regions)
- TOML deserialization approach (serde vs manual)
- Config file creation/templating (whether to include an `--init` command)
- Exact color sample rendering format

</decisions>

<specifics>
## Specific Ideas

- Priority layering preview shown: CLI > global config > profile > defaults — all layers active simultaneously
- Error message for missing profile should suggest available profiles: "profile 'nginx' not found. Available: spring-boot, django"
- Rule syntax in TOML reuses the same style spec format as CLI (`bold+red`, `bg:white`, etc.)

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `MatchScope` enum: already has `Line`, `Match`, `Capture(usize)` variants (rules.rs:8-16)
- `parse_scope()`: already parses scope strings including cap1/cap2 (rules.rs:28-37)
- `parse_rule()`: handles PATTERN:STYLE[:SCOPE] with right-to-left colon splitting (rules.rs:46-79)
- `color::parse_style()`: full style parsing with combined specs (color.rs:14-36) — reusable for TOML rule style fields
- `build_rules()`: assembles user + default rules with priority ordering (rules.rs:124-135)

### Established Patterns
- `anyhow::Result` for error handling throughout
- `clap::Parser` derive macro for CLI (cli.rs)
- `owo_colors` for styling with `XtermColors` for 256-color
- `regex::Regex` for pattern compilation
- Rules carry `priority: usize` field for ordering

### Integration Points
- `Engine::apply()` (engine.rs:22-35): currently first-match + whole-line coloring — needs rewrite to span-based model
- `Cli` struct (cli.rs:5-14): needs `--profile`, `--config`, `--list-profiles`, `--list-colors` flags
- `build_rules()` (rules.rs:124): needs to accept config file rules and merge with CLI rules
- `main.rs`: needs early-exit paths for discovery commands

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 02-advanced-rules-config*
*Context gathered: 2026-03-12*

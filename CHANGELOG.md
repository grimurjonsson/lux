# Changelog

All notable changes to lux will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.19] - 2026-09-01
Improved `--rule` help text to document all available SCOPE options (line, match, capture groups, next-line coloring, and text insertion).

### Added
- adding better help stuff for scope

## [0.1.18] - 2026-08-21
## [0.1.17] - 2026-08-21
Adds text insertion rules (insert-before/after, prepend, append), a "next" scope for coloring lines after a trigger match, local profiles with a `lux profile show` command, and fixes emoji width rendering by probing the terminal instead of trusting its name.

### Added
- add text insertion rules (insert-before/after, prepend, append) (#13)
- add next scope to color N lines after a trigger match
- lux profile show profile_name added
- improve the README.md

### Fixed
- probe the terminal for real emoji width instead of trusting its name

### Other
- docs: add local profiles section to README
- local profiles

## [0.1.16] - 2026-08-21
Adds terminal-width-aware tables, inline markdown formatting, and clickable links; introduces local profiles with a `lux profile show` command, text insertion rules (insert-before/after, prepend, append), and a `next` scope for coloring lines after a trigger match.

### Added
- terminal-width-aware tables, inline markdown formatting, clickable links
- add text insertion rules (insert-before/after, prepend, append) (#13)
- add next scope to color N lines after a trigger match
- lux profile show profile_name added
- improve the README.md

### Other
- docs: add local profiles section to README
- local profiles

## [0.1.15] - 2026-08-10
Lux now renders GFM markdown tables as styled unicode boxes with inline formatting across all modes, expands @file.md includes via a new --expand-refs flag, and adds local profiles, `lux profile show`, text insertion rules, and a `next` scope for coloring lines after a match.

### Added
- expand includes in pager mode; document --expand-refs
- expand @file.md includes in file view with --expand-refs
- recursive include renderer with cycle/depth/missing safety
- add include-ref detection and gutter frame builders
- add --expand-refs flag
- box-draw markdown tables in pager mode; document feature
- box-draw markdown tables in follow mode with idle flush
- box-draw markdown tables in file view and stdin modes
- add TableAssembler streaming state machine
- render parsed tables as styled unicode boxes
- render inline markdown (bold/italic/code/links/strike) in table cells
- add GFM table cell splitting and delimiter detection
- add text insertion rules (insert-before/after, prepend, append) (#13)
- add next scope to color N lines after a trigger match
- lux profile show profile_name added
- improve the README.md

### Fixed
- unicode-aware frame padding, clippy nit, flush-before-ref test
- skip table detection inside fenced code blocks
- reject intraword and whitespace-flanked emphasis in table cells
- per-column table widths, drop global equalize

### Other
- chore: clippy/doc/test cleanup for md-table review findings
- test: deterministically cover follow-mode idle table flush
- docs: add local profiles section to README
- local profiles

## [Unreleased]

### Added
- Markdown tables render as aligned unicode box-drawn tables when viewing
  markdown content (file view, pager, stdin, and follow modes). Inline
  markdown in cells (bold, italic, code, links, strikethrough) is styled.
  Raw source is preserved when color is off or output is piped.
- `--expand-refs` (alias `--expand-referenced-files`): when viewing a
  markdown file, own-line `@other.md` references expand inline, framed by
  a per-depth colored gutter bar naming the source file. Recursive with
  cycle detection and a depth cap; missing files render an inline note.

## [0.1.14] - 2026-04-22
Fixed ANSI preservation in pipe mode so external colors survive when lux rules don't claim them. Added markdown auto-detection on stdin and extended the `help` profile so `just --list`-style output gets styled out of the box.

### Added
- auto-detect Markdown on stdin and apply syntect highlighting (no file needed)
- detect `just --list` / `cargo --list` style output and apply the `help` profile
- help profile now styles 4-space-indented recipe names and dims trailing `# comment` columns

### Fixed
- piped input keeps its original ANSI codes when no rule matches
- `match`/`capture` rules restyle only the matched region and restore surrounding ANSI afterwards (no more collateral colour stripping from unrelated `cap` rules)

## [0.1.13] - 2026-04-01
Added text insertion rules, a next-scope for coloring lines after a match, local profile support with a new `lux profile show` command, and fixed slow-style output.

### Added
- add text insertion rules (insert-before/after, prepend, append) (#13)
- add next scope to color N lines after a trigger match
- lux profile show profile_name added
- improve the README.md

### Fixed
- fixing the slow-style output a bit

### Other
- docs: add local profiles section to README
- local profiles

## [0.1.12] - 2026-03-25
Added duration parsing for --slow (e.g. `--slow 200ms`), a `next` scope to color lines after a trigger match, local profile discovery, and a `lux profile show` command.

### Added
- add duration parsing for --slow flag
- add next scope to color N lines after a trigger match
- lux profile show profile_name added
- improve the README.md

### Other
- chore: add .worktrees/ to gitignore
- local profiles

## [0.1.11] - 2026-03-23
Added support for coloring N lines after a trigger match and loading profiles from local project directories.

### Added
- add next scope to color N lines after a trigger match

### Other
- local profiles

## [0.1.10] - 2026-03-20


`lux profile show` now accepts a profile name argument to display a specific profile.

### Added
- lux profile show profile_name added

## [0.1.9] - 2026-03-19


Improved the README documentation.

### Added
- improve the README.md

## [0.1.8] - 2026-03-19
Added automatic steps to keep the README up to date.

### Other
- automatic steps to try to keep readme.md up to date

## [0.1.7] - 2026-03-19


Updated release script to auto-confirm with `-y` flag and fixed a Node.js 20/24 compatibility issue.

### Added
- adding -y flag to release script

### Other
- node20v24 issue

## [0.1.6] - 2026-03-19


Updated CI to use newer actions/checkout version for Node 24 compatibility.

### Other
- actions/checkout version bumped for node24

## [0.1.5] - 2026-03-19
Adding pager support so long output can be scrolled with `less`.

### Added
- Adding "less" support

## [0.1.4] - 2026-03-18
Fix version checking against dev versions during update flow.

### Fixed
- Version comparison no longer fails on pre-release/dev version strings

## [0.1.3] - 2026-03-18
Fix update checking logic.

### Fixed
- Corrected update notification behavior

## [0.1.2] - 2026-03-18
Added automatic update checking with background GitHub API polling, interactive upgrade prompts, preference saving, and binary download. New `update` subcommand. Removed Windows release support due to Unix dependency. Added README and LICENSE.

### Added
- add interactive update prompt, preference saving, and binary download
- add background_check function for update thread
- add GitHub API check via curl and interval logic
- add update module with version comparison and state file I/O
- add update config fields and Update subcommand variant

### Fixed
- removing windows release cause of unix dependency, added readming and LICENSE

## [0.1.1] - 2026-03-18
New CLI tool "lux" (renamed from ctail) with built-in support for Markdown, YAML, and TOML files, improved syntax highlighting using the Catppuccin theme, shorthand syntax for named arguments, and release commands.

### Added
- added release commands
- shorthands for named arguments
- built-in support for .md, .yaml, .toml
- initial commit

### Fixed
- better highlighting (th syntec... and catpucchin theme)

## [0.1.0] - 2026-03-18

### Added
- Initial release: colored log output with regex pattern matching
- Built-in log level defaults (error, warn, info, debug, trace)
- TOML configuration with named profiles
- Profile auto-selection by filename
- File following by descriptor (-f) and by name (-F) with rotation detection
- Line filtering (--include/--exclude)
- Trigger patterns with context buffer (-t, -b, -a)
- Interactive profile creation wizard
- Syntax highlighting via syntect
- Shell completions (bash, zsh, fish)
- RGB/hex colors and text styles (bold, italic, underline, dim)
- ANSI input stripping

# Changelog

All notable changes to lux will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

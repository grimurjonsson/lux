# Changelog

All notable changes to lux will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

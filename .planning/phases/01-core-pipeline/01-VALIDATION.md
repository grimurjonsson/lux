---
phase: 1
slug: core-pipeline
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test + assert_cmd + predicates |
| **Config file** | None — Wave 0 installs |
| **Quick run command** | `cargo test` |
| **Full suite command** | `cargo test -- --include-ignored` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test`
- **After every plan wave:** Run `cargo test -- --include-ignored`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 0 | INP-01 | integration | `cargo test --test integration pipe_stdin` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 0 | INP-04 | integration | `cargo test --test integration sigpipe` | ❌ W0 | ⬜ pending |
| 01-01-03 | 01 | 0 | CLR-01 | unit | `cargo test rules::test_default_error` | ❌ W0 | ⬜ pending |
| 01-01-04 | 01 | 0 | CLR-02 | unit | `cargo test rules::test_default_warn` | ❌ W0 | ⬜ pending |
| 01-01-05 | 01 | 0 | CLR-03 | unit | `cargo test rules::test_default_debug` | ❌ W0 | ⬜ pending |
| 01-01-06 | 01 | 0 | CLR-04 | unit+integration | `cargo test rules::test_custom_rule` | ❌ W0 | ⬜ pending |
| 01-01-07 | 01 | 0 | CLR-07 | unit | `cargo test color::test_hex_parsing` | ❌ W0 | ⬜ pending |
| 01-01-08 | 01 | 0 | CLR-08 | unit | `cargo test color::test_ansi_colors` | ❌ W0 | ⬜ pending |
| 01-01-09 | 01 | 0 | CLR-09 | unit | `cargo test color::test_styles` | ❌ W0 | ⬜ pending |
| 01-01-10 | 01 | 0 | CFG-01 | integration | `cargo test --test integration cli_rules` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/integration.rs` — integration tests for CLI (pipe, SIGPIPE, `-r` flags)
- [ ] Unit tests in `src/color.rs` — color/style parsing
- [ ] Unit tests in `src/rules.rs` — rule parsing, default rules
- [ ] `Cargo.toml` dev-dependencies: `assert_cmd`, `predicates`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Color output visually correct | CLR-01, CLR-02, CLR-03 | Color rendering is terminal-dependent | Run `echo "ERROR test" \| cargo run` and visually confirm red output |
| NO_COLOR suppression | SC-05 | Env var interaction | Run `NO_COLOR=1 echo "ERROR" \| cargo run` — verify no ANSI escapes |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

---
phase: 4
slug: cli-polish-filtering-and-ux-enhancements
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-13
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test + assert_cmd 2.2.0 + predicates 3.1.4 |
| **Config file** | Cargo.toml [dev-dependencies] |
| **Quick run command** | `cargo test` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test`
- **After every plan wave:** Run `cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 1 | LineFilter | unit | `cargo test filter::tests` | ❌ W0 | ⬜ pending |
| 04-01-02 | 01 | 1 | --include/--exclude | integration | `cargo test --test integration filter_include` | ❌ W0 | ⬜ pending |
| 04-01-03 | 01 | 1 | Filter+trigger | integration | `cargo test --test integration filter_trigger` | ❌ W0 | ⬜ pending |
| 04-02-01 | 02 | 2 | completions | integration | `cargo test --test integration completions` | ❌ W0 | ⬜ pending |
| 04-02-02 | 02 | 2 | --version | integration | `cargo test --test integration version_output` | ❌ W0 | ⬜ pending |
| 04-02-03 | 02 | 2 | FORCE_COLOR | unit | `cargo test output::tests::force_color` | ❌ W0 | ⬜ pending |
| 04-02-04 | 02 | 2 | ANSI stripping | unit | `cargo test engine::tests::ansi_strip` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/filter.rs` — LineFilter struct with unit tests (new module)
- [ ] Integration tests for --include/--exclude in `tests/integration.rs`
- [ ] Integration test for completions subcommand
- [ ] Integration test for --version output
- [ ] Unit test for FORCE_COLOR in `src/output.rs`
- [ ] Unit test for ANSI stripping in engine

*Existing test infrastructure (assert_cmd, predicates) covers framework needs.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Shell completions work in actual shell | completions | Requires interactive shell | Run `ctail completions bash > /tmp/ctail.bash && source /tmp/ctail.bash && ctail <TAB>` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

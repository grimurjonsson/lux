---
phase: 3
slug: file-following
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test + assert_cmd 2.2.0 + tempfile 3.27.0 |
| **Config file** | Cargo.toml `[dev-dependencies]` |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | INP-02/03 | unit | `cargo test --lib tail::tests` | No -- W0 | ⬜ pending |
| 03-01-02 | 01 | 1 | INP-02/03 | unit | `cargo test --lib tail::tests::from_line` | No -- W0 | ⬜ pending |
| 03-02-01 | 02 | 1 | INP-02 | integration | `cargo test --test integration follow_descriptor` | No -- W0 | ⬜ pending |
| 03-02-02 | 02 | 1 | INP-02 | integration | `cargo test --test integration follow_missing_file` | No -- W0 | ⬜ pending |
| 03-02-03 | 02 | 1 | INP-02 | integration | `cargo test --test integration follow_descriptor_deleted` | No -- W0 | ⬜ pending |
| 03-03-01 | 03 | 2 | INP-03 | integration | `cargo test --test integration follow_name_rotation` | No -- W0 | ⬜ pending |
| 03-03-02 | 03 | 2 | INP-03 | integration | `cargo test --test integration follow_copytruncate` | No -- W0 | ⬜ pending |
| 03-03-03 | 03 | 2 | INP-03 | integration | `cargo test --test integration follow_name_missing` | No -- W0 | ⬜ pending |
| 03-04-01 | 01 | 1 | INP-02/03 | integration | `cargo test --test integration n_standalone` | No -- W0 | ⬜ pending |
| 03-04-02 | 01 | 1 | INP-02/03 | integration | `cargo test --test integration bare_file_follows` | No -- W0 | ⬜ pending |
| 03-04-03 | 01 | 1 | INP-02/03 | integration | `cargo test --test integration file_stdin_conflict` | No -- W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/tail.rs` — last-N-lines backward-seeking with unit tests
- [ ] `src/follow.rs` — file following logic stubs with unit tests
- [ ] Integration test stubs in `tests/integration.rs` for follow scenarios
- [ ] Note: `tempfile` and `assert_cmd` already in dev-dependencies — no framework install needed

*Existing infrastructure covers test framework; Wave 0 adds phase-specific test files.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real logrotate interaction | INP-03 | Requires system logrotate daemon | 1. Create test log, configure logrotate, run ctail -F, trigger rotation, verify continued output |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

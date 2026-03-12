---
phase: 2
slug: advanced-rules-config
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test + assert_cmd 2.2.0 |
| **Config file** | None — `#[cfg(test)]` and `tests/integration.rs` |
| **Quick run command** | `cargo test` |
| **Full suite command** | `cargo test --all-targets` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test`
- **After every plan wave:** Run `cargo test --all-targets`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | CLR-05 | unit | `cargo test --lib engine::tests::test_match_scope` | ❌ W0 | ⬜ pending |
| 02-01-02 | 01 | 1 | CLR-05 | unit | `cargo test --lib engine::tests::test_line_scope` | ❌ W0 | ⬜ pending |
| 02-01-03 | 01 | 1 | CLR-06 | unit | `cargo test --lib engine::tests::test_multi_rule_overlap` | ❌ W0 | ⬜ pending |
| 02-01-04 | 01 | 1 | CLR-06 | unit | `cargo test --lib engine::tests::test_priority_conflict` | ❌ W0 | ⬜ pending |
| 02-02-01 | 02 | 2 | CFG-02 | integration | `cargo test --test integration config_file_rules` | ❌ W0 | ⬜ pending |
| 02-02-02 | 02 | 2 | CFG-03 | integration | `cargo test --test integration profile_selection` | ❌ W0 | ⬜ pending |
| 02-02-03 | 02 | 2 | CFG-04 | integration | `cargo test --test integration list_profiles` | ❌ W0 | ⬜ pending |
| 02-02-04 | 02 | 2 | CFG-05 | integration | `cargo test --test integration list_colors` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/engine.rs` tests — rewrite existing tests for span-based model, add match/capture scope tests
- [ ] `tests/integration.rs` — add config-related integration tests (temp dir with config file)
- [ ] `tempfile` dev-dependency — needed for config file integration tests
- [ ] Existing test infrastructure (assert_cmd, predicates) covers CLI integration

*Existing infrastructure partially covers phase requirements. Wave 0 fills gaps for config and span-based engine tests.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| --list-colors shows colored samples | CFG-05 | Visual verification of color rendering | Run `ctail --list-colors` and visually confirm colors render correctly |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

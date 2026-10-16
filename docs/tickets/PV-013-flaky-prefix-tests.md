# PV-013: make change-ID prefix tests deterministic

- **Status:** In progress
- **Branch:** `feat/PV-013-flaky-prefix-tests`
- **Depends on:** none. Runs in parallel with PV-011 (CLI), which must not touch the files below.
- **Touches:** `crates/paper-core/tests/resolve.rs`, `crates/paper-core/tests/history.rs` (only if
  the audit finds the same flaw), `.github/workflows/ci.yml`, `docs/tickets/NOTES.md`
- **Must not touch:** any `src/` file, any other test file. PV-011 owns those.
- **Specs:** `01-concepts.md` § Points (change-ID prefix rules); ADR-0003

## Goal

The test suite is deterministic: no test depends on random change IDs avoiding a fixed prefix.
CI shows every failing test binary, not just the first.

## Context

- `an_ambiguous_change_id_prefix_lists_its_candidates` (`tests/resolve.rs`) fails about 0.5% of
  runs. It gives two commits the change-ID prefixes `kmnn…` and `kmpp…` and resolves `km`, `kmn`
  and `kmp`. But `plain.adopt` calls `Project::init`, which writes the draft (and possibly
  another commit) with `ChangeId::random()`. A random ID starting with `km` (1 in 256) makes `km`
  match three commits; one starting with `kmn`/`kmp` (1 in 4096) makes the closing lookups
  ambiguous. Diagnosis from the PV-010 review, round 2.
- The behaviour under test is correct: a random ID that shares the prefix *is* ambiguous. The
  test is wrong, not the product.
- Change IDs use the letters `k`–`z` (16 values per character).
- CI (`.github/workflows/ci.yml`) runs `cargo test --workspace --all-targets`, which stops at the
  first failing test binary. On PV-003 this hid whether later CLI test binaries passed on Windows.

## Pipeline note

This ticket changes only tests and CI, so there is nothing to implement: the test-author
rewrites the tests (and proves them deterministic), there is no GREEN phase, and the reviewer
reviews as usual.

## Scope

1. Fix `an_ambiguous_change_id_prefix_lists_its_candidates`: give the two fixed IDs a long
   shared prefix that diverges late (e.g. 8 shared letters, so a random clash is about 1 in 4
   billion per random ID), or derive the prefix from what is actually in the project. Keep every
   behaviour it checks (ambiguity lists both candidates with messages and short ids ≥ 4; a longer
   prefix resolves each).
2. **Audit** every test in `crates/paper-core/tests/resolve.rs` and `tests/history.rs` for the
   same flaw: a fixed change-ID or hash prefix that random IDs or real commit hashes could also
   match, or an assertion on a shortest-prefix length that a random neighbour could lengthen.
   Fix each one the same way, and list in the handoff log what was checked and why each
   remaining fixed prefix is safe (with its rough collision odds).
3. CI: `cargo test --workspace --all-targets --no-fail-fast`, so every failing binary shows up.
4. `docs/tickets/NOTES.md`, dated 2026-10-16, two lessons from PV-003/PV-010:
   - a test that assumes random IDs avoid a fixed prefix is flaky; give fixed IDs long prefixes;
   - CI stopped at the first failing test binary and hid later Windows results (now
     `--no-fail-fast`); redact JSON snapshots on parsed values, not on escaped text.

## Out of scope

- Any change to resolution, prefix rules or `ChangeId` (src).
- CLI tests (PV-011 owns `paper-cli`).

## Acceptance criteria

- [ ] The ambiguous-prefix test keeps all its assertions and can't fail from a random ID with realistic odds (≤ 1 in 10⁹ per run).
- [ ] Every fixed prefix in `tests/resolve.rs` and `tests/history.rs` is audited; the handoff log lists them with their odds.
- [ ] The fixed tests pass 500 times in a row locally (`for i in $(seq 500)` over the resolve and history test binaries), recorded in the handoff log.
- [ ] CI uses `--no-fail-fast`.
- [ ] NOTES.md has the dated entry.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass.

## Test plan

- Edit the tests in place; reuse the `change_id(prefix, fill)` and `Plain` helpers.
- Repeat runs: build once, then loop over the test binary
  (`cargo test -p paper-core --test resolve --no-run`, then run the binary path it prints).

## Summary

## Handoff log

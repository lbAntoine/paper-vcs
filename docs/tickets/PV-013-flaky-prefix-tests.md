# PV-013: make change-ID prefix tests deterministic

- **Status:** Done
- **Issue:** #26
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

Change-ID and hash prefix tests no longer depend on random IDs avoiding a fixed prefix.

- Long-prefix variants: the fixed IDs share 8 to 10 letters, so a random clash is at most about
  1 in 10⁹ per run. Short-prefix variants (`km`, `kmn`, `knn`) rebuild the project, at most 50
  times, until no random change ID shares the prefix, then assert exact candidates and short
  ids, so the four-letter floor is still exercised.
- The 4-character hash test rebuilds, at most 20 times, until its prefix is unique.
- Two `history.rs` short-id tests use a 10-letter shared head.
- Every fixed prefix in `tests/resolve.rs` and `tests/history.rs` was audited (odds in the
  handoff log). The other paper-core test files are clean.
- CI runs `cargo test --no-fail-fast`; NOTES.md records the lessons.
- Repeat runs: 500/500 (resolve and history, round 1), 500/500 (resolve, round 2), and 300 more
  by the reviewer.

Deviation: test-only ticket, so the test-author made the fix and there was no GREEN phase.

Follow-ups in PV-014: the CLI `Env::redact` word-prefix flaw, the `change_id.rs` doc range
(`l..=z` vs `k..=z`), and a shared `all_commits` test helper.

## Handoff log

### FIX · test-author · 2026-10-16

Test-only ticket, so I made the fix myself (no src changes, no stubs).

**Fixed** (`tests/resolve.rs`, `tests/history.rs`):
- `an_ambiguous_change_id_prefix_lists_its_candidates`: IDs now share nine letters
  (`kmqrstuvw`) and diverge at the tenth (`n`/`p`); ambiguity is asserted on the nine-letter
  prefix, each ID resolved with its tenth letter. All assertions kept. Odds: 16^-9 (1.5e-11)
  per random ID.
- `commits_sharing_one_change_id_resolve_to_the_newest`: was `knn` (1 in 4096 per random ID
  to turn it ambiguous, the same flaw). Now `knnnrstw` (8 letters, 16^-8 = 2.3e-10).
- `unknown_names_are_reported_with_the_name`: `zzzz` could be a random change-ID prefix
  (16^-4 = 1.5e-5 per ID). Now `zzzzzzzzzz` (16^-10 = 9e-13).
- `a_unique_hash_prefix_of_at_least_four_characters_resolves`: the 4-character hash prefix of a
  save could match another commit (about 1.5e-5 per other commit; hashes vary per run because
  of random change IDs). Now uses `project_with_unambiguous_tip(4)`, which rebuilds the project
  until `git cat-file --batch-all-objects` shows exactly one commit with that prefix.
- `history.rs` `short_change_ids_grow_until_they_tell_similar_ids_apart` and
  `short_change_ids_count_the_ids_of_every_line`: shared head `kmrwpq` (6 letters, 6e-8 per
  random ID, enough to lengthen the shortest prefix) is now `kmrwpqstvw` (10 letters, 9e-13);
  expected short ids are `kmrwpqstvwm` / `kmrwpqstvwk`.

**Audited, left as is:**
- resolve `a_line_wins_over...`: `kmrw` and the 6-char hash spelling are line names, and a line
  wins over any ID or hash match, so a random clash can't change the result. Safe by rule.
- resolve `a_unique_change_id_prefix_resolves...`: 8 characters of real IDs, 16^-8 per other
  ID (2.3e-10, about 3 other IDs: below 1e-9).
- resolve `a_hash_prefix_shorter_than_four...`: three characters are never a hash; the result
  does not depend on neighbours (and hex digits never overlap the k-z change-ID letters).
- resolve `an_ambiguous_hash_prefix...`: finds a real collision among deterministic plain
  commits; extra random commits only add candidates and the assertions are `contains`. Safe.
- resolve `last`, `@`, date tests: no prefixes.
- history `commits_made_with_plain_git...`: the 7-char hash id is of a deterministic plain
  commit (no change ID, fixed times) and hashes can't clash with k-z IDs. Safe.
- history `short_change_ids_are_at_least_four_letters_and_unique...`: asserts invariants over
  actual IDs, no fixed prefix. Safe.
- history `show_by_change_id_and_by_line_name...`: looks up the computed short ID, which is
  unique across the project including the draft by that test's contract. Safe.
- history other tests use no prefixes.

**Repeat runs** (build once, loop over the binaries, `-q`): resolve binary 500/500 passes,
history binary 500/500 passes (0 failures each).

CI: `--no-fail-fast` added. NOTES.md: dated entry added. fmt, clippy `-D warnings` and the
full suite pass.

### Review · reviewer · 2026-10-16
- Verdict: CHANGES. Tests, clippy and fmt pass; 200 more repeat runs of each binary, 0 failures. Odds in the FIX entry re-checked and correct. Other paper-core test files audited: clean. No overlap with PV-011.
- MAJOR: short change-ID prefixes (1–3 letters, allowed by 01-concepts) are no longer tested anywhere; `short_id.len() >= 4` became vacuous.
- MINOR: `project_with_unambiguous_tip` loops without a bound.
- MINOR: NOTES.md says 2–4 letter prefixes clashed in 0.5% of runs (only the 2-letter one did) and "9 to 10 letters" (one is 8).
- MINOR, follow-up (PV-014): the CLI test helper `Env::redact` replaces any 4+ letter prefix of a change ID, so an ID starting with a k–z word ("your", "most") rewrites snapshot text.
- NITs, follow-up (PV-014): `change_id.rs` doc says `l..=z`; history.rs comment's odds are conservative.
- Coordinator decisions: keep the long-prefix test, and add a short-prefix test that rebuilds the project (bounded, with a clear panic) while any random change ID starts with the short prefix, then checks `km` ambiguous and `kmn`/`kmp` unique, with exact short ids. Bound the unique-tip loop the same way. Fix the NOTES wording.

### FIX (review round 1) · test-author · 2026-10-16

- MAJOR: added `a_short_change_id_prefix_is_ambiguous_between_two_saves_and_longer_ones_are_not`
  (`kmnn…`/`kmpp…`). It rebuilds the project (helper `build_avoiding_random_prefix`, at most
  50 attempts, then a clear panic) while any other commit's change ID starts with `km`, then
  asserts `km` is Ambiguous with exactly Left and Right and short ids `kmnn`/`kmpp` (the
  4-letter floor, not vacuous), and that `kmn` and `kmp` resolve uniquely. The long-prefix test
  is kept. Also added `..._resolve_to_the_newest_from_a_short_prefix` (`knn`, same helper).
- `project_with_unambiguous_tip` is bounded to 20 attempts with a panic message.
- NOTES.md wording fixed (short 2 to 4 letter prefixes; 0.5% for the 2-letter one; "8 or more").
- Resolve binary: 500/500 passes after the change (0 failures). fmt, clippy and the full
  suite pass.

### Review round 2 · reviewer · 2026-10-16
- Verdict: APPROVE. Tests (`--no-fail-fast`), clippy and fmt pass; the resolve binary ran 300 more times, 0 failures.
- Short-prefix tests, bounded loops and NOTES wording verified. `tests/show_diff.rs` (from PV-011) has no prefix flaw.
- Deferred to PV-014: a shared `all_commits` helper for the two rebuild helpers, and a clearer comment on the four-letter floor.

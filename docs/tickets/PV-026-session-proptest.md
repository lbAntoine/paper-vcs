# PV-026: I4 acceptance, random 200-step sessions undo and redo exactly (proptest)

- **Status:** In progress
- **Issue:** #44
- **Branch:** `feat/PV-026-session-proptest`
- **Depends on:** PV-023/024/025/031 (merged). Runs in parallel with PV-018 (`paper ignore`).
- **Touches:**
  - `Cargo.toml` (workspace): `proptest = "1.12"` as a workspace dependency.
  - `crates/paper-core/Cargo.toml`: dev-dependency only.
  - `Cargo.lock`.
  - `crates/paper-core/tests/session_proptest.rs`: new.
  - `docs/specs/05-roadmap.md` § v0.2: tick the proptest line.
  - `docs/specs/03-architecture.md` § Testing strategy: one sentence on how to run it longer.
  - Source files, only if the test finds a bug. Then GREEN fixes it in the smallest place, and
    the coordinator checks the file against PV-018's Touches list before dispatch.
- **Must not touch:**
  - `crates/paper-core/src/profile/`, `project/ignore.rs`, `activity/op.rs`, `paper-cli/src/**`
    (all PV-018).
  - Any other test file.
- **Specs:** `01-concepts.md` I4; `03-architecture.md` § Undo / redo algorithm, § Testing
  strategy; `05-roadmap.md` § v0.2 ("a random 200-step session can be fully undone step by step,
  and redone")

## Goal

The v0.2 acceptance line holds and is proven. In a random 200-step session of edits and
commands, undoing step by step restores the exact state at every step, and redoing replays
every step exactly.

## Context

- Undo walks `position.parent` (03-architecture § Undo / redo algorithm). Repeated `undo` goes
  back through every earlier state, including states that were themselves undone. Redo follows
  `head.redo`.
- Edits found at a command's start become their own entry (I1). So a session's states are
  exactly the ops on the activity log, and each op's state is "the view plus the working files
  right after it".
- proptest 1.12 needs Rust 1.88; the workspace is already on Rust ≥ 1.89. 200-step sessions on
  real repos are slow, so CI runs few cases and `PROPTEST_CASES` raises them.

## Decisions (coordinator, 2026-10-18)

- **Through the paper-core API**, on a real temp repo (`git init -b main` and the isolated env).
  No CLI, for speed. The final state is cross-checked with the `git` binary (`fsck --strict`).
- **Generator:** a session of exactly 200 steps. The steps are:
  - write one of about 8 paths, some in subfolders, with random small text or bytes (including
    empty);
  - delete a file;
  - replace a file with a folder of the same name, or the reverse;
  - `save` all;
  - `save` with a path selection;
  - `restore` some paths from an earlier save;
  - `undo`, `redo`, and `undo --to` a random existing entry;
  - `status`, which records pending edits.

  Steps that are invalid at that moment (nothing to save, an unknown path) are allowed, and the
  model just expects the same error-free no-op or user error the API gives. No symlinks and no
  ignored files, so every checkout can write every file (`files_match` is always true). Leftovers
  are PV-031's tests.
- **Oracle:** after every step, record the head op's id with the state it produced. The state is
  the view (every line's tip and draft commit ids and the current line) plus the working files
  (path → bytes, folders implied). Then:
  1. Undo repeatedly until `NothingToUndo`. After each undo, the state must equal the recorded
     state of the op the head's `position` names, and `files_match` must be true.
  2. Redo repeatedly until `NothingToRedo`. After each redo, the same check holds, and after the
     last redo the state equals the final state of the session.
  3. `git fsck --strict` is clean at the end.
- **Case count:** `PROPTEST_CASES` is honoured, with a default of 3 cases in CI.
  - Shrinking is kept but bounded (`max_shrink_iters` about 64), so a failure finishes in
    minutes.
  - `failure_persistence` is off, so nothing is written to the source tree. The failing seed and
    the minimal session are printed instead.
- **Oracle sanity tests**, passing on main:
  - the state capture sees a one-byte edit and a new empty folder;
  - the comparison flags a deliberately wrong expected state;
  - a fixed short session (save, edit, undo, redo) passes the same checks.
- **If the property fails on main, that is the point of the ticket.** RED reports the minimal
  failing session. GREEN fixes the bug, and the coordinator decides first whether the fix fits
  this ticket or needs its own.

## Scope

1. `session_proptest.rs`: generator, model, oracle, sanity tests.
2. Workspace dev-dependency `proptest`.
3. Fixes for any bug the property finds, within the limits above.
4. Specs: tick 05-roadmap § v0.2, and one sentence in 03-architecture § Testing strategy
   (`PROPTEST_CASES=200 cargo test -p paper-core --test session_proptest` for a long run).

## Out of scope

- CLI-level property tests.
- Symlinks, ignored files and leftovers.
- Crash points (PV-025 covers them).
- Concurrent edits.

## Acceptance criteria

- [ ] A random 200-step session is fully undone step by step, with each state exact (view and
  working files), then fully redone back to the session's final state, for every generated case.
- [ ] `files_match` is true at every undo and redo. `git fsck --strict` is clean.
- [ ] With the default case count the test finishes in under about 3 minutes on CI, and
  `PROPTEST_CASES` raises the count.
- [ ] The oracle sanity tests pass, and a deliberately wrong expectation is detected.
- [ ] No files are written to the source tree by a failing run.
- [ ] Any bug found is fixed and has its own minimal regression test.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- This ticket *is* the test.
- RED writes the property and the sanity tests, then runs the property locally with
  `PROPTEST_CASES=50` and reports the result.
  - If it passes on main, that's expected for an acceptance test, and RED says so with the case
    count and run time.
  - If it fails, RED reports the minimal session.
- Time the default run.

## Summary

## Handoff log

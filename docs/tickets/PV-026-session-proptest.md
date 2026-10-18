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
  - Shrinking is kept but bounded (`max_shrink_iters` 16 by default; `PROPTEST_MAX_SHRINK_ITERS` raises it), so a failure finishes in
    minutes.
  - `failure_persistence` is off, so nothing is written to the source tree. The failing seed and
    the minimal session are printed instead.
- **Oracle sanity tests**, passing on main:
  - the state capture sees a one-byte edit and a new empty folder;
  - the comparison flags a deliberately wrong expected state;
  - a fixed short session (save, edit, undo, redo) passes the same checks.
- **Accepted (RED deviation):** redo stops at the session's end position rather than at
  `NothingToRedo`, because a session can end with redo entries still on the stack.
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

- [x] A random 200-step session is fully undone step by step, with each state exact (view and
  working files), then fully redone back to the session's final state, for every generated case.
- [x] `files_match` is true at every undo and redo. `git fsck --strict` is clean.
- [x] With the default case count the test finishes in under about 3 minutes on CI, and
  `PROPTEST_CASES` raises the count.
- [x] The oracle sanity tests pass, and a deliberately wrong expectation is detected.
- [x] No files are written to the source tree by a failing run.
- [x] Any bug found (none found) is fixed and has its own minimal regression test.
- [x] Specs updated (only the listed sections).
- [x] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- This ticket *is* the test.
- RED writes the property and the sanity tests, then runs the property locally with
  `PROPTEST_CASES=50` and reports the result.
  - If it passes on main, that's expected for an acceptance test, and RED says so with the case
    count and run time.
  - If it fails, RED reports the minimal session.
- Time the default run.

## Summary

The I4 acceptance line is proven: a random 200-step session (edits, saves, restores, undo, redo,
`undo --to`, status) is undone step by step with the exact view and working files at every
step, then redone to the session's end position, with `git fsck --strict` clean. The property
passes on main, so there is no source change. `crates/paper-core/tests/session_proptest.rs`
adds the property plus 5 oracle sanity tests; `proptest` is a dev-dependency only.
Default is 3 cases (about 61 s on macOS in the full suite, about 64 s alone; one case about
19 to 25 s). `PROPTEST_CASES` raises the count; 50 cases took about 19 minutes and passed.
Shrinking is capped at 16 iterations (`PROPTEST_MAX_SHRINK_ITERS` overrides); a failure prints
its `PROPTEST_RNG_SEED` and the shrunk session. Specs: 05-roadmap v0.2 ticked, and a sentence in
03-architecture § Testing strategy.

## Handoff log

### RED · test-author · 2026-10-18

**Result: the property passes on main. No bug found.** (Expected for an acceptance test.)

- `crates/paper-core/tests/session_proptest.rs`: 6 tests (1 property, 5 sanity). Workspace
  dependency `proptest = "1.12"` (1.12.0 checked in the registry source: `Config` fields
  `cases`, `max_shrink_iters`, `failure_persistence`), paper-core dev-dependency, `Cargo.lock`.
  No `src/` change and no stubs: the API already exists.
- **Generator:** exactly 200 steps, weights out of 100: write a pool path (30; empty, small
  text, or 1-7 random bytes), delete a file and prune empty parents (6), file to folder (4),
  folder to file (4), save all (10), save a selection of 1-2 names (5), restore 1-2 names from
  `last` or `last~1..3` (6), undo (12), redo (8), `undo --to` a random entry (5), status (5).
  Pool of 8 paths (3 in subfolders, `sec/deep/x.txt`); selections can also name `sec` and a
  `ghost.txt`. Save `NoChangesAt` and restore `UnknownPaths`/`Resolve` are accepted as user
  errors; every other error fails the session. A write replaces whatever stands in the way.
- **Oracle:** state = canonical view JSON + working files (path to bytes, plus empty folders).
  After each step, every op added since the last one is recorded: the head with the live state,
  an older new op (an edits entry) with its own view and the files as they were before the
  step. Inside the session, each undo/redo/undo-to checks `files_match` and that the state
  equals the recorded state of the head's `position`. At the end: a `status` flushes pending
  edits, then undo to `NothingToUndo` with the same check after each, then redo back to the
  session's end position (a session can end with redo entries on the stack, so redo stops where
  the session ended, not at `NothingToRedo`), checking each step. The redo positions must
  retrace the undo positions in reverse, the final state must equal the session's final state,
  and `git fsck --strict` must be clean.
- **Config:** `PROPTEST_CASES` honoured, default 3, `max_shrink_iters = 64`,
  `failure_persistence = None`. Nothing is written to the source tree.
- **Sanity tests (pass on main):** capture sees a one-byte edit and a new empty folder; compare
  flags a wrong file, an extra file and a wrong view; a fixed short session (save, edit, undo,
  redo, file-to-folder) passes the full checks; the position check flags a file changed behind
  undo's back; a session of only unsaved edits undoes and redoes.
- **Runs** (macOS, debug, isolated git config): 1 case 18.6 s; default 3 cases 64 s (about
  21 s per case; 106 s inside the full workspace run under load, so the 3-minute CI budget is
  tight on slower runners); `PROPTEST_CASES=50` 1156 s, passed. fmt and clippy clean, full
  workspace suite passes.
- Note for GREEN/coordinator: a failing oracle shrinks up to 64 times at about 20 s each, so a
  failure takes about 20 minutes to finish. Lower `max_shrink_iters` if that matters.
- Not verified: a deliberately broken `undo` run through the whole property (only the oracle's
  comparison and the position check are shown to detect wrong states).

### GREEN · rust-engineer · 2026-10-18

No source change (the property passes on main). Test file: `max_shrink_iters` 64 to 16 unless
`PROPTEST_MAX_SHRINK_ITERS` is set (proptest 1.12.0 `config.rs` reads that variable in
`Config::default()`, checked in the registry source). `PROPTEST_CASES` is respected the same way.
Without a persisted seed proptest prints none, so `config()` picks a fixed seed (from the clock,
or `PROPTEST_RNG_SEED` if set) and prints `session_proptest: PROPTEST_RNG_SEED=n` to stderr,
which libtest shows on failure. A comment on `config()` explains how to reproduce. Specs: ticked
05-roadmap v0.2 proptest line; added one paragraph to 03-architecture § Testing strategy. RED
deviation (redo stops at the session's end position) recorded as accepted. Checks: fmt, clippy,
full suite pass; default run about 61 s, `PROPTEST_CASES=1` about 25 s. Follow-up idea: the
3-minute CI budget is fine on macOS, watch Windows timing.

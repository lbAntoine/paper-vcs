# PV-038: undoing a redo of unignore --local deletes files from disk

- **Status:** In progress
- **Issue:** #74
- **Branch:** `fix/PV-038-undo-redo-ignore`
- **Depends on:** PV-032 (merged). Blocks PV-033 (the v0.2.0 release); the maintainer decided it is fixed before the tag.
- **Touches:**
  - `crates/paper-core/src/project/undo.rs`, the `tracking_only` decision only.
  - `crates/paper-core/tests/unignore.rs`.
  - `crates/paper-cli/tests/ignore.rs`, only if a CLI-level test is useful.
  - `docs/specs/03-architecture.md`, § Undo / redo algorithm, the "Tracking-only entries" bullet.
  - `docs/specs/02-cli.md`, § `paper ignore`, `--local`.
  - `docs/tickets/PV-038-undo-redo-ignore.md`.
- **Must not touch:** anything else. In particular `checkout.rs`, `recover.rs`, `ignore/`, the session proptest, and the PV-033 release files (CHANGELOG, README, Cargo.*).
- **Specs:** `03-architecture.md` § Undo / redo algorithm; `02-cli.md` § `paper ignore`

## Goal

No `undo` or `redo` ever deletes a file because an ignore or unignore was taken back or brought
back, whether the step names the ignore entry directly or through an `undo:` or `redo:` entry.

## Context

- Today, `undo.rs:207-208` sets `tracking_only` only when `step.entry.kind` is `Ignore` or
  `Unignore`.
- When the step's entry is an `undo:` or `redo:` op (kind `Undo` or `Redo`), for example after
  a new edits entry cleared the redo stack and repeated `undo` walks back through the redo,
  the step gets a full checkout. For a `--local` unignore, the target state's `.gitignore`
  layers don't ignore the files, because the local rule lives outside history. The checkout
  then deletes them from disk, never-saved files included.
- Reproduction, from the v0.2 verification:
  ```
  paper ignore --local raw
  paper unignore --local raw
  paper undo; paper redo
  echo m2 > m.tex; paper status
  paper undo                    # Undid: edited m.tex
  paper undo                    # Undid: redo: unignore /raw/ (in .git/info/exclude)
                                #   deleted raw/r1.csv
  ```
- Op fields: `position` is the op whose state is current, and `redo` is the redo stack.
  - A `Redo` op's `position` is the op it brought back.
  - An `Undo` op took back the op at the front of its own `redo` stack (`redo[0]`).
  - The resolution must be checked against how `step_undo` and `step_redo` build ops in
    `undo.rs`.

## Decisions (coordinator, 2026-10-19)

- **Look through wrappers.** Resolve the step's entry to the op whose effect the step reverses
  or reapplies:
  - for kind `Redo`, that is `position`;
  - for kind `Undo`, that is `redo[0]` (or whatever `step_undo` records as the op it undid;
    verify this);
  - repeat while the result is itself `Undo` or `Redo`, with a small bound (for example 64),
    then stop and use normal rules.

  If the resolved op is `Ignore` or `Unignore`, the step is tracking-only. `undo --to` keeps
  the normal rules, as before.
- Everything else in the tracking-only path (honest `files_match`, PV-031 leftovers, the
  `remembered` flag) is unchanged.
- **Specs.** The "Tracking-only entries" bullet and 02-cli § `--local` say that an `undo:` or
  `redo:` entry of an ignore or unignore counts too.
- Adding ignore and unignore steps to the session proptest is a v0.3 follow-up. Note it in the
  Summary.

## Acceptance criteria

- [ ] The reproduction above keeps `raw/r1.csv`, and a never-saved file in `raw/`, byte-identical on disk through every step. `git fsck --strict` is clean.
- [ ] The same chain with a shared `unignore`, a `--local` `ignore` and a shared `ignore` changes tracking only.
- [ ] The mirror case: undoing an `undo:` entry of an ignore or unignore (reachable via `undo --to` then `undo`, or through a redo after new edits) is tracking-only.
- [ ] Ordinary `undo` and `redo` of saves, restores and edits are unchanged. The existing suites pass.
- [ ] Specs updated, only in the listed sections.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/unignore.rs`: the reproduction for `--local` and for shared rules,
  in both directions where reachable. Cover a never-saved file. Assert the activity
  descriptions name the `redo:` or `undo:` entry being taken back, so the test reaches the
  wrapper path.
- The tests must fail on `origin/main` (files deleted).

## Summary

## Handoff log

### RED · test-author · 2026-10-19

- File: `crates/paper-core/tests/unignore.rs`, 3 new tests (each loops over its rules, with a
  never-saved file `data/raw/never-saved.csv` in the folder), no stubs, no src changes.
  - `undoing_a_redo_of_each_ignore_and_unignore_changes_tracking_only`: the reproduction for
    local and shared unignore, local and shared ignore. FAILS on origin/main: the step over
    the `redo:` entry deletes every file in `data/raw/` (left `{}`). The loop stops at its
    first rule (`--local` unignore), so the other three rules are untested until GREEN.
  - `redoing_again_after_undoing_a_redo_brings_the_rule_back_without_touching_files`: the same,
    then `redo` of the `redo:` entry. FAILS on main for the same reason.
  - `undoing_and_redoing_an_undo_of_each_ignore_and_unignore_changes_tracking_only`: PASSES
    on main by design (a pin, see below).
- Each step asserts the stepped-over entry's description prefix (`edited`, `redo:`, `undo:`),
  raw files byte-identical, and the tracking in the draft; fsck --strict and `git status` run.
- Main: 59 passed, 2 failed in the file; the rest of the workspace passes. fmt and clippy clean.
- Findings for GREEN:
  - The mirror (`undo:` wrapper) is not reachable as a deletion on main. Deletion needs a
    file that is present, untracked at the target, and not ignored by the target's rules.
    Only a `--local` rule that is gone from `.git/info/exclude` gives that, i.e. after a local
    unignore. But after undoing an unignore, the next command records the left files as an
    ordinary edit, and undoing that edit removes them, which is the existing PV-031 design and
    not part of this ticket. For an ignore the mirror is tracking-only already: a shared rule
    is in the target's `.gitignore`, a local rule is still in the exclude file. So the mirror
    test covers `ignore` and `ignore --local` only and passes on main. It guards the fix.
  - Worth a thought for the coordinator: undoing the `edited data/raw/...` entry that recorded
    leftovers of an undone unignore deletes those files (ordinary undo of a save-like edit).
    I did not write a test for it.
  - I did not use `undo --to`: it records `position` = the target, so the next undo names the
    target itself, never a wrapper.

### GREEN · rust-engineer · 2026-10-19

- `project/undo.rs`: new helper `changes_tracking_only` resolves the step's entry through
  `Redo` (its `position`) and `Undo` (`redo[0]`, which is what `step_undo` records as the op it
  took back), up to 64 wrappers, and is tracking-only when it reaches `Ignore` or `Unignore`.
  An `undo --to` op has an empty redo stack, so it keeps the normal rules; `undo --to` itself
  is unchanged.
- Specs: 03-architecture "Tracking-only entries" and 02-cli `paper ignore` `--local` output
  paragraph say `undo:` / `redo:` entries of an ignore or unignore count too.
- All four rules now pass in both RED loops. fmt, clippy and the full suite pass; session
  proptest passes with `PROPTEST_CASES=3`.
- Follow-up (v0.3): add ignore and unignore steps to the session proptest.

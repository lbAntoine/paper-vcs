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

# PV-023: undo, redo and undo --to in paper-core, with checkout

- **Status:** In progress
- **Issue:** #41
- **Branch:** `feat/PV-023-undo-core`
- **Depends on:** PV-022 (export step and keep-alive chain, merged). Runs alone.
- **Touches:** `crates/paper-core/src/project/undo.rs` (new), `crates/paper-core/src/project/checkout.rs` (new,
  or a shared module extracted from `restore`/`restore_plan`), `crates/paper-core/src/project/{mod,restore,restore_plan,save}.rs`,
  `crates/paper-core/src/activity/` (new op kinds, op-id resolution), `crates/paper-core/src/error.rs`,
  `crates/paper-core/tests/undo.rs` (new), `docs/specs/03-architecture.md` (§ Undo / redo algorithm,
  § Command lifecycle step 8)
- **Specs:** `03-architecture.md` § Undo / redo algorithm, § Command lifecycle (steps 6–8),
  § On-disk layout (`checkout-pending`); `01-concepts.md` I1, I4, I6; issue #42's note on `--to`

## Goal

paper-core can take back any activity entry and bring it back again, including the working
files: `undo` after a `save` gives the edits back as the draft, a second `undo` takes the edits
back out of the files, and `redo` replays them. The CLI commands are PV-024.

## Context

- Ops already carry `parent`, `position` and `redo` (PV-020); every op today is a normal op
  (`position` = itself, `redo` = []).
- `Project::record` = keep-alive → op → export (PV-022); export moves refs, HEAD and the index to
  the view (and leaves alone what plain git moved).
- `restore` (`restore.rs`, `restore_plan.rs`) already rewrites working files from a tree safely:
  it never writes through symlinks or over files paper doesn't record, handles file/folder swaps
  and case-only renames, and reports what it skipped.
- Short op ids are unique against every op file (PV-021); #42 says `--to` resolves a prefix
  against the whole log with a teaching error on ambiguity.

## Decisions (coordinator, 2026-10-18)

- **Transitions** exactly as in § Undo / redo algorithm:
  - `undo`: `P = head.position`; if `P.parent` is null → `NothingToUndo` (a friendly outcome, not
    an error); else new op: `view = P.parent.view`, `position = P.parent`, `redo = [P] + head.redo`.
  - `redo`: `R = head.redo.first()` or `NothingToRedo`; new op: `view = R.view`, `position = R`,
    `redo = rest`.
  - `undo --to X`: new op: `view = X.view`, `position = X`, `redo = []`. X can be any op in the log
    (on the head chain or only on a redo stack); its prefix is resolved against every op file;
    unknown → `UnknownEntry`, ambiguous → `AmbiguousEntry` listing the candidates (short id,
    description), both teaching errors.
  - A normal op (any command, including an edits entry) has `redo = []`, which clears the redo
    stack.
- **Snapshot first (I1):** `undo`, `redo` and `undo --to` snapshot before anything else. Edits made
  since the last command become an edits entry (a normal op), so the first `undo` takes those edits
  back, and they can be redone. This follows the maintainer's "edits are their own entry".
- **Refuse when git moved (like `save`):** if a line ref or HEAD differs from `exported.json`
  (plain git moved it), `undo`/`redo`/`--to` refuse with the same errors as `save`
  (`OutOfSyncWithGit`, `HeadOnAnotherBranch`) before writing anything. Undo never moves what
  plain git owns.
- **Op kinds and descriptions:** new kinds `undo` and `redo` (`--to` uses `undo`). Descriptions:
  `undo: <undone entry's description>`, `redo: <entry's description>`,
  `go back to <short id>: <entry's description>`.
- **Order (lifecycle):** snapshot → keep-alive → op → export → **checkout**. Checkout (step 8): write
  `.git/paper/checkout-pending` (naming the target draft tree), rewrite the working files from the
  current working state to the new view's draft tree, refresh the stat cache, delete the marker.
  Finishing a pending checkout after a crash is PV-025; the marker is written now so PV-025 can
  find it.
- **Checkout reuses restore's rules:** files paper doesn't record (symlinks, oversized, nested
  repos, ignored or untracked files in the way) are never overwritten or deleted; they are
  reported in the outcome as skipped, like restore. Factor restore's apply step into a shared
  checkout used by both, without changing restore's behaviour or tests.
- **Outcome:** `Undone { entry: ActivityEntry (what was undone or redone), changed: Vec<Change>,
  skipped: Vec<SkippedPath> }` (names may differ), and `NothingToUndo` / `NothingToRedo`.

## Scope

1. `Project::{undo, redo, undo_to}` with the transitions, refusals and outcomes above.
2. Op-id prefix resolution against every op file, with unknown/ambiguous errors.
3. The checkout step with the `checkout-pending` marker, shared with `restore`.
4. Op kinds `undo`/`redo` and their descriptions; `activity` shows them (no listing changes needed).
5. Specs: 03-architecture § Undo / redo algorithm (snapshot first, refusals, `--to` resolution,
  descriptions), § Command lifecycle step 8 (marker, shared rules, PV-025 repairs).

## Out of scope

- CLI commands and wording (PV-024); crash recovery and fault injection (PV-025); the 200-step
  proptest (PV-026); remote warnings (v0.6).

## Acceptance criteria

- [ ] Edit, `save`, then `undo`: the line's tip is back to the previous save (`refs/heads/main`, `git log`), the saved edits are the unsaved draft again, and the working files are byte-identical to before `undo`.
- [ ] A second `undo` takes the edits entry back: the working files return to their content before the edits; `redo` twice restores both steps exactly (files and refs).
- [ ] `undo` with unsaved edits first records an edits entry, then undoes it (the edits leave the files and can be redone).
- [ ] A new command after an `undo` clears the redo stack (`redo` → `NothingToRedo`).
- [ ] `undo` at the first entry → `NothingToUndo`; `redo` with an empty stack → `NothingToRedo`; nothing is recorded.
- [ ] `undo --to X` restores X's state (files and refs) for X on the chain and for X only on a redo stack; unknown and ambiguous prefixes give teaching errors and change nothing.
- [ ] Undo of a `restore` puts the files back as they were before the restore.
- [ ] After a plain-git commit or `git switch`, `undo` and `redo` refuse with `save`'s errors and change nothing.
- [ ] Files paper doesn't record (an ignored file, a symlink) are left alone by the checkout and reported as skipped.
- [ ] `checkout-pending` exists during the checkout and is gone afterwards; after every undo/redo, `git fsck --strict` is clean, HEAD is on the line, and `git status` matches the draft.
- [ ] `activity` lists `undo: …`, `redo: …` and `go back to …` entries.
- [ ] All existing restore tests pass unchanged.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/undo.rs` with real repos: byte-level working-file comparisons before and
  after, refs via the git binary, `git log`, `git status --porcelain`, `fsck --strict`.
- A short scripted session (edit, save, restore, edit, undo ×n, redo ×n) asserting the exact state
  at each step; the 200-step random version is PV-026.

## Summary

## Handoff log

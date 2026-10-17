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

### RED · test-author · 2026-10-18

- Tests: `crates/paper-core/tests/undo.rs`, 23 tests. All fail on `todo!()` in `Project::undo` (18) or
  `Project::undo_to` (5). Every pre-existing test still passes; fmt and clippy are clean (run with an
  empty `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM=1`, `INSTA_UPDATE=no`).
- Coverage: undo/redo of a save (byte-level files, refs, `git log`, `status --porcelain`, HEAD, fsck,
  marker absent); edits entry on undo with unsaved edits; adds/deletes and emptied folders; redo cleared
  by a new command and by new edits; `NothingToUndo`/`NothingToRedo` record nothing; op bookkeeping
  (`position`, `redo`, `view`, kind) read from op files; `activity` descriptions; `undo --to` on the
  chain, on a redo stack only, by full id, with unsaved edits; ambiguous (crafted op files) and unknown
  prefixes with teaching errors via `PaperError`; undo of a restore; refusals after a plain-git commit,
  `git checkout -b`, detached HEAD (undo, redo and `--to`, files edited, nothing written); ignored file
  skipped like restore (`Blocked` + `KeptForSwap`); a `#[cfg(unix)]` symlink test; one scripted session
  (edit, save, restore, edit, undo x4 + nothing, redo x4 + nothing) with exact state at each step.
- Stubs: `crates/paper-core/src/project/undo.rs` (`Undone`, `UndoOutcome`, `UndoError`, `Project::{undo,
  redo, undo_to}`), `OpKind::{Undo, Redo}` (plus their names in `paper-cli` `render/json.rs`, needed to
  compile), `impl From<UndoError> for PaperError` (`todo!()`).
- API choices the tests pin: `UndoError::Save(SaveError)` carries `save`'s refusals;
  `UnknownEntry { prefix }`, `AmbiguousEntry { prefix, candidates: Vec<ActivityEntry> }`;
  `undo_to(&str)`; the refusal check happens before the snapshot (like `save`); the unknown/ambiguous
  tests run on a clean tree.
- Not tested: the `checkout-pending` marker's file format (the ticket does not define one; PV-025 hook
  needed to observe it mid-checkout). Only its absence afterwards is asserted.
- The symlink test is weak on purpose (symlink and target untouched); how the snapshot treats a tracked
  file replaced by a symlink decides whether a skip is reported.

### GREEN · rust-engineer · 2026-10-18

- Implemented `Project::{undo, redo, undo_to}` (`project/undo.rs`): refusal check (save's errors) and
  prefix resolution first, then snapshot, then keep-alive, op, export (`record_at`), then checkout with
  the `checkout-pending` marker. `ActivityLog::append_at` writes `position`/`redo`; `Project::record_at`
  returns the op. Teaching errors for unknown/ambiguous entries in `error.rs`.
- `project/checkout.rs` (new): restore's plan and apply steps factored out (`plan_checkout`,
  `apply_checkout`, symlink and skip rules, `PendingCheckout` with a round-trip unit test). `restore.rs`
  now calls it; restore's behaviour and tests are unchanged and it does not write the marker.
- Marker format: `{"schema":1,"op":"<op id>","tree":"<draft tree hash>"}`, for PV-025.
- Specs: 03-architecture § On-disk layout (`checkout-pending` line), § Command lifecycle (new
  Checkout paragraph), § Undo / redo algorithm (details; `redo` takes `head.redo.first()`, the spec said
  `pop()`, which contradicted `redo = [P] + head.redo`).
- BLOCKED on two RED tests that are wrong (no assertion changed by me):
  1. `undo_refuses_when_git_detached_head`: the `stored()` helper runs `git symbolic-ref HEAD`, which
     fails on a detached HEAD, so the test panics in its own helper before undo's result is compared.
     Fix: read `.git/HEAD` instead.
  2. `undoing_all_the_way_back_...`: `stored()` includes `refs/paper/keep`, which moves with every op by
     design (PV-022), so refs can never be equal after undo+redo. Fix: filter `refs/paper/keep` out of
     `stored()`.
  With exactly those two helper changes (applied temporarily, then reverted) all 23 tests pass.
- Authorised RED-helper corrections (coordinator): `stored()` in `tests/undo.rs` now reads `.git/HEAD`
  (detached HEAD no longer panics) and leaves `refs/paper/keep` out. No assertion changed.

### Review · reviewer · 2026-10-18
- Verdict: CHANGES. Tests, clippy and fmt pass. The transitions match the spec literally (every edge case probed), refusals write nothing, the checkout only touches paths in the snapshotted draft, the factored restore is faithful, and the error wording follows rule 6.
- MAJOR: the marker is written after the op, so a crash between the op and the marker leaves head at the undo op, the files on the old draft and no marker; the next snapshot then silently records edits that reverse the undo.
- MAJOR: when the checkout skips paths, the files no longer match the new draft; the next command records an edits entry that partly reverses the undo and clears redo, silently.
- MINOR: the `stored()` keep filter applies to every test, so a refused command that moved keep wouldn't be caught.
- MINOR: an empty prefix matches every op; ambiguous candidates are unsorted, and an unreadable leftover turns the teaching error into an activity error.
- MINOR: lifecycle step 8 still lists restore among marker writers; "exists during the checkout" isn't tested.
- NITs: NothingToUndo/Redo docs, an impossible `None` branch, a hard-coded marker path.
- Note: repeated `undo` walks back through every state, including undone ones (`position = P.parent`), as the spec specifies.
- Coordinator decisions:
  - Write the marker before the op, naming the target draft tree and the head op it starts from; PV-025 finishes the checkout when the head view's draft tree equals the marker's tree, else drops the marker (spec).
  - `Undone` gains a flag saying whether the working files match the new draft; the spec says skipped paths keep their content and the next command records them as edits.
  - Filter keep only in the round-trip test.
  - Reject empty and non-hex prefixes with a teaching error.
  - Sort candidates newest first; list unreadable leftovers by id only.
  - Fix step 8's wording; add the walk-back sentence to § Undo / redo algorithm.
  - Fix the NITs.
  - The "marker exists during the checkout" check moves to PV-025 (fault injection).

### GREEN (review round 1) · rust-engineer · 2026-10-18

- Marker now written before the op, as `{"schema":1,"from":"<head op id>","tree":"<target draft tree>"}`; path
  derived from `paper_dir`; round-trip test adjusted; PV-025 repair rule documented in the lifecycle.
- `Undone::files_match` (snapshot tree after the checkout equals the target tree); tested in the ignored-file
  scenario (false) and a plain undo (true). Spec says skipped paths are recorded by the next command.
- `stored()` includes `refs/paper/keep` again; only the undo+redo round-trip test filters it.
- Empty prefix: new `UndoError::EmptyEntry` with a teaching error, tested. Deviation: a non-hex prefix such as
  `zzzz` stays `UnknownEntry` (its message now says ids are 0-9a-f), because an existing test asserts
  `UnknownEntry` for `zzzz`.
- Ambiguous candidates sorted newest first; an unreadable leftover is listed by id with
  "(unreadable operation file)" and no longer turns into an activity error.
- Specs: step 8 wording, walk-back and skipped-path notes. NITs fixed (docs, `Resolved::To(Op)`, marker path).


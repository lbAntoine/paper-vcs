# PV-027: paper gc

- **Status:** Done
- **Issue:** #45
- **Branch:** `feat/PV-027-gc`
- **Depends on:** PV-022, PV-023 (merged). Runs in parallel with PV-025 (crash recovery).
- **Touches:** `crates/paper-core/src/activity/` (deleting op files, the horizon), `crates/paper-core/src/project/gc.rs` (new),
  `crates/paper-core/src/project/mod.rs` (one `mod gc;` line and its re-exports only),
  `crates/paper-core/src/error.rs` (additions only),
  `crates/paper-core/tests/gc.rs` (new), `crates/paper-cli/src/**` (the `gc` command),
  `crates/paper-cli/tests/gc.rs` (new), `docs/specs/02-cli.md` (new § `paper gc`),
  `docs/specs/03-architecture.md` (§ On-disk layout: keep-alive chain and gc paragraphs only)
- **Must not touch:** `crates/paper-core/src/project/{checkout,export,save,init,undo,activity}.rs`
  and the open path in `mod.rs` (PV-025); 03-architecture § Command lifecycle and § Crash safety (PV-025).
- **Specs:** `03-architecture.md` § On-disk layout (keep-alive chain, `paper gc`, crash leftovers);
  `05-roadmap.md` § v0.2 (`gc`)

## Goal

A researcher with a long-lived project can trim paper's undo history to a recent window, so the
activity log and the keep-alive chain stop growing forever and git can reclaim old objects.
Nothing inside the window is affected, and nothing is cleaned up automatically.

## Context

- Ops live in `.git/paper/activity/ops/<id>.json`; `head` names the current one; each op names its
  `parent`, `position` and `redo` stack (PV-020/023).
- `refs/paper/keep` (PV-022): one commit per op whose parents are the previous keep commit and the
  op's new commits, so every commit any op named stays reachable.
- Crash leftovers documented for gc: `.tmp*` files and orphan ops (not reachable from head);
  `view.json.v1` from the format-1 migration.
- `paper activity` walks the head chain (`project/activity.rs`); undo walks `position.parent`.

## Decisions (coordinator, 2026-10-18)

- **`paper gc [--older-than <duration>] [--dry-run]`**, default `--older-than 90d` (`d` days, `w`
  weeks; at least `1d`). Never runs automatically before v1.0.
- **What is kept:** every op on the head chain newer than the cutoff, plus every op on the head's
  redo stack and on the redo stacks of kept ops (so `redo` still works), plus the head op itself
  even if older. The oldest kept op on the head chain becomes the **horizon**.
- **Undo position kept (coordinator, after GREEN):** after several undos the head op's `position`
  names an older op, and the next `undo` targets that op's parent. gc also keeps the head's
  `position` op, so the horizon is the oldest of: the cutoff's oldest kept op, the head's
  `position` op, and every redo target. The chain from the horizon to the head is kept whole. No
  change to `undo.rs`: undoing past the horizon reports `NothingToUndo`.
- **The horizon:** `.git/paper/activity/horizon` holds that op's id (written atomically).
  `ActivityLog::read` returns the horizon op with `parent: None`, so the activity listing and
  `undo` both stop there without any change to `undo.rs` or `project/activity.rs` (undo reports
  `NothingToUndo`). `ActivityLog` exposes whether a horizon exists; the CLI adds a note to
  `paper activity` and to `Nothing to undo.` saying older history was cleaned up by `paper gc`.
  Without a horizon file, behaviour is unchanged.
- **What is removed:** op files not kept, `.tmp*` files in `activity/` and `ops/`, and
  `view.json.v1`.
- **Keep-alive chain rebuilt:** a new `refs/paper/keep` whose commits name every commit referenced
  by the kept ops' views, chained in groups of at most 50 parents per commit (big octopus commits
  upset some git tools). The old chain becomes unreachable, so a later `git gc` can reclaim old
  objects; paper doesn't run `git gc` itself, and the output says so.
- **Order and safety:** under the lock; write the horizon first, then the new keep chain, then delete
  files. After a crash between the steps the old keep chain still covers every op, and the ops
  below the horizon are only orphans. A crash leaves extra files, never missing ones that are
  still needed; running `gc` again finishes the job.
- **Not undoable:** gc records no activity entry (it trims the record undo uses). `--dry-run` lists
  what would be removed and changes nothing. The output reports how many entries were removed and
  kept, the horizon's date, and the files swept.
- **JSON:** `{"schema":1,"dry_run","removed_entries","kept_entries","horizon","swept":[...]}`, every
  key always present.

## Scope

1. paper-core `Project::gc(older_than, dry_run) -> GcOutcome`, the horizon file, the rebuilt chain.
2. The horizon is applied when ops are read; the CLI notes it in `activity` and `Nothing to undo.`.
3. CLI `paper gc` (human and JSON).
4. Specs: 02-cli new § `paper gc`; 03-architecture § On-disk layout (horizon file; chain rebuild in
   groups of 50; gc is not undoable).

## Out of scope

- Running `git gc`; automatic gc; recovery after a crash mid-gc beyond "run it again" (PV-025 owns
  open-time recovery).

## Acceptance criteria

- [x] With ops older and newer than the cutoff, `gc` removes the older ones, keeps the newer, the head and every redo-stack op, and writes the horizon; `paper activity` stops there and says older history was cleaned up.
- [x] `undo` walks back to the horizon and then reports nothing more to undo (with the note); `redo` still works for kept redo entries.
- [x] Every commit named by a kept op is reachable from the rebuilt `refs/paper/keep` (no keep commit has more than 51 parents); after expiring reflogs and `git gc --prune=now`, undo to the horizon still works and `git fsck --strict` is clean.
- [x] `.tmp*` files, orphan ops and `view.json.v1` are swept.
- [x] `--dry-run` changes nothing (every file and ref byte-identical) and lists what would go.
- [x] `--older-than` parses `d` and `w`, rejects `0d` and garbage with a three-part user error.
- [x] Running `gc` twice is safe; the second run removes nothing new.
- [x] gc records no activity entry; `--json` has the fixed key set; snapshot-tested.
- [x] Specs updated (only the listed sections).
- [x] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- Core `tests/gc.rs`: build histories with controlled op times (write ops with old `time` values
  through the activity API, or set times in hand-made op files as earlier tickets did), then gc and
  check files, refs, the horizon, `activity` and `undo`.
- CLI `tests/gc.rs`: human and JSON output, `--dry-run`, bad durations.

## Summary

`paper gc [--older-than 90d] [--dry-run]` trims undo history to a recent window. It keeps the
head chain newer than the cutoff, the head, the op the next undo starts from (the head's
`position`) and every redo target, and the chain from the oldest of those to the head stays whole.
`.git/paper/activity/horizon` names the oldest kept op; `ActivityLog::read` returns it with
`parent: None`, so `paper activity` and `undo` stop there (with a note when the listing reaches
it). Order: horizon, then a rebuilt `refs/paper/keep` (groups of at most 50 parents), then
deletions; a crash leaves extra files only, and a re-run finishes the job, including the chain.
Sweeps `.tmp*` files, orphan ops and `view.json.v1`, never an op the head names. Records no
activity entry; `--dry-run` is byte-identical; human and JSON output, specs 02-cli and
03-architecture.

## Handoff log

### RED · test-author · 2026-10-18

- `crates/paper-core/tests/gc.rs`: 20 tests (real temp repos, op files dated by hand, cross-checked with `git rev-list/log/cat-file/fsck`). 16 fail on `todo!()`; 4 pass today: three sanity tests for the shared helpers (`backdate`, `backdate_oldest`, the byte snapshot of `.git`) and `the_default_age_is_90_days`. Covers: parse of `d`/`w` and rejects (three-part user error), removed/kept/horizon/horizon file, head kept when older, cutoff sizes, nothing old, old redo-stack ops kept and `redo` still working, undo to the horizon then `NothingToUndo`, keep chain (every kept commit reachable, removed-only commits not, at most 51 parents, after `reflog expire` + `git gc --prune=now` + `fsck --strict`, undo to the horizon still works), sweep of `.tmp*`/orphan/`view.json.v1`, `--dry-run` byte-identical `.git` and same outcome as the real run, second run, finishing an interrupted run, no activity entry, reopen after gc.
- `crates/paper-cli/tests/gc.rs`: 13 tests through the binary. 11 fail (`unrecognized subcommand 'gc'`); 2 pass today: helper sanity (`backdating_shows_in_activity...`) and `without_a_gc_activity_and_undo_carry_no_note`. Covers JSON key set and counts, human output, default 90d, weeks, activity and `Nothing to undo.` notes, no activity entry, second run, sweep, `--dry-run` byte-identical, bad durations.
- Stubs: `project/gc.rs` (`GcAge::parse`, `GcAge::DEFAULT`/`days`/`Default`, `GcOutcome`, `GcError`, `Project::gc(&mut self, GcAge, bool)`), `ActivityLog::has_horizon()` (`todo!()`), `From<GcError> for PaperError` (`todo!()`), `mod gc;` + re-export in `project/mod.rs`. Pre-existing tests all pass; fmt and clippy clean; new tests run with an empty `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_NOSYSTEM=1`.
- Interpretations GREEN should confirm or change (my reading of the Decisions):
  - `removed_entries`/`kept_entries` count head-chain entries only; the chain between the horizon and the head is kept whole (old redo targets pull the horizon down to the oldest one). `swept` = `.tmp*`, orphan ops (not on the head chain) and `view.json.v1`, paths relative to `.git/paper/`, `/`-separated, sorted. Op files of removed chain entries are not listed in `swept`.
  - `GcOutcome.horizon` is an `ActivityEntry`; JSON `"horizon"` is a string that contains the horizon entry's date (`YYYY-MM-DD` of its `time`); the CLI test only checks that. Human output must contain both counts, "git gc" and, for `--dry-run`, "dry run" (case-insensitive).
  - Notes: `paper activity` and `Nothing to undo.` contain "older history was cleaned up by `paper gc`" (case-insensitive); `Nothing to undo.` stays the first line.
  - A "bad duration" exits 1 with the three-part error; `--older-than ""` also.
  - `has_horizon()` returns `bool`. Whether `gc` writes a horizon when nothing was removed is not tested.
- No snapshots written; the human and JSON snapshots are for GREEN to add and review. The JSON `horizon` shape and the `GcAge::parse` API are the questions to settle at review.

### GREEN · rust-engineer · 2026-10-18

- Implemented `GcAge::parse`, `Project::gc` (`project/gc.rs`), the horizon in `ActivityLog` (`has_horizon`, `horizon`, `write_horizon`; `read` returns the horizon op with `parent: None`), `From<GcError> for PaperError`, and the CLI `paper gc [--older-than] [--dry-run]` (`render/gc.rs`; notes in `paper activity` and `Nothing to undo.`, human only). Order: new keep chain, then horizon, then deletions. A gc with nothing to remove writes no horizon and no keep chain, and still sweeps `.tmp*` and `view.json.v1`. The horizon is found by a fixpoint: newest-cutoff index, pulled older by redo targets of everything kept.
- Added tests: core no-op gc leaves `.git` byte-identical; CLI snapshots `gc_human`, `gc_dry_run_human`, `gc_dry_run_json` (read before accepting). `--older-than` takes `allow_hyphen_values` so `-3d` reaches the three-part error.
- Specs: 02-cli new § `paper gc`; 03-architecture on-disk layout (horizon file, gc paragraph).
- BLOCKED on one RED test, `every_kept_commit_stays_reachable_through_a_chain_of_at_most_50_parents_each` (assertion at tests/gc.rs:456 and the later "should have been pruned" loop). `only_removed` is the difference of the op *view* commit sets (tips and drafts), but old save tips are git ancestors of newer kept tips (`Save 1` is the parent of `Save 2`), so they are correctly still reachable and never prunable. Fix for the test: compute `only_removed` as removed-op commits not reachable from the kept commits (`git rev-list <kept...>`), i.e. old drafts only. Everything else in the suite passes.
- Known limitation (follow-up): when `head.position` is older than the horizon (undo several steps, then gc with the redo targets as the oldest kept ops), the position op is removed, so the next `undo` hits `MissingOp`. The RED test `old_redo_stack_entries_are_kept...` pins the horizon at the oldest redo target, so position is not kept. Fix needs `undo.rs` (PV-025 area) or keeping the position op.

### RED (fix) · test-author · 2026-10-18

- `crates/paper-core/tests/gc.rs` only (23 tests now).
- `every_kept_commit_stays_reachable_...`: `only_removed` is now the commits named by removed ops that are not reachable (`git rev-list`) from any kept op's commits, so old save tips (ancestors of newer kept tips) no longer count. To make the set non-empty the history now does two edit+`snapshot()` rounds before the first save: the first snapshot's draft is rewritten by the second (sibling, same parent), so no kept commit leads to it. A guard asserts the set is non-empty. The test passes on 9c01740, as expected.
- New `gc_keeps_the_op_the_next_undo_starts_from_and_undo_still_works`: 5 saves, 3 undos, all but the 3 undo ops backdated, gc 30d. Expects horizon = head's `position` (one op older than the oldest redo target), kept = index+1, then `undo` -> `NothingToUndo` (no `MissingOp`) and 3 redos work. FAILS on 9c01740: the horizon is the oldest redo target, not the position (wrong horizon; the position op is removed, which is what makes the next undo `MissingOp`).
- New `undo_after_gc_walks_back_from_the_position_to_the_horizon_and_stops`: 6 saves, 3 undos, oldest 3 ops backdated (cutoff keeps more than the position needs); undo walks at least one step then `NothingToUndo`. Passes on 9c01740 (a regression guard for the cutoff-binding case).
- Adjusted `old_redo_stack_entries_are_kept_and_redo_still_works`: the horizon is now the head's `position` op, which sits one op older than the oldest redo target, so kept = position index + 1 (one more than before) and removed one fewer. It now FAILS on 9c01740 (kept 4 vs 5). No other count expectation changed; the other 21 tests pass on 9c01740.

### GREEN (fix) · rust-engineer · 2026-10-18

- `crates/paper-core/src/project/gc.rs`: `horizon_index` now starts from the oldest of the cutoff's oldest kept op and the head's `position` op, then extends for redo targets as before. `undo.rs` untouched.
- Specs: `02-cli.md` (§ `paper gc` kept list) and `03-architecture.md` (gc paragraph) name the head's position op.
- No deviations.

### GREEN (review round 1) · rust-engineer · 2026-10-18

- `paper activity` shows the horizon note only when the last listed entry is the horizon op, so a `-n` that truncates the list gets no note. New CLI test with more kept entries than the default limit (no note by default, note with `-n 1000`). Specified in 02-cli § `paper gc`.
- Write order is now horizon, then the rebuilt keep chain, then deletions (reason in the ticket Decision and the 03-architecture gc paragraph).
- Wording: gc help and output say git can reclaim old drafts and undone work, and that saves are always kept. The `gc_human` and `gc_dry_run_human` snapshots changed in that one line only.
- `--older-than --dry-run` (value starting with `--`): the error now says a duration such as `90d` is needed after `--older-than`. `allow_hyphen_values` kept; test added.
- `gc.rs` uses `log.ops_dir()`. An op named by the head (redo ids, position) is never an orphan; core test with a hand-made inconsistent log. Rewrapped the long lines in gc.rs and both specs; the spec says the horizon date is UTC.
- No existing test assertions changed.

### GREEN (post-approval) · rust-engineer · 2026-10-18

- `Project::gc` now rebuilds the keep chain when there are entries to remove or orphan ops, and writes the horizon only when entries are removed. A re-run after a crash between the horizon and the keep chain (orphans only) now finishes the rebuild. A no-op gc with no orphans still writes no keep ref or horizon; `--dry-run` changes nothing.
- Core test `a_rerun_after_a_crash_between_horizon_and_keep_chain_rebuilds_the_chain` compares `rev-list --count refs/paper/keep` after a clean gc with a gc re-run on a project that has only the horizon file written.
- Updated the gc doc comment and the 03-architecture gc paragraph. Rewrapped 02-cli.md (gc bullets) and the `cli.rs` gc option doc to 100 columns or fewer.

### RED (CI fix) · test-author · 2026-10-18

Windows CI failed in 3 `paper-core/tests/gc.rs` tests: `git_dir_bytes` read `.git/paper/lock`
while a `Project` held the std file lock on it (mandatory on Windows, os error 33). Both
`git_dir_bytes` helpers (`paper-core/tests/gc.rs`, `paper-cli/tests/gc.rs`) now skip
`paper/lock` (after `\` to `/` normalisation). `lock-holder` stays in the snapshot: it is
written and read through separate handles, never the locked one. Other helpers checked:
`recover.rs::tree_bytes` reads the lock only after the project is dropped; `crash.rs` and
`undo.rs` skip `.git`. The sanity test still sees a save through refs and ops files. No
implementation touched.

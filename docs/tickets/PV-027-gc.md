# PV-027: paper gc

- **Status:** In progress
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
- **Order and safety:** under the lock; write the new keep chain and the horizon first, then delete
  files. A crash midway leaves extra files, never missing ones that are still needed; running `gc`
  again finishes the job.
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

- [ ] With ops older and newer than the cutoff, `gc` removes the older ones, keeps the newer, the head and every redo-stack op, and writes the horizon; `paper activity` stops there and says older history was cleaned up.
- [ ] `undo` walks back to the horizon and then reports nothing more to undo (with the note); `redo` still works for kept redo entries.
- [ ] Every commit named by a kept op is reachable from the rebuilt `refs/paper/keep` (no keep commit has more than 51 parents); after expiring reflogs and `git gc --prune=now`, undo to the horizon still works and `git fsck --strict` is clean.
- [ ] `.tmp*` files, orphan ops and `view.json.v1` are swept.
- [ ] `--dry-run` changes nothing (every file and ref byte-identical) and lists what would go.
- [ ] `--older-than` parses `d` and `w`, rejects `0d` and garbage with a three-part user error.
- [ ] Running `gc` twice is safe; the second run removes nothing new.
- [ ] gc records no activity entry; `--json` has the fixed key set; snapshot-tested.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- Core `tests/gc.rs`: build histories with controlled op times (write ops with old `time` values
  through the activity API, or set times in hand-made op files as earlier tickets did), then gc and
  check files, refs, the horizon, `activity` and `undo`.
- CLI `tests/gc.rs`: human and JSON output, `--dry-run`, bad durations.

## Summary

## Handoff log

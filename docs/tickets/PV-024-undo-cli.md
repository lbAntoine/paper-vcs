# PV-024: paper undo, redo and undo --to in the CLI

- **Status:** In progress
- **Issue:** #42
- **Branch:** `feat/PV-024-undo-cli`
- **Depends on:** PV-023 (core undo, merged). Runs alone. Closes the sprint v0.2-a goal.
- **Touches:** `crates/paper-cli/src/**` (commands, rendering), `crates/paper-cli/tests/undo.rs` (new),
  snapshots, `docs/specs/02-cli.md` (§ `paper undo` / `redo` / `activity` only)
- **Specs:** `02-cli.md` § `paper undo` / `redo` / `activity`, § Global behaviour (exit codes, JSON,
  three-part errors); `03-architecture.md` § Undo / redo algorithm (behaviour, read-only here)

## Goal

A researcher can type `paper undo`, `paper redo` and `paper undo --to <id>` and get back any
earlier state of their project, files included, with output that says plainly what happened and
what didn't.

## Context

- paper-core: `Project::{undo, redo, undo_to(&str)}` → `UndoOutcome::{Undone(Undone),
  NothingToUndo, NothingToRedo}`, `Undone { entry: ActivityEntry, changed, skipped, files_match }`;
  `UndoError` converts to three-part `PaperError`s (refusals reuse `save`'s, plus empty, unknown and
  ambiguous entries).
- CLI patterns: `restore` already renders changed and skipped files (with reasons) in human and
  JSON form; `activity` renders `ActivityEntry`.

## Decisions (coordinator, 2026-10-18)

- **Commands:** `paper undo`, `paper undo --to <id>`, `paper redo`. They open the project like every
  state-changing command (the lock; core snapshots first, so no extra CLI snapshot).
- **Human output:**
  - `Undid: save "Rewrite intro"` / `Redid: …` / `Went back to a41c: save "…"` (the entry's
    description), then the files the checkout changed, as `restore` lists them, then skipped files
    with reasons.
  - When `files_match` is false, a warning: `N files were left as they are (see above); your next
    paper command will record them as edits, and that clears what you can redo.`
  - `Nothing to undo.` / `Nothing to redo.` (exit 0).
- **JSON:** `{"schema":1,"outcome":"undone"|"redone"|"went_back"|"nothing_to_undo"|"nothing_to_redo",
  "entry":{id,short_id,time,kind,description}|null,"changed":[...],"skipped":[...],"files_match":bool|null}`;
  every key always present (`null` / `[]` when nothing happened). `changed` and `skipped` use the
  same shapes as `restore --json`.
- **Errors:** the existing three-part conversions (exit 1 for user errors such as an unknown or
  ambiguous id; refusals after plain-git changes as in `save`; exit 3 for project errors).
- **Spec 02-cli § undo/redo/activity:** describe the output and JSON above; state the walk-back
  behaviour in user words (repeated `undo` steps back through every earlier state, including states
  that were undone; `redo` brings back what you just undid); say `undo` takes back unsaved edits
  first; list `undo` and `redo` among `activity`'s kinds.

## Scope

1. `undo`, `redo`, `undo --to` commands, human and JSON rendering, the `files_match` warning.
2. CLI tests (isolated env) including snapshots, with ids and times redacted.
3. Spec 02-cli § undo/redo/activity as decided.

## Out of scope

- Core behaviour (PV-023); crash recovery (PV-025); undo of remote state (v0.6).

## Acceptance criteria

- [ ] `paper undo` after `paper save` prints `Undid: save "…"`, `git log` loses the save, and the edits are unsaved again (`paper status`); `paper redo` brings it back (`Redid: …`).
- [ ] With unsaved edits, `paper undo` takes back the edits first and says so (`Undid: edited …`), and lists the files it changed.
- [ ] `paper undo --to <short id from paper activity>` goes there (`Went back to …`); an unknown, empty or ambiguous id is a three-part user error (exit 1), and nothing changes.
- [ ] `Nothing to undo.` and `Nothing to redo.` exit 0.
- [ ] After a plain `git commit` or `git switch`, `paper undo` exits with `save`'s errors and changes nothing.
- [ ] A checkout that skips a file prints the skipped file with its reason and the `files_match` warning.
- [ ] `--json` for every outcome has the fixed key set; snapshot-tested.
- [ ] `paper activity` lists `undo: …`, `redo: …` and `go back to …` entries.
- [ ] Spec updated (only § undo/redo/activity).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-cli/tests/undo.rs` with `tests/common/mod.rs` (isolated env, redaction), driving the
  real binary; cross-check with `git log` and the files on disk.

## Summary

## Handoff log

### RED · test-author · 2026-10-18

- `crates/paper-cli/tests/undo.rs`: 19 tests driving the real binary in the isolated env, cross-checked with `git log`/`status`/`fsck`, `paper status` and files on disk. Covers undo/redo after a save, walk-back, unsaved edits, `--to` (valid, unknown, empty, ambiguous with crafted op files), both "Nothing to ..." outcomes (human and exact JSON), refusals after plain `git commit` and `git switch` (compared with `save`'s error and exit code), the ignored-file skip scenario (human lines, warning, JSON), JSON key sets for all outcomes, and `activity` entries.
- Failure mode: all 19 fail with `unrecognized subcommand 'undo'` (clap exit 1). No stubs added: tests drive the binary only. Pre-existing tests all pass.
- Snapshots not written (ids are random): `undo_to_unknown`, `undo_json`, `redo_json`, `undo_to_json` are for GREEN to accept and review.
- Assumptions for GREEN: the warning counts skipped paths (`2 files were left ...`); `Skipped` list format reuses `restore`'s `path  (reason)`; `changed` entries are `{path, kind}`.

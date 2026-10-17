# PV-021: activity entries for every command; paper activity

- **Status:** In progress
- **Issue:** #39
- **Branch:** `feat/PV-021-activity`
- **Depends on:** PV-020 (operation log, merged). Runs alone.
- **Touches:** `crates/paper-core/src/activity/` (descriptions, listing), `crates/paper-core/src/project/{mod,save,restore,init,migrate}.rs`
  (only op kinds and descriptions), `crates/paper-core/src/project/activity.rs` (new: the listing API),
  `crates/paper-core/src/error.rs`, `crates/paper-core/tests/activity.rs`, `crates/paper-cli/src/**`
  (the `activity` command), `crates/paper-cli/tests/activity.rs` (new), snapshots,
  `docs/specs/01-concepts.md` (§ Invariants, I4), `docs/specs/02-cli.md` (§ `paper undo` / `redo` /
  `activity`), `docs/specs/03-architecture.md` (§ Command lifecycle: "Snapshot and op folding")
- **Specs:** `01-concepts.md` I4; `02-cli.md` § `paper activity`; `03-architecture.md` § Command
  lifecycle, § Undo / redo algorithm

## Goal

A researcher can run `paper activity` and see, newest first, everything they did to the project
in their own words: the edits paper recorded, each save with its message, each restore. Every
entry is one step `paper undo` (PV-023/024) will be able to take back.

## Decisions (maintainer and coordinator, 2026-10-17)

- **Edits are their own entry (maintainer).** When a command finds working-file edits made since
  the last command, the snapshot records an `edits` entry (op kind `snapshot`), and then the
  command records its own entry. So a state-changing command records at most two entries: one for
  edits found first (only if there were any), one for the command. Read-only commands (`status`,
  `diff`, `history`, `show`) record only an `edits` entry, and only if something changed. `undo`
  will step back one entry at a time: undoing a `save` gives the unsaved edits back as the draft;
  a second `undo` visibly undoes the `edits` entry. This replaces the spec's "folded into that
  command's operation" and I4's "exactly one activity entry". This matches jj.
- **What records an entry today:** `init` (`init`, or `adopt` in the description), the migration
  (`migrate`), snapshots with changes (`snapshot`), `save` (`save`), `restore` (`restore`, only
  if the draft changed). `config set --project` writes `paper.toml`, a working file; the next
  command records it as an edit. No new kinds.
- **Descriptions** (stored in the op, in researcher vocabulary, rule 6), for example:
  `edited sections/intro.tex, main.tex` (at most 3 paths, then `and N more`),
  `save "Rewrite related work"`, `restore sections/intro.tex from last~1`,
  `start the project` / `adopt the git repository`, `upgrade to paper format 2`.
- **`paper activity [-n N] [--json]`:** the entries from `head` back along `parent`, newest first.
  Each line: the shortest unique op-id prefix (at least 4 hex characters, among all listed ops),
  the time (same relative style as `history`), the description. Default limit 20, `-n` changes
  it. JSON: `{"schema":1,"entries":[{"id","short_id","time","kind","description"}]}`, every key
  always present. `activity` doesn't snapshot first: listing the log mustn't add to it. It
  takes the lock like every command.
- **No `command`/`user` fields.** The op keeps `description`; 03-architecture § Domain model drops
  the PV-021 note.
- **The listing reads only the head chain** (`*.json` op files reachable from `head`), never
  `ops/` as a directory, so crash leftovers are ignored.

## Scope

1. Descriptions for every op kind, written by the commands that record them.
2. paper-core `Project::activity(limit) -> Vec<ActivityEntry>` (or similar): head chain, newest
   first, with short ids.
3. CLI `paper activity` (human and JSON), not snapshotting first.
4. Specs: 01-concepts I4 (at most one edits entry plus one command entry); 02-cli § activity
   (layout, `-n`, JSON, no snapshot); 03-architecture § Command lifecycle (replace "Snapshot and op
   folding" with the edits-entry rule), § Domain model note.

## Out of scope

- `undo`, `redo`, `--to`, marking undone entries in the listing (PV-023/024).
- Exported state and the keep-alive chain (PV-022).

## Acceptance criteria

- [ ] Editing files then `save` records two entries (edits, save) with the right descriptions; `save` with no edits records one.
- [ ] `restore` records an edits entry when files had changed and a `restore` entry when the draft changed; nothing when it changes nothing.
- [ ] `status` (and every read-only command) records an edits entry only when files changed, and nothing otherwise; `activity` itself never records anything.
- [ ] `init` records one entry (`start the project` or `adopt …`); a migrated project shows the `upgrade to paper format 2` entry at the bottom.
- [ ] The edits description lists at most three paths, then `and N more`, sorted.
- [ ] `paper activity` lists entries newest first with unique short ids (≥ 4 hex), relative times and descriptions; `-n` limits; `--json` has the fixed key set; snapshot-tested.
- [ ] Specs updated.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- Core: `tests/activity.rs` additions with real repos (edit, save, restore, status sequences, then
  read the head chain).
- CLI: `tests/activity.rs` with the isolated env; snapshots with ids and times redacted.

## Summary

## Handoff log

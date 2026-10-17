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

### RED · test-author · 2026-10-17

**Tests.**
- `crates/paper-core/tests/activity.rs` (appended, section "PV-021"): 26 new tests. 23 fail, 3 pass on main.
  - Descriptions and entry counts: init/adopt, migrate, save (edits + save, save alone, nothing to save), edits description (1 file, 3 sorted, `and 1 more`, `and 3 more`, add + delete), restore (edits + restore, `from last~1`, no-op, edits only), status (edits only if changed, `snapshot: false`).
  - Listing: head chain with every field, limit, leftover files in `ops/` ignored, `activity` records and snapshots nothing.
  - Short ids: 400 random ops (unique, >= 4 hex, minimal), unique among the listed entries only (limit 40), and the pure `short_ids` on crafted ids (no clash, shared prefixes, differ only in the last character, empty). Nothing depends on a particular random id.
- `crates/paper-cli/tests/activity.rs` (new): 15 tests, all fail (the command is a `todo!()`). Human and JSON snapshots (`activity__activity_human.snap`, `activity__activity_json.snap`, written by hand, ids/times/padding redacted by insta filters), unique hex short ids, `-n`, default 20, fixed JSON key set, `--json` before and after the subcommand, no snapshot/no record, `status`/`diff`/`history`/`show` record edits only when files changed, save/restore/init entries, not-a-project error, busy project exit 3, `git fsck`/`status` clean.

**Pass on main, acceptable.** `a_save_that_finds_nothing_to_save_records_nothing`, `a_restore_that_changes_nothing_records_nothing`, `status_without_a_snapshot_records_nothing`: they pin existing PV-020 behaviour the ticket keeps. Every test that pins a new rule (descriptions, listing, short ids, CLI) fails on main: wrong description (`record the working files`, `save`, `start project`), or `todo!()`. Existing tests all pass.

**Stubs.** `paper_core::project::{ActivityEntry, Project::activity(&self, limit: usize) -> Result<Vec<ActivityEntry>, ActivityError>}` (new `project/activity.rs`; fields `id: OpId, short_id: String, time: String` (RFC 3339), `kind: OpKind, description: String`); `paper_core::activity::{short_ids(&[OpId]) -> Vec<String>, MIN_SHORT_ID}` (new `activity/short.rs`); CLI `Command::Activity { limit }` (`-n`, default 20) dispatching to `todo!()`.

**Format choices the tests pin (decided from the Decisions section; please confirm).**
- Edits description: `edited a, b, c` then ` and N more` with no comma before `and` (`edited a.tex, b.tex, c.tex and 1 more`). Paths are the files that differ from the previous draft (added, modified and deleted all count), sorted bytewise before truncating. The ticket's example `edited sections/intro.tex, main.tex` is not sorted; the acceptance criterion says sorted, so the tests follow the criterion.
- `restore <path> from <point as typed>` (`last` when `--from` is absent).
- Human line: `<short id>  <time padded to 14>  <description>`, like `history`. JSON `kind` is the lowercase op kind (`init`, `snapshot`, `save`, ...), `time` is the op's RFC 3339 time.
- `short_ids` returns prefixes in input order, grows a prefix until no other id shares it.

**Questions.** None blocking.

### GREEN · rust-engineer · 2026-10-17

**Implemented.** Descriptions in `activity/describe.rs` (`edited`, `restored`, `saved`), used by `snapshot_as` (now takes `Option<&str>`; `None` describes the files that differ from the previous draft), `save`, `restore`, `init` (`start the project` / `adopt the git repository`) and the migration (`upgrade to paper format 2`). `Display for Point` (spelling as typed; `last~0` prints `last`). `short_ids`, `Project::activity` (head chain only, no snapshot), `From<ActivityError> for PaperError`, CLI `paper activity` (opens without snapshotting; human and `--json`). Edits entries needed no new recording logic: `snapshot()` already records its own op before `save`/`restore`.

**Specs.** 01-concepts I4; 02-cli `activity`; 03-architecture "Snapshot and op folding" replaced by "Edits entries", and the Domain model note.

**BLOCKED (two RED tests, assertions not changed).**
1. `activity_lists_what_was_done_newest_first`: the hand-written `activity__activity_human.snap` has `[ID]  [TIME]  save ...`. The base `settings()` filter from `common` replaces the time text with `[TIME]` before the test's own `[TIME] *` filter runs, so the 14-wide padding survives, exactly as in `history__history_human.snap` (`[ID]  [TIME]        Rewrite intro`). Actual: `[ID]  [TIME]        save "Rewrite intro"`. The ticket decision (time padded to 14, like `history`) and the snapshot disagree; the snapshot should be updated, or the filter order fixed.
2. `activity_outside_a_project_is_a_three_part_error` expects exit 3; not-a-project is a `User` error (exit 1) everywhere else (`basics` snapshots, `PaperError::not_a_project`). Test should say 1.

The JSON snapshot matches byte for byte. All other tests pass, with fmt and clippy clean.

**RED test corrections, authorised by the coordinator.** (1) `activity__activity_human.snap`: only the spacing changed, to the 14-wide time padding `history__history_human.snap` shows (`[ID]  [TIME]        ...`). (2) `activity_outside_a_project_is_a_three_part_error` now expects exit 1 (a user error), not 3. Both blockers above are resolved.

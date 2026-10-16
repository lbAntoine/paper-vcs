# PV-010: diff, status and restore in paper-core

- **Status:** In progress
- **Branch:** `feat/PV-010-diff-status-restore`
- **Depends on:** PV-002 (resolve, show), PV-009 (line diff). Both merged.
  Runs in parallel with PV-003 (CLI). PV-011 wires these commands into the CLI afterwards.
- **Touches:** `crates/paper-core/src/project/mod.rs`, `crates/paper-core/src/project/diff.rs` (new),
  `crates/paper-core/src/project/status.rs` (new), `crates/paper-core/src/project/restore.rs` (new),
  `crates/paper-core/src/project/save.rs` (visibility of helpers only, if needed),
  `crates/paper-core/tests/diff.rs` (new), `crates/paper-core/tests/status.rs` (new),
  `crates/paper-core/tests/restore.rs` (new),
  `docs/specs/02-cli.md` (§ `paper status`, § `paper diff`, § `paper restore` only)
- **Must not touch:** `paper-cli`, `paper-git`, `crates/paper-core/src/lib.rs`, the root
  `Cargo.toml`, `Cargo.lock` (no new dependencies), and the other sections of `02-cli.md`.
  PV-003 owns those.
- **Specs:** `02-cli.md` § `paper status`, § `paper diff`, § `paper restore`;
  `04-content.md` § Line diff, § Binary summary; `01-concepts.md` § Invariants (I1, I6, I7);
  ADR-0002 (plain git), ADR-0004 (per-line drafts)

## Goal

Every front-end can ask "what's unsaved?", compare any two points file by file with line
diffs, and put files back the way they were at a point. With PV-003 and PV-011 this
completes the v0.1 command set (`status`, `diff --lines`, `restore`).

## Context

- `Project::resolve` / `resolve_with(&Known)` (`project/resolve.rs`) turns a `Point` into a commit.
  `Project::show` (`project/history.rs`) already compares a save with its first parent.
- `tree_diff::diff_trees` lists `Change`s between two trees. `GitRepo::tree_files` and
  `GitRepo::read_blob` give file contents.
- `paper_diff::diff_files(old, new, &LineDiffOptions::default())` returns a `FileDiff`
  (text hunks with `added`/`removed` counts, or a binary size summary).
- `Project::snapshot` (`project/save.rs`) is I1: it records the working files in the draft and
  returns a `DraftState` (changes, skipped, large). `save::matches` is the shared path rule
  (exact file or whole directory components, trailing `/` optional).
- The stat cache (`snapshot/cache.rs`) must stay correct after files are rewritten.
- There is no activity log yet (v0.2). "Undoable" for `restore` means: the draft is
  snapshotted before anything is overwritten, so the previous content exists as a commit.

## Scope

1. **`Project::diff(&self, request: &DiffRequest) -> Result<Vec<FileDiffEntry>, DiffError>`**
   - `DiffRequest { from: Option<Point>, to: Option<Point>, paths: Vec<String> }`.
     `from` defaults to `last`, `to` to `@` (the stored draft: the front-end snapshots first).
   - `FileDiffEntry { path: String, kind: ChangeKind, diff: paper_diff::FileDiff }`, sorted by path.
     Added files diff against nothing, deleted files against nothing on the new side.
   - `paths` filters with `save::matches`. A path that names no file on either side →
     `DiffError::UnknownPaths { paths }` listing every such path (typo guard, like `save`).
     A path that exists but is unchanged gives no entries, not an error.
   - Resolution errors are wrapped (`DiffError::Resolve`).
2. **`Project::status(&mut self, options: &StatusOptions) -> Result<Status, StatusError>`**
   - `StatusOptions { snapshot: bool }`. With `snapshot` (the default), it runs
     `Project::snapshot` first (I1); without it (`--no-snapshot`), it reads the stored draft and
     `skipped`/`large` are empty.
   - `Status { line: LineName, last: HistoryEntry, changes: Vec<StatusEntry>, skipped: Vec<Skipped>,
     large: Vec<(String, u64)> }`, where `last` is the line's tip as `history` would list it.
   - `StatusEntry { path, kind: ChangeKind, stat: ChangeStat }`, sorted by path, with
     `ChangeStat::Lines { added, removed }` for text and `ChangeStat::Bytes { old, new }` for
     binary files (`old`/`new` are `None` for an added/deleted side). Words come in v0.3.
   - An empty draft gives `changes: []`; the front-end prints `Nothing unsaved.`
3. **`Project::restore(&mut self, request: &RestoreRequest) -> Result<Restored, RestoreError>`**
   - `RestoreRequest { paths: Vec<String>, from: Option<Point> }`, `from` defaulting to `last`.
     `paths` must not be empty (`RestoreError::NoPaths`).
   - Steps: snapshot (I1, so unsaved edits are recorded before anything is overwritten);
     resolve `from`; for every file matching a path in the `from` tree or in the draft:
     write the `from` content (keeping the executable bit) or delete the file if it doesn't
     exist at `from`; remove directories left empty; then snapshot again so the draft and the
     stat cache match the working files.
   - A path matching no file in the `from` tree nor in the draft →
     `RestoreError::UnknownPaths { paths }`, listing all of them, and **nothing is written**.
   - Restoring a file that already matches `from` is a no-op for that file.
   - Only paths taken from the trees are written, never the user's string, so `../x` or an
     absolute path can only fail the match. A file/directory flip works both ways
     (restoring file `notes` over a draft directory `notes/`, and the reverse).
   - Files skipped by the snapshot (symlinks, oversize, nested repos) are never overwritten
     or deleted; a symlink entry in the `from` tree is reported in `Restored.skipped`.
   - The git index and HEAD are untouched (I6): `git status` shows the restored files as
     unstaged changes against the tip.
   - `Restored { from: CommitId, changes: Vec<Change>, skipped: Vec<String> }`, where
     `changes` are the files rewritten or deleted, compared with the draft before the restore.
4. **Errors** in the style of `SaveError` (CLAUDE.md rule 6): say what happened and what to run
   next, in researcher vocabulary. Example: `UnknownPaths` says the paths aren't in the draft
   or in that save, and suggests `paper status` or `paper show <point>`.
5. **Specs:** `02-cli.md` § `paper status`, § `paper diff`, § `paper restore` updated to match
   (the typo guard for paths, `--stat` from the line counts, restore's no-op and skipped rules,
   status showing line counts until v0.3). No other section.

## Out of scope

- CLI wiring, rendering, `--since`, `--stat` output, `show` printing file diffs (PV-011).
- Word diff (v0.3), Office diffs (v0.7), conflicts and keeps in `status` (v0.4).
- Activity entries and undo of `restore` (v0.2). The lock (later).

## Acceptance criteria

- [ ] `diff` with no points compares `last` with `@`: an edited, an added and a deleted text file each give the expected kind and hunks.
- [ ] `diff A` compares A with `@`; `diff A B` compares two saves; `diff B A` gives the reverse.
- [ ] A binary file (NUL byte) gives a binary summary with both sizes, not hunks.
- [ ] `paths` restricts entries by file and by directory (whole components); an unknown path → `UnknownPaths` listing all of them; an unchanged existing path → no entries, no error.
- [ ] An unknown point → `DiffError::Resolve(UnknownPoint)`.
- [ ] `status` reports the line, the last save, and each change with line counts (text) or sizes (binary), sorted by path.
- [ ] `status` snapshots first: a file edited after the last command shows up; with `snapshot: false` it doesn't.
- [ ] `status` of an empty draft has no changes; oversized files appear in `large`/`skipped`.
- [ ] `restore <file>` (default `last`) puts back the saved content; the edit made before it still exists in the old draft commit (reachable via the reflog or an object id recorded by the test).
- [ ] `restore --from last~2` restores a whole directory: modified files rewritten, files added since deleted, files deleted since recreated, empty directories removed.
- [ ] `restore` of a file that doesn't exist at the point deletes it.
- [ ] `restore` with an unknown path writes nothing (working files and refs byte-for-byte unchanged) and lists every unknown path.
- [ ] `restore` keeps the executable bit (Unix), and a file/directory flip works.
- [ ] After `restore`, the draft matches the working files (a following `status` with `snapshot: false` equals one with `snapshot: true`), `git fsck --strict` is clean, `git status --porcelain` lists the restored files as unstaged, and HEAD and the index are unchanged.
- [ ] Specs updated (only the three sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass.

## Test plan

- `crates/paper-core/tests/diff.rs`, `status.rs`, `restore.rs`. Build projects with
  `Project::init` + `Project::save`, edit files on disk between saves. Reuse the helpers in the
  existing `tests/save.rs` and `tests/history.rs` (copy small helpers rather than adding a shared
  module that PV-003 might also touch).
- Cross-check writes with the `git` binary: `fsck --strict`, `status --porcelain`,
  `rev-parse HEAD`, `ls-files -s` (index unchanged).
- Oversize files: use `SnapshotOptions` with a tiny `max_file_bytes` if `Project::init` lets the
  test set it; otherwise a file over the default is too costly, so test `large` with whatever
  hook `tests/save.rs` uses.

## Summary

## Handoff log

### RED · test-author · 2026-10-16

- Tests: `crates/paper-core/tests/diff.rs` (8), `status.rs` (4), `restore.rs` (10). 22 new, all fail on
  `todo!("PV-010")` in the stubs; all 150+ pre-existing tests pass. fmt and clippy are clean.
- Stubs (no logic): `project/diff.rs`, `project/status.rs`, `project/restore.rs` with the request,
  result and error types from the ticket (`DiffError`, `StatusError`, `RestoreError` wrap
  `ResolveError`/`SaveError`/`GitError`), re-exported from `project/mod.rs`.
- Notes for the implementer: `Project::init` already makes a first save, so `save_all` in the tests
  is a no-op when the draft has no edits. The `large`/`skipped` test uses
  `SnapshotOptions { warn_file_bytes: 50, max_file_bytes: 1000 }`. The symlink test is
  deliberately loose (user's plain file in place of a committed symlink must survive; `skipped`
  is asserted only if restore succeeds). Tighten it if the implementation chooses a definite
  outcome. The `UnknownPaths` order asserted is the order the user gave.
- Questions: none.

### GREEN · rust-engineer · 2026-10-16

- Implemented `Project::diff`, `status` and `restore` in `project/{diff,status,restore}.rs`.
  `diff.rs` exports a `pub(super) file_diff` helper that `status` reuses for its line counts.
  `history.rs`: `entry` became `pub(super)` (visibility only) so `status` can build `last`.
- `restore` skips (reports in `Restored.skipped`) symlink entries in the `from` tree, paths the
  snapshot skipped (or under a skipped nested repository), and anything that is a symlink on
  disk. Deletions run first, then empty parent directories are pruned, then writes (atomic,
  mode 0755/0644), then a second snapshot.
- Specs: only the three sections of `02-cli.md` (status line counts and `--no-snapshot`, diff
  typo guard and `--stat`, restore rules).
- Test change: the symlink restore test now points the view's tip at a plain-git commit holding
  a symlink (edits `view.json`, reopens) because `resolve` only knows paper's lines, and
  asserts `skipped == ["link"]`, no changes and the user's file intact. Nothing else weakened.
- Follow-up: `restore` runs its first snapshot before the unknown-path check, so a refused
  restore still records the draft (harmless, I1).
- Test fix: `status.rs` asserted `last.message == "first draft"`, but `save_all` is a no-op there
  (`init` already saved the starting files, as the RED note says), so the tip is "Start project".
  The assertion now expects that message; the line, commit and counts assertions are unchanged.

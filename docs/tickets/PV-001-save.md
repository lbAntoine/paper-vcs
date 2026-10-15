# PV-001: save

- **Status:** Ready
- **Branch:** `feat/PV-001-save`
- **Depends on:** none (snapshot and init are on main)
- **Touches:** `crates/paper-core/src/project/mod.rs`, `crates/paper-core/src/project/save.rs` (new),
  `crates/paper-core/src/tree_diff.rs` (new), `crates/paper-core/src/lib.rs`,
  `crates/paper-core/tests/save.rs` (new), `docs/specs/02-cli.md`
- **Specs:** `docs/specs/02-cli.md` § `paper save`; `docs/specs/01-concepts.md` § Invariants (I1, I2, I6);
  `docs/specs/03-architecture.md` § Command lifecycle; ADR-0003 (change IDs), ADR-0004 (per-line drafts)

## Goal

`Project::save` turns the current line's draft (or part of it) into a save: the line moves
forward, a fresh empty draft starts on top, and plain git sees a normal commit on the
branch with a clean `git status` for everything that was saved.

## Context

- `Project` (`project/mod.rs`) already has `open`, `init`, `view()`, `repo()`, plus the
  crate-private helpers `line_ref`, `draft_ref`, `signature_now`, `write_draft`.
  `project/init.rs` shows the expected style: errors with `thiserror`, refs written through
  `GitRepo`, `View` persisted with `view.save`, the stat cache with `cache.save`.
- `snapshot::snapshot(repo, base_tree, previous_cache, options)` records the working files
  as a tree; `StatCache::load/save` persist the cache in `.git/paper/wc-state.json`.
- `GitRepo` has everything needed: `read_commit`, `write_commit` (with `change_id`),
  `tree_files`, `edit_tree`, `set_ref`, `find_ref`, `head_branch`, `reset_index_to`.
- Paths are repository-relative with `/` separators. Front-ends normalise user input
  (including Windows `\`) before calling paper-core.
- `paper-core` never prompts (CLAUDE.md rule 1): the CLI asks for a missing message, so
  `save` receives the message as data and rejects an empty one.

## Scope

1. **`tree_diff`** (new module, `pub`): `diff_trees(repo, from: &TreeId, to: &TreeId) ->
   Result<Vec<Change>, GitError>`, comparing `tree_files` maps. Reuses `snapshot::Change` /
   `ChangeKind` (`Added`, `Modified`, `Deleted`; a kind change such as becoming executable is
   `Modified`). Sorted by path.
2. **`Project::snapshot(&mut self) -> Result<DraftState, …>`** (`pub`): invariant I1 as a
   reusable step. Snapshots the working files on top of the current draft's tree, and if the
   tree changed, rewrites the draft commit (same change ID, same parent, new tree), updates
   `refs/paper/draft/<line>`, the view and the stat cache. Returns `DraftState` with the
   draft's changes relative to the line's last save (`diff_trees(tip_tree, draft_tree)`),
   plus the snapshot's `skipped` and `large` lists. `PV-002`'s `status` will reuse it.
3. **`Project::save(&mut self, request: &SaveRequest) -> Result<SaveOutcome, SaveError>`**:
   - `SaveRequest { message: String, paths: PathSelection }` with
     `PathSelection::{All, Only(Vec<String>), AllBut(Vec<String>)}`. A path selects the file
     with exactly that path, or every file under it when it names a directory
     (`sections` or `sections/`).
   - Order: refuse if git moved the line (below) → reject an empty or whitespace-only message
     → `self.snapshot()` → select changes → write.
   - Empty draft (`All`) or empty selection → `SaveOutcome::NothingToSave`, nothing written.
   - `Only`/`AllBut` paths that match no changed file → `SaveError::NoChangesAt { paths }`
     listing every unmatched path, nothing written.
   - **Save commit:** tree = the tip's tree with the selected changes applied (for `All`,
     exactly the draft tree); parent = old tip; message = trimmed message; **change ID = the
     draft's change ID** (the draft becomes the save, ADR-0003); author and committer = the
     project identity, now.
   - **New draft:** tree = the full snapshotted working tree (unsaved changes stay in it),
     parent = the new save, empty message, **new** change ID.
   - Writes: `refs/heads/<line>` → save, `refs/paper/draft/<line>` → new draft, git index
     reset to the save's tree, HEAD stays attached to the line, view and stat cache saved.
   - `SaveOutcome::Saved(Saved { save: CommitId, change_id: ChangeId, line: LineName,
     saved: Vec<Change>, remaining: Vec<Change> })`.
4. **Refuse when git moved the line:** if `refs/heads/<line>` no longer points at the view's
   tip, or HEAD isn't attached to the current line, `save` returns
   `SaveError::OutOfSyncWithGit { line }` and writes nothing. Importing outside changes is
   later work (03-architecture § Git interop).
5. **Spec:** document in `02-cli.md` § `paper save` how paths select files (exact file or
   directory prefix), the `NoChangesAt` error, and the git-moved refusal.

`SaveError` covers: `MissingMessage`, `NoChangesAt { paths }`, `OutOfSyncWithGit { line }`,
and wrapped `GitError` / `SnapshotError` / `ViewError` / I/O errors, in the style of `InitError`.
`Project` gains `SnapshotOptions::default()` as a field for now (configuration is PV-005).

## Out of scope

- Conflict markers / `--with-conflicts` (conflicts arrive with `keep`, v0.4).
- Crash recovery between the ref, index and view writes (the activity log, v0.2).
- Importing changes made with plain git (refuse instead, scope item 4).
- Message prompting, `$EDITOR`, CLI wiring (PV-003).

## Acceptance criteria

- [ ] `diff_trees` reports added, modified (content or executable bit) and deleted files, sorted.
- [ ] `Project::snapshot` rewrites the draft commit only when the working files changed, keeps its change ID and parent, and persists the new draft in the view and `refs/paper/draft/<line>`.
- [ ] Saving with no changes returns `NothingToSave` and writes nothing (refs and view unchanged).
- [ ] Saving everything creates one commit on the line: parent = old tip, tree = working files, trimmed message, change ID = the old draft's change ID.
- [ ] After a full save the new draft has the same tree as the save, a new change ID, and `git status --porcelain` is empty.
- [ ] New, modified and deleted files are all saved.
- [ ] `Only(paths)` saves just the matching files; the rest stays in the new draft (`remaining`), and `git status` shows them as unsaved.
- [ ] A directory path selects every changed file under it; `AllBut(paths)` saves everything except the matches.
- [ ] Paths matching no changed file → `NoChangesAt` listing them; nothing written.
- [ ] Empty or whitespace-only message → `MissingMessage`; nothing written.
- [ ] A commit made with plain `git commit` on the line → `OutOfSyncWithGit`; nothing written.
- [ ] Two saves in a row give a linear history: `Start project` → first → second.
- [ ] `Project::open` after a save sees the new tip and draft.
- [ ] Every test that writes to the repository ends with `git fsck --strict` passing.
- [ ] `02-cli.md` documents path selection, `NoChangesAt` and the git-moved refusal.

## Test plan

- `crates/paper-core/tests/save.rs`: integration tests through `Project::init` → edit files →
  `Project::save`, cross-checked with `git log`, `git cat-file -p` (change-id header),
  `git status --porcelain`, `git ls-tree -r --name-only`, `git fsck --strict`. Reuse the
  helper style of `tests/init.rs` (`git()` with `-c user.name/-c user.email`).
- `diff_trees` unit tests in `tree_diff.rs`, using a temp `GitRepo` and `edit_tree`.
- `Project::snapshot` tests: unchanged working files keep the same draft commit id; an edit
  produces a new draft commit with the same change ID.

## Summary

## Handoff log

### RED · test-author · 2026-10-16
- Tests: crates/paper-core/tests/save.rs (24 tests: 5 for `Project::snapshot`, 19 for `Project::save`), crates/paper-core/src/tree_diff.rs (4 unit tests)
- Failing on: todo!() in `Project::snapshot` (7 tests, directly or via helpers), `Project::save` (17) and `diff_trees` (4); all 28 new tests fail on a todo!(), none on an assertion yet. The 12 init, 11 snapshot, paper-git and other pre-existing tests still pass.
- Stubs: `tree_diff::diff_trees`, `Project::snapshot`, `Project::save`, `DraftState`, `SaveRequest`, `PathSelection`, `SaveOutcome`, `Saved`, `SaveError` (all in `project/save.rs`, re-exported from `project`); `pub mod tree_diff` in lib.rs. The `SnapshotOptions` field on `Project` is left to GREEN (an unread field would trip dead_code).
- Interpretations to confirm: `DraftState` fields are `changes`, `skipped`, `large`. `NoChangesAt` lists every unmatched path of `Only` and `AllBut` (tests compare sorted). A path matches a directory only at a component boundary (`sec` does not match `sections/...`). The message check runs before looking for changes, so a blank message is `MissingMessage` even with nothing to save. HEAD attached to another branch is `OutOfSyncWithGit`. Tests that expect an unchanged store after `NoChangesAt` or `NothingToSave` call `project.snapshot()` first, because save snapshots before selecting.
- Questions: none

### GREEN · rust-engineer · 2026-10-16
- Implemented `diff_trees`, `Project::snapshot`, `Project::save` (sync check, message check, snapshot, select, write), and the `options: SnapshotOptions` field on `Project` (default, set by `open` and `init`).
- Snapshot rewrites the draft keeping author, message, parents and change ID; committer is now. The stat cache is saved on every snapshot.
- Partial saves apply the selected changes to the tip's tree; a symlink entry would be written as a regular file (snapshot never records symlinks).
- Spec: `02-cli.md` § `paper save` documents path selection, `NoChangesAt` and the git-moved refusal. No ADR changes, no deviations.

### Review · reviewer · 2026-10-16
- Verdict: APPROVE (no blocker/major). Checks: tests pass, clippy pass.
- MINOR: write order in `save` → draft ref, index, line ref (the single commit point), view; comment that the line-ref → view window stays until the v0.2 activity log.
- MINOR: `remaining` computed before the tree edit is wrong when a path flips between file and directory (the editor replaces the blob/tree, saving an unselected deletion) → derive `remaining` from `diff_trees(save_tree, draft_tree)` and test it.
- MINOR: `OutOfSyncWithGit` lacks why/what-next and conflates "HEAD on another branch" with "commit made with git" → split or add guidance.
- MINOR: three repository-writing tests lack `git fsck --strict`.
- MINOR: no test for saving an executable-bit change.
- NIT: symlink gap unreachable today → `debug_assert!` or comment. NIT: `init` drops the `SnapshotOptions` it was given. NIT: comment the `change_id` fallback.
- Coordinator decision: fix all MINOR and NIT items in one review round.

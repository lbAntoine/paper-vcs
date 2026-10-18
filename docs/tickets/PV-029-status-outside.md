# PV-029: status says when the line moved outside paper

- **Status:** Done
- **Issue:** #52
- **Branch:** `feat/PV-029-status-outside`
- **Depends on:** PV-022 (merged). Runs in parallel with PV-031 (undo leftovers).
- **Touches:** `crates/paper-core/src/project/status.rs`, `crates/paper-core/src/error.rs`
  (additions only, if any), `crates/paper-core/tests/status.rs`,
  `crates/paper-cli/src/render/status.rs`, `crates/paper-cli/src/render/json_diff.rs` (the
  status JSON only), `crates/paper-cli/src/commands.rs` (status wiring only, if needed),
  `crates/paper-cli/tests/status.rs` and its snapshots, `docs/specs/02-cli.md` (§ `paper status`
  only)
- **Must not touch:** `crates/paper-core/src/project/{undo,save,checkout,recover,gc}.rs`,
  `crates/paper-cli/src/render/undo.rs`, 02-cli § `paper undo`, 03-architecture (PV-031).
- **Specs:** `02-cli.md` § `paper status`; `03-architecture.md` § Git interop (read only)

## Goal

After a plain `git commit`, `git reset` or `git switch`, `paper status` says so plainly, along
with what to do next, instead of quietly showing the plain-git changes as unsaved work.

## Context

- Since PV-022, paper leaves alone a branch or HEAD that plain git moved, and `exported.json`
  keeps what paper last wrote. `save` refuses in that case through
  `Project::check_in_sync` (`save.rs:271`, `pub(super)`), which returns
  `SaveError::OutOfSyncWithGit { line }` or `SaveError::HeadOnAnotherBranch { line, found }`.
  Their three-part texts are in `error.rs:588-610`.
- `Project::status` (`status.rs`) returns `Status`. Human output is `render/status.rs`, and
  JSON is `render/json_diff.rs::status`.
- Importing such changes is v0.4. This ticket only reports them honestly.

## Decisions (coordinator, 2026-10-18)

- **Core.** `Status` gains `outside_paper: Option<OutsidePaper>`:
  - `OutsidePaper::LineMoved { line }`;
  - `OutsidePaper::OnAnotherBranch { line, found: Option<String> }`, where `found` is `None`
    for a detached HEAD.

  It is computed by calling `check_in_sync` (reused unchanged) and mapping its two errors.
  Other errors propagate. It is computed with or without `--no-snapshot`. Nothing else in
  `status` changes: the draft is still compared with paper's last save.
- **Human output.** Right after the `On <line> · last save …` header, a block reusing the
  wording of save's refusal:
  - **Moved line:** `The line 'main' was changed outside paper: commits were added to it, or
    it was moved, with plain git. Paper can't save on top of that yet.` followed by
    `  git log main    see what changed`.
  - **Another branch:** `Git is on the branch 'other', not on the line 'main'.` (or `on no
    branch (detached HEAD)`) followed by `  git switch main    go back to the line`.

  The suggestion lines are formatted like the error renderer's. The rest of the status follows
  unchanged. The exit code stays 0, since status reports and doesn't fail.
- **JSON.** A new key `"outside_paper"`, always present:
  - `null` when nothing moved;
  - `{"kind":"line_moved","line":"main"}`;
  - `{"kind":"on_another_branch","line":"main","found":"other"}`, with `"found": null` for a
    detached HEAD.

  Every other key is unchanged.
- When both apply (HEAD on another branch *and* the line moved), report `OnAnotherBranch`, the
  same order `check_in_sync` checks in.

## Scope

1. `Status::outside_paper` and its computation.
2. Human and JSON rendering.
3. Spec 02-cli § `paper status`: the new block, the JSON key, and a sentence that the draft is
   still compared with paper's last save.

## Out of scope

- Importing the plain-git commits (v0.4).
- Changing `save` or `undo` refusals (PV-030 owns undo wording).
- Any change to the snapshot.

## Acceptance criteria

- [x] After a plain `git commit` on the line, `paper status` shows the moved-line block with
  `git log <line>`. The exit code is 0, and `save`'s refusal is unchanged.
- [x] After `git switch -c other` (and after a detached `git checkout <sha>`), `status` shows the
  another-branch block with `git switch <line>`.
- [x] A healthy project shows no block, and its JSON has `"outside_paper": null`. Every existing
  status snapshot is byte-identical apart from the new JSON key.
- [x] JSON has the three shapes above. Snapshot-tested.
- [x] Works with `--no-snapshot`.
- [x] `paper status` writes nothing to git refs or HEAD in these states (cross-checked with `git
  rev-parse` and `git symbolic-ref`).
- [x] Specs updated (only the listed section).
- [x] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/status.rs`: `outside_paper` for a plain commit, `git switch -c`, a
  detached HEAD, both conditions together, and a healthy project. Drive git with the binary on a
  real temp repo.
- `crates/paper-cli/tests/status.rs`:
  - human and JSON output for each case, through the binary;
  - a refs and HEAD cross-check, showing nothing is written.
- Write no snapshot by hand. GREEN writes and reviews the new snapshots. Assertions on text use
  `contains` for the key phrases (`changed outside paper`, `git log main`, `git switch main`).
- Tests must fail on `origin/main`.

## Summary

`paper status` now reports when plain git moved the line or HEAD: `Status::outside_paper` (`LineMoved`, `OnAnotherBranch`) is computed through `check_in_sync`, the human output shows a block under the header with the next command, and `--json` always carries `outside_paper` (`null` or a `kind`-tagged object). The exit code stays 0, nothing is written to refs or HEAD, and the draft is still compared with paper's last save. Spec 02-cli updated.

## Handoff log

### RED · test-author · 2026-10-18

**Tests** (all drive a real temp repo and the `git` binary; run with an empty `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_NOSYSTEM=1`).

`crates/paper-core/tests/status.rs` (+9 tests, 7 fail on the `None` placeholder):
- fail: `status_reports_the_line_moved_after_a_plain_git_commit` (also checks refs/HEAD untouched),
  `..._line_moved_without_a_snapshot`, `status_reports_another_branch_after_git_switch` (also
  checks refs/HEAD), `..._another_branch_without_a_snapshot`, `status_reports_a_detached_head_as_found_none`,
  `status_reports_another_branch_when_the_line_also_moved`,
  `the_draft_is_still_compared_with_papers_last_save_when_the_line_moved`.
- pass on main by design: `a_healthy_project_has_no_outside_paper_state`, and the helper sanity test
  `git_helper_commits_and_switches_in_the_temp_repository`.

`crates/paper-cli/tests/status.rs` (+11 tests, 11 fail in total incl. 2 changed ones):
- fail: `status_after_a_plain_commit_says_the_line_changed_outside_paper` (key phrases, block after
  header, refs/HEAD untouched), `status_after_git_switch_says_git_is_on_another_branch`,
  `status_on_a_detached_head_says_git_is_on_no_branch`,
  `status_with_no_snapshot_also_reports_the_line_moved_and_another_branch`,
  `status_still_lists_the_draft_below_the_outside_paper_block`, `status_json_says_the_line_moved`,
  `status_json_says_git_is_on_another_branch` (uses `--no-snapshot`),
  `status_json_says_found_null_for_a_detached_head`, `a_healthy_project_shows_no_outside_paper_block`
  (fails only on the JSON key `outside_paper: null`; the human part passes on main).
- pass on main by design: `save_still_refuses_after_a_plain_commit_and_status_has_not_changed_that`
  (the refusal is unchanged), and the helper sanity test `refs_and_head_helper_changes_when_git_moves_the_line`.
- **Changed existing tests:** `status_json_has_a_fixed_key_set` and
  `status_json_for_an_empty_draft_has_every_key_with_empty_lists` now list `outside_paper` in
  `assert_keys` (the ticket says the JSON gains this key), so they fail until GREEN. Their snapshots
  (`status__status_json.snap`) need GREEN to re-accept with the new key; the human snapshots stay
  byte-identical.

**Stubs.** `OutsidePaper` enum (`LineMoved { line }`, `OnAnotherBranch { line, found: Option<String> }`)
and `Status::outside_paper: Option<OutsidePaper>` in `project/status.rs`, exported from `project/mod.rs`.
`status` sets the field to `None` with a comment (a field can't be `todo!()`). No `todo!()` needed; no
rendering changed.

**Design choices.** Text asserted with `contains` only (`changed outside paper`, `git log main`,
`see what changed`, `Git is on the branch 'other', not on the line 'main'.`,
`Git is on no branch (detached HEAD), not on the line 'main'.`, `git switch main`, `go back to the line`).
The "both conditions" case is built with `switch -c other`, a commit, then `branch -f main HEAD`.
No snapshot was written.

**Probe.** A detached worktree of `origin/main` with the stubs and the new tests copied in gave the same
result as the branch: 7 of 13 core tests and 11 of 26 cli tests fail, all on the assertions. The probe was removed.

### GREEN · rust-engineer · 2026-10-18

- `Project::status` calls `check_in_sync` unchanged and maps `OutOfSyncWithGit` to `LineMoved` and
  `HeadOnAnotherBranch` to `OnAnotherBranch`; other errors propagate. Works with `--no-snapshot`.
- Human block in `render/status.rs` after the header: save's refusal wording, then
  `  next: <command>  # <explanation>` (the error renderer's format, with `next:` lead), then a blank line.
- JSON `outside_paper` key in `render/json_diff.rs` (null, `line_moved`, `on_another_branch` with `found`).
- Spec 02-cli § `paper status`: the block, the JSON key, the draft-vs-last-save sentence.
- Snapshots: `status__status_json.snap` re-accepted (only the new key). New: `status_line_moved`,
  `status_another_branch`, `status_detached_head` and their `status_json_*` twins; each read before accepting.
  I added the `insta::assert_snapshot!` lines to the existing RED tests (no assertion changed).
- Deviation: the suggestion line uses the error renderer's `next: cmd  # why` shape, not the bare
  `cmd    why` in the Decisions sketch, as the coordinator asked for the error renderer's format.
- fmt, clippy `-D warnings` and the full suite pass.

### GREEN (review fixes) · rust-engineer · 2026-10-18

- Spec 02-cli (JSON output): `paper status --json` key list now includes `outside_paper`.
- `render/json_diff.rs`: the `serde_json::json!` value is replaced by a typed `OutsidePaperOut` enum (`#[serde(tag = "kind", rename_all = "snake_case")]`); the wire order is `kind, line, found`. No snapshot changed: the CLI test helper `redact_json` parses and re-prints through `serde_json::Value`, which sorts keys, so the accepted snapshots are unaffected and still pass with `INSTA_UPDATE=no`.
- Regression tests in `crates/paper-cli/tests/status.rs`: no block and `outside_paper: null` right after `save`, `undo`, `redo` and `restore`.
- Moved-line tests in `crates/paper-core/tests/status.rs`: `git reset --hard HEAD~1` (after a second paper save) and `git commit --amend`.
- fmt, clippy `-D warnings` and `cargo test --workspace --all-targets --no-fail-fast` pass.

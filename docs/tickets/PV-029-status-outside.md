# PV-029: status says when the line moved outside paper

- **Status:** In progress
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

- [ ] After a plain `git commit` on the line, `paper status` shows the moved-line block with
  `git log <line>`. The exit code is 0, and `save`'s refusal is unchanged.
- [ ] After `git switch -c other` (and after a detached `git checkout <sha>`), `status` shows the
  another-branch block with `git switch <line>`.
- [ ] A healthy project shows no block, and its JSON has `"outside_paper": null`. Every existing
  status snapshot is byte-identical apart from the new JSON key.
- [ ] JSON has the three shapes above. Snapshot-tested.
- [ ] Works with `--no-snapshot`.
- [ ] `paper status` writes nothing to git refs or HEAD in these states (cross-checked with `git
  rev-parse` and `git symbolic-ref`).
- [ ] Specs updated (only the listed section).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

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

## Handoff log

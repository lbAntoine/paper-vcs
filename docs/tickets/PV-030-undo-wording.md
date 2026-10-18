# PV-030: undo wording polish and stale spec sentences

- **Status:** In progress
- **Issue:** #55
- **Branch:** `feat/PV-030-undo-wording`
- **Depends on:** PV-024 (merged). Runs in parallel with PV-032 (unignore).
- **Touches:**
  - `crates/paper-core/src/error.rs`: the undo-family mapping of `UndoError::Save`, and the `OutOfSyncWithGit` explanation.
  - `crates/paper-core/src/project/undo.rs`: the `undo --to` no-op.
  - `crates/paper-core/tests/undo.rs`.
  - `crates/paper-cli/src/render/undo.rs`.
  - `crates/paper-cli/src/commands.rs`: undo wiring only, if needed.
  - `crates/paper-cli/tests/undo.rs` and its snapshots.
  - `crates/paper-cli/tests/save.rs`: only if a refusal assertion changes.
  - `docs/specs/02-cli.md`: § `paper undo`, § `paper restore`, § `paper save` (refusal text only).
  - `docs/specs/03-architecture.md`: the stale sentences listed below.
  - Bookkeeping:
    - `tasks.json`: move PV-035 to sprint v0.3.
    - `docs/tickets/PV-026-session-proptest.md`: tick its CI-time box, which was measured as 47 s on Windows in PR #69.
- **Must not touch:**
  - `crates/paper-core/src/project/ignore.rs` (and `ignore/`), `profile/`, `activity/`.
  - `crates/paper-cli/src/render/ignore.rs`.
  - 02-cli § `paper ignore`, and 01-concepts. These belong to PV-032.
- **Specs:** `02-cli.md` § `paper undo` / § `paper restore` / § `paper save`; `03-architecture.md`

## Goal

Undo's messages read naturally and say what to do next. Repeating `undo --to` is harmless. The
specs stop describing v0.2 work as future or as already present when it isn't.

## Context

- `UndoError::Save(inner) => inner.into()` (`error.rs:737`). After a plain `git commit`, undo
  reuses save's refusal ("Paper can't save on top of that yet, and nothing was saved or
  changed."). The text is accurate but reads oddly for undo.
- Running `undo --to <id>` twice records two identical "go back to …" entries.
- Stale text:
  - 03-architecture.md:129-130: "Today every view change appends its own op … one op per
    command arrives with `paper activity`". This contradicts I4's "at most two entries", and
    `paper activity` exists.
  - 02-cli.md:282-285: restore says "until v0.2 the draft snapshot below is the safety net"
    and "the activity log that `undo` reads arrives in v0.2".
  - Search the specs for "later the undo and redo kinds", for other "until v0.2", "arrives in
    v0.2" and "planned (v0.2" phrases, and for anything describing `gc` as not existing. `gc`
    shipped in PV-027, so the issue's gc item is now the reverse: make sure nothing still calls
    it planned.
  - Leave 02-cli § `paper ignore`'s "planned (v0.2, PV-032)" sentence to PV-032.

## Decisions (coordinator, 2026-10-18)

- **Undo-family refusal wording.** When the line or HEAD was moved with plain git, `undo`,
  `redo` and `undo --to` keep save's exit codes and `next` suggestions, with their own text:
  - **Line moved:** headline `The line 'main' was changed outside paper`. Explanation:
    `Commits were added to it, or it was moved, with plain git since paper last saw it. Paper
    can't undo or redo across that yet, and nothing was undone or changed.`
  - **Another branch:** `Git is on the branch 'other', not on the line 'main'`, with
    explanation `Undo works on the line you are on, and paper's current line and git's branch
    have to agree. Nothing was undone.` Suggestion: `git switch main`.
  - Implement this as a separate mapping for `UndoError::Save` that matches the two variants.
    `From<SaveError>` stays unchanged for `save` itself.
- **A way out after a plain-git commit.** For save and the undo family, the `OutOfSyncWithGit`
  explanation ends with: `Paper will be able to bring in changes made with plain git in v0.4;
  until then, keep working with plain git on this line, or move it back to paper's last save.`
  - Suggestions: `git log <line>` (see what changed), then
    `git branch -f <line> <paper's last save id>` (move the line back; your git commits stay
    reachable in `git reflog`).
  - The second suggestion needs the short commit id of paper's last save on that line. If the
    error can't carry it cheaply, use `git reflog <line>` (find paper's last save) instead.
    Choose one and say which in the handoff.
  - Don't promise anything else.
- **Repeated `undo --to`.** When the target is already the current position (`position ==
  target`, nothing to check out), the command records nothing (besides the I1 edits entry, if
  any) and returns a new `UndoOutcome::AlreadyThere`.
  - CLI: `Already there.`, exit 0. JSON `outcome: "already_there"`, `entry` naming the target,
    and `changed`/`skipped` empty.
  - If unsaved edits were recorded first, the position moved to that edits entry, so the target
    is no longer current and the normal go-back happens. That is correct, and a test covers it.
- **Stale sentences.** Rewrite each one to describe what is true now (v0.2), in the same
  register. Don't add new promises.
- **Bookkeeping in this branch.**
  - `tasks.json`: PV-035's sprint becomes `v0.3`, and its description notes the maintainer
    decision ("ships in v0.2.0 as a known issue").
  - PV-026 ticket: tick its "under ~3 minutes on CI" box, with "Windows 47 s (PR #69)".

## Scope

1. Undo-family refusal mapping and the way-out text (save and undo).
2. `UndoOutcome::AlreadyThere` and its rendering, in human and JSON form.
3. Spec fixes: 02-cli § `paper undo` (`AlreadyThere`, refusal text), § `paper restore` (stale
   sentences), § `paper save` (refusal text); 03-architecture (stale sentences).
4. Bookkeeping (above).

## Out of scope

- The `--local` undo note: PV-032 adds it to `render/undo.rs` after this ticket merges.
- A JSON field naming leftover files (PV-031 dropped `left_over`).
- Importing plain-git commits (v0.4).

## Acceptance criteria

- [ ] After a plain `git commit`, `paper undo` and `paper redo` show the undo wording. Exit codes and `next` are unchanged, and `paper save` keeps save's wording.
- [ ] The `OutOfSyncWithGit` explanation, for save and undo, ends with the v0.4 sentence, followed by the two suggestions.
- [ ] `git switch -c other`, then `paper undo`, gives the another-branch undo wording.
- [ ] Running `paper undo --to <id>` twice: the second prints `Already there.`, exits 0 and records nothing. JSON is `"outcome":"already_there"`. With unsaved edits in between, the normal go-back happens.
- [ ] No spec sentence describes v0.2 features as future, or describes a missing feature as present. The listed sentences are fixed.
- [ ] Bookkeeping done (PV-035 sprint, PV-026 box).
- [ ] Specs updated (only the listed sections). Snapshots reviewed.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/undo.rs`: `AlreadyThere` (and the unsaved-edits case), and the undo-family error mapping (`PaperError` text for both variants).
- `crates/paper-cli/tests/undo.rs`: refusal output after a plain `git commit` and after `git switch -c`, plus repeated `--to`, in human and JSON form.
- `crates/paper-cli/tests/save.rs`: save's refusal still says "save", and includes the v0.4 way out.
- Tests must fail on `origin/main`. No hand-written snapshots. Assert key phrases with `contains`.

## Summary

## Handoff log

### RED · test-author · 2026-10-18

Tests added (11 new, all fail for the stated reason; 2 pass by design):

- `crates/paper-core/tests/undo.rs` (+8; 6 fail): moved-line undo wording for undo/redo/undo --to
  (`undo_redo_and_undo_to_word_a_moved_line_for_undo_not_for_save`), another-branch wording
  (`undo_family_words_another_branch_...`), detached HEAD says "Nothing was undone.", save keeps its
  wording and gains the v0.4 way out, `undo_to_the_entry_the_project_is_at_records_nothing`,
  `repeating_undo_to_...second_is_already_there`. Fail on message/why assertions or
  `assert_eq!(.., AlreadyThere)`. Passes by design on main: `undo_to_the_same_entry_with_unsaved_edits_in_between_goes_back_again`
  (the normal go-back; guards GREEN against over-reaching).
- `crates/paper-cli/tests/undo.rs` (+4; 3 fail): plain-commit wording for undo/redo/undo --to,
  `git switch -c other` wording, repeated `--to` human (`Already there.\n`, exit 0, state and
  activity unchanged) and JSON (`already_there`, entry.short_id = target, changed/skipped `[]`;
  `files_match` deliberately not asserted). Passes by design: the unsaved-edits go-back.
- `crates/paper-cli/tests/save.rs` (+1, fails): save still says "can't save on top of that yet", has "v0.4" and "git log main", no "undo or redo".
- The way-out assertion in core is strict: `why` ends with the exact v0.4 sentence; `next[0]` is
  `git log main`; `next[1]` starts with `git branch -f main` or `git reflog main` (GREEN chooses).
- Probe: copied tests and stubs onto a detached `origin/main` worktree (removed); same result as on
  the branch: 11 failures, assertion failures only, pre-existing tests all pass.

Stubs: `UndoOutcome::AlreadyThere` (never returned); `render/undo.rs` arms are `todo!()`.
Outside the Touches list: `crates/paper-core/tests/session_proptest.rs` got three match arms for the
new variant (needed to compile). `variant_ok` treats `AlreadyThere` as a model mismatch, so GREEN must
model `Aim::To(current position)` as `AlreadyThere` (no change) there, or the proptest will fail.
Existing tests `undo_after_a_plain_git_commit_exits_with_saves_error...` and the git-switch one
compare only the first stderr line and the exit code, which stay equal, so they still pass.

Questions: none.

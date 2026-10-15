# PV-007: save and snapshot hardening

- **Status:** Ready
- **Branch:** `feat/PV-007-save-hardening`
- **Depends on:** PV-001 (merged)
- **Touches:** `crates/paper-core/src/project/save.rs`, `crates/paper-core/tests/save.rs`,
  `crates/paper-core/tests/snapshot.rs`, `docs/specs/02-cli.md`
- **Specs:** `docs/specs/02-cli.md` § `paper save`; PV-001 ticket, review round 2 entry

## Goal

Close the four findings from PV-001's second review: a partial save only pulls in what a valid
tree strictly needs, a full save stays fast on large projects, the snapshot fix for file↔folder
changes has its own tests, and the "line changed with plain git" error covers every way that
can happen.

## Context

- `save.rs`: `select` (path matching), `close_over_conflicts` (the closure from PV-001 review
  round 1, currently symmetric, transitive and O(n²)), and the in-sync checks that return
  `OutOfSyncWithGit` / `HeadOnAnotherBranch`.
- `02-cli.md` § `paper save` (around lines 85–95) describes the current closure behaviour.
- The snapshot fix (removals before additions; a former folder that's now a file counts as a
  deletion) lives in `snapshot/mod.rs` and is currently only exercised through `tests/save.rs`.

## Scope

1. **One-way closure.** Only a selected *addition or modification* forces other changes:
   - a selected file `a/b/c` forces the deletion of any changed path that is one of its
     ancestors (`a`, `a/b`) and is a file in the tip;
   - a selected file `p` forces the deletion of every tip file under `p/`.

   Selected deletions force nothing. Forced deletions are themselves deletions, so the closure
   is a single pass, not a fixpoint. Excluded paths (`AllBut`) that a selected change forces
   are still saved: a valid tree needs them, and `saved` reports them.
2. **Fast path.** With `PathSelection::All`, or when the selection already covers every change,
   skip the closure entirely. Otherwise use sorted lookups (`BTreeSet`/`BTreeMap` ranges), not
   a scan of every pair, so the closure is O(n log n).
3. **Snapshot tests.** Add two tests to `tests/snapshot.rs` that call `snapshot()` directly:
   a tracked file `notes` replaced by `notes/a.tex`, and a tracked `notes/a.tex` replaced by a
   file `notes`. Check the resulting tree with `git ls-tree -r`.
4. **Error wording.** `OutOfSyncWithGit` covers every way the line can differ from what paper
   last recorded: commits made with plain git, `git reset`, or the branch being deleted. The
   message should say "the line '<line>' was changed with plain git since paper last saw it;
   paper can't save on top of that yet" or equivalent, without assuming new commits.
5. **Spec.** Update `02-cli.md` § `paper save` to describe the one-way rule.

## Out of scope

- Importing changes made with plain git (still refused).
- Any change to `snapshot` behaviour (tests only).

## Acceptance criteria

- [ ] Tip has `notes/a.tex` and `notes/b.tex`; the working files have a file `notes`.
  `Only(["notes/a.tex"])` saves only the deletion of `notes/a.tex`; `remaining` holds the
  addition of `notes` and the deletion of `notes/b.tex`; `git fsck --strict` passes.
- [ ] Same setup: `Only(["notes"])` saves the addition of `notes` and both deletions.
- [ ] Tip has a file `notes`; the working files have `notes/a.tex` and `notes/b.tex`.
  `Only(["notes/a.tex"])` saves the deletion of `notes` and the addition of `notes/a.tex`
  only; `notes/b.tex` stays in `remaining`.
- [ ] `AllBut` with an excluded path that a selected change forces still saves it and reports it in `saved`.
- [ ] A full save with `PathSelection::All` doesn't run the closure, and the closure has no pairwise scan (checked in review).
- [ ] Both snapshot flip tests pass, calling `snapshot()` directly and checking `git ls-tree -r`.
- [ ] After `git reset` moves the line back, `save` returns `OutOfSyncWithGit`, its message
  doesn't claim new commits, and nothing is written.
- [ ] After the line's branch is deleted with plain git (HEAD still attached), `save` returns `OutOfSyncWithGit`.
- [ ] `02-cli.md` describes the one-way rule.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass.

## Test plan

- `tests/save.rs`: the four closure scenarios above (two-file folder in both directions,
  `Only(["notes"])`, `AllBut` with a forced exclusion), each checking `saved`, `remaining`,
  `git ls-tree -r main` and `git fsck --strict`. Two out-of-sync scenarios: `git reset --hard`
  to the previous save, and `git update-ref -d refs/heads/main`.
- `tests/snapshot.rs`: the two flip tests.
- Existing PV-001 flip tests may need their expectations updated where the closure now pulls
  in less. Each such change must be listed in the handoff log with the reason.

## Summary

## Handoff log

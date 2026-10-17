# PV-022: export step, export record and keep-alive chain

- **Status:** In progress
- **Issue:** #40
- **Branch:** `feat/PV-022-export-keep`
- **Depends on:** PV-021 (activity entries, merged). Runs alone.
- **Touches:** `crates/paper-core/src/project/{mod,init,save,restore,migrate}.rs`,
  `crates/paper-core/src/project/export.rs` (new), `crates/paper-core/src/activity/` (keep-alive,
  if it lives there), `crates/paper-core/src/error.rs`, `crates/paper-core/tests/export.rs` (new),
  `crates/paper-git/src/repo.rs` (only if a small helper is missing, e.g. reading many refs),
  `docs/specs/03-architecture.md` (§ On-disk layout, § Command lifecycle)
- **Specs:** `03-architecture.md` § On-disk layout (`exported.json`, `refs/paper/keep`), § Command
  lifecycle (steps 6–7); `01-concepts.md` I5, I6; ADR-0002

## Goal

Undo (PV-023) can bring back any earlier state, because nothing paper ever pointed at can be
garbage-collected by git, and one export step makes git's refs, HEAD and index follow whichever
view is current. paper also remembers what it last wrote, so it can tell its own changes from
someone else's.

## Context

- Ref writes are scattered: `init.rs` (`set_ref` for line and draft, `set_head_branch`,
  `reset_index_to`), the snapshot step and `save` in `save.rs`. Ops are recorded by
  `Project::record` (`project/mod.rs`) after those writes.
- `save::check_in_sync` compares the line's ref with the *view* to detect plain-git changes.
- `GitRepo::write_commit` takes any number of parents; `empty_tree` exists.
- Old drafts become unreachable once a snapshot rewrites the draft ref, so `git gc --prune=now`
  can delete them today.

## Decisions (coordinator, 2026-10-17)

- **One export step.** `record(kind, view, description)` becomes: (1) append the keep-alive
  commit, (2) append the op (step 6), (3) export the view (step 7). Commands stop writing refs,
  HEAD and the index themselves; they write commits, then call `record`. Export writes, for every
  line in the view: `refs/heads/<line>` = tip, `refs/paper/draft/<line>` = draft; HEAD →
  `refs/heads/<current line>`; the index = the current tip's tree, **only when the tip changed**
  since the last export (resetting it loses git's stat data, so `git status` would rehash);
  then `exported.json`.
- **`exported.json`:** `{"schema":1,"head":"<line>","refs":{"refs/heads/main":"<hash>",
  "refs/paper/draft/main":"<hash>","refs/paper/keep":"<hash>"}}`, written atomically after the refs.
  It is what paper last wrote, nothing more.
- **Keep-alive chain:** for each op, one commit on `refs/paper/keep` with the empty tree, message
  `paper keep-alive`, and parents = the previous keep commit (if any) + every commit in the op's
  view that wasn't in the previous op's view (the first op: all of them). One ref keeps every
  commit any op ever named reachable, so `git gc` can't remove what undo needs. Written before the
  op, so a crash leaves at worst one extra keep commit. Author/committer: paper's identity, current
  time. The migration op gets a keep commit too.
- **Detecting plain-git changes:** `check_in_sync` compares the line's ref with `exported.json`
  ("what paper last wrote"), not with the view. A missing `exported.json` (a project from before
  this ticket) falls back to the view, then the next export writes it.
- **Crash between op and export** (steps 6–7): refs are briefly behind the head op; the next
  command's export brings them in line. Detecting and repairing this on open is PV-025; note it in
  the spec.
- **Never pushed (I5):** nothing here pushes; `refs/paper/*` stays local.

## Scope

1. Export step and `exported.json`; commands no longer write refs, HEAD or the index directly.
2. Keep-alive chain appended for every op, migration included.
3. `check_in_sync` against `exported.json`, with the fallback.
4. Specs: 03-architecture § On-disk layout (`exported.json` format; keep commit shape), § Command
  lifecycle (record = keep → op → export; index only when the tip changed; the crash window and
  PV-025).

## Out of scope

- Undo/redo (PV-023/024); recovery on open (PV-025); rebuilding the chain and pruning (`paper gc`,
  PV-027); importing external git changes (v0.4).

## Acceptance criteria

- [ ] After every command, `refs/heads/<line>`, `refs/paper/draft/<line>` and HEAD match the head op's view, and `exported.json` matches the refs.
- [ ] A command that doesn't change the tip (snapshot, restore) leaves `.git/index` untouched (same bytes, or same mtime); `save` updates it to the new tip's tree; `git status` is clean after `save`.
- [ ] Each op adds exactly one keep commit whose parents are the previous keep commit plus the op's new commit ids; the first op's keep commit names every commit in its view.
- [ ] After several edit/snapshot/save/restore rounds, `git gc --prune=now` keeps every draft and save any op ever named (`git cat-file -e` on each); `git fsck --strict` is clean.
- [ ] `git push` of a line pushes no `refs/paper/*` (checked with a bare remote and `git ls-remote`).
- [ ] `save` still refuses after a plain-git commit on the line or a `git switch`, with the same errors as today; a project without `exported.json` still works and gets one after its next command.
- [ ] A migrated v0.1 project gets a keep commit and `exported.json` on first open.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/export.rs` with real repos, cross-checked with the git binary:
  `for-each-ref`, `symbolic-ref HEAD`, `ls-files -s` (index), `cat-file -e` after
  `gc --prune=now`, `rev-list --parents -n 1 refs/paper/keep`, `ls-remote` after a push.
- Existing tests that assert refs after commands must keep passing unchanged.

## Summary

## Handoff log

# PV-022: export step, export record and keep-alive chain

- **Status:** Done
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

One export step after every op, a record of what paper last wrote, and a keep-alive chain so
`git gc` can never remove a commit undo needs.

- `record` = keep-alive commit → op → export. Export writes each line's tip and draft refs and
  HEAD from the view, resets the index only when the tip moved or git's HEAD isn't on the current
  line (index first, then refs, then HEAD), and writes `exported.json` last. Commands no longer
  write refs, HEAD or the index themselves.
- A branch or HEAD that plain git moved since paper last wrote it is left alone (`exported.json`
  keeps paper's value), so `save` still refuses; the format-1 migration follows the same rule.
- `refs/paper/keep`: one empty-tree commit per op whose parents are the previous keep commit and
  the op's new commit ids; old drafts and saves survive `gc --prune=now` after reflogs expire.
- `check_in_sync` compares against `exported.json`, falling back to the view.

Deviations: leave-alone for plain-git moves and keeping the index on adopt (beyond the ticket,
kept after review); the activity tests' `git_state` helper excludes `refs/paper/keep`
(authorised). The index-before-refs ordering has no test yet: it needs fault injection (PV-025).

Follow-up: #52 (PV-029) so `status` says when the line moved outside paper.

## Handoff log

### RED · test-author · 2026-10-18

- `crates/paper-core/tests/export.rs` (new): 25 tests, no stubs needed (everything goes through
  the existing public API: `Project`, `View`, files under `.git/paper/`, the `git` binary).
- **17 fail on assertions** (no `refs/paper/keep`, no `exported.json`): export shape and refs for
  new/adopted/reopened projects; the keep chain (first op names all commits, one keep commit per
  op, parents = previous keep + new commits, empty tree, `paper keep-alive`, paper identity,
  no keep for no-op commands); gc survival (reflogs expired first, then `gc --prune=now`, then
  `cat-file -e` on every commit any op names); migration (format-1 rewrite, keep + exported.json);
  missing `exported.json` gets one after `save`/`snapshot`; `check_in_sync` against
  `exported.json` (refs behind the head op, exported.json naming them, save still allowed).
- **8 pass on main, on purpose** (they pin behaviour the export step must not break): index
  untouched by snapshot and by restore (main never touches the index there either; the tests
  guard against the new export resetting it), index equals the tip tree after init and after
  `save`, `save` refuses after a plain-git commit / `git switch` / plain-git commit with no
  `exported.json` (fallback to the view), nothing under `refs/paper/*` is pushed.
- Assumptions: `exported.json` lives at `.git/paper/exported.json`; keep parents are compared as
  sets (order not pinned); a deleted `exported.json` is removed with `.ok()` so the test fails for
  the right reason on main.
- Existing tests unchanged and passing. Ran with empty `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM=1`.

### GREEN · rust-engineer · 2026-10-18

- New `crates/paper-core/src/project/export.rs`: `write_keep` (keep commit, then `refs/paper/keep`),
  `export` (draft and line refs, HEAD, index, `exported.json` via `write_atomic`), `read_exported`
  (a missing, unreadable or other-schema file is `None`), `ExportError` (public; `Export` variants
  added to `SaveError`, `InitError`, `OpenError` and their `PaperError` arms).
- `Project::record` is now keep -> op -> export. `snapshot_as`, `save`, `init` (fresh and adopt) and
  migration no longer write refs, HEAD or the index. `check_in_sync` compares the line ref with
  `exported.json`, falling back to the view.
- Index: reset only when the current line's tip (or the head line) differs from what was last
  written (`exported.json`, else the previous view). `init` of a new or empty repository resets it;
  adopting a repository with history leaves the index as it was (staged changes stay staged).
- Deviations/additions (not in the ticket, both to avoid clobbering plain git): the export leaves
  alone a `refs/heads/<line>` or HEAD that plain git moved since paper last wrote it, and keeps the
  old value in `exported.json` so `save` still refuses. Without this, `paper snapshot` after a
  `git switch` or plain-git commit would have reset HEAD or the branch.
- Migration reuses an existing `refs/paper/keep` instead of writing a second keep commit: a v0.1
  project has none, and a leftover one is from an earlier attempt at the same migration. This keeps
  the existing activity tests (refs and `log --all` unchanged by migration) passing unchanged.
- Spec: 03-architecture § On-disk layout (exported.json, keep commit shape, migration) and
  § Command lifecycle (record/export paragraph, steps 6-7, crash window -> PV-025).
- No test assertion changed. Verified with `INSTA_UPDATE=no`, empty `GIT_CONFIG_GLOBAL`, fmt, clippy.

### Review · reviewer · 2026-10-18
- Verdict: CHANGES. Tests, clippy and fmt pass. Ordering keep → op → export, the index rule, `exported.json`, the keep chain's reachability by induction (gc probe after expiring reflogs), and "no ref writes outside export" verified.
- BLOCKER: the migration exports with `Mode::Fresh`, which rewrites every ref and HEAD unchecked. Reproduced with the CLI: a plain-git commit on a v0.1 project, then `paper status` after upgrading, resets `main` to paper's old tip; `exported.json` then lets the next `save` orphan the commit for good. Breaks ADR-0002.
- MAJOR: nothing tests "leave alone what plain git moved"; the three `save` tests return before export runs.
- MINOR: the migration's keep-ref reuse is unsafe if `view.json` changed after a crashed attempt.
- MINOR: after HEAD was left alone and the user switches back, the next export resets the index anyway.
- MINOR (pre-existing): `status` doesn't say the line moved outside paper.
- NIT: `Mode::Fresh` is now only for `init`.
- GREEN's additions: keep "leave plain-git moves alone" and "adopt keeps the index"; replace the keep reuse.
- Coordinator decisions:
  - The migration exports with `Mode::Follow { previous: &view }`; tests for a plain-git commit and for a `git switch` on a format-1 project before the first open (refs and HEAD unchanged, `save` refuses).
  - Tests for leave-alone through `snapshot` and `restore`: ref and HEAD untouched, `exported.json` keeps paper's value, index bytes unchanged, draft ref still exported, a later `save` refuses.
  - The migration always appends a keep commit; adjust the activity tests' `git_state` helper to exclude `refs/paper/keep` and keep-alive commits (authorised: the behaviour adds that ref on purpose).
  - Count a head change for the index reset only when git's HEAD differs from the current line.
  - Document that `Fresh` is for `init` only; fix the spec's migration sentence.
  - The `status` follow-up becomes its own issue.

### GREEN (review round 1) · rust-engineer · 2026-10-18

- BLOCKER: the migration now exports with `Mode::Follow { previous: &view }`, so a plain-git commit or `git switch` made before the first open is left alone. Spec sentence fixed. `Mode::Fresh` is documented as `init`-only.
- Migration always appends its keep commit; the keep-ref reuse is gone. In `tests/activity.rs`, the `git_state` helper now compares only `refs/heads`, `refs/tags`, `refs/paper/draft` and `log --all --exclude=refs/paper/keep` (authorised: the keep chain is added on purpose). No other assertion changed.
- Index: a head change counts only when git's actual HEAD differs from the current line.
- New tests in `tests/export.rs` (7 added, 2 migration + 5 leave-alone): migration after a plain-git commit and after a `git switch`; snapshot and restore after each (ref/HEAD untouched, `exported.json` keeps paper's value, index bytes unchanged, draft ref exported, `save` refuses); switching back does not reset the index.
- `status` follow-up is #52 (PV-029), not done here. fmt, clippy and the full suite pass (INSTA_UPDATE=no, empty GIT_CONFIG_GLOBAL).

### GREEN (post-approval) · rust-engineer · 2026-10-18

- The export rewrites the index before HEAD and the line refs; spec clauses and the rewrap done. No ordering test: it needs fault injection between the index write and the ref writes, which the project does not have yet (crash tests arrive with PV-025).

### Review round 2 · reviewer · 2026-10-18
- Verdict: APPROVE. The migration BLOCKER no longer reproduces (plain-git commit and `git switch` variants, CLI); leave-alone tested through `snapshot` and `restore`; the narrowed `git_state` loses nothing for today's fixtures; `--exclude` usage correct.
- Fixed after approval: index written before refs and HEAD (crash-safe for line changes); two 03-architecture clauses; one rewrap.

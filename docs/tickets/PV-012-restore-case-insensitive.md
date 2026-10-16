# PV-012: restore on case-insensitive filesystems, and all-or-nothing flips

- **Status:** In progress
- **Branch:** `feat/PV-012-restore-case-insensitive`
- **Depends on:** PV-010, PV-014 (merged). Runs in parallel with PV-008 (save's path selection).
- **Touches:** `crates/paper-core/src/project/restore.rs` (and a new sibling module under
  `crates/paper-core/src/project/` if it grows past ~400 lines), `crates/paper-core/tests/restore.rs`,
  `docs/specs/02-cli.md` (§ `paper restore` only)
- **Must not touch:** `crates/paper-core/src/project/save.rs` (including `matches`), `crates/paper-core/tests/save.rs` (PV-008).
- **Specs:** `02-cli.md` § `paper restore`; `01-concepts.md` I1, I6; found in the PV-010 review, round 2

## Goal

`paper restore` gives the same end result on macOS and Windows (whose default filesystems ignore
case) as on Linux, and never leaves a file/folder swap half done.

## Context

- `Project::restore` (`project/restore.rs`) decides everything first: `deletes` (files not in the
  `from` save), `candidates` to write, and `blocked(path, in_draft, doomed)`, which refuses a write
  when something paper doesn't record is in the way. Then it deletes, then writes, then snapshots.
- **Bug 1, case-only renames.** On a case-insensitive filesystem, restoring `Notes.tex` (in the save)
  over a draft that has `notes.tex`: `notes.tex` is doomed, but `blocked` sees `notes.tex` on disk at
  the path `Notes.tex`, so the write is skipped as blocked, while the deletion still runs. The user
  ends with neither file (recoverable from the pre-restore draft, but wrong). The same class applies
  to a folder renamed by case (`Sections/` in the save, `sections/` in the draft).
- **Bug 2, half-done flips.** When the write half of a file/folder swap is blocked (e.g. an ignored
  file inside the folder that must become a file), the deletions that were only there to make room
  still run.
- Linux CI is case-sensitive; macOS and Windows CI are case-insensitive. Tests must assert an end
  state that is correct on both.

## Decisions (coordinator, 2026-10-16)

- **Case-only renames:** an entry on disk that blocks a write is not an obstacle if it is a doomed
  path (or under a doomed folder) that equals the target ignoring ASCII and Unicode simple case.
  The deletion runs first, then the write, so the result is the save's spelling on every
  filesystem. Folders are pruned before writes so a case-renamed folder ends with the save's
  spelling too.
- **All-or-nothing flips:** if a write is blocked, every deletion that exists only because of that
  write (the doomed files under the target path, and a doomed file at one of the target's parent
  paths) is dropped from the plan, and those paths are reported as skipped (`Blocked`) with the
  target. Unrelated deletions in the same restore still happen.
- Detecting case sensitivity at run time is allowed in tests only; the product logic must not
  depend on knowing it.

## Scope

1. Case-only renames for files and folders, as decided.
2. All-or-nothing flips, as decided.
3. Specs: 02-cli § `paper restore` (case-only renames give the save's spelling; a blocked swap leaves
   both sides as they were and lists them as skipped).

## Out of scope

- Case-only renames in `save`/snapshot (the draft records what the walk sees) and in `diff`.
- Unicode normalisation (NFC/NFD) differences on macOS.

## Acceptance criteria

- [ ] Restoring `Notes.tex` over a draft `notes.tex` (save has only `Notes.tex`) ends with exactly one file, spelled `Notes.tex` (checked by listing the folder), with the save's content, on Linux, macOS and Windows.
- [ ] The reverse (`notes.tex` in the save, `Notes.tex` in the draft) works the same way.
- [ ] A folder renamed by case (`Sections/a.tex` in the save, `sections/a.tex` in the draft) ends spelled `Sections`, with the save's content.
- [ ] A file/folder swap whose write is blocked by an ignored file leaves every file on both sides untouched, lists them as skipped, and still performs an unrelated deletion in the same restore.
- [ ] After each of these, the draft matches the working files, `git fsck --strict` is clean, and HEAD and the index are unchanged.
- [ ] Every existing restore test still passes unchanged.
- [ ] Specs updated.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS and Windows).

## Test plan

- `crates/paper-core/tests/restore.rs`, reusing its helpers (projects use the `generic` profile).
- Build the "save" side with real saves, then rename on disk to create the draft side, snapshot, and
  restore. Assert end states by listing directory entries (`fs::read_dir` names), not by
  `Path::exists` (which ignores case on macOS and Windows).

## Summary

## Handoff log

### RED · test-author · 2026-10-16

- `crates/paper-core/tests/restore.rs`: 4 new tests (16 existing still pass), plus helpers `names`, `rename_on_disk`, `restore_and_check_untouched_history`.
  - 3 case-only tests (file, reverse file, folder): restore both spellings as paths; assert the listing has only the save's spelling, content, HEAD/index unchanged, draft matches, fsck. On macOS they fail today (draft not matching: neither file is left). On Linux they pass already (case-sensitive), which is expected.
  - 1 flip test: ignored `chap/cache.log` blocks `chap` becoming a file; asserts `chap/one.tex` kept, both paths skipped `Blocked`, only `extra.tex` deleted. Fails today (`chap/one.tex` is deleted).
- No stubs: the public API already suffices. Case tests pass both spellings in `paths` so the selection is the same everywhere. No runtime case probe used.
- Questions: none.

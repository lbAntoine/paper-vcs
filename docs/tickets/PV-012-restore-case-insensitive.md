# PV-012: restore on case-insensitive filesystems, and all-or-nothing flips

- **Status:** In progress
- **Branch:** `feat/PV-012-restore-case-insensitive`
- **Depends on:** PV-010, PV-014 (merged). Runs in parallel with PV-008 (save's path selection).
- **Touches:** `crates/paper-core/src/project/restore.rs`, `crates/paper-core/src/project/restore_plan.rs` (new),
  `crates/paper-core/src/project/mod.rs`, `crates/paper-core/src/snapshot/mod.rs` (added by the
  coordinator after the reproduction below), `crates/paper-core/tests/restore.rs`,
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

### GREEN · rust-engineer · 2026-10-16 — BLOCKED on two test bugs

- Planning moved to a new `project/restore_plan.rs` (`Project::settle`, `blocked`, helpers); `restore.rs` is 291 lines.
  - Writes are settled to a fixpoint: a blocked write is dropped together with the deletions that exist only for it (doomed files under the target, a doomed file at a parent path, or the entry the target aliases), and all are reported `Blocked`. Unrelated deletions still run.
  - Case-only: an entry that is in the way but spelled differently (found by listing each folder, never by probing the filesystem) is not an obstacle if the restore is removing it. Deletions already ran before writes, so folders end with the save's spelling.
- Deviation, needed for the case tests: `snapshot/mod.rs` second loop (tracked paths the walk didn't see) now requires the exact spelling to be listed (`has_exact_name`). Before, on a case-insensitive filesystem `Notes.tex` was "found" through `notes.tex`, so the draft kept both spellings and restore could never reach a matching draft. Behaviour is unchanged on case-sensitive filesystems.
- Spec: 02-cli § `paper restore` updated.
- On this machine (macOS, case-insensitive) with the RED tests as written: 4 fail, all for test bugs, not product behaviour. With `names` patched and the flip assertions fixed locally, all 20 restore tests and the full suite pass; fmt and clippy are clean. I reverted that local patch.
  1. `names(dir, "")` lists the project root, which always holds `paper.toml` (written by `init`), and it hides dot-entries, so the root-listing assertions in the three case tests (`["Notes.tex"]`, `["notes.tex"]`, `["Sections"]`) can never hold, and the flip test's `[".gitignore", "chap", "keep.tex"]` can't either. Fix: let `names` also drop `paper.toml`, and expect `["chap", "keep.tex"]`.
  2. `Project::snapshot().changes` is the diff against the last save (the tip), not against the previous draft. In the flip test the draft legitimately differs from the tip (the file/folder swap was kept, `extra.tex` deleted), so `assert_eq!(snapshot().changes, vec![], "draft matches")` can't hold there. Fix: compare against the expected remaining changes (`chap` Deleted, `chap/one.tex` Added) or drop that assertion for this test.
- What `paper restore Notes.tex` does today (only the saved spelling named; draft holds `notes.tex`), on a case-insensitive filesystem: `notes.tex` is not a target, so it is not doomed. The write of `Notes.tex` finds `notes.tex` on disk (same file, other spelling), which is an untracked-looking obstacle, so the write is blocked: nothing changes and `Notes.tex` is listed as skipped `Blocked`. On a case-sensitive filesystem it simply writes `Notes.tex` next to `notes.tex`. No behaviour added beyond the ticket.

### RED (fix) · test-author · 2026-10-16

- Root listings now expect `paper.toml` explicitly (the dotfile filter stays; `.gitignore` is hidden by it).
- "Draft matches" now compares `status` with `snapshot: false` to `snapshot: true` (new helper `assert_draft_records_working_files`), modelled on `after_restore_the_draft_matches_the_working_files`.
- All 20 restore tests pass against d0cd6c5 on macOS.

### Reproduction · coordinator · 2026-10-16
- Reproduced on `main` (d67ad0d) with the CLI on macOS: save `Notes.tex`, replace it on disk with `notes.tex`, `paper restore .`.
  - **On disk the result is already right** (`Notes.tex` with the saved content): bug 1 as described in Context no longer reproduces, fixed incidentally by PV-014's restore changes.
  - **The real case bug is in snapshot:** on a case-insensitive filesystem, a tracked `Notes.tex` still "exists" through `notes.tex`, so the draft records both spellings. Before the restore, `status` shows `modified Notes.tex` and `new notes.tex`; after it, `new notes.tex` although no such file exists.
- GREEN's `snapshot/mod.rs` change (record a tracked path the walk didn't produce only if its exact spelling is listed) is the fix, so `snapshot/mod.rs` joins Touches.
- The fixed RED tests pass against `main`'s code for the three case-only tests, because the "draft records the working files" check compares two equally wrong views. They must also assert the draft tree's exact file list. Bug 2 (blocked flip) still reproduces and its test fails on `main`, as it should.

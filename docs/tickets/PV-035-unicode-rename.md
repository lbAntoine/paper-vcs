# PV-035: a rename that changes only the Unicode form of a name loses the file (macOS)

- **Status:** In progress
- **Issue:** #67
- **Branch:** `fix/PV-035-unicode-rename`
- **Depends on:** none. Runs in parallel with PV-040 (paper-diff only).
- **Touches:**
  - `crates/paper-core/src/project/restore_plan.rs` (`settle`, `obstacle`, a same-file helper)
  - `crates/paper-core/src/project/checkout.rs` (only if the same-file rule must reach the to-draft planner too)
  - `crates/paper-core/tests/restore.rs`, `crates/paper-core/tests/undo.rs` (or a new `tests/same_file.rs`)
  - `docs/specs/03-architecture.md` (the checkout and restore planning rules)
  - `docs/specs/02-cli.md` (§ `paper restore`, only if a skip reason's wording changes)
- **Must not touch:** `Cargo.toml` and `Cargo.lock` (no new dependency), `paper-diff`, `undo.rs`'s tracking-only logic, `recover.rs`.
- **Specs:** `03-architecture.md` (checkout and restore planning), `02-cli.md` § `paper restore` (case aliases); NOTES.md 2026-10-19 (data-safety invariant rule)

## Data-safety invariant

A planned deletion never removes a file that another path in the same plan refers to on disk
(the same file, through the filesystem's own name folding), unless that path is written after
the deletion as part of the same swap.

## Goal

On a filesystem that folds names (APFS folds Unicode normalisation, and case-insensitive
volumes fold case), `paper undo` and `paper restore` never delete the only copy of a file. That
holds whatever folding rule the filesystem uses.

## Context

- **Reproduction** (issue #67, confirmed by the v0.2 verification for both `undo` and
  `restore --from last~1 .`):
  1. Track `été.tex` in NFC.
  2. Rename it to its NFD spelling, with the same bytes.
  3. `paper save`.
  4. `paper undo` twice.

  The plan deletes the NFD path and blocks the NFC write, and no file is left.
- **Cause:** `actual_spelling` (`restore_plan.rs:136`) matches names by `to_lowercase` only.
  - `obstacle` doesn't see that the NFD file on disk is the NFC target, so the NFC write is
    `Blocked`.
  - `settle` keeps a deletion for a swap only when `makes_room` matches a spelling it found by
    name, so the NFD deletion goes ahead.
  - The same gap exists for any folding `to_lowercase` misses (`ß`, `İ`), as the doc comment
    on `actual_spelling` already admits.
- **Existing guards:** the PV-018 guards in `checkout.rs` (`spelled_exactly`, `has_case_twin`,
  the "already there" rule) compare bytes, so they already skip the rule for an NFD/NFC pair.
  Check that this holds.

## Decisions (coordinator, 2026-10-20)

### Compare files, not names

On Unix, two paths are the same file when `symlink_metadata` gives the same `(dev, ino)`, via
`std::os::unix::fs::MetadataExt`. Add a small helper, for example
`same_file(root, a, b) -> bool`. Apply it in two places:

- **In `obstacle`:** an existing entry at `path` that is the same file as a doomed path is not
  an obstacle. Treat it like the case-alias swap: the deletion runs first, then the write.
- **In `settle`:** when a write is blocked, also keep every deletion that is the same file as
  the blocked target (`KeptForSwap`), not only the ones found by name.

### No new dependency

Not `unicode-normalization`, and no Unicode tables. The filesystem decides what counts as the
same file.

### Windows

NTFS doesn't normalise names, and std has no stable file id there. Keep today's name logic on
Windows. Document this, and make the tests `cfg(unix)` with a runtime check that the
filesystem folds the pair:

- write the NFC name;
- `symlink_metadata` the NFD name;
- if it doesn't resolve, the test returns early and says why.

On Linux ext4 the test is a no-op. On macOS APFS it runs.

### Keep the existing name logic

It still decides `CaseAlias` reasons and the spelling to report. The same-file rule is an
extra guard that is never weaker.

## Acceptance criteria

- [ ] The #67 reproduction through `undo` twice and through `restore --from last~1 .` (file and folder variants): the file survives with its content. The result is either the NFC spelling, swapped, or the NFD spelling left as is, never neither. `git fsck --strict` is clean.
- [ ] The same for a case-only rename where the filesystem folds case. The existing case-rename tests still pass.
- [ ] No planned deletion removes a file that a blocked write's target refers to on disk. The invariant is tested directly at the planner level.
- [ ] Ordinary restore and undo are unchanged. The existing suites pass on all three OSes.
- [ ] Specs updated, only in the listed sections, including the Windows note.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- Core tests for the reproduction via the API: two undos, and `restore --from last~1 .`, for a
  file and for a folder. Each skips unless the filesystem folds the NFC and NFD pair, and must
  fail on `origin/main` on macOS (file gone).
- A planner-level test of the invariant.
- The existing restore case-alias tests pass unchanged.
- No snapshots by hand.

## Summary

## Handoff log

### RED · test-author · 2026-10-20
- Tests: crates/paper-core/tests/same_file.rs (6 tests, whole file `cfg(unix)`, each skips with an eprintln unless the filesystem folds the pair: write one spelling, `symlink_metadata` the other).
  - NFC to NFD rename of a file: `undo` twice, and `restore --from last~1` naming both spellings (2).
  - Same for a folder (`Résumé/cv.tex`) (2).
  - Case-only rename (`Notes.tex` to `notes.tex`): `undo` twice, and restore (2).
- Each test asserts exactly one spelling is on disk, the content is intact under either name, and `git fsck --strict` is clean.
- Failing on (proved on a detached `origin/main` probe worktree with its own target dir, macOS APFS): the 4 Unicode-form tests fail with "exactly one spelling" left 0 (file gone). The 2 case-only tests pass on main by design (existing case-alias handling already covers them); they guard against regressions.
- Planner-level invariant: `restore_plan` is `pub(super)`, so not reachable from the public API. Covered end to end by the survival assertions above. The GREEN phase may add a unit test next to `restore_plan.rs`.
- Note: `restore` has no `.` path, so tests name both spellings explicitly, as the existing case-alias restore tests do.
- Stubs: none. Existing suites all pass; fmt and clippy clean.
- Questions: none

### GREEN · rust-engineer · 2026-10-20
- `restore_plan.rs`: `same_file(root, a, b)` (Unix: `symlink_metadata` `(dev, ino)`; other platforms: `false`, so the name logic decides) and `any_same_file`. `obstacle` no longer treats an entry that is the same file as a doomed path as an obstacle (parent file check and final check): deletion, then write. `settle` also keeps (`KeptForSwap`) every deletion that is the same file as a blocked target.
- `checkout.rs`: the "already there" rule could still lose the file (disk shows NFC, draft tracks NFD: NFD is deleted while NFC is judged already there and not written). Paths judged already there are now re-planned as writes when a deletion is the same file, so `settle` swaps them. Reaches `settle` otherwise unchanged.
- Unit tests in `restore_plan.rs` (`cfg(all(test, unix))`) using a hard link: helper behaviour, a swap through `settle`, and the invariant (`assert_invariant`) on a plan with an unrelated blocker. They run on any Unix filesystem.
- Spec: 03-architecture gets the same-file rule, the invariant and the Windows note. 02-cli unchanged (no skip wording changed).
- Known limit: a real hard link between an untracked path and a doomed tracked path is read as a folded name and swapped (documented in the spec).
- Checks: fmt, clippy `-D warnings`, full suite with `--no-fail-fast` pass on macOS; the 4 Unicode-form tests pass.

### GREEN (review round 1) · rust-engineer · 2026-10-20
- 1 (MAJOR): the parent-prefix file check, `only_holds` (child files) and `settle` (deletions that are the same file as any prefix of a blocked target) now use the same-file rule. `actual_spelling` also matches an entry by file identity, so `real` is right for NFD/NFC. New tests (runtime fold check): folder to NFD file via undo x2 and via restore, NFC file to NFD folder via undo x2. A restore variant of the reverse case was not added.
- 2 (MAJOR): unit test in `checkout.rs` for the already-there re-plan. It fails with the re-plan disabled (checked in a scratch copy with its own target dir). Deviation: it uses an NFC/NFD pair with a runtime fold check (skips on Linux), not a hard link, because with item 3 a hard link is no longer "the same file".
- 3: `same_file` = same inode and not both names listed exactly. A real hard link keeps the old behaviour (`Blocked`); Linux-runnable unit tests assert it. Hard-link sentence dropped from the spec; spec covers the parent/folder rule.
- 4: a draft path that is the same file is reported as `CaseAlias`; integration test added.
- 5: stray `same_file.rs-E` removed. 6: `root` hoisted.

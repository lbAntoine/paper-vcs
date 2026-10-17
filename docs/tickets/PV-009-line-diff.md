# PV-009: line diff in paper-diff

- **Status:** Done
- **Issue:** #21
- **Branch:** `feat/PV-009-line-diff`
- **Depends on:** none. Runs in parallel with PV-002 (paper-git/paper-core, disjoint files).
- **Touches:** `Cargo.toml` (workspace dependency), `Cargo.lock`, `crates/paper-diff/Cargo.toml`,
  `crates/paper-diff/src/**`, `crates/paper-diff/tests/**` (new), `docs/specs/04-content.md`,
  `docs/specs/03-architecture.md`
- **Specs:** `04-content.md` § File classes and § Diff (binary summary, `--lines`);
  `02-cli.md` § `paper diff`; `03-architecture.md` § Crates (`paper-diff`: pure functions, no git)

## Goal

`paper-diff` can compare two versions of a file: say whether it's text or binary, and for text
produce git-style unified hunks with counts. PV-010's `paper diff --lines` and `show` render
these; word diff (v0.3) builds on the same structure later.

## Context

- `paper-diff` is empty today: just `lib.rs` with a doc comment and no dependencies. It must stay
  free of git and paper-core: inputs are byte slices, outputs are plain data.
- Use the **`similar`** crate (line diff with grouped hunks and a choice of algorithms). It
  covers lines today and words later through custom tokens. Pick its patience or histogram-like
  algorithm (`Algorithm::Patience`), not Myers, for readable hunks. Update the
  "imara-diff or similar" mention in `03-architecture.md` to the choice made.

## Scope

1. **Classification:** `pub fn classify(bytes: &[u8]) -> Content` → `Content::Text(&str)` when
   the bytes are valid UTF-8 with no NUL byte in the first 8 KiB, else `Content::Binary`
   (04-content.md § File classes). Office formats and size limits are not this ticket.
2. **`pub fn diff_files(old: Option<&[u8]>, new: Option<&[u8]>, options: &LineDiffOptions) -> FileDiff`**:
   - `old: None` means the file was added; `new: None` means it was deleted.
   - `FileDiff::Identical`, `FileDiff::Binary { old_size: Option<u64>, new_size: Option<u64> }`
     (binary if either side is binary), or `FileDiff::Text(TextDiff)`.
   - `LineDiffOptions { context: usize }`, defaulting to 3 like git.
   - `TextDiff { hunks: Vec<Hunk>, added: usize, removed: usize }`,
     `Hunk { old_start, old_len, new_start, new_len, lines: Vec<DiffLine> }`,
     `DiffLine { kind: LineKind::{Context, Added, Removed}, text: String, missing_newline: bool }`.
     Line numbers are 1-based, as in git. An empty side has start 0 and length 0.
3. **`pub fn to_unified(diff: &TextDiff, old_label: &str, new_label: &str) -> String`**: git's
   unified format (`--- a/…`, `+++ b/…`, `@@ -l,s +l,s @@`, and
   `\ No newline at end of file` where it applies). An added file uses `/dev/null` as its old label.
4. **Specs:** `04-content.md` § Diff: text vs binary, the `--lines` output format, context 3.

## Out of scope

- Word diff, tokenisers, CJK and LaTeX (v0.3).
- Colour, terminal rendering, stats formatting (CLI).
- Reading files or trees (PV-010 passes the bytes in).

## Acceptance criteria

- [ ] `classify`: UTF-8 text is text; a NUL in the first 8 KiB, or invalid UTF-8, is binary; a NUL after 8 KiB in valid UTF-8 is still text.
- [ ] Identical inputs → `Identical`, including two absent sides and two empty files.
- [ ] Added and deleted files produce one hunk covering the whole file, with correct starts and lengths (0 for the empty side).
- [ ] A single-line edit in the middle of a long file gives one hunk with 3 lines of context on each side.
- [ ] Two edits further apart than 2 × context give two hunks; closer ones merge into one.
- [ ] `added`/`removed` counts are right.
- [ ] A missing final newline on either side is reported (`missing_newline`) and rendered as `\ No newline at end of file`.
- [ ] Either side binary → `Binary` with both sizes.
- [ ] For simple single-region edits, `to_unified` output matches `git diff --no-index -U3` hunk headers and body lines (cross-checked with the git binary on temp files).
- [ ] `paper-diff` still has no dependency on gix, paper-git or paper-core.
- [ ] Specs updated; fmt, clippy (`-D warnings`) and the full suite pass.

## Test plan

- Unit tests next to the code for classification and hunk structure.
- `crates/paper-diff/tests/unified.rs`: write two temp files, run `git diff --no-index -U3`,
  and compare from the first `@@` line onward with `to_unified` (labels differ, so skip the
  header lines). Keep these cases to single-region edits, where algorithms can't disagree.

## Summary

`paper-diff` gained `classify` (text vs binary, per 04-content), `diff_files` (git-style hunks
with patience ordering via `similar` 2.7, 3 lines of context, counts, missing-newline markers)
and `to_unified` (git's unified format). Lines end at `\n` only, so a lone `\r` stays inside its
line. Nine cross-checks compare the output with `git diff --no-index -U3`, with git's config
isolated. The only runtime dependency is `similar`.

Deviations: none in the end. The `Hunk.section` heading added during GREEN was removed in review
as noise for prose and LaTeX. Format-aware headings can return with the v0.3 tokenisers. Note for
PV-010: an added or deleted *empty* file gives `Text` with no hunks, and `to_unified` doesn't
escape control characters.

Also on this branch: the workflow now builds each worktree in its own `target/` (see NOTES.md).

## Handoff log

### RED · test-author · 2026-10-16
- Tests: crates/paper-diff/src/lib.rs `#[cfg(test)]` (17 tests: classify, identical, added/deleted, context, hunk merge/split, counts, missing newline, binary, unified format); crates/paper-diff/tests/unified.rs (7 tests cross-checking `to_unified` against `git diff --no-index -U3`, exit code 1 treated as success)
- Failing on: todo!() in `classify` and `diff_files`/`to_unified` (16 lib tests, 7 integration tests); `default_context_is_three` passes (Default is implemented in the stub)
- Stubs: Content, BINARY_SNIFF_LEN, classify, LineDiffOptions (+Default), LineKind, DiffLine, Hunk, TextDiff, FileDiff, diff_files, to_unified. `similar` not added (GREEN).
- Questions: none. The dependency-free criterion (no gix/paper-git/paper-core) has no test; check Cargo.toml in review.

### GREEN · rust-engineer · 2026-10-16
- Implemented `classify`, `diff_files`, `to_unified` in `crates/paper-diff/src/lib.rs` with `similar` 2.7 (`Algorithm::Patience`, `grouped_ops`). Added `similar` to workspace deps; paper-diff has no gix/paper-git/paper-core dependency.
- Deviation: `Hunk` gained a `section: Option<String>` field. git prints its function-context heading (nearest earlier old line starting with a letter, `_` or `$`, max 80 bytes) after `@@`, and the cross-check tests compare it. Documented in 04-content.md.
- `None` vs `Some(b"")` yields a `Text` diff with no hunks (a change, but nothing to show); only equal inputs are `Identical`.
- Specs: 03-architecture.md and 04-content.md (new "Line diff" section, patience instead of imara-diff for word diff).
- Follow-up: `similar` 3.x exists; 2.7 kept for now.

### Review · reviewer · 2026-10-16
- Verdict: CHANGES. Checks: paper-diff tests and clippy pass. The other suites only pass in an
  isolated target dir (the shared one ran PV-002's paper-git binary).
- MAJOR: `similar`'s `diff_lines` treats a lone `\r` as a line end. Line numbers drift from git's,
  `missing_newline` gets set mid-file, and `section_before` indexes the wrong line → split with
  `split_inclusive('\n')` and diff the slices. Test a lone CR, and cross-check CR and CRLF against git.
- MAJOR: `Hunk.section` is an unrequested public field that the spec now promises. In prose it's
  noise, and in LaTeX `\section` is never picked. It's also O(lines × hunks) → remove it and the
  spec sentence, and normalise git's `@@ … @@ <heading>` in the cross-checks. Format-aware headings
  can come later with the v0.3 tokenisers.
- MINOR: `None` vs `Some(b"")` → `Text` with no hunks is a sound contract, but it's untested and
  undocumented → test both directions and document it.
- MINOR: the git cross-checks inherit user config → isolate it (`GIT_CONFIG_NOSYSTEM`,
  `GIT_CONFIG_GLOBAL=/dev/null`, `HOME`).
- MINOR: the cross-checks leak temp dirs → use `tempfile` as a dev-dependency.
- MINOR: no test for valid UTF-8 in the first 8 KiB followed by an invalid byte.
- NIT: document that `to_unified` doesn't escape control characters (`\r`).
- Coordinator: authorised test changes. In tests/unified.rs, normalise git's hunk heading and
  isolate the git config; add the new tests. Don't weaken other assertions.

### GREEN (review round 1) · rust-engineer · 2026-10-16
- Lines are now split on `\n` only (`split_inclusive` + `diff_slices`); lone `\r` stays in its line. Unit test plus git cross-checks for mid-line CR and CRLF.
- Removed `Hunk.section` and the spec sentence; `tests/unified.rs` normalises git's `@@ ... @@ <heading>` to `@@ ... @@` (only change to test assertions). Added a spec note on `\n`-only line ends.
- `None` vs `Some(b"")` tested both ways and documented on `diff_files` and `FileDiff::Text`.
- Cross-checks use `tempfile` (dev-dependency) and isolate git config (`GIT_CONFIG_NOSYSTEM`, `GIT_CONFIG_GLOBAL=/dev/null`, `HOME`).
- Added test: invalid byte after 8 KiB of valid UTF-8 is `Binary`. Documented that `to_unified` does not escape control characters.
- Checks run with the isolated target dir: fmt, clippy -D warnings, full suite pass.

### Review round 2 · reviewer · 2026-10-16
- Verdict: APPROVE. Tests, clippy and fmt pass in the worktree's own target. Both majors and every minor are fixed.
- NIT, skipped: `normalise_headings` uses `+ 3 + 3` where named lengths would read better.

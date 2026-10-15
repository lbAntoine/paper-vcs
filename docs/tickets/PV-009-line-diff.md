# PV-009: line diff in paper-diff

- **Status:** Ready
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

## Handoff log

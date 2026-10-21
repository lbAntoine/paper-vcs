# PV-041: word diff with paragraph grouping and plain rendering

- **Status:** In progress
- **Issue:** #80
- **Branch:** `feat/PV-041-word-diff`
- **Depends on:** PV-040 (merged). Runs in parallel with PV-043 (`paper help`, CLI only).
- **Touches:** `crates/paper-diff/src/word.rs` (new), `crates/paper-diff/src/lib.rs` (`mod` and
  `pub use` lines only), `crates/paper-diff/tests/word.rs` (new), `docs/specs/04-content.md`
  (§ Word diff, steps 2 and 3)
- **Must not touch:** `paper-core` and `paper-cli`. Wiring the word diff into `diff`, `show` and
  `status`, colour rendering and JSON are PV-042. The tokeniser (`tokenise.rs`) and the line diff
  stay as they are; if one of them needs a change, report it first.
- **Specs:** `04-content.md` § Word diff (steps 2–3, the example output)

## Goal

`paper-diff` can compare two versions of a text as words and return what a reader needs: the
changed paragraphs, the words added and removed in each, the totals, and a plain rendering
(`[-old-]{+new+}`) that reads as prose.

## Context

- PV-040 shipped `tokenise(text, Tokeniser) -> Vec<Token>` with kinds `Word`, `Space`, `Punct`,
  `Command` and `Comment`. Its invariants: the tokens round-trip, no token is empty, no two
  `Space` tokens are adjacent, and line breaks only appear in `Space` tokens. A paragraph
  break (a blank line, counted in `\n` as in the line diff) is therefore always inside a single
  `Space` token.
- `similar` 2.7 is already a dependency (the line diff uses `Algorithm::Patience`).
- `04-content.md` shows the target output:
  ```
  sections/intro.tex  +12 words −4 words
    ¶ 3
    Recent work has [-mostly-]{+largely+} focused on supervised settings
    {+, although self-supervised approaches are gaining traction+}.
  ```

## Decisions (coordinator, 2026-10-21)

- **API**, in a new module `word`:
  ```rust
  pub enum Segment { Same(String), Removed(String), Added(String) }
  pub struct Paragraph { pub number: usize, pub segments: Vec<Segment> }
  pub struct WordDiff {
      pub paragraphs: Vec<Paragraph>, // only paragraphs with a change, in file order
      pub added: usize,               // words added (Word tokens; a CJK grapheme counts as one)
      pub removed: usize,             // words removed
      pub layout_only: bool,          // the texts differ only in spacing or line breaks
  }
  pub fn diff_words(old: &str, new: &str, tokeniser: Tokeniser) -> WordDiff
  pub fn to_word_markup(diff: &WordDiff) -> String   // the plain rendering
  ```
  Names may follow the crate's style. If they change, say so in the handoff.
- **Comparison.** Run a patience diff over the token sequences. Tokens compare equal when they
  are the same kind and the same text, with one exception: two `Space` tokens compare equal
  when neither contains a paragraph break, or when both do. Re-wrapping a paragraph, changing
  indentation or adding a double space is then not a change, which matters for hard-wrapped
  LaTeX. A `Same` segment uses the new side's text. If every difference is of this kind,
  `paragraphs` is empty and `layout_only` is true.
- **Counting.** `added` and `removed` count `Word` tokens only. Punctuation, spaces, commands
  and comments change the text but not the counts. A LaTeX command that changes is still
  shown, as `[-\emph-]{+\textbf+}`.
- **Paragraphs.** A paragraph is a run of tokens between paragraph breaks.
  - `number` is 1-based, counted on the new side. A paragraph that exists only on the old side
    (deleted whole) takes the number of the position where it was, and that position is
    documented.
  - Only paragraphs that contain a change are returned.
  - Inside a returned paragraph, an unchanged run longer than 40 words is cut down to its first
    10 words, ` … `, and its last 10 words. The cut never splits a token. The paragraph's start
    and end are kept so the reader can find it.
  - A change that adds or removes a paragraph break shows the break as an added or removed
    segment. In the markup it is `{+¶+}` or `[-¶-]`.
- **Plain markup.**
  - Removed text is `[-…-]` and added text is `{+…+}`. A removed segment comes before the added
    segment it replaces.
  - Paragraphs are separated by `¶ N` header lines, as in the spec example.
  - Literal `[-`, `-]`, `{+` and `+}` in the text are left as they are (git's `--word-diff` does
    the same). Document this.
  - The `+N words −M words` header line belongs to the CLI (PV-042). `to_word_markup` renders
    paragraphs only.
- **Purity.** No I/O and no printing. Pure functions over `&str`, like the rest of `paper-diff`.
- **Performance.** Linear in practice for prose. A 1 MB file with scattered edits diffs in well
  under a second in a release build. Measure it and put the numbers in the handoff.

## Acceptance criteria

- [ ] The spec's example (a word swap plus an added clause) gives exactly the segments and
  markup shown, with `+N` and `−M` counted as decided.
- [ ] A re-wrapped paragraph (line breaks and spacing moved, no words changed) is
  `layout_only` with no paragraphs. A real edit inside it shows only the changed words.
- [ ] An added, a deleted and a split paragraph, and a joined pair of paragraphs, get the right
  numbers, segments and `¶` markers.
- [ ] Korean or Japanese edits show only the changed graphemes. A changed LaTeX command shows as
  one removed and one added command. A changed `%` comment shows as a comment change.
- [ ] A long unchanged run inside a changed paragraph is cut as decided, never inside a token.
- [ ] Round-trip property (proptest): concatenating the `Same` and `Added` text of a diff gives
  the new text with paragraphs restored, and `Same` plus `Removed` gives the old text, for
  inputs with no cuts. Test against an option that turns cutting off, or a helper that keeps
  everything.
- [ ] No I/O and no printing. Public items document `# Panics`.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS and Windows).

## Test plan

- `crates/paper-diff/tests/word.rs`:
  - a table of small before and after pairs with their expected segments and markup;
  - the spec example;
  - CJK and LaTeX cases;
  - the round-trip proptest.
- Use an internal flag or a test helper for the "no cuts" round trip, so the property can be
  checked on long inputs.
- No hand-written snapshots of unwritten output. Expected markup strings in tables are fine,
  because they are the specification.
- The tests must fail on `origin/main`: the module doesn't exist there.

## Summary

## Handoff log

### RED · test-author · 2026-10-21

- **Files:** `crates/paper-diff/tests/word.rs` (43 tests: 33 behaviour tests that fail on `todo!()`, 9 helper/option/constant sanity tests that pass, 1 `#[ignore]` timing test). Stubs in `crates/paper-diff/src/word.rs`; `mod word` and `pub use` in `lib.rs`. Pre-existing tests all pass; fmt and clippy are clean.
- **API added beyond the ticket** (names may change in GREEN, tell the coordinator): `WordDiffOptions { cut_long_runs, changed_only }` with `Default` (true, true) and `WordDiffOptions::keep_everything()` (false, false); `diff_words_with(old, new, tokeniser, &options)`; consts `CUT_THRESHOLD_WORDS = 40`, `CUT_KEEP_WORDS = 10`. `diff_words` = `diff_words_with` with the defaults. `keep_everything` also returns unchanged paragraphs, which the round trip needs.
- **Decisions the tests assert (GREEN to confirm or raise):**
  1. A paragraph is a run of tokens between *new-side* breaks. An unchanged break (Same, including layout-equal ones) separates `Paragraph` entries and is in no segment. An **Added** break is the last segment of the paragraph it ends (`Added("\n\n")`, markup `{+¶+}`). A **Removed** break stays inside the current paragraph (joined case), except when the paragraph so far holds only removed text: then it is an old-only paragraph, closed by the removed break, numbered with the number the next new-side paragraph has.
  2. Break segments (`"\n\n"` exactly as in the text) are never merged with neighbouring segments. Other adjacent same-kind segments are merged (`Removed("two three")`).
  3. A split paragraph is one entry for its first part only (`[Same, Removed(" "), Added(break)]`); the unchanged second part is not returned. A joined pair is one entry: `[Same, Removed(break), Added(" "), Same]`.
  4. A deleted last paragraph shows inside the paragraph before it (number 1 in `A one¶B two` -> `A one`), deleted middle/first paragraphs are standalone old-only entries.
  5. Markup: each paragraph is `"¶ N\n" + body + "\n"`, paragraphs concatenated, empty diff is `""`. Segment text is rendered as is (`[-t-]`, `{+t+}`, break segments as `¶`).
  6. Cut: first 10 words (ending at the end of the 10th `Word` token), `" … "`, last 10 words (from the start of the 10th-last `Word` token to the end of the run, trailing/leading space of the run kept). Exactly 40 words is not cut, 41 is. Applies to every unchanged run in a returned paragraph, at the paragraph edges too.
  7. `layout_only` is false for identical texts. Space tokens: break means 2 or more `\n` (so `a \n \n b` equals `a\n\nb`, lone `\r` is not a break).
- **Alignment-sensitive expectations** (similar's patience output decides; tests assume common prefix/suffix are matched first so insertions/deletions slide after an unchanged break): added paragraph in the middle, deleted middle paragraph, deleted first paragraph, paragraph added first. If similar aligns differently, GREEN should post-process (slide) rather than change the tests, or raise it. Texts avoid `.` next to edits for the same reason, except the spec example where `.` is unique on each side. Mid-sentence word insertion/deletion only asserts the structure (one Added/Removed run whose trimmed text is the word), because the space can go on either side.
- **Spec example:** the spec shows the added clause on a second line; that is display wrapping, the markup in the test is one line. Counts asserted: `+8 words -1 word` (`self-supervised` is `self`, `-`, `supervised`).
- **Not tested (open):** adding or removing only whitespace tokens (`"a"` -> `"a\n"`, leading space): is that `layout_only` with no paragraphs, or an Added space segment? Trailing newline at the end of a paragraph (kept in the last Same segment?) and its markup. Both are left to GREEN; tell the coordinator the choice.
- **Can only be judged with GREEN:** all alignment-sensitive cases above, the exact segment merging, the proptest round trip (uses a `rebuild` helper that re-inserts `"\n\n"` between paragraphs unless the paragraph ends in an Added/Removed break; its logic is sanity-tested), counts property, and the performance test (`cargo test --release -p paper-diff -- --ignored`, record numbers in the GREEN handoff).

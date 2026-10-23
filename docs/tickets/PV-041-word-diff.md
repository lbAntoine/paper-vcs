# PV-041: word diff with paragraph grouping and plain rendering

- **Status:** Done
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
- **Lone spaces (review round 1).** A space added or removed on its own counts as layout only
  when both neighbours are words of a space-separated script; next to CJK or punctuation it is
  shown. Leading and trailing white space of the text is layout.
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

- [x] The spec's example (a word swap plus an added clause) gives exactly the segments and
  markup shown, with `+N` and `−M` counted as decided.
- [x] A re-wrapped paragraph (line breaks and spacing moved, no words changed) is
  `layout_only` with no paragraphs. A real edit inside it shows only the changed words.
- [x] An added, a deleted and a split paragraph, and a joined pair of paragraphs, get the right
  numbers, segments and `¶` markers.
- [x] Korean or Japanese edits show only the changed graphemes. A changed LaTeX command shows as
  one removed and one added command. A changed `%` comment shows as a comment change.
- [x] A long unchanged run inside a changed paragraph is cut as decided, never inside a token.
- [x] Round-trip property (proptest): concatenating the `Same` and `Added` text of a diff gives
  the new text with paragraphs restored, and `Same` plus `Removed` gives the old text, for
  inputs with no cuts. Test against an option that turns cutting off, or a helper that keeps
  everything.
- [x] No I/O and no printing. Public items document `# Panics`.
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

`paper-diff` has a word diff (`diff_words`, `diff_words_with`, `to_word_markup`). The token
streams are compared as a whole with a patience diff (tokens found on one side only are taken
out first, which keeps prose fast), plain spacing is not a change, pure insertions and
deletions are slid to paragraph boundaries, and the result is grouped by paragraph with long
unchanged runs cut (`Segment::Cut`). Plain markup is `[-old-]{+new+}` under `¶ N` headers.
About 0.1 s for 1 MB with scattered edits (release build).

### Known gaps

- Each call stops aligning after about 2 seconds. A very large text with a very large edit
  (about 1 MB with a fifth of its words changed) can reach that limit, and the diff then shows
  most of the text as rewritten. The result can differ on a slower machine.
- A sentence moved inside one paragraph is not slid to a sentence boundary; only paragraph
  breaks guide where a change sits.

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

### GREEN · rust-engineer · 2026-10-21

- **Implemented** `crates/paper-diff/src/word.rs`: tokens get a key (same kind and text, with all plain `Space` tokens sharing one key and all break `Space` tokens another); common prefix and suffix are stripped, the middle goes through `similar` patience; changes are normalised (removed before added; a change of plain spaces only is layout: added kept as unchanged text, removed dropped); items are grouped into paragraphs by the RED rules; segments merge (break segments stand alone); cut applies per unchanged run. No sliding post-pass was needed: all alignment-sensitive RED tests passed as written.
- **Coordinator decisions applied.** Trailing ASCII white space of each text is split off before tokenising, so `"a"` -> `"a\n"` and `"a"` -> `"a\n\n"` are `layout_only` with no paragraphs; the new side's trailing text is appended to the last unchanged run (kept in the data). `to_word_markup` trims trailing `\r`/`\n` of a paragraph's last segment. Two table tests added for these (`white_space_added_or_removed_at_the_very_end_is_layout_only`, `a_trailing_newline_stays_in_the_data_and_is_trimmed_from_the_markup`); no existing assertion changed.
- **API:** as in RED; no renames. `layout_only` is `old != new` and nothing but layout changed.
- **Spec:** 04-content § Word diff steps 2-3 updated (comparison rule, layout-only, counts, paragraphs, cut, markup).
- **Spec example markup:** `¶ 3\nRecent work has [-mostly-]{+largely+} focused on supervised settings{+, although self-supervised approaches are gaining traction+}.\n`
- **Timing** (`cargo test --release -p paper-diff -- --ignored`, ~1 MB, scattered edit in each of ~1000 paragraphs, repeated words so patience has no unique anchors): the test passes, whole test (input build + diff) 0.08 s.
- **Follow-ups:** a removed paragraph break can land before the removed text instead of after it when repeated words let the alignment shift (then it shows inside the previous paragraph, like a deleted last paragraph); a slide pass could normalise this if users notice.

### GREEN (review round 1) · rust-engineer · 2026-10-21

- **1 Slide.** `slide` moves every pure insertion or deletion across equal-keyed tokens to the place where it ends with a break, else starts right after one, else stays. Tests added for the "We..." delete, the "The new para" add and a deleted first paragraph starting with the same word.
- **2 Time.** Two levels: paragraphs are lined up by patience over paragraph keys (interned key slices), then words are diffed inside 1:1 pairs of changed paragraphs; groups of different sizes are word-diffed together when old*new tokens <= 4M, else shown whole. Inside `diff_keys`, tokens whose key is on one side only are taken out before the diff (an unrelated rewrite is then cheap). A shared 2 s deadline goes to `capture_diff_slices_deadline`. Release timings (`cargo test --release -p paper-diff -- --ignored`): 1 MB with scattered edits, passes (whole test 0.08 s); 1 MB with 20% of words replaced 58 ms; a 3000-word unrelated paragraph rewrite in a 1 MB file 61 ms. No test reaches the deadline. Documented in 04-content.
- **3 Bare break.** Break rendering is for Added and Removed segments only (`Segment::is_break`).
- **4 Lone spaces.** Deviation from the wording of the request: "layout only when at least one neighbour is Latin" would make `a , b` -> `a, b` layout (the `a` is Latin), contradicting the examples. Implemented as layout only when BOTH neighbours are words of a space-separated script (Latin, Cyrillic, ...); both examples are shown. Tests and spec updated.
- **5 API.** New `Segment::Cut` (unit variant, rendered ` … `) and `Segment::is_break()`. RED tests changed only as far as `Cut` requires (all in `word_cut.rs`): `an_unchanged_run_of_forty_one_words_is_cut_to_ten_and_ten`, `a_long_unchanged_run_keeps_its_first_and_last_ten_words` (data only; markup string unchanged), `runs_on_both_sides_of_a_change_are_cut_separately`, `a_cut_never_splits_a_token`. `rebuild` takes the break string and ignores `Cut`.
- **6 Proptest.** Round trip now uses atoms `ab`, `cd`, `한`, `国`, `\emph`, `%c`, `😀`, `,`, a plain space from `" "`, `"\r"`, NBSP and a break from `"\n\n"`, `"\r\n\r\n"`, `"\n\n\n"` per case. The counts property keeps its narrow word alphabet (it counts with `split_whitespace`).
- **7 Whitespace, files.** Leading and trailing layout split with `char::is_whitespace`, symmetric. Tests split: `word.rs`, `word_cut.rs`, `word_props.rs`, `word_perf.rs`, shared helpers in `tests/common/mod.rs`. Added tests for leading blank lines and the bare-break case.
- **Not covered:** a sentence moved inside one paragraph is only helped where the hunk touches a break; sentence-level sliding is not implemented.

### GREEN (review round 2) · rust-engineer · 2026-10-21

- **Paragraph-first layer removed.** `GROUP_LIMIT`, paragraph keys, positional pairing and `token_span` are gone. Words are diffed over the whole token streams again: common prefix and suffix, then removal of tokens whose key is on one side only, then patience with a deadline, then the slide pass. The deadline is computed once per `diff_words` call (`Instant::now() + 2 s` in `align`). Kept: bare break, lone-space rule, `Cut`, `is_break`, symmetric whitespace.
- **Structure.** `crates/paper-diff/src/word/align.rs` holds `Marks`, `runs`, `diff_keys`, `align`, `slide`; `word.rs` is 472 lines.
- **Spec:** 04-content § Word diff, Alignment bullet rewritten (whole-text diff, one-sided removal, per-call deadline, when it can come out coarse).
- **Tests added:** `edits_in_every_paragraph_plus_one_inserted_paragraph_are_counted_exactly` (6x200 gives +36 -6, 12x100 gives +42 -12); `swapping_the_last_two_paragraphs_reads_as_one_moved_block` (+5 -5, markup asserted); `swapping_two_middle_paragraphs_that_are_both_edited_reads_as_moved_blocks` (+6 -6, markup asserted). Round-trip proptest now draws the plain space for each gap and the break for each paragraph, and compares both rebuilt sides to the inputs after normalising white space (`normalise` in `tests/common`, by whitespace runs rather than by retokenising, so a LaTeX comment cannot change extent). No other assertion changed.
- **Timings** (`cargo test --release -p paper-diff -- --ignored --nocapture`): 1 MB, scattered edit in every paragraph, repeated words: whole test 0.08 s; 1 MB with 20% of words replaced by fresh words: 77-89 ms; one unrelated 3000-word paragraph rewrite: 63-68 ms; 1 MB Zipf text (2000-word vocabulary, 120-word paragraphs, every 100th word replaced): 96 ms; same with a 300-word vocabulary: 81 ms (asserted under 1 s).
- **Recorded, not asserted (hit the 2 s deadline, so the result is coarse):** 1 MB single paragraph of a 50-word vocabulary with every 5th word replaced: 2.05 s (+293246 -293246, i.e. fully replaced); 1 MB Zipf text (2000 words) with a fifth of the words replaced: 2.05 s (+190895 -190895). Both documented in 04-content.

### GREEN (post-approval) · rust-engineer · 2026-10-23

- Deleted the stray empty `crates/paper-diff/src/ord.rs`.
- Deadline wording fixed in `word.rs`, 04-content and `word/align.rs`: the trigger is edit size (about 1 MB with a fifth of the words changed), not vocabulary.
- Round-trip proptest is stricter: the new side is compared after normalising only paragraph-break runs (`normalise_breaks`), so plain spacing must come back exactly; the old side keeps full normalisation. Comment corrected.
- `keys` and `is_break_token` are private again; doc comment re-wrapped.
- Ticket: Summary and Known gaps written; acceptance criteria ticked except the CI one.

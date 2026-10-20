# PV-040: word tokenisers (generic, CJK, LaTeX)

- **Status:** In progress
- **Issue:** #79
- **Branch:** `feat/PV-040-word-tokenisers`
- **Depends on:** none. Runs in parallel with PV-035.
- **Touches:**
  - `crates/paper-diff/src/tokenise.rs` (new)
  - `crates/paper-diff/src/lib.rs` (the `mod`/`pub use` lines only)
  - `crates/paper-diff/Cargo.toml` (`unicode-segmentation`; `proptest` dev-dependency)
  - root `Cargo.toml` (workspace dependency `unicode-segmentation = "1.11"`)
  - `Cargo.lock`
  - `crates/paper-diff/tests/tokenise.rs` (new)
  - `docs/specs/04-content.md` (§ Word diff, step 1, only if the implementation needs a precision)
- **Must not touch:** anything in `paper-core` or `paper-cli`, and the existing line diff in `lib.rs`. The word diff itself is PV-041.
- **Specs:** `04-content.md` § Word diff (step 1); ADR on `paper-diff` being pure functions (03-architecture § Crates)

## Goal

`paper-diff` can split any text into tokens that a word diff can compare. A Korean or Japanese
edit shows only the changed characters, and a LaTeX command is never split.

## Context

- `paper-diff` is pure functions over bytes or strings: no git, no file access, no printing.
  `classify` already decides text versus binary.
- `unicode-segmentation` 1.11.0 is in the local registry:
  - `UnicodeSegmentation::split_word_bounds` gives UAX #29 word boundaries;
  - `graphemes(true)` gives extended grapheme clusters.

  Check the exact API in the registry source before using it.
- `proptest` 1.12 is already a workspace dependency (PV-026).

## Decisions (coordinator, 2026-10-20)

### API

A new module `tokenise`:

```rust
pub enum Tokeniser { Generic, Latex }
pub fn tokeniser_for(path: &str) -> Tokeniser   // by extension: .tex .bib .sty .cls → Latex, else Generic
pub enum TokenKind { Word, Space, Punct, Command, Comment }
pub struct Token<'a> { pub text: &'a str, pub kind: TokenKind }
pub fn tokenise(text: &str, tokeniser: Tokeniser) -> Vec<Token<'_>>
```

Exact names may follow the crate's style. Say so in the handoff if they change.

### The invariant

Concatenating the `text` of every token gives back the input, byte for byte, for every input.
Prove it with a proptest over arbitrary strings, including CJK, combining marks, emoji, CRLF
and LaTeX-like text.

### Generic tokeniser

- Built on UAX #29 word bounds.
- A run of whitespace, line breaks included, is one `Space` token.
- Every punctuation mark is its own `Punct` token.
- Words and numbers are `Word`.

### CJK

Inside a word-bound segment, characters in Han, Hiragana, Katakana and Hangul are split per
extended grapheme. Each grapheme becomes its own `Word`.

- So `안녕하세요` becomes five tokens.
- Mixed text (`v0.3에서`) splits at the script change.

Use Unicode script property ranges via `char` code point ranges, with no new crate. Document
the ranges.

### LaTeX tokeniser

The generic rules apply, plus:

- `\command` (a backslash followed by ASCII letters) and `\command*` are single `Command`
  tokens.
- A backslash followed by one non-letter (`\%`, `\\`, `\{`) is one `Command`.
- `%` up to the end of the line, without the line break, is one `Comment` token.
- An escaped `\%` is not a comment.

### Not in scope

Markdown and Typst use Generic in v0.3. The diff, counting and rendering are PV-041.

## Acceptance criteria

- [x] `tokenise` round-trips every input byte for byte (proptest, default case count, plus a fixed set of tricky strings).
- [x] Generic: words, numbers, punctuation and whitespace runs are separate tokens with the right kinds; contractions and apostrophes follow UAX #29.
- [x] CJK: Korean, Japanese (kana and kanji) and Chinese are split per grapheme; mixed Latin and CJK splits at the boundary; Hangul syllables made of combining jamo stay one grapheme.
- [x] LaTeX: `\section*{Intro}`, `\\`, `\%`, `% comment`, `50\% done` and a comment at the end of the file are tokenised as decided.
- [x] `tokeniser_for` picks by extension, case-insensitively (`.TEX`).
- [x] No printing or I/O. Public items document `# Panics` (they never panic).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-diff/tests/tokenise.rs`:
  - table-driven cases per tokeniser, asserting `(text, kind)` sequences;
  - the round-trip proptest;
  - CJK cases written with `\u{…}` escapes where combining characters make the source ambiguous.
- No snapshots by hand.
- Tests must fail on `origin/main`, since the module doesn't exist there.

## Summary

Added `paper-diff`'s `tokenise` module: `tokenise`, `tokeniser_for`, `Tokeniser`, `TokenKind`
and `Token`. Generic text uses UAX #29 word bounds with whitespace runs merged into one
`Space`; Han, kana and Hangul are split per grapheme; the LaTeX tokeniser adds `\command`,
`\command*`, one-character escapes and `%` comments. Tokens always concatenate back to the
input, and line breaks only appear in `Space` tokens. `unicode-segmentation` is pinned to 1.11.0
in the lock file. Known gap: `@` in `.sty`/`.cls` command names.

## Handoff log

### RED · test-author · 2026-10-20

- Tests: `crates/paper-diff/tests/tokenise.rs`, 31 tests (2 helper sanity, 7 generic, 9 CJK, 7 LaTeX, 3 `tokeniser_for`, 1 fixed tricky-string round trip, 2 proptests: arbitrary `String` and a LaTeX/CJK/emoji/combining alphabet). All 31 fail on the `todo!()` stubs; every pre-existing test passes (`--no-fail-fast`). fmt and clippy `-D warnings` are clean.
- Stubs: `crates/paper-diff/src/tokenise.rs` with `Tokeniser {Generic, Latex}`, `TokenKind {Word, Space, Punct, Command, Comment}`, `Token<'a> {text, kind}`, `tokeniser_for`, `tokenise` (bodies `todo!()`). `lib.rs` gets `mod tokenise;` and a `pub use` line. API names are exactly as in the ticket.
- Dependencies: `unicode-segmentation = "1.11"` (workspace and paper-diff), `proptest` dev-dependency. Cargo resolved 1.13.3 (not the local 1.11.0); a scratch run gave identical results for the cases below on both versions.
- Choices taken from the scratch run of `split_word_bounds`:
  - `don't` and `can\u{2019}t` are one segment (Word); `'` around `b` are separate Punct.
  - `3.14`, `1,000`, `v0.3` are one Word; `50%` is `50` + `%`; `$5` is `$` + `5`.
  - `e-mail` splits at `-`; `snake_case` stays one Word (`_` is ExtendNumLet).
  - Whitespace: UAX #29 returns `" "`, `"\t"`, `"\r\n"`, `" "` separately for `a \t\r\n b`, so the implementation must merge adjacent whitespace segments into one `Space`.
  - `日本語のテスト`: UAX #29 keeps the katakana run `テスト` whole, so per-grapheme splitting must be applied to it as well (7 tokens expected).
  - `v0.3에서` is one UAX #29 segment; expected `v0.3`, `에`, `서`.
- Not tested (ticket silent, left to GREEN to decide, covered only by round trip): a lone trailing `\`, `\` + multibyte non-letter, `\section**`, CRLF at the end of a comment line, and `\` followed by a space.
- Extra assertion not in the ticket: empty input gives no tokens.
- Can only be confirmed GREEN: the actual kinds and splits, the 1.13 vs 1.11 segmentation equivalence on the full tables, and the proptest round trip (default 256 cases; persistence disabled in the test config to avoid the missing `lib.rs` warning).

### GREEN · rust-engineer · 2026-10-20

- Implemented `crates/paper-diff/src/tokenise.rs` (215 lines): `tokeniser_for` (extension, case-insensitive, last path component only), `tokenise`. Generic = UAX #29 `split_word_bound_indices`, adjacent whitespace segments merged into one `Space`, segments with an alphanumeric char are `Word` (split per grapheme at CJK graphemes, non-CJK runs kept whole), everything else `Punct`. LaTeX scans for `\` and `%`, running Generic on the text between.
- API names unchanged from the ticket. No spec or ADR change needed.
- Dependency: lock pinned to unicode-segmentation 1.11.0 (`cargo update -p unicode-segmentation --precise 1.11.0`); only that package's lock entry changed. `"1.11"` kept in Cargo.toml.
- Coordinator decisions applied: `\` + space is one `Command`; a lone trailing `\` is `Punct`; `\section**` is `\section*` + `*`; a `%` comment stops before `\n` or `\r\n`. Coordinator-requested addition: test `latex_open_cases_follow_the_coordinator_decisions` in `tests/tokenise.rs` (a table for these four).
- CJK ranges are code point ranges documented on `is_cjk` (Hangul, Hiragana, Katakana incl. half-width, Han incl. Ext A and Ext B+ in the supplementary planes, compatibility ideographs).
- Behaviour note: `\` followed by a multibyte char is one `Command` of both; `\` + `\r\n` makes `\` + `\r` a Command and `\n` the Space.
- Checks: fmt, clippy `-D warnings`, full `cargo test --workspace --all-targets --no-fail-fast` all pass.

### GREEN (review round 1) · rust-engineer · 2026-10-20

- `command_len`: a line break (`\n`, `\r\n`, lone `\r`) after `\` is treated like end of input: the backslash is a one-byte `Punct` and the break joins the next `Space`. Line breaks now only appear in `Space` tokens. Escapes take a whole grapheme (`\` + flag, ZWJ sequence); a command's letters extend to the end of the grapheme holding the last letter (`\e` + U+0301). Table cases added to `latex_open_cases_follow_the_coordinator_decisions` (coordinator-sanctioned), including the three requested break cases.
- Deleted the dead `tokenise.proptest-regressions`. Both proptests now also assert no empty token, no adjacent `Space` tokens, and no non-`Space` token with `\n` or `\r`.
- Katakana starts at U+30A1; Han supplementary range is `U+20000..=U+3FFFD`; doc comments for `is_cjk` and `TokenKind::Punct` fixed.
- A `%` comment now also ends before a lone `\r` (found by the new proptest assertion), so line breaks never sit inside a Comment.

## Known gaps

- `@` is not treated as a letter in `.sty`/`.cls` files (`\my@cmd` splits at `@`). Follow-up.

### GREEN (post-approval) · rust-engineer · 2026-10-20

- Spec `04-content.md` step 1: added the comment/line-break/paragraph note. Added the table case `% c\rx` to the LaTeX open cases. Ticket Summary filled in; acceptance criteria ticked except the CI one.

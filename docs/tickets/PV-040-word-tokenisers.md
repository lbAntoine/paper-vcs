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

- [ ] `tokenise` round-trips every input byte for byte (proptest, default case count, plus a fixed set of tricky strings).
- [ ] Generic: words, numbers, punctuation and whitespace runs are separate tokens with the right kinds; contractions and apostrophes follow UAX #29.
- [ ] CJK: Korean, Japanese (kana and kanji) and Chinese are split per grapheme; mixed Latin and CJK splits at the boundary; Hangul syllables made of combining jamo stay one grapheme.
- [ ] LaTeX: `\section*{Intro}`, `\\`, `\%`, `% comment`, `50\% done` and a comment at the end of the file are tokenised as decided.
- [ ] `tokeniser_for` picks by extension, case-insensitively (`.TEX`).
- [ ] No printing or I/O. Public items document `# Panics` (they never panic).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-diff/tests/tokenise.rs`:
  - table-driven cases per tokeniser, asserting `(text, kind)` sequences;
  - the round-trip proptest;
  - CJK cases written with `\u{…}` escapes where combining characters make the source ambiguous.
- No snapshots by hand.
- Tests must fail on `origin/main`, since the module doesn't exist there.

## Summary

## Handoff log

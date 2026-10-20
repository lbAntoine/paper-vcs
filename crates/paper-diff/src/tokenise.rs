//! Word tokenisers for the word diff (04-content.md, Word diff, step 1).
//!
//! Pure functions over strings. Concatenating the `text` of every token gives
//! back the input byte for byte.

/// Which set of tokenising rules to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tokeniser {
    /// UAX #29 words, with CJK split per grapheme.
    Generic,
    /// Generic rules plus `\commands` and `%` comments.
    Latex,
}

/// What a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A word or a number. Each CJK grapheme is its own word.
    Word,
    /// A run of whitespace, line breaks included.
    Space,
    /// One punctuation mark or symbol.
    Punct,
    /// A LaTeX command such as `\section*` or `\%` (LaTeX only).
    Command,
    /// A LaTeX comment, from `%` to the end of the line (LaTeX only).
    Comment,
}

/// A slice of the input and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    /// The text of the token, borrowed from the input.
    pub text: &'a str,
    /// The kind of the token.
    pub kind: TokenKind,
}

/// Pick a tokeniser from a path's extension, case-insensitively:
/// `.tex`, `.bib`, `.sty` and `.cls` are LaTeX, everything else is generic.
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn tokeniser_for(_path: &str) -> Tokeniser {
    todo!("PV-040 GREEN")
}

/// Split `text` into tokens. The tokens' texts concatenate to `text`.
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn tokenise(_text: &str, _tokeniser: Tokeniser) -> Vec<Token<'_>> {
    todo!("PV-040 GREEN")
}

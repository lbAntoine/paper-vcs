//! Word tokenisers for the word diff (04-content.md, Word diff, step 1).
//!
//! Pure functions over strings. Concatenating the `text` of every token gives
//! back the input byte for byte.

use unicode_segmentation::UnicodeSegmentation;

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
pub fn tokeniser_for(path: &str) -> Tokeniser {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let Some((_, ext)) = name.rsplit_once('.') else {
        return Tokeniser::Generic;
    };
    if ["tex", "bib", "sty", "cls"]
        .iter()
        .any(|latex| ext.eq_ignore_ascii_case(latex))
    {
        Tokeniser::Latex
    } else {
        Tokeniser::Generic
    }
}

/// Split `text` into tokens. The tokens' texts concatenate to `text`.
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn tokenise(text: &str, tokeniser: Tokeniser) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    match tokeniser {
        Tokeniser::Generic => generic_into(text, &mut out),
        Tokeniser::Latex => latex_into(text, &mut out),
    }
    out
}

/// True for characters of the scripts that are split per grapheme.
///
/// The ranges are code points, with no script tables:
///
/// - Hangul: Jamo `U+1100..=U+11FF`, Compatibility Jamo `U+3130..=U+318F`,
///   Jamo Extended-A `U+A960..=U+A97F`, Syllables `U+AC00..=U+D7AF`, Jamo
///   Extended-B `U+D7B0..=U+D7FF`, half-width Hangul `U+FFA0..=U+FFDC`.
/// - Hiragana `U+3040..=U+309F`.
/// - Katakana `U+30A0..=U+30FF`, Phonetic Extensions `U+31F0..=U+31FF`,
///   half-width Katakana `U+FF66..=U+FF9F`.
/// - CJK Unified Ideographs `U+4E00..=U+9FFF`, Extension A `U+3400..=U+4DBF`,
///   Extensions B and later `U+20000..=U+323AF`.
/// - CJK Compatibility Ideographs `U+F900..=U+FAFF`.
fn is_cjk(c: char) -> bool {
    matches!(u32::from(c),
        0x1100..=0x11FF
        | 0x3130..=0x318F
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7FF
        | 0xFFA0..=0xFFDC
        | 0x3040..=0x30FF
        | 0x31F0..=0x31FF
        | 0xFF66..=0xFF9F
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0x20000..=0x323AF
        | 0xF900..=0xFAFF)
}

fn generic_into<'a>(text: &'a str, out: &mut Vec<Token<'a>>) {
    let mut space_start: Option<usize> = None;
    let mut space_end = 0;
    for (start, segment) in text.split_word_bound_indices() {
        if segment.chars().all(char::is_whitespace) {
            space_start.get_or_insert(start);
            space_end = start + segment.len();
            continue;
        }
        if let Some(from) = space_start.take() {
            out.push(Token {
                text: &text[from..space_end],
                kind: TokenKind::Space,
            });
        }
        if segment.chars().any(char::is_alphanumeric) {
            words_into(segment, out);
        } else {
            out.push(Token {
                text: segment,
                kind: TokenKind::Punct,
            });
        }
    }
    if let Some(from) = space_start {
        out.push(Token {
            text: &text[from..space_end],
            kind: TokenKind::Space,
        });
    }
}

/// Push a word segment, one token per CJK grapheme and one per non-CJK run.
fn words_into<'a>(segment: &'a str, out: &mut Vec<Token<'a>>) {
    let mut run_start: Option<usize> = None;
    for (start, grapheme) in segment.grapheme_indices(true) {
        if grapheme.chars().next().is_some_and(is_cjk) {
            if let Some(from) = run_start.take() {
                out.push(Token {
                    text: &segment[from..start],
                    kind: TokenKind::Word,
                });
            }
            out.push(Token {
                text: grapheme,
                kind: TokenKind::Word,
            });
        } else {
            run_start.get_or_insert(start);
        }
    }
    if let Some(from) = run_start {
        out.push(Token {
            text: &segment[from..],
            kind: TokenKind::Word,
        });
    }
}

fn latex_into<'a>(text: &'a str, out: &mut Vec<Token<'a>>) {
    let mut rest = text;
    while !rest.is_empty() {
        let Some(at) = rest.find(['\\', '%']) else {
            generic_into(rest, out);
            return;
        };
        generic_into(&rest[..at], out);
        let special = &rest[at..];
        let len = if special.starts_with('%') {
            let end = comment_len(special);
            out.push(Token {
                text: &special[..end],
                kind: TokenKind::Comment,
            });
            end
        } else {
            let (len, kind) = command_len(special);
            out.push(Token {
                text: &special[..len],
                kind,
            });
            len
        };
        rest = &special[len..];
    }
}

/// Length of a comment starting at `%`, up to (not including) `\n` or `\r\n`.
fn comment_len(text: &str) -> usize {
    match text.find('\n') {
        Some(n) if text[..n].ends_with('\r') => n - 1,
        Some(n) => n,
        None => text.len(),
    }
}

/// Length and kind of the token starting at a backslash.
fn command_len(text: &str) -> (usize, TokenKind) {
    let after = &text[1..];
    let letters = after.bytes().take_while(u8::is_ascii_alphabetic).count();
    if letters > 0 {
        let star = usize::from(after.as_bytes().get(letters) == Some(&b'*'));
        return (1 + letters + star, TokenKind::Command);
    }
    match after.chars().next() {
        Some(c) => (1 + c.len_utf8(), TokenKind::Command),
        None => (1, TokenKind::Punct),
    }
}

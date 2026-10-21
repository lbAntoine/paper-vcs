//! Word diff with paragraph grouping and a plain rendering
//! (04-content.md, Word diff, steps 2 and 3).
//!
//! Pure functions over strings: no I/O, no printing.

use crate::Tokeniser;

/// An unchanged run with more than this many words is cut.
pub const CUT_THRESHOLD_WORDS: usize = 40;

/// A cut run keeps this many words at each end.
pub const CUT_KEEP_WORDS: usize = 10;

/// One piece of a paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Text on both sides (the new side's spelling of it). A cut run holds
    /// its first words, ` … `, and its last words.
    Same(String),
    /// Text only on the old side.
    Removed(String),
    /// Text only on the new side.
    Added(String),
}

/// A paragraph that contains a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    /// 1-based position on the new side.
    pub number: usize,
    /// The pieces, in reading order.
    pub segments: Vec<Segment>,
}

/// The word-level comparison of two versions of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordDiff {
    /// Paragraphs with a change, in file order.
    pub paragraphs: Vec<Paragraph>,
    /// Words added (`Word` tokens only).
    pub added: usize,
    /// Words removed (`Word` tokens only).
    pub removed: usize,
    /// The texts differ only in spacing or line breaks.
    pub layout_only: bool,
}

/// Options for [`diff_words_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordDiffOptions {
    /// Cut long unchanged runs (see [`CUT_THRESHOLD_WORDS`]).
    pub cut_long_runs: bool,
    /// Return only paragraphs that contain a change.
    pub changed_only: bool,
}

impl Default for WordDiffOptions {
    fn default() -> Self {
        Self {
            cut_long_runs: true,
            changed_only: true,
        }
    }
}

impl WordDiffOptions {
    /// Keep everything: no cuts, and unchanged paragraphs are returned too.
    /// Used to check that a diff loses no text.
    #[must_use]
    pub fn keep_everything() -> Self {
        Self {
            cut_long_runs: false,
            changed_only: false,
        }
    }
}

/// Compare two versions of a text word by word.
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn diff_words(_old: &str, _new: &str, _tokeniser: Tokeniser) -> WordDiff {
    todo!("PV-041")
}

/// [`diff_words`] with explicit options.
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn diff_words_with(
    _old: &str,
    _new: &str,
    _tokeniser: Tokeniser,
    _options: &WordDiffOptions,
) -> WordDiff {
    todo!("PV-041")
}

/// Render the paragraphs of a word diff as plain text.
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn to_word_markup(_diff: &WordDiff) -> String {
    todo!("PV-041")
}

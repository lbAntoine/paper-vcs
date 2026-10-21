//! Word diff with paragraph grouping and a plain rendering
//! (04-content.md, Word diff, steps 2 and 3).
//!
//! Pure functions over strings: no I/O, no printing.

use std::collections::HashMap;
use std::fmt::Write as _;

use similar::{Algorithm, DiffTag, capture_diff_slices};

use crate::{Token, TokenKind, Tokeniser, tokenise};

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
/// Tokens are compared with a patience diff. Two tokens are equal when they
/// have the same kind and text, except that two `Space` tokens are equal when
/// neither holds a paragraph break (two or more `\n`) or when both do. Moving
/// line breaks, re-indenting or doubling a space is therefore not a change,
/// and an unchanged run is reported with the new side's spacing. White space
/// at the very end of a text is likewise layout.
///
/// A paragraph is a run of tokens between paragraph breaks. Its `number` is
/// 1-based on the new side. A paragraph that exists only in the old text takes
/// the number the next new paragraph has (or, at the end, is shown inside the
/// paragraph before it).
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn diff_words(old: &str, new: &str, tokeniser: Tokeniser) -> WordDiff {
    diff_words_with(old, new, tokeniser, &WordDiffOptions::default())
}

/// [`diff_words`] with explicit options.
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn diff_words_with(
    old: &str,
    new: &str,
    tokeniser: Tokeniser,
    options: &WordDiffOptions,
) -> WordDiff {
    let (old_core, _) = split_trailing_layout(old);
    let (new_core, new_trailing) = split_trailing_layout(new);
    let old_tokens = tokenise(old_core, tokeniser);
    let new_tokens = tokenise(new_core, tokeniser);

    let (items, added, removed, changed) = align(&old_tokens, &new_tokens, new_trailing);
    let paragraphs = group(items, *options);
    WordDiff {
        paragraphs,
        added,
        removed,
        layout_only: !changed && old != new,
    }
}

/// Render the paragraphs of a word diff as plain text.
///
/// Each paragraph is a `¶ N` line followed by its body and a newline.
/// Removed text is `[-…-]`, added text is `{+…+}`, and an added or removed
/// paragraph break is `{+¶+}` or `[-¶-]`. Trailing line breaks at the end of a
/// body are not shown. Literal `[-`, `-]`, `{+` and `+}` in the text are not
/// escaped (git's `--word-diff` does the same), so the markup can be
/// ambiguous. The `+N words −M words` header is not part of it.
///
/// # Panics
///
/// Never panics.
#[must_use]
pub fn to_word_markup(diff: &WordDiff) -> String {
    let mut out = String::new();
    for paragraph in &diff.paragraphs {
        let _ = writeln!(out, "¶ {}", paragraph.number);
        let last = paragraph.segments.len().saturating_sub(1);
        for (i, segment) in paragraph.segments.iter().enumerate() {
            let (open, text, close) = match segment {
                Segment::Same(t) => ("", t.as_str(), ""),
                Segment::Removed(t) => ("[-", t.as_str(), "-]"),
                Segment::Added(t) => ("{+", t.as_str(), "+}"),
            };
            if is_break_text(text) {
                out.push_str(open);
                out.push('¶');
                out.push_str(close);
                continue;
            }
            let text = if i == last {
                text.trim_end_matches(['\n', '\r'])
            } else {
                text
            };
            if !text.is_empty() {
                out.push_str(open);
                out.push_str(text);
                out.push_str(close);
            }
        }
        out.push('\n');
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Same,
    Removed,
    Added,
}

/// One token with its fate.
#[derive(Clone, Copy)]
struct Item<'a> {
    op: Op,
    text: &'a str,
    word: bool,
    /// A `Space` token holding a paragraph break.
    brk: bool,
}

impl<'a> Item<'a> {
    fn new(op: Op, token: &Token<'a>) -> Self {
        let space = token.kind == TokenKind::Space;
        let brk = space && is_break_text(token.text);
        Self {
            op,
            text: token.text,
            word: token.kind == TokenKind::Word,
            brk,
        }
    }
}

/// True for white space holding at least two `\n`.
fn is_break_text(text: &str) -> bool {
    text.bytes().filter(|&b| b == b'\n').count() >= 2 && text.chars().all(char::is_whitespace)
}

/// Split off the ASCII white space at the end of a text.
fn split_trailing_layout(text: &str) -> (&str, &str) {
    let core = text.trim_end_matches([' ', '\t', '\r', '\n']);
    text.split_at(core.len())
}

/// A key per token such that tokens that count as equal share a key.
fn keys(tokens: &[Token<'_>], ids: &mut HashMap<(u8, String), u32>) -> Vec<u32> {
    tokens
        .iter()
        .map(|t| {
            let kind = match t.kind {
                TokenKind::Word => 0,
                TokenKind::Space => 1,
                TokenKind::Punct => 2,
                TokenKind::Command => 3,
                TokenKind::Comment => 4,
            };
            let text = if t.kind == TokenKind::Space {
                // All plain spaces share one key, all paragraph breaks another.
                if is_break_text(t.text) { "\n\n" } else { " " }
            } else {
                t.text
            };
            let next = u32::try_from(ids.len()).unwrap_or(u32::MAX);
            *ids.entry((kind, text.to_owned())).or_insert(next)
        })
        .collect()
}

/// Diff the tokens. Returns the items in reading order, the word counts and
/// whether anything other than layout changed. Within a change, removed items
/// come before added ones. A change made only of plain spaces is layout: an
/// added space is kept as unchanged text, a removed one is dropped.
fn align<'a>(
    old: &[Token<'a>],
    new: &[Token<'a>],
    new_trailing: &'a str,
) -> (Vec<Item<'a>>, usize, usize, bool) {
    let mut ids = HashMap::new();
    let old_keys = keys(old, &mut ids);
    let new_keys = keys(new, &mut ids);

    let prefix = old_keys
        .iter()
        .zip(&new_keys)
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = old_keys.len().min(new_keys.len()) - prefix;
    let suffix = old_keys
        .iter()
        .rev()
        .zip(new_keys.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    let old_mid = prefix..old_keys.len() - suffix;
    let new_mid = prefix..new_keys.len() - suffix;

    let mut out = Flow::default();
    for token in &new[..prefix] {
        out.same(token);
    }
    let ops = capture_diff_slices(
        Algorithm::Patience,
        &old_keys[old_mid.clone()],
        &new_keys[new_mid.clone()],
    );
    for op in ops {
        let (tag, o, n) = op.as_tag_tuple();
        match tag {
            DiffTag::Equal => {
                out.flush();
                for token in &new[new_mid.start + n.start..new_mid.start + n.end] {
                    out.same(token);
                }
            }
            DiffTag::Delete | DiffTag::Insert | DiffTag::Replace => {
                out.removed
                    .extend(&old[old_mid.start + o.start..old_mid.start + o.end]);
                out.added
                    .extend(&new[new_mid.start + n.start..new_mid.start + n.end]);
            }
        }
    }
    out.flush();
    for token in &new[new.len() - suffix..] {
        out.same(token);
    }
    if !new_trailing.is_empty() {
        out.items.push(Item {
            op: Op::Same,
            text: new_trailing,
            word: false,
            brk: false,
        });
    }
    (out.items, out.added_words, out.removed_words, out.changed)
}

#[derive(Default)]
struct Flow<'a> {
    items: Vec<Item<'a>>,
    removed: Vec<Token<'a>>,
    added: Vec<Token<'a>>,
    added_words: usize,
    removed_words: usize,
    changed: bool,
}

impl<'a> Flow<'a> {
    fn same(&mut self, token: &Token<'a>) {
        self.items.push(Item::new(Op::Same, token));
    }

    /// Close the pending change.
    fn flush(&mut self) {
        let removed = std::mem::take(&mut self.removed);
        let added = std::mem::take(&mut self.added);
        let layout = removed
            .iter()
            .chain(&added)
            .all(|t| t.kind == TokenKind::Space && !is_break_text(t.text));
        if layout {
            for token in &added {
                self.same(token);
            }
            return;
        }
        self.changed = true;
        for (tokens, op) in [(&removed, Op::Removed), (&added, Op::Added)] {
            for token in tokens {
                let item = Item::new(op, token);
                match op {
                    Op::Removed => self.removed_words += usize::from(item.word),
                    _ => self.added_words += usize::from(item.word),
                }
                self.items.push(item);
            }
        }
    }
}

/// Cut the items into paragraphs. See [`diff_words`] for the rules.
fn group(items: Vec<Item<'_>>, options: WordDiffOptions) -> Vec<Paragraph> {
    let mut out = Vec::new();
    let mut number = 1;
    let mut current: Vec<Item<'_>> = Vec::new();
    let mut changed = false;
    let mut finish = |current: &mut Vec<Item<'_>>, changed: &mut bool, number: usize| {
        if *changed || !options.changed_only {
            out.push(Paragraph {
                number,
                segments: segments(current, options.cut_long_runs),
            });
        }
        current.clear();
        *changed = false;
    };
    for item in items {
        match (item.op, item.brk) {
            (Op::Same, true) => {
                finish(&mut current, &mut changed, number);
                number += 1;
            }
            (Op::Added, true) => {
                current.push(item);
                changed = true;
                finish(&mut current, &mut changed, number);
                number += 1;
            }
            (Op::Removed, true) => {
                current.push(item);
                changed = true;
                // A paragraph that holds only removed text existed only in
                // the old version: it ends here and takes the next number.
                if current.iter().all(|i| i.op == Op::Removed) {
                    finish(&mut current, &mut changed, number);
                }
            }
            (op, false) => {
                current.push(item);
                changed |= op != Op::Same;
            }
        }
    }
    finish(&mut current, &mut changed, number);
    out
}

/// Merge items into segments. Break segments stand alone.
fn segments(items: &[Item<'_>], cut: bool) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < items.len() {
        let op = items[start].op;
        let mut end = start + 1;
        if !items[start].brk {
            while end < items.len() && items[end].op == op && !items[end].brk {
                end += 1;
            }
        }
        let run = &items[start..end];
        out.push(match op {
            Op::Same => Segment::Same(if cut { cut_run(run) } else { join(run) }),
            Op::Removed => Segment::Removed(join(run)),
            Op::Added => Segment::Added(join(run)),
        });
        start = end;
    }
    out
}

fn join(run: &[Item<'_>]) -> String {
    run.iter().map(|i| i.text).collect()
}

/// Shorten an unchanged run of more than [`CUT_THRESHOLD_WORDS`] words to its
/// first and last [`CUT_KEEP_WORDS`] words, joined by ` … `.
fn cut_run(run: &[Item<'_>]) -> String {
    let words: Vec<usize> = run
        .iter()
        .enumerate()
        .filter(|(_, i)| i.word)
        .map(|(at, _)| at)
        .collect();
    if words.len() <= CUT_THRESHOLD_WORDS {
        return join(run);
    }
    let head_end = words[CUT_KEEP_WORDS - 1] + 1;
    let tail_start = words[words.len() - CUT_KEEP_WORDS];
    format!("{} … {}", join(&run[..head_end]), join(&run[tail_start..]))
}

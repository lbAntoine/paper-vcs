//! Word diff with paragraph grouping and a plain rendering
//! (04-content.md, Word diff, steps 2 and 3).
//!
//! Pure functions over strings: no I/O, no printing.

mod align;

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::{Token, TokenKind, Tokeniser, tokenise};

/// An unchanged run with more than this many words is cut.
pub const CUT_THRESHOLD_WORDS: usize = 40;

/// A cut run keeps this many words at each end.
pub const CUT_KEEP_WORDS: usize = 10;

/// One piece of a paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Text on both sides (the new side's spelling of it).
    Same(String),
    /// Text only on the old side. A paragraph break is a segment of its own.
    Removed(String),
    /// Text only on the new side. A paragraph break is a segment of its own.
    Added(String),
    /// Where a long unchanged run was cut: the words between the `Same`
    /// segment before it and the one after it are left out.
    Cut,
}

impl Segment {
    /// True for an added or removed paragraph break.
    ///
    /// # Panics
    ///
    /// Never panics.
    #[must_use]
    pub fn is_break(&self) -> bool {
        matches!(self, Self::Added(t) | Self::Removed(t) if is_break_text(t))
    }
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
/// The whole texts are diffed token by token with a patience diff. Tokens
/// are equal when they have the same kind and text, except that two `Space`
/// tokens are equal when neither holds a paragraph break (two or more `\n`)
/// or when both do.
/// Moving line breaks, re-indenting or doubling a space is therefore not a
/// change, and an unchanged run is reported with the new side's spacing.
/// White space at the very start or end of a text is likewise layout. A lone
/// space added or removed between two tokens is layout only when both
/// neighbours are words of a space-separated script (Latin, Cyrillic, …).
///
/// A paragraph is a run of tokens between paragraph breaks. Its `number` is
/// 1-based on the new side. A paragraph that exists only in the old text takes
/// the number the next new paragraph has (or, at the end, is shown inside the
/// paragraph before it).
///
/// Reads the clock to bound the alignment of each call to about two seconds.
/// Past that, `similar` returns a coarser but still correct diff. A very
/// large text with a very large edit (about 1 MB with a fifth of its words
/// changed) can reach the limit; the diff then shows most of the text as
/// rewritten.
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
    let (_, old_core, _) = split_layout(old);
    let (new_lead, new_core, new_trail) = split_layout(new);
    let old_tokens = tokenise(old_core, tokeniser);
    let new_tokens = tokenise(new_core, tokeniser);

    let marks = align::align(&old_tokens, &new_tokens);
    let mut flow = Flow::default();
    flow.layout_text(new_lead);
    flow.walk(&old_tokens, &new_tokens, &marks);
    flow.layout_text(new_trail);

    WordDiff {
        paragraphs: group(flow.items, *options),
        added: flow.added_words,
        removed: flow.removed_words,
        layout_only: !flow.changed && old != new,
    }
}

/// Render the paragraphs of a word diff as plain text.
///
/// Each paragraph is a `¶ N` line followed by its body and a newline.
/// Removed text is `[-…-]`, added text is `{+…+}`, an added or removed
/// paragraph break is `{+¶+}` or `[-¶-]`, and a cut is ` … `. Line breaks at
/// the start and end of a body are not shown. Literal `[-`, `-]`, `{+` and
/// `+}` in the text are not escaped (git's `--word-diff` does the same), so
/// the markup can be ambiguous. The `+N words −M words` header is not part of
/// it.
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
                Segment::Cut => {
                    out.push_str(" … ");
                    continue;
                }
            };
            if segment.is_break() {
                let _ = write!(out, "{open}¶{close}");
                continue;
            }
            let mut text = text;
            if i == 0 && open.is_empty() {
                text = text.trim_start_matches(['\n', '\r']);
            }
            if i == last {
                text = text.trim_end_matches(['\n', '\r']);
            }
            if !text.is_empty() {
                let _ = write!(out, "{open}{text}{close}");
            }
        }
        out.push('\n');
    }
    out
}

// ------------------------------------------------------------------ tokens

/// True for white space holding at least two `\n`.
fn is_break_text(text: &str) -> bool {
    text.bytes().filter(|&b| b == b'\n').count() >= 2 && text.chars().all(char::is_whitespace)
}

/// Split a text into its leading white space, its core and its trailing white
/// space.
fn split_layout(text: &str) -> (&str, &str, &str) {
    let rest = text.trim_start();
    let core = rest.trim_end();
    let lead = &text[..text.len() - rest.len()];
    (lead, core, &rest[core.len()..])
}

/// True for the scripts whose words are not separated by spaces.
fn is_unspaced_script(c: char) -> bool {
    matches!(u32::from(c),
        0x1100..=0x11FF | 0x2E80..=0xA4CF | 0xA960..=0xA97F | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF | 0xFF66..=0xFF9F | 0x20000..=0x3FFFF)
}

/// True when the token is a word of a space-separated script, looking at the
/// character next to the space (`last`: the token's last character). No token
/// at the edge of the text counts as true.
fn spaced_word_at_edge(token: Option<&Token<'_>>, last: bool) -> bool {
    let Some(token) = token else { return true };
    let c = if last {
        token.text.chars().next_back()
    } else {
        token.text.chars().next()
    };
    token.kind == TokenKind::Word
        && c.is_some_and(|c| c.is_alphanumeric() && !is_unspaced_script(c))
}

fn is_plain_space(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Space && !is_break_text(token.text)
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

fn is_break_token(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Space && is_break_text(token.text)
}

// ------------------------------------------------------------------- items

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
        Self {
            op,
            text: token.text,
            word: token.kind == TokenKind::Word,
            brk: is_break_token(token),
        }
    }
}

#[derive(Default)]
struct Flow<'a> {
    items: Vec<Item<'a>>,
    added_words: usize,
    removed_words: usize,
    changed: bool,
}

impl<'a> Flow<'a> {
    fn same(&mut self, token: &Token<'a>) {
        self.items.push(Item::new(Op::Same, token));
    }

    /// White space at the edge of the new text: unchanged, never a break.
    fn layout_text(&mut self, text: &'a str) {
        if !text.is_empty() {
            self.items.push(Item {
                op: Op::Same,
                text,
                word: false,
                brk: false,
            });
        }
    }

    /// Turn the marks into items, in reading order. Within a change, removed
    /// items come before added ones.
    fn walk(&mut self, old: &[Token<'a>], new: &[Token<'a>], marks: &align::Marks) {
        let mut at = 0;
        for run in align::runs(&marks.del, &marks.ins) {
            for token in &new[at..run.new.start] {
                self.same(token);
            }
            let left = run.new.start.checked_sub(1).and_then(|i| new.get(i));
            let right = new.get(run.new.end);
            self.change(&old[run.old.clone()], &new[run.new.clone()], left, right);
            at = run.new.end;
        }
        for token in &new[at..] {
            self.same(token);
        }
    }

    /// A change made only of plain spaces between two words of a
    /// space-separated script is layout: an added space is kept as unchanged
    /// text, a removed one is dropped.
    fn change(
        &mut self,
        removed: &[Token<'a>],
        added: &[Token<'a>],
        left: Option<&Token<'a>>,
        right: Option<&Token<'a>>,
    ) {
        let layout = removed.iter().chain(added).all(is_plain_space)
            && spaced_word_at_edge(left, true)
            && spaced_word_at_edge(right, false);
        if layout {
            for token in added {
                self.same(token);
            }
            return;
        }
        self.changed = true;
        for token in removed {
            let item = Item::new(Op::Removed, token);
            self.removed_words += usize::from(item.word);
            self.items.push(item);
        }
        for token in added {
            let item = Item::new(Op::Added, token);
            self.added_words += usize::from(item.word);
            self.items.push(item);
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
        match op {
            Op::Same => push_same(&mut out, run, cut),
            Op::Removed => out.push(Segment::Removed(join(run))),
            Op::Added => out.push(Segment::Added(join(run))),
        }
        start = end;
    }
    out
}

fn join(run: &[Item<'_>]) -> String {
    run.iter().map(|i| i.text).collect()
}

/// Push an unchanged run. One of more than [`CUT_THRESHOLD_WORDS`] words
/// becomes its first and last [`CUT_KEEP_WORDS`] words around a `Cut`.
fn push_same(out: &mut Vec<Segment>, run: &[Item<'_>], cut: bool) {
    let words: Vec<usize> = run
        .iter()
        .enumerate()
        .filter(|(_, i)| i.word)
        .map(|(at, _)| at)
        .collect();
    if !cut || words.len() <= CUT_THRESHOLD_WORDS {
        out.push(Segment::Same(join(run)));
        return;
    }
    let head_end = words[CUT_KEEP_WORDS - 1] + 1;
    let tail_start = words[words.len() - CUT_KEEP_WORDS];
    out.push(Segment::Same(join(&run[..head_end])));
    out.push(Segment::Cut);
    out.push(Segment::Same(join(&run[tail_start..])));
}

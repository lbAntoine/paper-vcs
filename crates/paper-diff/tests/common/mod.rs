//! Helpers shared by the word diff tests.
#![allow(dead_code)]

use paper_diff::Segment::{Added, Cut, Removed, Same};
use paper_diff::{
    Paragraph, Segment, Tokeniser, WordDiff, WordDiffOptions, diff_words, diff_words_with,
    to_word_markup,
};

pub const GEN: Tokeniser = Tokeniser::Generic;
pub const TEX: Tokeniser = Tokeniser::Latex;
pub const BREAK: &str = "\n\n";

// ---------------------------------------------------------------- helpers

pub fn same(t: &str) -> Segment {
    Same(t.to_owned())
}
pub fn removed(t: &str) -> Segment {
    Removed(t.to_owned())
}
pub fn added(t: &str) -> Segment {
    Added(t.to_owned())
}
pub fn para(number: usize, segments: Vec<Segment>) -> Paragraph {
    Paragraph { number, segments }
}

/// `prefix` + each number from `from` to `to` inclusive, joined by one space.
pub fn run(prefix: &str, from: usize, to: usize) -> String {
    (from..=to)
        .map(|i| format!("{prefix}{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `n` consecutive Hangul syllables with no space between them.
pub fn hangul(from: u32, to: u32) -> String {
    (from..to)
        .map(|i| char::from_u32(0xAC00 + i).unwrap())
        .collect()
}

/// Rebuild the old and the new text from a diff that keeps everything
/// (`WordDiffOptions::keep_everything`). A paragraph that does not end in an
/// added or removed break is followed by the unchanged paragraph break `brk`.
pub fn rebuild(diff: &WordDiff, brk: &str) -> (String, String) {
    let (mut old, mut new) = (String::new(), String::new());
    for (i, p) in diff.paragraphs.iter().enumerate() {
        for seg in &p.segments {
            match seg {
                Same(t) => {
                    old.push_str(t);
                    new.push_str(t);
                }
                Removed(t) => old.push_str(t),
                Added(t) => new.push_str(t),
                Cut => {}
            }
        }
        let closed_by_break = p.segments.last().is_some_and(Segment::is_break);
        if i + 1 < diff.paragraphs.len() && !closed_by_break {
            old.push_str(brk);
            new.push_str(brk);
        }
    }
    (old, new)
}

pub fn full(old: &str, new: &str, tok: Tokeniser) -> WordDiff {
    diff_words_with(old, new, tok, &WordDiffOptions::keep_everything())
}

pub struct Case<'a> {
    pub old: &'a str,
    pub new: &'a str,
    pub segments: Vec<Segment>,
    pub added: usize,
    pub removed: usize,
    pub markup: &'a str,
}

/// One changed paragraph numbered 1.
pub fn check_single(tok: Tokeniser, cases: &[Case<'_>]) {
    for c in cases {
        let d = diff_words(c.old, c.new, tok);
        let ctx = format!("{:?} -> {:?}", c.old, c.new);
        assert_eq!(d.paragraphs, vec![para(1, c.segments.clone())], "{ctx}");
        assert_eq!((d.added, d.removed), (c.added, c.removed), "{ctx}");
        assert!(!d.layout_only, "{ctx}");
        assert_eq!(to_word_markup(&d), c.markup, "{ctx}");
    }
}

//! Property tests of the word diff (PV-041).

mod common;

use common::*;
use paper_diff::Segment::{Added, Removed};
use paper_diff::diff_words;
use proptest::prelude::*;

fn paragraph_text() -> impl Strategy<Value = String> {
    proptest::collection::vec("[a-d]{2}", 1..6).prop_map(|words| words.join(" "))
}

/// Paragraphs of single-spaced words joined by exactly one blank line, so
/// that no unchanged spacing differs between the two sides.
fn document() -> impl Strategy<Value = String> {
    proptest::collection::vec(paragraph_text(), 0..4).prop_map(|ps| ps.join(BREAK))
}

const ATOMS: [&str; 9] = ["ab", "cd", "한", "国", "\\emph", "%c", "😀", ",", "ab"];
const SPACES: [&str; 3] = [" ", "\r", "\u{a0}"];
const BREAKS: [&str; 3] = ["\n\n", "\r\n\r\n", "\n\n\n"];

/// Two documents built from the same atoms, the same plain space and the same
/// paragraph break, plus that break. Atoms are CJK, a LaTeX command and
/// comment, an emoji and punctuation as well as words.
fn mixed_pair() -> impl Strategy<Value = (String, String, &'static str)> {
    let paragraphs = || {
        proptest::collection::vec(
            proptest::collection::vec(proptest::sample::select(ATOMS.to_vec()), 1..6),
            0..4,
        )
    };
    (
        paragraphs(),
        paragraphs(),
        proptest::sample::select(SPACES.to_vec()),
        proptest::sample::select(BREAKS.to_vec()),
    )
        .prop_map(|(a, b, space, brk)| {
            let build = |doc: Vec<Vec<&str>>| {
                doc.iter()
                    .map(|p| p.join(space))
                    .collect::<Vec<_>>()
                    .join(brk)
            };
            (build(a), build(b), brk)
        })
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn same_plus_removed_is_the_old_text_and_same_plus_added_is_the_new_text(
        (old, new, brk) in mixed_pair(),
    ) {
        for tok in [GEN, TEX] {
            let d = full(&old, &new, tok);
            prop_assert_eq!(rebuild(&d, brk), (old.clone(), new.clone()));
        }
    }

    #[test]
    fn counts_do_not_depend_on_cutting_or_filtering(old in document(), new in document()) {
        let cut = diff_words(&old, &new, GEN);
        let all = full(&old, &new, GEN);
        prop_assert_eq!(cut.added, all.added);
        prop_assert_eq!(cut.removed, all.removed);
        prop_assert_eq!(cut.layout_only, all.layout_only);
        let added_words: usize = all.paragraphs.iter().flat_map(|p| &p.segments)
            .map(|s| if let Added(t) = s { t.split_whitespace().count() } else { 0 }).sum();
        let removed_words: usize = all.paragraphs.iter().flat_map(|p| &p.segments)
            .map(|s| if let Removed(t) = s { t.split_whitespace().count() } else { 0 }).sum();
        prop_assert_eq!(all.added, added_words);
        prop_assert_eq!(all.removed, removed_words);
    }

    #[test]
    fn identical_texts_never_produce_paragraphs(text in document()) {
        let d = diff_words(&text, &text, GEN);
        prop_assert!(d.paragraphs.is_empty());
        prop_assert!(!d.layout_only);
    }
}

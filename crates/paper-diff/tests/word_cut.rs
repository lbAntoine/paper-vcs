//! Cutting long unchanged runs (PV-041).

mod common;

use common::*;
use paper_diff::Segment::Cut;
use paper_diff::{WordDiff, diff_words, to_word_markup};

// ----------------------------------------------------------- long runs

fn long_run_diff(words_in_run: usize) -> (WordDiff, String) {
    // "start" + (words_in_run - 1) numbered words = words_in_run words, then a change.
    let body = format!("start {}", run("w", 1, words_in_run - 1));
    let d = diff_words(&format!("{body} end"), &format!("{body} fin"), GEN);
    (d, body)
}

#[test]
fn an_unchanged_run_of_exactly_forty_words_is_not_cut() {
    let (d, body) = long_run_diff(40);
    assert_eq!(
        d.paragraphs,
        vec![para(
            1,
            vec![same(&format!("{body} ")), removed("end"), added("fin")]
        )]
    );
}

#[test]
fn an_unchanged_run_of_forty_one_words_is_cut_to_ten_and_ten() {
    let (d, _) = long_run_diff(41);
    // run = start w1..w40: keep start w1..w9, then w31..w40 and the space.
    let head = format!("start {}", run("w", 1, 9));
    let tail = format!("{} ", run("w", 31, 40));
    assert_eq!(
        d.paragraphs,
        vec![para(
            1,
            vec![same(&head), Cut, same(&tail), removed("end"), added("fin")]
        )]
    );
    assert_eq!((d.added, d.removed), (1, 1));
}

#[test]
fn a_long_unchanged_run_keeps_its_first_and_last_ten_words() {
    let (d, _) = long_run_diff(51);
    let head = format!("start {}", run("w", 1, 9));
    let tail = format!("{} ", run("w", 41, 50));
    assert_eq!(
        d.paragraphs[0].segments,
        vec![same(&head), Cut, same(&tail), removed("end"), added("fin")]
    );
    assert_eq!(
        to_word_markup(&d),
        format!("¶ 1\n{head} … {tail}[-end-]{{+fin+}}\n")
    );
}

#[test]
fn runs_on_both_sides_of_a_change_are_cut_separately() {
    let old = format!("{} X {}", run("r", 1, 45), run("s", 1, 45));
    let new = format!("{} Y {}", run("r", 1, 45), run("s", 1, 45));
    let d = diff_words(&old, &new, GEN);
    assert_eq!(
        d.paragraphs[0].segments,
        vec![
            same(&run("r", 1, 10)),
            Cut,
            same(&format!("{} ", run("r", 36, 45))),
            removed("X"),
            added("Y"),
            same(&format!(" {}", run("s", 1, 10))),
            Cut,
            same(&run("s", 36, 45)),
        ]
    );
}

#[test]
fn a_cut_never_splits_a_token() {
    // 50 Hangul syllables are 50 one-grapheme words with no space between.
    let body = hangul(0, 50);
    let d = diff_words(&format!("{body} end"), &format!("{body} fin"), GEN);
    assert_eq!(
        d.paragraphs[0].segments,
        vec![
            same(&hangul(0, 10)),
            Cut,
            same(&format!("{} ", hangul(40, 50))),
            removed("end"),
            added("fin")
        ]
    );
}

#[test]
fn keep_everything_does_not_cut_and_returns_unchanged_paragraphs() {
    let body = format!("start {}", run("w", 1, 50));
    let old = format!("{body} end\n\nB two");
    let new = format!("{body} fin\n\nB two");
    let d = full(&old, &new, GEN);
    assert_eq!(
        d.paragraphs,
        vec![
            para(
                1,
                vec![same(&format!("{body} ")), removed("end"), added("fin")]
            ),
            para(2, vec![same("B two")]),
        ]
    );
    assert_eq!((d.added, d.removed), (1, 1));
}

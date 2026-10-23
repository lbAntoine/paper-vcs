//! Semantic cleanup of the word diff (PV-042): an unchanged run with no word, between two
//! changes, is absorbed so a changed stretch is one removed run and one added run.

use paper_diff::{Tokeniser, diff_words, to_word_markup};

fn markup(old: &str, new: &str) -> String {
    to_word_markup(&diff_words(old, new, Tokeniser::Generic))
}

#[test]
fn a_replaced_sentence_is_one_removed_and_one_added_run() {
    assert_eq!(
        markup("Hello big bad wolf here", "Hello tiny good fox here"),
        "¶ 1\nHello [-big bad wolf-]{+tiny good fox+} here\n"
    );
}

#[test]
fn single_letters_become_one_run_each_side() {
    assert_eq!(markup("a b c", "x y z"), "¶ 1\n[-a b c-]{+x y z+}\n");
}

#[test]
fn punctuation_between_two_changes_is_absorbed() {
    assert_eq!(
        markup("go one, two stop", "go three, four stop"),
        "¶ 1\ngo [-one, two-]{+three, four+} stop\n"
    );
}

#[test]
fn a_single_unchanged_word_between_two_changes_is_kept() {
    assert_eq!(
        markup("alpha beta gamma", "ALPHA beta GAMMA"),
        "¶ 1\n[-alpha-]{+ALPHA+} beta [-gamma-]{+GAMMA+}\n"
    );
}

#[test]
fn counts_do_not_change() {
    let diff = diff_words("a b c", "x y z", Tokeniser::Generic);
    assert_eq!((diff.added, diff.removed), (3, 3));
}

#[test]
fn a_run_next_to_a_paragraph_break_is_not_absorbed() {
    let out = markup("、\n\nab", "ab\n\n、");
    assert!(!out.contains("[-、-]{+、+}"), "{out}");
}

#[test]
fn an_unchanged_comment_between_two_edits_is_kept() {
    let diff = diff_words("alpha % note\nbeta", "ALPHA % note\nBETA", Tokeniser::Latex);
    assert_eq!(
        to_word_markup(&diff),
        "¶ 1\n[-alpha-]{+ALPHA+} % note\n[-beta-]{+BETA+}\n"
    );
}

#[test]
fn an_unchanged_command_between_two_edits_is_kept() {
    let diff = diff_words("alpha \\emph beta", "ALPHA \\emph BETA", Tokeniser::Latex);
    assert_eq!(
        to_word_markup(&diff),
        "¶ 1\n[-alpha-]{+ALPHA+} \\emph [-beta-]{+BETA+}\n"
    );
}

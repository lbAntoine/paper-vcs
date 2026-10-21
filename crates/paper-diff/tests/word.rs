//! Behaviour of the word diff (PV-041).
//!
//! Expected segments and markup strings in the tables are the specification.
//! Texts avoid repeated tokens at the edit site (no `.` or doubled words next
//! to a change), so that the expected alignment is the only reasonable one.
//! Cuts, round trips and timings are in `word_cut.rs`, `word_props.rs` and
//! `word_perf.rs`.

mod common;

use common::*;
use paper_diff::Segment::{Added, Removed};
use paper_diff::{
    CUT_KEEP_WORDS, CUT_THRESHOLD_WORDS, WordDiff, WordDiffOptions, diff_words, to_word_markup,
};

// ------------------------------------------------- sanity tests of helpers

#[test]
fn run_builds_numbered_words() {
    assert_eq!(run("w", 1, 3), "w1 w2 w3");
    assert_eq!(run("x", 5, 5), "x5");
}

#[test]
fn hangul_builds_adjacent_syllables() {
    assert_eq!(hangul(0, 3), "가각갂");
    assert_eq!(hangul(2, 2), "");
}

#[test]
fn rebuild_joins_unchanged_paragraphs_with_a_break() {
    let d = WordDiff {
        paragraphs: vec![para(1, vec![same("A")]), para(2, vec![same("B")])],
        added: 0,
        removed: 0,
        layout_only: false,
    };
    assert_eq!(
        rebuild(&d, BREAK),
        ("A\n\nB".to_owned(), "A\n\nB".to_owned())
    );
}

#[test]
fn rebuild_does_not_double_a_break_that_is_a_segment() {
    // New side gained "\n\nB" after A; the added break closes paragraph 1.
    let d = WordDiff {
        paragraphs: vec![
            para(1, vec![same("A"), added(BREAK)]),
            para(2, vec![added("B")]),
        ],
        added: 1,
        removed: 0,
        layout_only: false,
    };
    assert_eq!(rebuild(&d, BREAK), ("A".to_owned(), "A\n\nB".to_owned()));
}

#[test]
fn rebuild_handles_a_paragraph_that_exists_only_on_the_old_side() {
    let d = WordDiff {
        paragraphs: vec![
            para(1, vec![same("A")]),
            para(2, vec![removed("B"), removed(BREAK)]),
            para(2, vec![same("C")]),
        ],
        added: 0,
        removed: 1,
        layout_only: false,
    };
    assert_eq!(
        rebuild(&d, BREAK),
        ("A\n\nB\n\nC".to_owned(), "A\n\nC".to_owned())
    );
}

#[test]
fn rebuild_of_nothing_is_empty() {
    let d = WordDiff {
        paragraphs: vec![],
        added: 0,
        removed: 0,
        layout_only: false,
    };
    assert_eq!(rebuild(&d, BREAK), (String::new(), String::new()));
}

// ----------------------------------------------------------- options

#[test]
fn default_options_cut_and_return_changed_paragraphs_only() {
    let o = WordDiffOptions::default();
    assert!(o.cut_long_runs);
    assert!(o.changed_only);
}

#[test]
fn keep_everything_turns_cutting_and_filtering_off() {
    let o = WordDiffOptions::keep_everything();
    assert!(!o.cut_long_runs);
    assert!(!o.changed_only);
}

#[test]
fn the_cut_constants_are_forty_and_ten() {
    assert_eq!(CUT_THRESHOLD_WORDS, 40);
    assert_eq!(CUT_KEEP_WORDS, 10);
}

// ------------------------------------------------------------ the table

#[test]
fn single_paragraph_edits_give_the_expected_segments_counts_and_markup() {
    check_single(
        GEN,
        &[
            Case {
                old: "the cat sat",
                new: "the dog sat",
                segments: vec![same("the "), removed("cat"), added("dog"), same(" sat")],
                added: 1,
                removed: 1,
                markup: "¶ 1\nthe [-cat-]{+dog+} sat\n",
            },
            Case {
                old: "a b",
                new: "a b c",
                segments: vec![same("a b"), added(" c")],
                added: 1,
                removed: 0,
                markup: "¶ 1\na b{+ c+}\n",
            },
            Case {
                old: "a b c",
                new: "a b",
                segments: vec![same("a b"), removed(" c")],
                added: 0,
                removed: 1,
                markup: "¶ 1\na b[- c-]\n",
            },
            Case {
                old: "a b",
                new: "x a b",
                segments: vec![added("x "), same("a b")],
                added: 1,
                removed: 0,
                markup: "¶ 1\n{+x +}a b\n",
            },
            Case {
                old: "Hello world.",
                new: "Hello world!",
                segments: vec![same("Hello world"), removed("."), added("!")],
                added: 0,
                removed: 0,
                markup: "¶ 1\nHello world[-.-]{+!+}\n",
            },
            Case {
                old: "a b",
                new: "a, b",
                segments: vec![same("a"), added(","), same(" b")],
                added: 0,
                removed: 0,
                markup: "¶ 1\na{+,+} b\n",
            },
            Case {
                old: "",
                new: "x y",
                segments: vec![added("x y")],
                added: 2,
                removed: 0,
                markup: "¶ 1\n{+x y+}\n",
            },
            Case {
                old: "x y",
                new: "",
                segments: vec![removed("x y")],
                added: 0,
                removed: 2,
                markup: "¶ 1\n[-x y-]\n",
            },
        ],
    );
}

#[test]
fn identical_texts_have_no_paragraphs_and_are_not_layout_only() {
    for text in ["", "a b", "a b\n\nc d\n"] {
        let d = diff_words(text, text, GEN);
        assert!(d.paragraphs.is_empty(), "{text:?}");
        assert_eq!((d.added, d.removed), (0, 0), "{text:?}");
        assert!(!d.layout_only, "{text:?}");
        assert_eq!(to_word_markup(&d), "", "{text:?}");
    }
}

#[test]
fn inserting_a_word_in_the_middle_shows_one_added_run_of_that_word() {
    let (old, new) = ("alpha beta gamma", "alpha beta extra gamma");
    let d = diff_words(old, new, GEN);
    assert_eq!((d.added, d.removed), (1, 0));
    assert_eq!(d.paragraphs.len(), 1);
    let adds: Vec<_> = d.paragraphs[0]
        .segments
        .iter()
        .filter(|s| matches!(s, Added(_)))
        .collect();
    assert_eq!(adds.len(), 1);
    assert!(matches!(adds[0], Added(t) if t.trim() == "extra"));
    assert!(
        !d.paragraphs[0]
            .segments
            .iter()
            .any(|s| matches!(s, Removed(_)))
    );
    assert_eq!(
        rebuild(&full(old, new, GEN), BREAK),
        (old.to_owned(), new.to_owned())
    );
}

#[test]
fn deleting_a_word_in_the_middle_shows_one_removed_run_of_that_word() {
    let d = diff_words("alpha beta extra gamma", "alpha beta gamma", GEN);
    assert_eq!((d.added, d.removed), (0, 1));
    let rems: Vec<_> = d.paragraphs[0]
        .segments
        .iter()
        .filter(|s| matches!(s, Removed(_)))
        .collect();
    assert_eq!(rems.len(), 1);
    assert!(matches!(rems[0], Removed(t) if t.trim() == "extra"));
}

#[test]
fn marker_characters_in_the_text_are_left_as_they_are() {
    // Like git's --word-diff, no escaping: the markup can be ambiguous.
    let d = diff_words("a b", "a [-b-]", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(
            1,
            vec![same("a "), added("[-"), same("b"), added("-]")]
        )]
    );
    assert_eq!(to_word_markup(&d), "¶ 1\na {+[-+}b{+-]+}\n");
}

// ------------------------------------------------------- the spec example

#[test]
fn the_spec_example_gives_the_documented_segments_and_markup() {
    // 04-content.md § Word diff. The spec wraps the second line for display;
    // the markup itself is one line.
    let before =
        "Intro text\n\nMethods text\n\nRecent work has mostly focused on supervised settings.";
    let after = "Intro text\n\nMethods text\n\nRecent work has largely focused on supervised settings, although self-supervised approaches are gaining traction.";
    let d = diff_words(before, after, GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(
            3,
            vec![
                same("Recent work has "),
                removed("mostly"),
                added("largely"),
                same(" focused on supervised settings"),
                added(", although self-supervised approaches are gaining traction"),
                same("."),
            ]
        )]
    );
    // largely + (although self supervised approaches are gaining traction)
    assert_eq!((d.added, d.removed), (8, 1));
    assert!(!d.layout_only);
    assert_eq!(
        to_word_markup(&d),
        "¶ 3\nRecent work has [-mostly-]{+largely+} focused on supervised settings{+, although self-supervised approaches are gaining traction+}.\n"
    );
}

// ------------------------------------------------------------ layout only

#[test]
fn a_rewrapped_paragraph_is_layout_only_with_no_paragraphs() {
    let d = diff_words("one two\nthree four\n", "one two three four\n", GEN);
    assert!(d.paragraphs.is_empty());
    assert_eq!((d.added, d.removed), (0, 0));
    assert!(d.layout_only);
    assert_eq!(to_word_markup(&d), "");
}

#[test]
fn indentation_and_double_spaces_are_layout_only() {
    for (old, new) in [
        ("a b", "a  b"),
        ("a\n  b", "a b"),
        ("a\tb", "a b"),
        ("a\r\rb", "a b"),
        ("a\n\nb", "a \n \n b"),
        ("a\n\nb", "a\n\n\n\nb"),
        ("a\r\n\r\nb", "a\n\nb"),
    ] {
        let d = diff_words(old, new, GEN);
        assert!(d.paragraphs.is_empty(), "{old:?} -> {new:?}");
        assert!(d.layout_only, "{old:?} -> {new:?}");
    }
}

#[test]
fn a_real_edit_inside_a_rewrapped_paragraph_shows_only_the_changed_words() {
    let old = "alpha beta\ngamma delta\n\nsecond para\n";
    let new = "alpha beta gamma\nepsilon\n\nsecond para\n";
    let d = diff_words(old, new, GEN);
    // An unchanged run uses the new side's spacing.
    assert_eq!(
        d.paragraphs,
        vec![para(
            1,
            vec![
                same("alpha beta gamma\n"),
                removed("delta"),
                added("epsilon")
            ]
        )]
    );
    assert_eq!((d.added, d.removed), (1, 1));
    assert!(!d.layout_only);
    assert_eq!(
        to_word_markup(&d),
        "¶ 1\nalpha beta gamma\n[-delta-]{+epsilon+}\n"
    );
}

#[test]
fn white_space_added_or_removed_at_the_very_end_is_layout_only() {
    for (old, new) in [
        ("a", "a\n"),
        ("a", "a\n\n"),
        ("a\n\n", "a"),
        ("a\n", "a\n\n"),
        ("a\n\nb", "a\n\nb\n\n"),
    ] {
        let d = diff_words(old, new, GEN);
        assert!(d.paragraphs.is_empty(), "{old:?} -> {new:?}");
        assert_eq!((d.added, d.removed), (0, 0), "{old:?} -> {new:?}");
        assert!(d.layout_only, "{old:?} -> {new:?}");
    }
}

#[test]
fn a_trailing_newline_stays_in_the_data_and_is_trimmed_from_the_markup() {
    let d = diff_words("a b\n", "a c\n", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(
            1,
            vec![same("a "), removed("b"), added("c"), same("\n")]
        )]
    );
    assert_eq!(to_word_markup(&d), "¶ 1\na [-b-]{+c+}\n");
}

#[test]
fn a_paragraph_break_against_a_plain_space_is_a_change_not_layout() {
    let d = diff_words("a\n\nb", "a b", GEN);
    assert!(!d.layout_only);
    assert!(!d.paragraphs.is_empty());
    let d = diff_words("a\nb", "a\n\nb", GEN);
    assert!(!d.layout_only);
    assert!(!d.paragraphs.is_empty());
}

// -------------------------------------------------------------- paragraphs

#[test]
fn an_added_paragraph_at_the_end_is_numbered_and_marks_its_break() {
    let d = diff_words("A one\n\nB two", "A one\n\nB two\n\nC three", GEN);
    assert_eq!(
        d.paragraphs,
        vec![
            para(2, vec![same("B two"), added(BREAK)]),
            para(3, vec![added("C three")]),
        ]
    );
    assert_eq!((d.added, d.removed), (2, 0));
    assert_eq!(to_word_markup(&d), "¶ 2\nB two{+¶+}\n¶ 3\n{+C three+}\n");
}

#[test]
fn an_added_paragraph_in_the_middle_is_numbered_by_its_new_position() {
    // Assumes the insertion lands after the unchanged break (a common prefix
    // is matched first); GREEN to confirm.
    let d = diff_words("A one\n\nC three", "A one\n\nB two\n\nC three", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(2, vec![added("B two"), added(BREAK)])]
    );
    assert_eq!((d.added, d.removed), (2, 0));
    assert_eq!(to_word_markup(&d), "¶ 2\n{+B two+}{+¶+}\n");
}

#[test]
fn a_paragraph_added_first_pushes_later_numbers_forward() {
    let d = diff_words("A one\n\nB two", "Z\n\nA one\n\nB zzz", GEN);
    assert_eq!(
        d.paragraphs,
        vec![
            para(1, vec![added("Z"), added(BREAK)]),
            para(3, vec![same("B "), removed("two"), added("zzz")]),
        ]
    );
    assert_eq!((d.added, d.removed), (2, 1));
}

#[test]
fn a_deleted_paragraph_in_the_middle_takes_the_number_of_its_position() {
    // The deleted paragraph stood second; the next paragraph is now second.
    let d = diff_words("A one\n\nB two\n\nC three", "A one\n\nC three", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(2, vec![removed("B two"), removed(BREAK)])]
    );
    assert_eq!((d.added, d.removed), (0, 2));
    assert_eq!(to_word_markup(&d), "¶ 2\n[-B two-][-¶-]\n");
}

#[test]
fn a_deleted_first_paragraph_is_numbered_one() {
    let d = diff_words("B two\n\nA one", "A one", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(1, vec![removed("B two"), removed(BREAK)])]
    );
    assert_eq!(to_word_markup(&d), "¶ 1\n[-B two-][-¶-]\n");
}

#[test]
fn a_deleted_last_paragraph_shows_inside_the_one_before_it() {
    let d = diff_words("A one\n\nB two", "A one", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(
            1,
            vec![same("A one"), removed(BREAK), removed("B two")]
        )]
    );
    assert_eq!((d.added, d.removed), (0, 2));
    assert_eq!(to_word_markup(&d), "¶ 1\nA one[-¶-][-B two-]\n");
}

#[test]
fn a_split_paragraph_marks_the_new_break() {
    let d = diff_words("A one two", "A one\n\ntwo", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(1, vec![same("A one"), removed(" "), added(BREAK)])]
    );
    assert_eq!((d.added, d.removed), (0, 0));
    assert!(!d.layout_only);
    assert_eq!(to_word_markup(&d), "¶ 1\nA one[- -]{+¶+}\n");
}

#[test]
fn a_joined_pair_of_paragraphs_marks_the_removed_break() {
    let d = diff_words("A one\n\ntwo", "A one two", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(
            1,
            vec![same("A one"), removed(BREAK), added(" "), same("two")]
        )]
    );
    assert_eq!((d.added, d.removed), (0, 0));
    assert_eq!(to_word_markup(&d), "¶ 1\nA one[-¶-]{+ +}two\n");
}

#[test]
fn unchanged_paragraphs_are_not_returned() {
    let d = diff_words("A one\n\nB two\n\nC three", "A one\n\nB 2\n\nC three", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(2, vec![same("B "), removed("two"), added("2")])]
    );
}

// ------------------------------------------------------------ CJK / LaTeX

#[test]
fn a_korean_edit_shows_only_the_changed_graphemes() {
    check_single(
        GEN,
        &[Case {
            old: "우리는 학교에 간다",
            new: "우리는 학교에 갔다",
            segments: vec![
                same("우리는 학교에 "),
                removed("간"),
                added("갔"),
                same("다"),
            ],
            added: 1,
            removed: 1,
            markup: "¶ 1\n우리는 학교에 [-간-]{+갔+}다\n",
        }],
    );
}

#[test]
fn a_japanese_edit_shows_only_the_changed_graphemes() {
    check_single(
        GEN,
        &[Case {
            old: "私は学生です",
            new: "私は先生です",
            segments: vec![same("私は"), removed("学"), added("先"), same("生です")],
            added: 1,
            removed: 1,
            markup: "¶ 1\n私は[-学-]{+先+}生です\n",
        }],
    );
}

#[test]
fn a_changed_latex_command_is_one_removed_and_one_added_command_and_no_words() {
    check_single(
        TEX,
        &[Case {
            old: "We use \\emph{big} data",
            new: "We use \\textbf{big} data",
            segments: vec![
                same("We use "),
                removed("\\emph"),
                added("\\textbf"),
                same("{big} data"),
            ],
            added: 0,
            removed: 0,
            markup: "¶ 1\nWe use [-\\emph-]{+\\textbf+}{big} data\n",
        }],
    );
}

#[test]
fn a_changed_latex_comment_is_a_comment_change_and_no_words() {
    check_single(
        TEX,
        &[Case {
            old: "Text % old note\nMore",
            new: "Text % new note\nMore",
            segments: vec![
                same("Text "),
                removed("% old note"),
                added("% new note"),
                same("\nMore"),
            ],
            added: 0,
            removed: 0,
            markup: "¶ 1\nText [-% old note-]{+% new note+}\nMore\n",
        }],
    );
}

#[test]
fn a_changed_word_in_latex_text_is_counted() {
    let d = diff_words("We \\emph{like} it", "We \\emph{love} it", TEX);
    assert_eq!((d.added, d.removed), (1, 1));
    assert_eq!(
        d.paragraphs,
        vec![para(
            1,
            vec![
                same("We \\emph{"),
                removed("like"),
                added("love"),
                same("} it")
            ]
        )]
    );
}

// ------------------------------------------------- the slide of hunks

#[test]
fn a_deleted_paragraph_that_starts_like_the_next_one_is_shown_whole() {
    let d = diff_words(
        "Intro text.\n\nWe study x in detail.\n\nWe show y.\n",
        "Intro text.\n\nWe show y.\n",
        GEN,
    );
    assert_eq!(
        d.paragraphs,
        vec![para(
            2,
            vec![removed("We study x in detail."), removed(BREAK)]
        )]
    );
    assert_eq!(to_word_markup(&d), "¶ 2\n[-We study x in detail.-][-¶-]\n");
}

#[test]
fn an_added_paragraph_that_starts_like_the_next_one_is_shown_whole() {
    let d = diff_words(
        "Intro text.\n\nThe c three.\n",
        "Intro text.\n\nThe new para.\n\nThe c three.\n",
        GEN,
    );
    assert_eq!(
        d.paragraphs,
        vec![para(2, vec![added("The new para."), added(BREAK)])]
    );
    assert_eq!(to_word_markup(&d), "¶ 2\n{+The new para.+}{+¶+}\n");
}

#[test]
fn a_deleted_first_paragraph_is_whole_when_both_start_with_the_same_word() {
    let d = diff_words("The old one.\n\nThe next.\n", "The next.\n", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(1, vec![removed("The old one."), removed(BREAK)])]
    );
}

// ------------------------------------------------ lone spaces and layout

#[test]
fn a_lone_space_removed_between_cjk_graphemes_is_a_change() {
    let d = diff_words("한 국어", "한국어", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(1, vec![same("한"), removed(" "), same("국어")])]
    );
    assert!(!d.layout_only);
    assert_eq!((d.added, d.removed), (0, 0));
}

#[test]
fn a_lone_space_around_punctuation_is_a_change() {
    let d = diff_words("a , b", "a, b", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(1, vec![same("a"), removed(" "), same(", b")])]
    );
    assert_eq!(to_word_markup(&d), "¶ 1\na[- -], b\n");
    let d = diff_words("a, b", "a , b", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(1, vec![same("a"), added(" "), same(", b")])]
    );
}

#[test]
fn leading_blank_lines_are_layout() {
    let d = diff_words("a b", "\n\na b", GEN);
    assert!(d.paragraphs.is_empty());
    assert!(d.layout_only);
    let d = diff_words("\n\na b", "a b", GEN);
    assert!(d.paragraphs.is_empty());
    assert!(d.layout_only);
}

// --------------------------------------------------------- break markup

#[test]
fn an_unchanged_trailing_break_is_not_rendered_as_a_paragraph_mark() {
    let d = diff_words("a b\n\n", "a\n\n", GEN);
    assert_eq!(
        d.paragraphs,
        vec![para(1, vec![same("a"), removed(" b"), same(BREAK)])]
    );
    assert_eq!(to_word_markup(&d), "¶ 1\na[- b-]\n");
    assert!(!d.paragraphs[0].segments[2].is_break());
}

#[test]
fn only_added_and_removed_breaks_are_break_segments() {
    assert!(added(BREAK).is_break());
    assert!(removed("\r\n\r\n").is_break());
    assert!(!same(BREAK).is_break());
    assert!(!added(" ").is_break());
    assert!(!paper_diff::Segment::Cut.is_break());
}

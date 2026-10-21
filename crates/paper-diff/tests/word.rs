//! Behaviour of the word diff (PV-041).
//!
//! Expected segments and markup strings in the tables are the specification.
//! Texts avoid repeated tokens at the edit site (no `.` or doubled words next
//! to a change), so that the expected alignment is the only reasonable one.

use paper_diff::Segment::{Added, Removed, Same};
use paper_diff::{
    CUT_KEEP_WORDS, CUT_THRESHOLD_WORDS, Paragraph, Segment, Tokeniser, WordDiff, WordDiffOptions,
    diff_words, diff_words_with, to_word_markup,
};
use proptest::prelude::*;

const GEN: Tokeniser = Tokeniser::Generic;
const TEX: Tokeniser = Tokeniser::Latex;
const BREAK: &str = "\n\n";

// ---------------------------------------------------------------- helpers

fn same(t: &str) -> Segment {
    Same(t.to_owned())
}
fn removed(t: &str) -> Segment {
    Removed(t.to_owned())
}
fn added(t: &str) -> Segment {
    Added(t.to_owned())
}
fn para(number: usize, segments: Vec<Segment>) -> Paragraph {
    Paragraph { number, segments }
}

/// `prefix` + each number from `from` to `to` inclusive, joined by one space.
fn run(prefix: &str, from: usize, to: usize) -> String {
    (from..=to)
        .map(|i| format!("{prefix}{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `n` consecutive Hangul syllables with no space between them.
fn hangul(from: u32, to: u32) -> String {
    (from..to)
        .map(|i| char::from_u32(0xAC00 + i).unwrap())
        .collect()
}

/// Rebuild the old and the new text from a diff that keeps everything
/// (`WordDiffOptions::keep_everything`). A paragraph that does not end in an
/// added or removed break is followed by an unchanged paragraph break.
fn rebuild(diff: &WordDiff) -> (String, String) {
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
            }
        }
        let closed_by_break = matches!(
            p.segments.last(),
            Some(Added(t) | Removed(t)) if t == BREAK
        );
        if i + 1 < diff.paragraphs.len() && !closed_by_break {
            old.push_str(BREAK);
            new.push_str(BREAK);
        }
    }
    (old, new)
}

fn full(old: &str, new: &str, tok: Tokeniser) -> WordDiff {
    diff_words_with(old, new, tok, &WordDiffOptions::keep_everything())
}

struct Case<'a> {
    old: &'a str,
    new: &'a str,
    segments: Vec<Segment>,
    added: usize,
    removed: usize,
    markup: &'a str,
}

/// One changed paragraph numbered 1.
fn check_single(tok: Tokeniser, cases: &[Case<'_>]) {
    for c in cases {
        let d = diff_words(c.old, c.new, tok);
        let ctx = format!("{:?} -> {:?}", c.old, c.new);
        assert_eq!(d.paragraphs, vec![para(1, c.segments.clone())], "{ctx}");
        assert_eq!((d.added, d.removed), (c.added, c.removed), "{ctx}");
        assert!(!d.layout_only, "{ctx}");
        assert_eq!(to_word_markup(&d), c.markup, "{ctx}");
    }
}

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
    assert_eq!(rebuild(&d), ("A\n\nB".to_owned(), "A\n\nB".to_owned()));
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
    assert_eq!(rebuild(&d), ("A".to_owned(), "A\n\nB".to_owned()));
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
    assert_eq!(rebuild(&d), ("A\n\nB\n\nC".to_owned(), "A\n\nC".to_owned()));
}

#[test]
fn rebuild_of_nothing_is_empty() {
    let d = WordDiff {
        paragraphs: vec![],
        added: 0,
        removed: 0,
        layout_only: false,
    };
    assert_eq!(rebuild(&d), (String::new(), String::new()));
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
        rebuild(&full(old, new, GEN)),
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
    let expected = format!("start {} … {} ", run("w", 1, 9), run("w", 31, 40));
    assert_eq!(
        d.paragraphs,
        vec![para(1, vec![same(&expected), removed("end"), added("fin")])]
    );
    assert_eq!((d.added, d.removed), (1, 1));
}

#[test]
fn a_long_unchanged_run_keeps_its_first_and_last_ten_words() {
    let (d, _) = long_run_diff(51);
    let expected = format!("start {} … {} ", run("w", 1, 9), run("w", 41, 50));
    assert_eq!(
        d.paragraphs[0].segments,
        vec![same(&expected), removed("end"), added("fin")]
    );
    assert_eq!(
        to_word_markup(&d),
        format!("¶ 1\n{expected}[-end-]{{+fin+}}\n")
    );
}

#[test]
fn runs_on_both_sides_of_a_change_are_cut_separately() {
    let old = format!("{} X {}", run("r", 1, 45), run("s", 1, 45));
    let new = format!("{} Y {}", run("r", 1, 45), run("s", 1, 45));
    let d = diff_words(&old, &new, GEN);
    let before = format!("{} … {} ", run("r", 1, 10), run("r", 36, 45));
    let after = format!(" {} … {}", run("s", 1, 10), run("s", 36, 45));
    assert_eq!(
        d.paragraphs[0].segments,
        vec![same(&before), removed("X"), added("Y"), same(&after)]
    );
}

#[test]
fn a_cut_never_splits_a_token() {
    // 50 Hangul syllables are 50 one-grapheme words with no space between.
    let body = hangul(0, 50);
    let d = diff_words(&format!("{body} end"), &format!("{body} fin"), GEN);
    let expected = format!("{} … {} ", hangul(0, 10), hangul(40, 50));
    assert_eq!(
        d.paragraphs[0].segments,
        vec![same(&expected), removed("end"), added("fin")]
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

#[test]
#[ignore = "timing check, run with: cargo test --release -p paper-diff -- --ignored"]
fn a_one_megabyte_file_with_scattered_edits_diffs_in_well_under_a_second() {
    let paragraph = format!("{}\n\n", run("word", 1, 120));
    let old = paragraph.repeat(1_000_000 / paragraph.len());
    let new = old.replace("word60 ", "changed ");
    let start = std::time::Instant::now();
    let d = diff_words(&old, &new, GEN);
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert!(!d.paragraphs.is_empty());
}

// ------------------------------------------------------------ proptests

fn paragraph_text() -> impl Strategy<Value = String> {
    proptest::collection::vec("[a-d]{2}", 1..6).prop_map(|words| words.join(" "))
}

/// Paragraphs of single-spaced words joined by exactly one blank line, so
/// that no unchanged spacing differs between the two sides.
fn document() -> impl Strategy<Value = String> {
    proptest::collection::vec(paragraph_text(), 0..4).prop_map(|ps| ps.join(BREAK))
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn same_plus_removed_is_the_old_text_and_same_plus_added_is_the_new_text(
        old in document(), new in document(),
    ) {
        for tok in [GEN, TEX] {
            let d = full(&old, &new, tok);
            prop_assert_eq!(rebuild(&d), (old.clone(), new.clone()));
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

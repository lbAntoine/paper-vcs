//! Behaviour of the word tokenisers (PV-040).
//!
//! Where UAX #29 decides, the expectations were taken from a scratch run of
//! `unicode-segmentation` 1.11.0 `split_word_bounds`.

use paper_diff::TokenKind::{Command as C, Comment as Cm, Punct as P, Space as S, Word as W};
use paper_diff::{Token, TokenKind, Tokeniser, tokenise, tokeniser_for};
use proptest::prelude::*;

fn pairs(text: &str, tokeniser: Tokeniser) -> Vec<(&str, TokenKind)> {
    tokenise(text, tokeniser)
        .into_iter()
        .map(|Token { text, kind }| (text, kind))
        .collect()
}

fn joined(text: &str, tokeniser: Tokeniser) -> String {
    tokenise(text, tokeniser)
        .iter()
        .map(|t| t.text)
        .collect::<String>()
}

fn check_table(tokeniser: Tokeniser, table: &[(&str, Vec<(&str, TokenKind)>)]) {
    for (input, expected) in table {
        assert_eq!(&pairs(input, tokeniser), expected, "input {input:?}");
    }
}

// --- helper sanity -------------------------------------------------------

#[test]
fn the_pairs_helper_reports_text_and_kind_in_order() {
    // Uses only types, so it holds once tokenise works: a single space.
    assert_eq!(pairs(" ", Tokeniser::Generic), vec![(" ", S)]);
}

#[test]
fn the_joined_helper_concatenates_token_texts() {
    assert_eq!(joined("a b", Tokeniser::Generic), "a b");
}

// --- generic -------------------------------------------------------------

#[test]
fn empty_input_gives_no_tokens() {
    assert!(tokenise("", Tokeniser::Generic).is_empty());
    assert!(tokenise("", Tokeniser::Latex).is_empty());
}

#[test]
fn generic_separates_words_spaces_and_punctuation() {
    check_table(
        Tokeniser::Generic,
        &[
            ("hello world", vec![("hello", W), (" ", S), ("world", W)]),
            (
                "Hi, you!",
                vec![("Hi", W), (",", P), (" ", S), ("you", W), ("!", P)],
            ),
            ("a...", vec![("a", W), (".", P), (".", P), (".", P)]),
            ("$5", vec![("$", P), ("5", W)]),
            ("(x)", vec![("(", P), ("x", W), (")", P)]),
        ],
    );
}

#[test]
fn generic_whitespace_runs_are_one_space_token_with_line_breaks_included() {
    check_table(
        Tokeniser::Generic,
        &[
            ("a  b", vec![("a", W), ("  ", S), ("b", W)]),
            ("a\n\nb", vec![("a", W), ("\n\n", S), ("b", W)]),
            ("a \t\r\n b", vec![("a", W), (" \t\r\n ", S), ("b", W)]),
            ("  a", vec![("  ", S), ("a", W)]),
            ("a\n", vec![("a", W), ("\n", S)]),
        ],
    );
}

#[test]
fn generic_numbers_keep_inner_separators_per_uax29() {
    check_table(
        Tokeniser::Generic,
        &[
            ("3.14", vec![("3.14", W)]),
            ("1,000", vec![("1,000", W)]),
            ("50%", vec![("50", W), ("%", P)]),
            ("v0.3", vec![("v0.3", W)]),
        ],
    );
}

#[test]
fn generic_contractions_and_apostrophes_follow_uax29() {
    check_table(
        Tokeniser::Generic,
        &[
            ("don't", vec![("don't", W)]),
            ("can\u{2019}t", vec![("can\u{2019}t", W)]),
            (
                "a 'b'",
                vec![("a", W), (" ", S), ("'", P), ("b", W), ("'", P)],
            ),
        ],
    );
}

#[test]
fn generic_hyphens_split_words_and_underscores_join_them() {
    check_table(
        Tokeniser::Generic,
        &[
            ("e-mail", vec![("e", W), ("-", P), ("mail", W)]),
            ("snake_case", vec![("snake_case", W)]),
        ],
    );
}

#[test]
fn generic_does_not_treat_latex_syntax_specially() {
    check_table(
        Tokeniser::Generic,
        &[
            ("\\section", vec![("\\", P), ("section", W)]),
            ("% note", vec![("%", P), (" ", S), ("note", W)]),
        ],
    );
}

// --- CJK -----------------------------------------------------------------

#[test]
fn korean_is_split_per_syllable() {
    // 안녕하세요
    let text = "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}";
    let expected: Vec<(&str, TokenKind)> = vec![
        ("\u{c548}", W),
        ("\u{b155}", W),
        ("\u{d558}", W),
        ("\u{c138}", W),
        ("\u{c694}", W),
    ];
    assert_eq!(pairs(text, Tokeniser::Generic), expected);
}

#[test]
fn japanese_kana_and_kanji_are_split_per_character() {
    // 日本語のテスト: kanji, hiragana and katakana (UAX #29 alone keeps the katakana run whole)
    let text = "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30b9}\u{30c8}";
    let expected: Vec<(&str, TokenKind)> = vec![
        ("\u{65e5}", W),
        ("\u{672c}", W),
        ("\u{8a9e}", W),
        ("\u{306e}", W),
        ("\u{30c6}", W),
        ("\u{30b9}", W),
        ("\u{30c8}", W),
    ];
    assert_eq!(pairs(text, Tokeniser::Generic), expected);
}

#[test]
fn chinese_is_split_per_character() {
    // 你好世界
    let text = "\u{4f60}\u{597d}\u{4e16}\u{754c}";
    let got = pairs(text, Tokeniser::Generic);
    assert_eq!(got.len(), 4);
    assert!(got.iter().all(|&(t, k)| k == W && t.chars().count() == 1));
}

#[test]
fn mixed_latin_and_hangul_splits_at_the_script_change() {
    // v0.3에서: UAX #29 returns one segment, the script change splits it
    let text = "v0.3\u{c5d0}\u{c11c}";
    let expected: Vec<(&str, TokenKind)> = vec![("v0.3", W), ("\u{c5d0}", W), ("\u{c11c}", W)];
    assert_eq!(pairs(text, Tokeniser::Generic), expected);
}

#[test]
fn cjk_next_to_spaces_and_punctuation_keeps_their_kinds() {
    // 안녕, world
    let text = "\u{c548}\u{b155}, world";
    let expected: Vec<(&str, TokenKind)> = vec![
        ("\u{c548}", W),
        ("\u{b155}", W),
        (",", P),
        (" ", S),
        ("world", W),
    ];
    assert_eq!(pairs(text, Tokeniser::Generic), expected);
}

#[test]
fn conjoining_jamo_forming_one_syllable_stay_one_token() {
    // U+1112 U+1161 U+11AB is one extended grapheme cluster (한 in decomposed form)
    let decomposed = "\u{1112}\u{1161}\u{11ab}";
    assert_eq!(pairs(decomposed, Tokeniser::Generic), vec![(decomposed, W)]);
    // two such syllables are two tokens
    let two = "\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}";
    assert_eq!(
        pairs(two, Tokeniser::Generic),
        vec![
            ("\u{1112}\u{1161}\u{11ab}", W),
            ("\u{1100}\u{1173}\u{11af}", W)
        ]
    );
}

#[test]
fn a_combining_mark_stays_with_its_cjk_base() {
    // 日 followed by U+3099 (combining voiced mark), then 本
    let expected: Vec<(&str, TokenKind)> = vec![("\u{65e5}\u{3099}", W), ("\u{672c}", W)];
    assert_eq!(
        pairs("\u{65e5}\u{3099}\u{672c}", Tokeniser::Generic),
        expected
    );
}

#[test]
fn a_combining_mark_on_a_latin_letter_stays_in_the_word() {
    // e + U+0301 + a
    let text = "e\u{301}a";
    assert_eq!(pairs(text, Tokeniser::Generic), vec![(text, W)]);
}

#[test]
fn cjk_splitting_also_applies_in_latex() {
    // 안녕 \alpha
    let expected: Vec<(&str, TokenKind)> =
        vec![("\u{c548}", W), ("\u{b155}", W), (" ", S), ("\\alpha", C)];
    assert_eq!(
        pairs("\u{c548}\u{b155} \\alpha", Tokeniser::Latex),
        expected
    );
}

// --- LaTeX ---------------------------------------------------------------

#[test]
fn latex_commands_are_single_tokens() {
    check_table(
        Tokeniser::Latex,
        &[
            (
                "\\section*{Intro}",
                vec![("\\section*", C), ("{", P), ("Intro", W), ("}", P)],
            ),
            (
                "\\section{A}",
                vec![("\\section", C), ("{", P), ("A", W), ("}", P)],
            ),
            ("\\alpha1", vec![("\\alpha", C), ("1", W)]),
            ("\\textbf x", vec![("\\textbf", C), (" ", S), ("x", W)]),
            ("\\section *", vec![("\\section", C), (" ", S), ("*", P)]),
        ],
    );
}

#[test]
fn latex_backslash_plus_one_non_letter_is_one_command() {
    check_table(
        Tokeniser::Latex,
        &[
            ("\\\\", vec![("\\\\", C)]),
            ("\\%", vec![("\\%", C)]),
            ("\\{x\\}", vec![("\\{", C), ("x", W), ("\\}", C)]),
            ("a\\\\b", vec![("a", W), ("\\\\", C), ("b", W)]),
        ],
    );
}

#[test]
fn latex_comments_run_to_the_end_of_the_line_without_the_line_break() {
    check_table(
        Tokeniser::Latex,
        &[
            ("% comment", vec![("% comment", Cm)]),
            (
                "a % c\nb",
                vec![("a", W), (" ", S), ("% c", Cm), ("\n", S), ("b", W)],
            ),
            ("% a\n% b", vec![("% a", Cm), ("\n", S), ("% b", Cm)]),
            ("%\nx", vec![("%", Cm), ("\n", S), ("x", W)]),
        ],
    );
}

#[test]
fn latex_comment_text_is_not_tokenised_further() {
    assert_eq!(
        pairs("% \\foo {bar} 3.14", Tokeniser::Latex),
        vec![("% \\foo {bar} 3.14", Cm)]
    );
}

#[test]
fn latex_escaped_percent_is_not_a_comment() {
    check_table(
        Tokeniser::Latex,
        &[
            (
                "50\\% done",
                vec![("50", W), ("\\%", C), (" ", S), ("done", W)],
            ),
            (
                "100\\% % real comment",
                vec![("100", W), ("\\%", C), (" ", S), ("% real comment", Cm)],
            ),
        ],
    );
}

#[test]
fn latex_comment_at_the_end_of_the_file_without_a_newline() {
    check_table(
        Tokeniser::Latex,
        &[
            ("x % end", vec![("x", W), (" ", S), ("% end", Cm)]),
            (
                "x % end\n",
                vec![("x", W), (" ", S), ("% end", Cm), ("\n", S)],
            ),
        ],
    );
}

#[test]
fn latex_prose_between_commands_uses_the_generic_rules() {
    assert_eq!(
        pairs("We don't use \\cite{a}.", Tokeniser::Latex),
        vec![
            ("We", W),
            (" ", S),
            ("don't", W),
            (" ", S),
            ("use", W),
            (" ", S),
            ("\\cite", C),
            ("{", P),
            ("a", W),
            ("}", P),
            (".", P),
        ]
    );
}

// --- tokeniser_for -------------------------------------------------------

#[test]
fn tokeniser_for_picks_latex_by_extension() {
    for path in [
        "paper.tex",
        "refs.bib",
        "style.sty",
        "article.cls",
        "dir/sub/a.tex",
    ] {
        assert_eq!(tokeniser_for(path), Tokeniser::Latex, "{path}");
    }
}

#[test]
fn tokeniser_for_ignores_extension_case() {
    for path in ["PAPER.TEX", "a.Tex", "refs.BIB", "x.StY", "y.CLS"] {
        assert_eq!(tokeniser_for(path), Tokeniser::Latex, "{path}");
    }
}

#[test]
fn tokeniser_for_defaults_to_generic() {
    for path in [
        "notes.md",
        "paper.typ",
        "README",
        "a.txt",
        "tex",
        "a.tex.bak",
        "a.texx",
        "",
    ] {
        assert_eq!(tokeniser_for(path), Tokeniser::Generic, "{path}");
    }
}

// --- round trip ----------------------------------------------------------

#[test]
fn tricky_strings_round_trip_byte_for_byte() {
    let tricky = [
        "",
        " ",
        "\n",
        "\r\n",
        "\r",
        "a\r\nb\rc\nd",
        "\\",
        "\\\\\\",
        "%",
        "\\%",
        "% c\r\nx",
        "\\section*",
        "\\section**",
        "\\\u{e9}",
        "\\\u{1f600}",
        "\u{1f600}\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
        "\u{1f1eb}\u{1f1f7}\u{1f1ef}\u{1f1f5}",
        "e\u{301}\u{301}\u{301}",
        "\u{301}",
        "\u{200d}",
        "\u{feff}a",
        "\u{1112}\u{1161}\u{11ab}",
        "\u{1161}\u{11ab}",
        "\u{c548}\u{b155}v0.3\u{c5d0}\u{c11c}",
        "\u{65e5}\u{3099}\u{672c}",
        "\u{30c6}\u{30b9}\u{30c8}\u{30fc}",
        "\u{4f60}\u{597d}, \u{4e16}\u{754c}!",
        "50\\% done % c",
        "don't can\u{2019}t 3.14 1,000",
        "\0\u{7f}\t\u{b}\u{c}\u{2028}\u{2029}\u{85}\u{a0}\u{3000}",
    ];
    for tokeniser in [Tokeniser::Generic, Tokeniser::Latex] {
        for text in tricky {
            assert_eq!(joined(text, tokeniser), text, "{tokeniser:?} {text:?}");
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn arbitrary_strings_round_trip(text in any::<String>()) {
        for tokeniser in [Tokeniser::Generic, Tokeniser::Latex] {
            prop_assert_eq!(joined(&text, tokeniser), text.clone());
        }
    }

    #[test]
    fn latex_like_cjk_and_emoji_text_round_trips(
        text in "[\\\\%a-z0-9{}*_.,' \n\r\t\u{1100}-\u{11ff}\u{3099}\u{300}-\u{305}\u{3040}-\u{3050}\u{30a0}-\u{30b0}\u{4e00}-\u{4e20}\u{ac00}-\u{ac10}\u{1f600}\u{200d}\u{2019}]{0,80}"
    ) {
        for tokeniser in [Tokeniser::Generic, Tokeniser::Latex] {
            prop_assert_eq!(joined(&text, tokeniser), text.clone());
        }
    }
}

// --- coordinator-requested additions (open cases in PV-040) --------------

#[test]
fn latex_open_cases_follow_the_coordinator_decisions() {
    check_table(
        Tokeniser::Latex,
        &[
            ("\\ x", vec![("\\ ", C), ("x", W)]),
            ("a\\", vec![("a", W), ("\\", P)]),
            ("\\section**", vec![("\\section*", C), ("*", P)]),
            ("% c\r\nx", vec![("% c", Cm), ("\r\n", S), ("x", W)]),
        ],
    );
}

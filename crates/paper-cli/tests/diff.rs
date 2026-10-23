//! `paper diff`: points and paths, unified output, `--stat`, JSON, and a cross-check with
//! `git apply`.

mod common;

use common::{Env, assert_keys, binary, settings};

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

const INTRO_V1_TO_V2: &str =
    "--- a/sections/intro.tex\n+++ b/sections/intro.tex\n@@ -1 +1 @@\n-Intro v1\n+Intro v2\n";
const RELATED_ADDED: &str = "--- /dev/null\n+++ b/related.tex\n@@ -0,0 +1 @@\n+Related work\n";

#[test]
fn diff_shows_the_unsaved_changes_as_a_unified_diff() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\nMore text\n");

    let run = env.paper(&["diff", "--lines"]);

    run.assert_code(0);
    assert_eq!(
        run.stdout,
        "--- a/sections/intro.tex\n+++ b/sections/intro.tex\n@@ -1 +1,2 @@\n-Intro v2\n+Intro v3\n+More text\n"
    );
    settings().bind(|| {
        insta::assert_snapshot!("diff_draft", env.redact(&run.stdout));
    });
}

#[test]
fn diff_with_no_differences_prints_nothing() {
    let env = project();
    let run = env.paper(&["diff"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "");
    assert_eq!(run.stderr, "");
}

#[test]
fn diff_from_a_point_compares_it_with_the_draft() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let run = env.paper(&["diff", "--lines", "last~2"]);

    run.assert_code(0);
    assert!(run.stdout.contains(INTRO_V1_TO_V2), "{run:#?}");
    assert!(run.stdout.contains(RELATED_ADDED), "{run:#?}");
    assert!(run.stdout.contains("+++ b/main.tex"), "{run:#?}");
}

#[test]
fn diff_between_two_points_compares_them_and_ignores_the_draft() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let run = env.paper(&["diff", "--lines", "last~2", "last"]);

    run.assert_code(0);
    assert!(run.stdout.contains(INTRO_V1_TO_V2), "{run:#?}");
    assert!(run.stdout.contains(RELATED_ADDED), "{run:#?}");
    assert!(!run.stdout.contains("main.tex"), "{run:#?}");
}

#[test]
fn diff_since_a_point_is_the_same_as_that_point_against_the_draft() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let since = env.paper(&["diff", "--since", "last~2"]);
    let explicit = env.paper(&["diff", "last~2", "@"]);

    since.assert_code(0);
    assert!(!since.stdout.is_empty(), "{since:#?}");
    assert_eq!(since.stdout, explicit.stdout);
}

#[test]
fn diff_since_together_with_a_point_is_a_user_error() {
    let env = project();
    let run = env.paper(&["diff", "--since", "last~2", "last"]);
    run.assert_three_part_error(1);
}

#[test]
fn paths_after_the_points_restrict_the_diff() {
    let env = project();

    let run = env.paper(&["diff", "--lines", "last~2", "last", "sections/intro.tex"]);

    run.assert_code(0);
    assert_eq!(run.stdout, INTRO_V1_TO_V2);
}

#[test]
fn a_path_with_no_points_before_it_is_a_path() {
    let env = project();
    env.write("main.tex", "changed main\n");
    env.write("related.tex", "changed related\n");

    let run = env.paper(&["diff", "--lines", "main.tex"]);

    run.assert_code(0);
    assert!(run.stdout.contains("+++ b/main.tex"), "{run:#?}");
    assert!(!run.stdout.contains("related.tex"), "{run:#?}");
}

#[test]
fn a_folder_path_selects_everything_under_it() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");
    env.write("sections/method.tex", "Method\n");
    env.write("main.tex", "changed main\n");

    let run = env.paper(&["diff", "--lines", "sections"]);

    run.assert_code(0);
    assert!(run.stdout.contains("+++ b/sections/intro.tex"), "{run:#?}");
    assert!(run.stdout.contains("+++ b/sections/method.tex"), "{run:#?}");
    assert!(!run.stdout.contains("main.tex"), "{run:#?}");
}

#[test]
fn double_dash_makes_everything_after_it_a_path() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let run = env.paper(&[
        "diff",
        "--lines",
        "last~2",
        "last",
        "--",
        "sections/intro.tex",
    ]);

    run.assert_code(0);
    assert_eq!(run.stdout, INTRO_V1_TO_V2);
}

#[test]
fn a_name_that_is_both_a_point_and_a_file_is_a_point_without_double_dash() {
    let env = project();
    env.write("last", "a file named like a point\n");
    env.write("main.tex", "changed main\n");

    let as_point = env.paper(&["diff", "--lines", "last"]);
    let as_path = env.paper(&["diff", "--lines", "--", "last"]);

    as_point.assert_code(0);
    assert!(as_point.stdout.contains("+++ b/main.tex"), "{as_point:#?}");
    assert!(as_point.stdout.contains("+++ b/last"), "{as_point:#?}");
    as_path.assert_code(0);
    assert!(as_path.stdout.contains("+++ b/last"), "{as_path:#?}");
    assert!(!as_path.stdout.contains("main.tex"), "{as_path:#?}");
}

#[test]
fn paths_are_relative_to_where_paper_runs() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");
    env.write("main.tex", "changed main\n");

    let run = env.paper_in(
        &env.work.join("sections"),
        &[],
        &["diff", "--lines", "intro.tex"],
    );

    run.assert_code(0);
    assert!(run.stdout.contains("--- a/sections/intro.tex"), "{run:#?}");
    assert!(run.stdout.contains("+++ b/sections/intro.tex"), "{run:#?}");
    assert!(!run.stdout.contains("main.tex"), "{run:#?}");
}

#[test]
fn diff_of_an_unknown_path_is_a_user_error_naming_it() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let run = env.paper(&["diff", "mian.tex"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("mian.tex"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("diff_unknown_path", env.redact(&run.stderr));
    });
}

#[test]
fn diff_of_an_unknown_path_after_double_dash_is_a_user_error_too() {
    let env = project();
    let run = env.paper(&["diff", "--", "nope"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("nope"), "{run:#?}");
}

#[test]
fn diff_shows_added_and_deleted_files_against_dev_null() {
    let env = project();
    env.write("new.txt", "fresh\n");
    std::fs::remove_file(env.work.join("related.tex")).unwrap();

    let run = env.paper(&["diff", "--lines"]);

    run.assert_code(0);
    assert!(
        run.stdout
            .contains("--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+fresh\n"),
        "{run:#?}"
    );
    assert!(
        run.stdout
            .contains("--- a/related.tex\n+++ /dev/null\n@@ -1 +0,0 @@\n-Related work\n"),
        "{run:#?}"
    );
}

#[test]
fn diff_output_applies_in_reverse_with_git() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\nMore text\n");
    env.write("new.txt", "fresh\nfile\n");
    std::fs::remove_file(env.work.join("related.tex")).unwrap();

    let run = env.paper(&["diff", "--lines"]);

    run.assert_code(0);
    let patch = env.root.join("draft.patch");
    std::fs::write(&patch, &run.stdout).unwrap();
    env.git_work(&["apply", "--check", "-R", patch.to_str().unwrap()]);
}

#[test]
fn diff_between_saves_applies_in_reverse_on_that_tree() {
    let env = project();
    let run = env.paper(&["diff", "--lines", "last~2", "last"]);
    run.assert_code(0);
    let patch = env.root.join("saves.patch");
    std::fs::write(&patch, &run.stdout).unwrap();
    // The working files are the `last` side (the draft is empty).
    env.git_work(&["apply", "--check", "-R", patch.to_str().unwrap()]);
}

#[test]
fn a_binary_file_prints_a_one_line_size_summary() {
    let env = project();
    env.write_bytes("figures/plot.bin", &binary(2048, 7));
    env.paper(&["save", "Add plot"]).assert_code(0);
    env.write_bytes("figures/plot.bin", &binary(4096, 9));

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    let line = run
        .stdout
        .lines()
        .find(|line| line.contains("figures/plot.bin"))
        .unwrap_or_else(|| panic!("{run:#?}"));
    assert!(line.contains("changed"), "{run:#?}");
    assert!(line.contains("2 KB \u{2192} 4 KB"), "{run:#?}");
    assert_eq!(run.stdout.lines().count(), 1, "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("diff_binary", env.redact(&run.stdout));
    });
}

#[test]
fn stat_prints_one_line_per_file_and_a_total() {
    let env = project();

    let run = env.paper(&["diff", "--stat", "--lines", "last~2", "last"]);

    run.assert_code(0);
    assert!(!run.stdout.contains("@@"), "{run:#?}");
    let line_for = |needle: &str| {
        run.stdout
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no line for {needle}: {run:#?}"))
    };
    assert!(line_for("related.tex").contains("+1 \u{2212}0"), "{run:#?}");
    assert!(
        line_for("sections/intro.tex").contains("+1 \u{2212}1"),
        "{run:#?}"
    );
    assert!(run.stdout.contains("2 files"), "total: {run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("diff_stat", env.redact(&run.stdout));
    });
}

#[test]
fn stat_shows_binary_sizes() {
    let env = project();
    env.write_bytes("figures/plot.bin", &binary(2048, 7));

    let run = env.paper(&["diff", "--stat"]);

    run.assert_code(0);
    assert!(run.stdout.contains("figures/plot.bin"), "{run:#?}");
    assert!(run.stdout.contains("2 KB"), "{run:#?}");
}

#[test]
fn colour_is_used_only_when_asked_for() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");

    let plain = env.paper(&["diff", "--lines"]);
    let coloured = env.paper(&["--color", "always", "diff", "--lines"]);

    assert!(!plain.stdout.contains('\x1b'), "{plain:#?}");
    assert!(
        coloured.stdout.contains("\x1b[31m-Intro v2"),
        "{coloured:#?}"
    );
    assert!(
        coloured.stdout.contains("\x1b[32m+Intro v3"),
        "{coloured:#?}"
    );
}

#[test]
fn diff_snapshots_first_and_with_no_snapshot_does_not() {
    let env = project();
    env.write("edited.txt", "made outside paper\n");

    let no_snapshot = env.paper(&["--no-snapshot", "diff", "--lines"]);
    let normal = env.paper(&["diff", "--lines"]);

    no_snapshot.assert_code(0);
    assert_eq!(no_snapshot.stdout, "");
    assert!(normal.stdout.contains("+++ b/edited.txt"), "{normal:#?}");
}

#[test]
fn diff_json_has_a_fixed_key_set_for_text_and_binary_files() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\nMore text");
    env.write_bytes("figures/plot.bin", &binary(2048, 7));

    let run = env.paper(&["--json", "diff", "--lines"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_keys(&json, &["schema", "files"]);
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "{run:#?}");
    let file_keys = [
        "path",
        "kind",
        "binary",
        "added",
        "removed",
        "old_bytes",
        "new_bytes",
        "hunks",
        "unit",
        "layout_only",
        "paragraphs",
    ];
    for file in files {
        assert_keys(file, &file_keys);
    }
    assert_eq!(files[0]["path"], "figures/plot.bin");
    assert_eq!(files[0]["binary"], true);
    assert_eq!(files[0]["new_bytes"], 2048);
    assert_eq!(files[0]["hunks"], serde_json::json!([]));
    assert_eq!(files[0]["unit"], serde_json::Value::Null);
    assert_eq!(files[0]["layout_only"], false);
    assert_eq!(files[0]["paragraphs"], serde_json::Value::Null);
    let text = &files[1];
    assert_eq!(text["unit"], "lines");
    assert_eq!(text["layout_only"], false);
    assert_eq!(text["paragraphs"], serde_json::Value::Null);
    assert_eq!(text["kind"], "modified");
    assert_eq!(text["binary"], false);
    assert_eq!(
        (text["added"].clone(), text["removed"].clone()),
        (2.into(), 1.into())
    );
    let hunk = &text["hunks"][0];
    assert_keys(
        hunk,
        &["old_start", "old_len", "new_start", "new_len", "lines"],
    );
    let last_line = hunk["lines"].as_array().unwrap().last().unwrap();
    assert_keys(last_line, &["kind", "text", "missing_newline"]);
    assert_eq!(last_line["kind"], "added");
    assert_eq!(last_line["text"], "More text");
    assert_eq!(last_line["missing_newline"], true);
    settings().bind(|| {
        insta::assert_snapshot!("diff_json", env.redact_json(&run.stdout));
    });
}

#[test]
fn diff_json_with_no_differences_has_an_empty_list() {
    let env = project();
    let run = env.paper(&["--json", "diff"]);
    run.assert_code(0);
    assert_eq!(run.json()["files"], serde_json::json!([]));
}

#[test]
fn diff_stat_json_carries_the_same_files() {
    let env = project();
    env.write("main.tex", "changed main\n");
    let run = env.paper(&["--json", "diff", "--stat"]);
    run.assert_code(0);
    assert_eq!(run.json()["files"][0]["path"], "main.tex");
}

#[test]
fn git_style_points_are_errors_not_paths() {
    let env = project();
    let run = env.paper(&["diff", "HEAD~1"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("last~1"), "{run:#?}");
}

#[test]
fn a_mistyped_last_is_a_point_error() {
    let env = project();
    let run = env.paper(&["diff", "last~x"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("not a way to name a save"), "{run:#?}");
    assert!(
        run.stderr.contains("'x' is not a number of saves"),
        "{run:#?}"
    );
}

#[test]
fn an_ambiguous_short_id_is_an_error_not_a_path() {
    let env = project();
    // Change IDs use 16 letters, so 19 saves are bound to share a first letter.
    for i in 0..16 {
        env.write("notes.txt", &format!("{i}\n"));
        env.paper(&["save", &format!("Note {i}")]).assert_code(0);
    }
    let ids: Vec<String> = (0..19)
        .map(|i| env.change_id(&format!("HEAD~{i}")))
        .collect();
    let letter = ids
        .iter()
        .map(|id| &id[..1])
        .find(|l| ids.iter().filter(|id| id.starts_with(l)).count() > 1)
        .unwrap();

    let run = env.paper(&["diff", letter]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("could mean several saves"), "{run:#?}");
}

#[test]
fn a_mistyped_word_in_a_point_position_says_it_is_not_a_save_line_date_or_file() {
    let env = project();
    let run = env.paper(&["diff", "lsat"]);
    run.assert_three_part_error(1);
    assert!(
        run.stderr
            .contains("'lsat' is not a save, line, date or file"),
        "{run:#?}"
    );
    assert!(run.stderr.contains("paper history"), "{run:#?}");
}

#[test]
fn dot_does_not_hide_an_unknown_path_next_to_it() {
    let env = project();
    let run = env.paper(&["diff", "--", ".", "nope"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("nope"), "{run:#?}");
}

#[test]
fn a_mistyped_file_path_gets_the_file_wording_not_the_point_wording() {
    let env = project();

    let run = env.paper(&["diff", "sections/nope.tex"]);

    run.assert_three_part_error(1);
    assert!(
        run.stderr.contains("No file matches: sections/nope.tex"),
        "{run:#?}"
    );
    assert!(
        !run.stderr.contains("is not a save, line, date or file"),
        "{run:#?}"
    );
    assert!(run.stderr.contains("paper status"), "{run:#?}");
    assert!(run.stderr.contains("paper show <point>"), "{run:#?}");
}

#[test]
fn a_mistyped_word_that_could_be_a_point_keeps_the_point_wording() {
    let env = project();

    let run = env.paper(&["diff", "lsat", "sections/intro.tex"]);

    run.assert_three_part_error(1);
    assert!(
        run.stderr
            .contains("'lsat' is not a save, line, date or file"),
        "{run:#?}"
    );
    assert!(run.stderr.contains("paper history"), "{run:#?}");
}

// ---- words by default (PV-042) ----

/// Counts use a true minus sign: write them with `-` and convert.
fn minus(text: &str) -> String {
    text.replace('-', "\u{2212}")
}

#[test]
fn the_minus_helper_writes_a_true_minus_sign() {
    assert_eq!(minus("+1 word -1 word"), "+1 word \u{2212}1 word");
}

fn word_project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

#[test]
fn diff_compares_text_word_by_word_by_default() {
    let env = word_project();

    let run = env.paper(&["diff", "last~2", "last", "sections/intro.tex"]);

    run.assert_code(0);
    assert_eq!(
        run.stdout,
        format!(
            "sections/intro.tex  {}\n  \u{b6} 1\n  Intro [-v1-]{{+v2+}}\n",
            minus("+1 word -1 word")
        ),
        "{run:#?}"
    );
    assert!(!run.stdout.contains("@@"), "{run:#?}");
    assert!(!run.stdout.contains("--- a/"), "{run:#?}");
}

#[test]
fn diff_counts_the_singular_and_the_plural() {
    let env = word_project();

    let run = env.paper(&["diff", "last~2", "last"]);

    run.assert_code(0);
    // related.tex is new: "Related work" is two words added, none removed.
    assert!(
        run.stdout
            .contains(&minus("related.tex  +2 words -0 words\n")),
        "{run:#?}"
    );
    assert!(
        run.stdout
            .contains(&minus("sections/intro.tex  +1 word -1 word\n")),
        "{run:#?}"
    );
}

#[test]
fn files_are_in_path_order_with_one_blank_line_between_them() {
    let env = word_project();

    let run = env.paper(&["diff", "last~2", "last"]);

    run.assert_code(0);
    let related = run.stdout.find("related.tex  ").unwrap();
    let intro = run.stdout.find("sections/intro.tex  ").unwrap();
    assert!(related < intro, "{run:#?}");
    assert!(
        run.stdout[related..intro].ends_with("\n\n"),
        "one blank line between files: {run:#?}"
    );
    assert!(!run.stdout[related..intro].ends_with("\n\n\n"), "{run:#?}");
    assert!(!run.stdout.ends_with("\n\n"), "{run:#?}");
}

#[test]
fn an_edit_inside_a_longer_paragraph_is_numbered_and_marked_up() {
    let env = word_project();
    env.write(
        "essay.txt",
        "First paragraph is calm.\n\nSecond paragraph is calm.\n\nThird paragraph is calm.\n",
    );
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write(
        "essay.txt",
        "First paragraph is calm.\n\nSecond paragraph is stormy.\n\nThird paragraph is calm.\n",
    );

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains(&minus("essay.txt  +1 word -1 word\n")),
        "{run:#?}"
    );
    assert!(run.stdout.contains("\n  \u{b6} 2\n"), "{run:#?}");
    assert!(
        run.stdout
            .contains("  Second paragraph is [-calm-]{+stormy+}"),
        "{run:#?}"
    );
    assert!(!run.stdout.contains("First paragraph"), "{run:#?}");
    assert!(!run.stdout.contains("Third paragraph"), "{run:#?}");
}

#[test]
fn an_added_file_is_words_added_to_nothing_and_a_deleted_file_words_removed() {
    let env = word_project();
    env.write("new.txt", "fresh\n");
    std::fs::remove_file(env.work.join("related.tex")).unwrap();

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains(&minus("new.txt  +1 word -0 words\n")),
        "{run:#?}"
    );
    assert!(run.stdout.contains("{+fresh+}"), "{run:#?}");
    assert!(
        run.stdout
            .contains(&minus("related.tex  +0 words -2 words\n")),
        "{run:#?}"
    );
    assert!(run.stdout.contains("[-Related work-]"), "{run:#?}");
    assert!(!run.stdout.contains("/dev/null"), "{run:#?}");
}

#[test]
fn a_paragraph_added_or_removed_shows_the_break_marker() {
    let env = word_project();
    env.write("essay.txt", "Alpha beta.\n\nGamma delta.\n");
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", "Alpha beta.\n");

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert!(run.stdout.contains("[-\u{b6}-]"), "{run:#?}");
    assert!(run.stdout.contains("Gamma delta"), "{run:#?}");
}

#[test]
fn a_rewrap_is_reported_as_spacing_and_line_breaks_only() {
    let env = word_project();
    env.write("essay.txt", "one two three\nfour five\n");
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", "one two\nthree  four five\n");

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert_eq!(
        run.stdout, "essay.txt  spacing and line breaks only\n",
        "{run:#?}"
    );
}

#[test]
fn a_long_unchanged_run_is_cut_with_an_ellipsis() {
    let env = word_project();
    let body: String = (0..120)
        .map(|i| ["w", &i.to_string(), " "].concat())
        .collect();
    env.write("essay.txt", &format!("first {body}last\n"));
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", &format!("FIRST {body}LAST\n"));

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert!(run.stdout.contains(" \u{2026} "), "{run:#?}");
    assert!(run.stdout.contains("[-first-]{+FIRST+}"), "{run:#?}");
    assert!(run.stdout.contains("[-last-]{+LAST+}"), "{run:#?}");
    assert!(!run.stdout.contains("w60"), "the middle is cut: {run:#?}");
}

#[test]
fn lines_gives_the_unified_diff_byte_for_byte() {
    let env = word_project();
    env.write("sections/intro.tex", "Intro v3\nMore text\n");

    let run = env.paper(&["diff", "--lines"]);

    run.assert_code(0);
    assert_eq!(
        run.stdout,
        "--- a/sections/intro.tex\n+++ b/sections/intro.tex\n@@ -1 +1,2 @@\n-Intro v2\n+Intro v3\n+More text\n"
    );
}

#[test]
fn words_flag_is_accepted_and_is_the_default() {
    let env = word_project();
    env.write("main.tex", "changed main\n");

    let default = env.paper(&["diff"]);
    let words = env.paper(&["diff", "--words"]);

    words.assert_code(0);
    assert!(!words.stdout.is_empty(), "{words:#?}");
    assert!(words.stdout.contains("main.tex  +"), "{words:#?}");
    assert!(!words.stdout.contains("@@"), "{words:#?}");
    assert_eq!(words.stdout, default.stdout);
}

#[test]
fn lines_and_words_together_are_refused() {
    let env = word_project();
    let run = env.paper(&["diff", "--lines", "--words"]);
    assert_ne!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout, "", "{run:#?}");
    assert!(!run.stderr.is_empty(), "{run:#?}");
}

#[test]
fn diff_default_lines_in_the_settings_gives_the_unified_diff() {
    let env = word_project();
    env.paper(&["config", "set", "diff.default", "lines"])
        .assert_code(0);
    env.write("sections/intro.tex", "Intro v3\n");

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert_eq!(
        run.stdout,
        "--- a/sections/intro.tex\n+++ b/sections/intro.tex\n@@ -1 +1 @@\n-Intro v2\n+Intro v3\n"
    );
}

#[test]
fn words_flag_overrides_diff_default_lines() {
    let env = word_project();
    env.paper(&["config", "set", "diff.default", "lines"])
        .assert_code(0);
    env.write("sections/intro.tex", "Intro v3\n");

    let run = env.paper(&["diff", "--words"]);

    run.assert_code(0);
    assert!(run.stdout.contains("[-v2-]{+v3+}"), "{run:#?}");
    assert!(!run.stdout.contains("@@"), "{run:#?}");
}

#[test]
fn lines_flag_overrides_diff_default_words() {
    let env = word_project();
    env.paper(&["config", "set", "diff.default", "words"])
        .assert_code(0);
    env.write("sections/intro.tex", "Intro v3\n");

    let run = env.paper(&["diff", "--lines"]);

    run.assert_code(0);
    assert!(run.stdout.contains("@@ -1 +1 @@"), "{run:#?}");
}

#[test]
fn since_uses_words_too() {
    let env = word_project();
    env.write("main.tex", "changed main\n");

    let run = env.paper(&["diff", "--since", "last~2"]);

    run.assert_code(0);
    assert!(run.stdout.contains("related.tex  +2 words"), "{run:#?}");
    assert!(run.stdout.contains("main.tex  +"), "{run:#?}");
    assert!(!run.stdout.contains("@@"), "{run:#?}");
}

#[test]
fn a_binary_file_is_the_same_in_words_and_lines_mode() {
    let env = word_project();
    env.write_bytes("figures/plot.bin", &binary(2048, 7));
    env.paper(&["save", "Add plot"]).assert_code(0);
    env.write_bytes("figures/plot.bin", &binary(4096, 9));

    let words = env.paper(&["diff", "--words"]);
    let lines = env.paper(&["diff", "--lines"]);

    words.assert_code(0);
    assert_eq!(words.stdout, lines.stdout);
    assert!(words.stdout.contains("2 KB \u{2192} 4 KB"), "{words:#?}");
}

#[test]
fn word_colour_paints_removed_red_struck_through_and_added_green_without_markers() {
    let env = word_project();
    env.write("sections/intro.tex", "Intro v3\n");

    let plain = env.paper(&["diff"]);
    let coloured = env.paper(&["--color", "always", "diff"]);

    coloured.assert_code(0);
    assert!(!plain.stdout.contains('\x1b'), "{plain:#?}");
    assert!(plain.stdout.contains("[-v2-]{+v3+}"), "{plain:#?}");
    assert!(coloured.stdout.contains("\x1b[31"), "{coloured:#?}");
    assert!(coloured.stdout.contains("\x1b[32"), "{coloured:#?}");
    assert!(
        coloured.stdout.contains("\x1b[9m") || coloured.stdout.contains(";9m"),
        "removed text is struck through: {coloured:#?}"
    );
    assert!(!coloured.stdout.contains("[-"), "{coloured:#?}");
    assert!(!coloured.stdout.contains("{+"), "{coloured:#?}");
    assert!(coloured.stdout.contains("v2") && coloured.stdout.contains("v3"));
}

#[test]
fn a_cut_is_dim_in_colour() {
    let env = word_project();
    let body: String = (0..120)
        .map(|i| ["w", &i.to_string(), " "].concat())
        .collect();
    env.write("essay.txt", &format!("first {body}last\n"));
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", &format!("FIRST {body}LAST\n"));

    let run = env.paper(&["--color", "always", "diff"]);

    run.assert_code(0);
    assert!(run.stdout.contains("\x1b[2m \u{2026} "), "{run:#?}");
}

/// The text lines of the first paragraph of `stdout`: everything after `  ¶ N`.
fn paragraph_lines(stdout: &str) -> Vec<&str> {
    let mut lines = stdout.lines().skip_while(|l| !l.starts_with("  \u{b6} "));
    lines.next();
    lines.collect()
}

#[test]
fn paragraph_lines_skips_the_header_and_the_paragraph_marker() {
    let out = "f.txt  +1 word \u{2212}0 words\n  \u{b6} 1\n  one\n  two\n";
    assert_eq!(paragraph_lines(out), vec!["  one", "  two"]);
}

#[test]
fn text_is_wrapped_at_80_columns_when_stdout_is_not_a_terminal() {
    let env = word_project();
    for filler in 0..12 {
        let pad = "x".repeat(filler);
        let words: Vec<String> = (0..60).map(|i| format!("word{i:02}{pad}")).collect();
        let old = format!("{}.\n", words.join(" "));
        let mut changed = words.clone();
        changed[57] = "REPLACED".to_owned();
        changed[3] = "SWAPPED".to_owned();
        let new = format!("{}.\n", changed.join(" "));
        env.write("essay.txt", &old);
        env.paper(&["save", &format!("Essay {filler}")])
            .assert_code(0);
        env.write("essay.txt", &new);

        let run = env.paper(&["diff", "essay.txt"]);

        run.assert_code(0);
        let body = paragraph_lines(&run.stdout);
        assert!(body.len() >= 3, "wrapped into several lines: {run:#?}");
        for line in &body {
            assert!(line.chars().count() <= 80, "{filler}: {line:?}");
            assert!(line.starts_with("  "), "indented two spaces: {line:?}");
            assert!(!line.trim_end().ends_with("[-"), "{filler}: {line:?}");
            assert!(!line.trim_end().ends_with("{+"), "{filler}: {line:?}");
            assert!(!line.trim_start().starts_with("-]"), "{filler}: {line:?}");
            assert!(!line.trim_start().starts_with("+}"), "{filler}: {line:?}");
        }
        let joined: Vec<&str> = body.iter().map(|l| l.trim()).collect();
        let joined = joined.join(" ");
        assert!(
            joined.contains(&format!("[-word57{pad}-]{{+REPLACED+}}")),
            "{filler}: no token is broken: {joined}"
        );
        assert!(
            joined.contains(&format!("[-word03{pad}-]{{+SWAPPED+}}")),
            "{filler}: {joined}"
        );
        // Put the file back so the next round starts from a clean draft.
        env.paper(&["save", &format!("Edit {filler}")])
            .assert_code(0);
    }
}

#[test]
fn a_token_longer_than_the_width_is_never_broken() {
    let env = word_project();
    let long = "y".repeat(120);
    env.write("essay.txt", &format!("start {long} end\n"));
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", &format!("start {long} finish\n"));

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert!(run.stdout.contains(&long), "{run:#?}");
    assert!(run.stdout.contains("[-end-]{+finish+}"), "{run:#?}");
}

#[test]
fn stat_counts_words_per_file_and_in_total() {
    let env = word_project();

    let run = env.paper(&["diff", "--stat", "last~2", "last"]);

    run.assert_code(0);
    assert!(!run.stdout.contains("@@"), "{run:#?}");
    let line_for = |needle: &str| {
        run.stdout
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no line for {needle}: {run:#?}"))
    };
    assert!(
        line_for("related.tex").contains(&minus("+2 words -0 words")),
        "{run:#?}"
    );
    assert!(
        line_for("sections/intro.tex").contains(&minus("+1 word -1 word")),
        "{run:#?}"
    );
    assert!(
        run.stdout
            .contains(&minus("2 files changed, +3 words -1 word\n")),
        "total: {run:#?}"
    );
    assert!(!run.stdout.contains("¶"), "{run:#?}");
}

#[test]
fn stat_keeps_the_sizes_line_of_a_binary_file() {
    let env = word_project();
    env.write_bytes("figures/plot.bin", &binary(2048, 7));
    env.write("main.tex", "changed main words\n");

    let run = env.paper(&["diff", "--stat"]);

    run.assert_code(0);
    let plot = run
        .stdout
        .lines()
        .find(|l| l.contains("figures/plot.bin"))
        .unwrap();
    assert!(plot.contains("new") && plot.contains("(2 KB)"), "{run:#?}");
    assert!(!plot.contains("words"), "{run:#?}");
    assert!(
        run.stdout
            .contains(&minus("2 files changed, +3 words -1 word\n")),
        "binary files add nothing to the word total: {run:#?}"
    );
}

#[test]
fn stat_lines_keeps_the_line_counts() {
    let env = word_project();
    let run = env.paper(&["diff", "--stat", "--lines", "last~2", "last"]);
    run.assert_code(0);
    assert!(run.stdout.contains("+1 \u{2212}1"), "{run:#?}");
    assert!(!run.stdout.contains("word"), "{run:#?}");
}

#[test]
fn diff_json_in_word_mode_has_unit_paragraphs_and_null_hunks() {
    let env = word_project();
    env.write("layout.txt", "a b c\n");
    env.paper(&["save", "Add layout"]).assert_code(0);
    env.write("sections/intro.tex", "Intro v3 more\n");
    env.write_bytes("figures/plot.bin", &binary(2048, 7));
    env.write("layout.txt", "a\nb  c\n");

    let run = env.paper(&["--json", "diff"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(&json, &["schema", "files"]);
    let files = json["files"].as_array().unwrap();
    let find = |path: &str| {
        files
            .iter()
            .find(|f| f["path"] == path)
            .unwrap_or_else(|| panic!("no {path}: {run:#?}"))
    };
    let file_keys = [
        "path",
        "kind",
        "binary",
        "added",
        "removed",
        "old_bytes",
        "new_bytes",
        "hunks",
        "unit",
        "layout_only",
        "paragraphs",
    ];
    for file in files {
        assert_keys(file, &file_keys);
    }

    let intro = find("sections/intro.tex");
    assert_eq!(intro["unit"], "words");
    assert_eq!(intro["hunks"], serde_json::Value::Null);
    assert_eq!(intro["layout_only"], false);
    assert_eq!(intro["binary"], false);
    assert_eq!(
        (intro["added"].clone(), intro["removed"].clone()),
        (2.into(), 1.into())
    );
    let paragraph = &intro["paragraphs"][0];
    assert_keys(paragraph, &["number", "segments"]);
    assert_eq!(paragraph["number"], 1);
    let segments = paragraph["segments"].as_array().unwrap();
    for segment in segments {
        assert_keys(segment, &["kind", "text"]);
    }
    let kinds: Vec<&str> = segments
        .iter()
        .map(|s| s["kind"].as_str().unwrap())
        .collect();
    for kind in &kinds {
        assert!(["same", "removed", "added", "cut"].contains(kind), "{kind}");
    }
    assert!(
        kinds.contains(&"removed") && kinds.contains(&"added"),
        "{kinds:?}"
    );
    let removed = segments.iter().find(|s| s["kind"] == "removed").unwrap();
    assert_eq!(removed["text"], "v2");

    let layout = find("layout.txt");
    assert_eq!(layout["unit"], "words");
    assert_eq!(layout["layout_only"], true);
    assert_eq!(layout["paragraphs"], serde_json::json!([]));
    assert_eq!(layout["hunks"], serde_json::Value::Null);

    let plot = find("figures/plot.bin");
    assert_eq!(plot["unit"], serde_json::Value::Null);
    assert_eq!(plot["binary"], true);
    assert_eq!(plot["paragraphs"], serde_json::Value::Null);
    assert_eq!(plot["hunks"], serde_json::json!([]));
}

#[test]
fn a_cut_segment_has_null_text_in_json() {
    let env = word_project();
    let body: String = (0..120)
        .map(|i| ["w", &i.to_string(), " "].concat())
        .collect();
    env.write("essay.txt", &format!("first {body}last\n"));
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", &format!("FIRST {body}LAST\n"));

    let run = env.paper(&["--json", "diff"]);

    run.assert_code(0);
    let segments = run.json()["files"][0]["paragraphs"][0]["segments"].clone();
    let cut = segments
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "cut")
        .unwrap_or_else(|| panic!("no cut: {run:#?}"));
    assert_eq!(cut["text"], serde_json::Value::Null);
    for segment in segments.as_array().unwrap() {
        if segment["kind"] != "cut" {
            assert!(segment["text"].is_string(), "{segment}");
        }
    }
}

#[test]
fn diff_json_in_line_mode_keeps_hunks_and_has_null_paragraphs() {
    let env = word_project();
    env.write("main.tex", "changed main\n");

    let run = env.paper(&["--json", "diff", "--lines"]);

    run.assert_code(0);
    let file = &run.json()["files"][0];
    assert_eq!(file["unit"], "lines");
    assert_eq!(file["paragraphs"], serde_json::Value::Null);
    assert!(file["hunks"].as_array().is_some_and(|h| !h.is_empty()));
    assert_eq!(file["layout_only"], false);
}

#[test]
fn diff_help_describes_both_modes_and_no_longer_says_words_are_unavailable() {
    let env = word_project();
    let run = env.paper(&["diff", "--help"]);
    run.assert_code(0);
    assert!(run.stdout.contains("--lines"), "{run:#?}");
    assert!(run.stdout.contains("--words"), "{run:#?}");
    assert!(!run.stdout.contains("Not yet available"), "{run:#?}");
    assert!(
        !run.stdout.contains("(the default)"),
        "lines is no longer the default: {run:#?}"
    );
    let show = env.paper(&["show", "--help"]);
    show.assert_code(0);
    assert!(
        show.stdout.contains("--lines") && show.stdout.contains("--words"),
        "{show:#?}"
    );
}

// ---------------------------------------------------------------- review round 1

/// The display width of a line, in terminal columns.
fn columns(line: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(line)
}

#[test]
fn the_line_break_that_ends_a_latex_comment_stays_a_line_break() {
    let env = word_project();
    env.write(
        "essay.tex",
        "% keep this comment about the calibration\nThe old sentence here.\n",
    );
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write(
        "essay.tex",
        "% keep this comment about the calibration\nThe new sentence here.\n",
    );

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains(
            "  % keep this comment about the calibration\n  The [-old-]{+new+} sentence here.\n"
        ),
        "the next sentence must not read as commented out: {run:#?}"
    );
}

#[test]
fn a_line_break_after_text_that_is_not_a_comment_is_still_joined() {
    let env = word_project();
    env.write("essay.txt", "100% sure\nThe old sentence here.\n");
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", "100% sure\nThe new sentence here.\n");

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    assert!(
        run.stdout
            .contains("  100% sure The [-old-]{+new+} sentence here.\n"),
        "{run:#?}"
    );
}

fn long_essay(env: &Env) {
    let words: Vec<String> = (0..60).map(|i| format!("word{i:02}")).collect();
    env.write("essay.txt", &format!("{}.\n", words.join(" ")));
    env.paper(&["save", "Add essay"]).assert_code(0);
    let mut changed = words.clone();
    "CHANGED".clone_into(&mut changed[30]);
    env.write("essay.txt", &format!("{}.\n", changed.join(" ")));
}

#[test]
fn columns_sets_the_wrap_width() {
    let env = word_project();
    long_essay(&env);

    let narrow = env.paper_in(&env.work, &[("COLUMNS", "40")], &["diff"]);
    let nonsense = env.paper_in(&env.work, &[("COLUMNS", "wide")], &["diff"]);
    let piped = env.paper(&["diff"]);

    for (run, limit) in [(&narrow, 40), (&nonsense, 80), (&piped, 80)] {
        run.assert_code(0);
        let body = paragraph_lines(&run.stdout);
        assert!(!body.is_empty(), "{run:#?}");
        assert!(
            body.iter().all(|l| columns(l) <= limit),
            "limit {limit}: {run:#?}"
        );
    }
    assert!(
        paragraph_lines(&narrow.stdout).len() > paragraph_lines(&piped.stdout).len(),
        "narrow wraps more: {narrow:#?}"
    );
    assert!(
        paragraph_lines(&piped.stdout)
            .iter()
            .any(|l| columns(l) > 40),
        "piped output wraps at 80, not at a terminal's width: {piped:#?}"
    );
}

fn cjk_wrap(text: &str, replaced: char, with: char) {
    let env = word_project();
    env.write("essay.txt", &format!("{text}\n"));
    env.paper(&["save", "Add essay"]).assert_code(0);
    let new: String = text
        .chars()
        .enumerate()
        .map(|(i, c)| if i == 30 && c == replaced { with } else { c })
        .collect();
    assert_ne!(new, text);
    env.write("essay.txt", &format!("{new}\n"));

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    let body = paragraph_lines(&run.stdout);
    assert!(body.len() >= 2, "wrapped: {run:#?}");
    for line in &body {
        assert!(columns(line) <= 80, "display width: {line:?}");
        assert!(line.starts_with("  "), "{line:?}");
        assert!(!line.trim_end().ends_with("[-"), "{line:?}");
        assert!(!line.trim_end().ends_with("{+"), "{line:?}");
        assert!(!line.trim_start().starts_with("-]"), "{line:?}");
        assert!(!line.trim_start().starts_with("+}"), "{line:?}");
    }
    let joined: String = body.iter().map(|l| l.trim()).collect();
    assert!(
        joined.contains(&format!("[-{replaced}-]{{+{with}+}}")),
        "{joined}"
    );
}

#[test]
fn korean_text_wraps_by_display_width_between_syllables() {
    let text: String = "가나다라마바사아자차카타파하"
        .chars()
        .cycle()
        .take(60)
        .collect();
    cjk_wrap(&text, '다', '힣');
}

#[test]
fn japanese_text_wraps_by_display_width_between_characters() {
    let text: String = "あいうえおかきくけこさしすせそ"
        .chars()
        .cycle()
        .take(60)
        .collect();
    cjk_wrap(&text, 'あ', 'ん');
}

#[test]
fn a_cut_at_the_end_of_a_line_leaves_no_trailing_space() {
    let env = word_project();
    let body: String = (0..120)
        .map(|i| ["w", &i.to_string(), " "].concat())
        .collect();
    env.write("essay.txt", &format!("first {body}last\n"));
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", &format!("FIRST {body}LAST\n"));

    for width in 30..60 {
        let run = env.paper_in(&env.work, &[("COLUMNS", &width.to_string())], &["diff"]);
        run.assert_code(0);
        for line in run.stdout.lines() {
            assert_eq!(line, line.trim_end(), "width {width}: {run:#?}");
        }
    }
}

#[test]
fn a_removed_run_is_painted_whole_spaces_included() {
    let env = word_project();
    env.write("essay.txt", "Hello big bad wolf here\n");
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", "Hello tiny good fox here\n");

    let run = env.paper(&["--color", "always", "diff"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains("\x1b[31;9m \x1b[0m"),
        "the space inside a removed run is struck through: {run:#?}"
    );
    assert!(run.stdout.contains("\x1b[32m \x1b[0m"), "{run:#?}");
}

#[test]
fn stat_of_only_binary_changes_totals_in_line_format() {
    let env = project();
    env.write_bytes("figures/plot.bin", &binary(2048, 7));
    std::fs::remove_file(env.work.join("related.tex")).unwrap();
    std::fs::remove_file(env.work.join("sections/intro.tex")).unwrap();
    std::fs::remove_file(env.work.join("main.tex")).unwrap();
    env.paper(&["save", "Only a plot"]).assert_code(0);
    env.write_bytes("figures/plot.bin", &binary(4096, 9));

    let run = env.paper(&["diff", "--stat"]);

    run.assert_code(0);
    assert!(
        run.stdout.ends_with("1 file changed, +0 \u{2212}0\n"),
        "{run:#?}"
    );
}

#[test]
fn word_output_in_plain_markup_is_snapshotted() {
    let env = word_project();
    env.write(
        "essay.txt",
        "First calm paragraph.\n\nSecond calm paragraph.\n\nThird calm paragraph.\n",
    );
    env.write("layout.txt", "one two three\nfour five\n");
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write(
        "essay.txt",
        "First calm paragraph.\n\nSecond stormy paragraph, with more.\n\nThird calm paragraph.\n",
    );
    env.write("layout.txt", "one two\nthree  four five\n");
    env.write("new.txt", "A fresh file.\n");

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    settings().bind(|| {
        insta::assert_snapshot!("diff_words_human", env.redact(&run.stdout));
    });
}

#[test]
fn word_json_is_snapshotted() {
    let env = word_project();
    env.write("essay.txt", "First calm paragraph.\n");
    env.write("layout.txt", "a b\n");
    env.paper(&["save", "Add essay"]).assert_code(0);
    env.write("essay.txt", "First stormy paragraph.\n");
    env.write("layout.txt", "a\nb\n");

    let run = env.paper(&["--json", "diff"]);

    run.assert_code(0);
    settings().bind(|| {
        insta::assert_snapshot!("diff_words_json", env.redact_json(&run.stdout));
    });
}

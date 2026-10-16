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

    let run = env.paper(&["diff"]);

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

    let run = env.paper(&["diff", "last~2"]);

    run.assert_code(0);
    assert!(run.stdout.contains(INTRO_V1_TO_V2), "{run:#?}");
    assert!(run.stdout.contains(RELATED_ADDED), "{run:#?}");
    assert!(run.stdout.contains("+++ b/main.tex"), "{run:#?}");
}

#[test]
fn diff_between_two_points_compares_them_and_ignores_the_draft() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let run = env.paper(&["diff", "last~2", "last"]);

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

    let run = env.paper(&["diff", "last~2", "last", "sections/intro.tex"]);

    run.assert_code(0);
    assert_eq!(run.stdout, INTRO_V1_TO_V2);
}

#[test]
fn a_path_with_no_points_before_it_is_a_path() {
    let env = project();
    env.write("main.tex", "changed main\n");
    env.write("related.tex", "changed related\n");

    let run = env.paper(&["diff", "main.tex"]);

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

    let run = env.paper(&["diff", "sections"]);

    run.assert_code(0);
    assert!(run.stdout.contains("+++ b/sections/intro.tex"), "{run:#?}");
    assert!(run.stdout.contains("+++ b/sections/method.tex"), "{run:#?}");
    assert!(!run.stdout.contains("main.tex"), "{run:#?}");
}

#[test]
fn double_dash_makes_everything_after_it_a_path() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let run = env.paper(&["diff", "last~2", "last", "--", "sections/intro.tex"]);

    run.assert_code(0);
    assert_eq!(run.stdout, INTRO_V1_TO_V2);
}

#[test]
fn a_name_that_is_both_a_point_and_a_file_is_a_point_without_double_dash() {
    let env = project();
    env.write("last", "a file named like a point\n");
    env.write("main.tex", "changed main\n");

    let as_point = env.paper(&["diff", "last"]);
    let as_path = env.paper(&["diff", "--", "last"]);

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

    let run = env.paper_in(&env.work.join("sections"), &[], &["diff", "intro.tex"]);

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

    let run = env.paper(&["diff"]);

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

    let run = env.paper(&["diff"]);

    run.assert_code(0);
    let patch = env.root.join("draft.patch");
    std::fs::write(&patch, &run.stdout).unwrap();
    env.git_work(&["apply", "--check", "-R", patch.to_str().unwrap()]);
}

#[test]
fn diff_between_saves_applies_in_reverse_on_that_tree() {
    let env = project();
    let run = env.paper(&["diff", "last~2", "last"]);
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

    let run = env.paper(&["diff", "--stat", "last~2", "last"]);

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
fn words_is_not_yet_available() {
    let env = project();
    let run = env.paper(&["diff", "--words"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("not yet"), "{run:#?}");
    assert!(run.stderr.contains("v0.3"), "{run:#?}");
}

#[test]
fn lines_is_accepted_and_is_the_default() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let default = env.paper(&["diff"]);
    let lines = env.paper(&["diff", "--lines"]);

    lines.assert_code(0);
    assert!(!lines.stdout.is_empty(), "{lines:#?}");
    assert_eq!(lines.stdout, default.stdout);
}

#[test]
fn colour_is_used_only_when_asked_for() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");

    let plain = env.paper(&["diff"]);
    let coloured = env.paper(&["--color", "always", "diff"]);

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

    let no_snapshot = env.paper(&["--no-snapshot", "diff"]);
    let normal = env.paper(&["diff"]);

    no_snapshot.assert_code(0);
    assert_eq!(no_snapshot.stdout, "");
    assert!(normal.stdout.contains("+++ b/edited.txt"), "{normal:#?}");
}

#[test]
fn diff_json_has_a_fixed_key_set_for_text_and_binary_files() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\nMore text");
    env.write_bytes("figures/plot.bin", &binary(2048, 7));

    let run = env.paper(&["--json", "diff"]);

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
    ];
    for file in files {
        assert_keys(file, &file_keys);
    }
    assert_eq!(files[0]["path"], "figures/plot.bin");
    assert_eq!(files[0]["binary"], true);
    assert_eq!(files[0]["new_bytes"], 2048);
    assert_eq!(files[0]["hunks"], serde_json::json!([]));
    let text = &files[1];
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

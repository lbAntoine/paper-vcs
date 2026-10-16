//! `paper show`, and the automatic snapshot (I1) every command except `init` performs.

mod common;

use common::{Env, assert_keys, settings};

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

#[test]
fn show_last_prints_metadata_and_changed_files() {
    let env = project();

    let run = env.paper(&["show", "last"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Rewrite intro"), "{run:#?}");
    assert!(run.stdout.contains("Mina Park"), "author: {run:#?}");
    assert!(run.stdout.contains("sections/intro.tex"), "{run:#?}");
    assert!(
        !run.stdout.contains("related.tex"),
        "only this save's files: {run:#?}"
    );
    let id = env.change_id("HEAD");
    assert!(run.stdout.contains(&id[..4]), "{run:#?}");
    assert!(
        run.stdout.contains(
            "\n--- a/sections/intro.tex\n+++ b/sections/intro.tex\n@@ -1 +1 @@\n-Intro v1\n+Intro v2\n"
        ),
        "the diff follows the metadata: {run:#?}"
    );
    settings().bind(|| {
        insta::assert_snapshot!("show_human", env.redact(&run.stdout));
    });
}

#[test]
fn show_by_change_id_prefix_finds_that_save() {
    let env = project();
    let id = env.change_id("HEAD~1");

    let run = env.paper(&["show", &id[..4]]);

    run.assert_code(0);
    assert!(run.stdout.contains("Add related work"), "{run:#?}");
    assert!(run.stdout.contains("related.tex"), "{run:#?}");
}

#[test]
fn show_the_first_save_lists_every_file_as_added() {
    let env = project();
    let run = env.paper(&["show", "last~2"]);
    run.assert_code(0);
    assert!(run.stdout.contains("Start project"), "{run:#?}");
    assert!(run.stdout.contains("main.tex"), "{run:#?}");
    assert!(run.stdout.contains("sections/intro.tex"), "{run:#?}");
    assert!(run.stdout.to_lowercase().contains("added"), "{run:#?}");
    assert!(
        run.stdout
            .contains("--- /dev/null\n+++ b/main.tex\n@@ -0,0 +1 @@\n+\\documentclass{article}\n"),
        "every file is a new file in the diff: {run:#?}"
    );
    assert!(
        run.stdout
            .contains("--- /dev/null\n+++ b/sections/intro.tex\n"),
        "{run:#?}"
    );
}

#[test]
fn show_stat_prints_the_counts_instead_of_the_diff() {
    let env = project();

    let run = env.paper(&["show", "--stat", "last"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Rewrite intro"), "{run:#?}");
    assert!(run.stdout.contains("sections/intro.tex"), "{run:#?}");
    assert!(run.stdout.contains("+1 \u{2212}1"), "{run:#?}");
    assert!(!run.stdout.contains("@@"), "{run:#?}");
    assert!(!run.stdout.contains("-Intro v1"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("show_stat", env.redact(&run.stdout));
    });
}

#[test]
fn show_the_draft_shows_its_diff_against_last() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");

    let run = env.paper(&["show", "@"]);

    run.assert_code(0);
    assert!(
        run.stdout
            .contains("--- a/sections/intro.tex\n+++ b/sections/intro.tex\n@@ -1 +1 @@\n-Intro v2\n+Intro v3\n"),
        "{run:#?}"
    );
}

#[test]
fn show_an_unknown_point_is_a_user_error_with_why_and_next() {
    let env = project();

    let run = env.paper(&["show", "nope"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("nope"), "{run:#?}");
    assert!(run.stderr.contains("paper history"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("show_unknown_point", env.redact(&run.stderr));
    });
}

#[test]
fn show_json_has_schema_1_the_save_and_its_changes() {
    let env = project();

    let run = env.paper(&["--json", "show", "last"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["save"]["message"], "Rewrite intro");
    assert_eq!(json["changes"][0]["path"], "sections/intro.tex");
    assert_eq!(json["changes"][0]["kind"], "modified");
    assert_keys(&json, &["schema", "save", "changes", "files"]);
    let file = &json["files"][0];
    assert_keys(
        file,
        &[
            "path",
            "kind",
            "binary",
            "added",
            "removed",
            "old_bytes",
            "new_bytes",
            "hunks",
        ],
    );
    assert_eq!(file["path"], "sections/intro.tex");
    assert_eq!(file["hunks"][0]["lines"][0]["kind"], "removed");
    assert_eq!(file["hunks"][0]["lines"][0]["text"], "Intro v1");
    settings().bind(|| {
        insta::assert_snapshot!("show_json", env.redact_json(&run.stdout));
    });
}

#[test]
fn an_unsaved_edit_appears_in_show_draft_because_commands_snapshot_first() {
    let env = project();
    env.write("edited.txt", "made outside paper\n");

    let run = env.paper(&["show", "@"]);

    run.assert_code(0);
    assert!(run.stdout.contains("edited.txt"), "{run:#?}");
}

#[test]
fn with_no_snapshot_the_edit_is_not_picked_up() {
    let env = project();
    env.write("edited.txt", "made outside paper\n");

    let run = env.paper(&["--no-snapshot", "show", "@"]);

    run.assert_code(0);
    assert!(!run.stdout.contains("edited.txt"), "{run:#?}");
}

#[test]
fn snapshotting_leaves_git_clean_and_the_saves_alone() {
    let env = project();
    env.write("edited.txt", "made outside paper\n");
    env.paper(&["show", "last"]).assert_code(0);
    env.fsck();
    assert_eq!(env.subjects().len(), 3);
}

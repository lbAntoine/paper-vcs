//! `paper show`, and the automatic snapshot (I1) every command except `init` performs.

mod common;

use common::{Env, settings};

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

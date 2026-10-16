//! `paper save`, cross-checked with the real `git` binary.

mod common;

use common::{Env, settings};

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

#[test]
fn save_makes_a_save_with_the_given_message() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");

    let run = env.paper(&["save", "Tighten intro"]);

    run.assert_code(0);
    assert_eq!(
        env.git_work(&["log", "-1", "--format=%s"]).trim(),
        "Tighten intro"
    );
    assert_eq!(env.subjects().len(), 4);
    env.fsck();
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    let id = env.change_id("HEAD");
    assert!(
        run.stdout.contains(&id[..4]),
        "short change id shown: {run:#?}"
    );
    assert!(run.stdout.contains("Tighten intro"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("save_human", env.redact(&run.stdout));
    });
}

#[test]
fn save_with_paths_saves_only_those_and_leaves_the_rest_in_the_draft() {
    let env = project();
    env.write("main.tex", "changed main\n");
    env.write("related.tex", "changed related\n");

    let run = env.paper(&["save", "Only main", "main.tex"]);

    run.assert_code(0);
    assert_eq!(
        env.git_work(&["show", "--name-only", "--format=", "HEAD"])
            .trim(),
        "main.tex"
    );
    env.fsck();
    assert_eq!(
        env.git_work(&["status", "--porcelain"]).trim(),
        "M related.tex"
    );
    assert!(
        run.stdout.contains("related.tex"),
        "left in the draft: {run:#?}"
    );
    settings().bind(|| {
        insta::assert_snapshot!("save_paths", env.redact(&run.stdout));
    });
}

#[test]
fn save_all_but_excludes_the_given_paths() {
    let env = project();
    env.write("main.tex", "changed main\n");
    env.write("related.tex", "changed related\n");

    let run = env.paper(&["save", "Not related", "--all-but", "related.tex"]);

    run.assert_code(0);
    assert_eq!(
        env.git_work(&["show", "--name-only", "--format=", "HEAD"])
            .trim(),
        "main.tex"
    );
    env.fsck();
    assert_eq!(
        env.git_work(&["status", "--porcelain"]).trim(),
        "M related.tex"
    );
}

#[test]
fn save_with_nothing_to_save_is_a_friendly_no_op() {
    let env = project();
    let before = env.git_work(&["rev-parse", "HEAD"]);

    let run = env.paper(&["save", "Nothing here"]);

    run.assert_code(0);
    assert_eq!(run.stdout.trim(), "Nothing to save.");
    assert_eq!(env.git_work(&["rev-parse", "HEAD"]), before);
    settings().bind(|| {
        insta::assert_snapshot!("save_nothing", env.redact(&run.stdout));
    });
}

#[test]
fn save_without_a_message_and_without_a_terminal_is_a_user_error() {
    let env = project();
    env.write("main.tex", "changed\n");
    let before = env.git_work(&["rev-parse", "HEAD"]);

    let run = env.paper(&["save"]);

    run.assert_three_part_error(1);
    assert_eq!(
        env.git_work(&["rev-parse", "HEAD"]),
        before,
        "nothing saved"
    );
    settings().bind(|| {
        insta::assert_snapshot!("save_missing_message", env.redact(&run.stderr));
    });
}

#[test]
fn save_with_a_path_that_has_no_changes_lists_it() {
    let env = project();
    env.write("main.tex", "changed\n");
    let before = env.git_work(&["rev-parse", "HEAD"]);

    let run = env.paper(&["save", "Typo", "mian.tex"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("mian.tex"), "{run:#?}");
    assert_eq!(
        env.git_work(&["rev-parse", "HEAD"]),
        before,
        "nothing saved"
    );
    settings().bind(|| {
        insta::assert_snapshot!("save_no_changes_at", env.redact(&run.stderr));
    });
}

#[test]
fn save_json_has_schema_1_and_the_saved_files() {
    let env = project();
    env.write("main.tex", "changed main\n");
    env.write("related.tex", "changed related\n");

    let run = env.paper(&["--json", "save", "Only main", "main.tex"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["saved"], true);
    assert_eq!(json["message"], "Only main");
    assert_eq!(json["saved_files"][0]["path"], "main.tex");
    assert_eq!(json["remaining"][0]["path"], "related.tex");
    settings().bind(|| {
        insta::assert_snapshot!("save_json", env.redact_json(&run.stdout));
    });
}

#[test]
fn save_json_for_a_no_op_says_nothing_was_saved() {
    let env = project();
    let run = env.paper(&["--json", "save", "Nothing here"]);
    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["saved"], false);
}

#[test]
fn quiet_save_prints_nothing_on_success() {
    let env = project();
    env.write("main.tex", "changed\n");
    let run = env.paper(&["-q", "save", "Quiet"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "");
    assert_eq!(env.subjects()[0], "Quiet");
}

#[test]
fn quiet_still_prints_errors() {
    let env = project();
    let run = env.paper(&["-q", "show", "nope"]);
    run.assert_three_part_error(1);
}

#[test]
fn verbose_save_adds_the_git_commit_in_parentheses() {
    let env = project();
    env.write("main.tex", "changed\n");

    let run = env.paper(&["-v", "save", "Verbose"]);

    run.assert_code(0);
    let hash = env.git_work(&["rev-parse", "--short=7", "HEAD"]);
    assert!(
        run.stdout.contains(&format!("(commit {})", hash.trim())),
        "{run:#?}"
    );
}

#[test]
fn save_snapshots_an_edit_made_since_the_last_command() {
    let env = project();
    env.write("new.txt", "fresh\n");
    env.paper(&["save", "Fresh file"]).assert_code(0);
    assert_eq!(
        env.git_work(&["show", "--name-only", "--format=", "HEAD"])
            .trim(),
        "new.txt"
    );
}

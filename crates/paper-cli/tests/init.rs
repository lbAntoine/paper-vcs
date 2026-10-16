//! `paper init`, cross-checked with the real `git` binary.

mod common;

use common::{Env, settings};

#[test]
fn init_in_an_empty_folder_creates_a_project_with_a_first_save() {
    let env = Env::new();
    env.write("main.tex", "\\documentclass{article}\n");
    env.write("notes/todo.md", "- write\n");

    let run = env.paper(&["init"]);

    run.assert_code(0);
    assert!(run.stdout.to_lowercase().contains("created"), "{run:#?}");
    assert_eq!(env.subjects(), ["Start project"]);
    assert!(env.work.join(".git/paper").is_dir());
    env.fsck();
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    settings().bind(|| {
        insta::assert_snapshot!("init_created", env.redact(&run.stdout));
    });
}

#[test]
fn init_reports_the_number_of_tracked_files() {
    let env = Env::new();
    env.write("a.txt", "a\n");
    env.write("b.txt", "b\n");
    let run = env.paper(&["init"]);
    run.assert_code(0);
    assert!(run.stdout.contains('2'), "{run:#?}");
}

#[test]
fn init_with_a_path_creates_the_project_there() {
    let env = Env::new();
    let run = env.paper(&["init", "thesis"]);
    run.assert_code(0);
    assert!(env.work.join("thesis/.git/paper").is_dir());
}

#[test]
fn init_with_dash_c_works_in_that_folder() {
    let env = Env::new();
    let work = env.work.to_string_lossy().into_owned();
    let run = env.paper_in(&env.root, &[], &["-C", &work, "init"]);
    run.assert_code(0);
    assert!(env.work.join(".git/paper").is_dir());
}

#[test]
fn init_in_an_existing_git_repo_adopts_it_without_rewriting() {
    let env = Env::new();
    env.plain_git_repo_with_side_branch();
    let before = env.git_work(&["rev-parse", "main", "side"]);

    let run = env.paper(&["init"]);

    run.assert_code(0);
    assert!(run.stdout.to_lowercase().contains("adopt"), "{run:#?}");
    assert_eq!(env.git_work(&["rev-parse", "main", "side"]), before);
    assert_eq!(env.subjects(), ["Second", "First"]);
    env.fsck();
    settings().bind(|| {
        insta::assert_snapshot!("init_adopted", env.redact(&run.stdout));
    });
}

#[test]
fn init_accepts_each_documented_profile() {
    for profile in ["latex", "markdown", "typst", "quarto", "jupyter", "generic"] {
        let env = Env::new();
        env.write("a.txt", "a\n");
        env.paper(&["init", "--profile", profile]).assert_code(0);
    }
}

#[test]
fn init_rejects_an_unknown_profile() {
    let env = Env::new();
    env.write("a.txt", "a\n");
    let run = env.paper(&["init", "--profile", "word"]);
    run.assert_code(1);
    assert!(
        run.stderr.contains("latex"),
        "should list the choices: {run:#?}"
    );
    assert!(!env.work.join(".git").exists());
}

#[test]
fn init_twice_is_a_user_error() {
    let env = Env::new();
    env.write("a.txt", "a\n");
    env.paper(&["init"]).assert_code(0);

    let run = env.paper(&["init"]);

    run.assert_three_part_error(1);
    settings().bind(|| {
        insta::assert_snapshot!("init_twice", env.redact(&run.stderr));
    });
}

#[test]
fn init_json_has_schema_1_and_the_counts() {
    let env = Env::new();
    env.write("a.txt", "a\n");
    env.write("b.txt", "b\n");

    let run = env.paper(&["--json", "init"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["adopted"], false);
    assert_eq!(json["tracked_files"], 2);
    settings().bind(|| {
        insta::assert_snapshot!("init_json", env.redact_json(&run.stdout));
    });
}

#[test]
fn quiet_init_prints_nothing() {
    let env = Env::new();
    env.write("a.txt", "a\n");
    let run = env.paper(&["-q", "init"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "");
}

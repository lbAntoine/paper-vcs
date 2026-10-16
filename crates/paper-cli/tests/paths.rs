//! Paths are relative to where `paper` runs, not to the project root.

mod common;

use common::Env;

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

#[test]
fn save_from_a_subfolder_takes_paths_relative_to_it() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");
    env.write("main.tex", "changed main\n");

    let run = env.paper_in(&env.work.join("sections"), &[], &["save", "x", "intro.tex"]);

    run.assert_code(0);
    assert_eq!(
        env.git_work(&["show", "--name-only", "--format=", "HEAD"])
            .trim(),
        "sections/intro.tex"
    );
    env.fsck();
}

#[test]
fn a_dot_slash_prefix_is_accepted() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");

    let run = env.paper_in(
        &env.work.join("sections"),
        &[],
        &["save", "x", "./intro.tex"],
    );

    run.assert_code(0);
    assert_eq!(
        env.git_work(&["show", "--name-only", "--format=", "HEAD"])
            .trim(),
        "sections/intro.tex"
    );
}

#[test]
fn dot_dot_reaches_a_file_in_the_parent() {
    let env = project();
    env.write("main.tex", "changed main\n");

    let run = env.paper_in(
        &env.work.join("sections"),
        &[],
        &["save", "x", "../main.tex"],
    );

    run.assert_code(0);
    assert_eq!(
        env.git_work(&["show", "--name-only", "--format=", "HEAD"])
            .trim(),
        "main.tex"
    );
}

#[test]
fn history_from_a_subfolder_takes_a_relative_path() {
    let env = project();

    let run = env.paper_in(&env.work.join("sections"), &[], &["history", "intro.tex"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Rewrite intro"), "{run:#?}");
    assert!(run.stdout.contains("Start project"), "{run:#?}");
    assert!(!run.stdout.contains("Add related work"), "{run:#?}");
}

#[test]
fn history_dot_from_a_subfolder_lists_saves_touching_that_folder() {
    let env = project();
    let run = env.paper_in(&env.work.join("sections"), &[], &["history", "."]);
    run.assert_code(0);
    assert!(!run.stdout.contains("Add related work"), "{run:#?}");
}

#[test]
fn a_path_outside_the_project_is_a_user_error() {
    let env = project();
    env.write("main.tex", "changed\n");

    let run = env.paper(&["save", "x", "../elsewhere.tex"]);

    run.assert_three_part_error(1);
    assert!(
        run.stderr
            .contains("../elsewhere.tex is outside this project"),
        "{run:#?}"
    );
    common::settings().bind(|| {
        insta::assert_snapshot!("path_outside_project", env.redact(&run.stderr));
    });
}

#[test]
fn all_but_takes_one_path_per_flag_and_leaves_the_message_alone() {
    let env = project();
    env.write("main.tex", "changed main\n");
    env.write("related.tex", "changed related\n");
    env.write("data.csv", "1,2\n");

    let run = env.paper(&[
        "save",
        "--all-but",
        "data.csv",
        "--all-but",
        "related.tex",
        "Fix intro",
    ]);

    run.assert_code(0);
    assert_eq!(env.subjects()[0], "Fix intro");
    assert_eq!(
        env.git_work(&["show", "--name-only", "--format=", "HEAD"])
            .trim(),
        "main.tex"
    );
}

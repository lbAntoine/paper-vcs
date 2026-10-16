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
    // The two files plus the `.gitignore` and `paper.toml` that init writes.
    assert!(run.stdout.contains("Tracking 4 files"), "{run:#?}");
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
    assert_eq!(json["tracked_files"], 4); // plus .gitignore and paper.toml
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

// ---- profiles (PV-004) ----

#[test]
fn init_says_which_profile_it_detected_what_it_wrote_and_the_tips() {
    let env = Env::new();
    env.write("main.tex", "\\documentclass{article}\n");
    env.write("main.aux", "junk");

    let run = env.paper(&["init"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains("Profile: latex (detected from *.tex)"),
        "{run:#?}"
    );
    assert!(run.stdout.contains(".gitignore"), "{run:#?}");
    assert!(run.stdout.contains("paper.toml"), "{run:#?}");
    assert!(run.stdout.contains("one sentence per line"), "{run:#?}");
    let tracked = env.git_work(&["ls-tree", "-r", "--name-only", "main"]);
    assert_eq!(tracked, ".gitignore\nmain.tex\npaper.toml\n");
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    env.fsck();
}

#[test]
fn init_with_profile_uses_it_and_reports_it_as_chosen() {
    let env = Env::new();
    env.write("main.tex", "\\documentclass{article}\n");

    let run = env.paper(&["init", "--profile", "markdown"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains("Profile: markdown (chosen)"),
        "{run:#?}"
    );
    assert!(
        env.read(".gitignore")
            .contains("paper-vcs profile: markdown")
    );
    assert!(env.read("paper.toml").contains("profile = \"markdown\""));
}

#[test]
fn init_json_has_profile_detected_written_and_tips() {
    let env = Env::new();
    env.write("main.tex", "\\documentclass{article}\n");

    let run = env.paper(&["--json", "init"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["profile"], "latex");
    assert_eq!(json["detected"], true);
    assert_eq!(
        json["written"],
        serde_json::json!([".gitignore", "paper.toml"])
    );
    assert!(json["tips"].as_array().is_some_and(|tips| !tips.is_empty()));
    settings().bind(|| {
        insta::assert_snapshot!("init_json_latex", env.redact_json(&run.stdout));
    });
}

#[test]
fn init_json_reports_a_chosen_profile_as_not_detected() {
    let env = Env::new();
    env.write("main.tex", "\\documentclass{article}\n");
    let run = env.paper(&["--json", "init", "--profile", "typst"]);
    run.assert_code(0);
    assert_eq!(run.json()["profile"], "typst");
    assert_eq!(run.json()["detected"], false);
}

#[test]
fn adopting_leaves_the_block_and_paper_toml_as_unsaved_changes() {
    let env = Env::new();
    env.plain_git_repo_with_side_branch();
    let before = env.git_work(&["log", "--format=%H %s", "--branches"]);

    let run = env.paper(&["init"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Profile: generic"), "{run:#?}");
    assert!(run.stdout.contains(".gitignore"), "{run:#?}");
    assert_eq!(
        env.git_work(&["log", "--format=%H %s", "--branches"]),
        before
    );
    let status = env.git_work(&["status", "--porcelain"]);
    assert!(
        status.contains(".gitignore") && status.contains("paper.toml"),
        "{status}"
    );
    env.fsck();
}

#[test]
fn adopting_with_a_conflicting_profile_is_a_user_error_and_writes_nothing() {
    let env = Env::new();
    env.plain_git_repo_with_side_branch();
    env.write("paper.toml", "profile = \"typst\"\n");

    let run = env.paper(&["init", "--profile", "latex"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("typst"), "{run:#?}");
    assert!(
        run.stderr.contains("config set"),
        "should say how to change it: {run:#?}"
    );
    assert!(!env.work.join(".gitignore").exists());
    assert!(!env.work.join(".git/paper").exists());
}

#[test]
fn a_damaged_block_during_init_suggests_running_init_again() {
    let env = Env::new();
    env.write("a.txt", "a\n");
    env.write(".gitignore", "# >>> paper-vcs profile: latex >>>\n*.aux\n");

    let run = env.paper(&["init"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("next: paper init"), "{run:#?}");
    assert!(!env.work.join("paper.toml").exists());
}

#[test]
fn a_gitignore_that_is_not_utf8_is_a_user_error_and_nothing_is_written() {
    let env = Env::new();
    env.write("a.txt", "a\n");
    std::fs::write(env.work.join(".gitignore"), b"caf\xe9\n").unwrap();

    let run = env.paper(&["init"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("UTF-8"), "{run:#?}");
    assert_eq!(
        std::fs::read(env.work.join(".gitignore")).unwrap(),
        b"caf\xe9\n"
    );
    assert!(!env.work.join("paper.toml").exists());
    assert!(!env.work.join(".git/paper").exists());
}

#[test]
fn adopting_a_repository_without_saves_puts_the_files_in_the_first_save() {
    let env = Env::new();
    env.git_work(&["init", "--quiet", "-b", "main"]);
    env.write("a.txt", "a\n");

    let run = env.paper(&["init"]);

    run.assert_code(0);
    assert!(!run.stdout.contains("unsaved"), "{run:#?}");
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    settings().bind(|| {
        insta::assert_snapshot!("init_adopted_no_commits", env.redact(&run.stdout));
    });
}

#[test]
fn a_damaged_dot_git_at_the_target_is_a_user_error_that_says_where_to_look() {
    let env = Env::new();
    env.write("a.txt", "a\n");
    std::fs::create_dir(env.work.join(".git")).unwrap();

    let run = env.paper(&["init"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("damaged .git"), "{run:#?}");
    assert!(
        !run.stderr.contains("report it"),
        "this is not a bug in paper: {run:#?}"
    );
    assert!(run.stderr.contains("next:"), "{run:#?}");
    assert!(
        run.stderr.contains("move") || run.stderr.contains("inspect"),
        "the next step says to inspect or move it: {run:#?}"
    );
    assert!(!env.work.join("paper.toml").exists());
}

#[test]
fn a_damaged_dot_git_inside_a_parent_repository_is_still_a_damaged_git_error() {
    let env = Env::new();
    env.git_work(&["init", "--quiet", "-b", "main"]);
    let child = env.work.join("child");
    std::fs::create_dir_all(child.join(".git")).unwrap();
    let pointer = env.work.join("pointer");
    std::fs::create_dir(&pointer).unwrap();
    std::fs::write(pointer.join(".git"), "gitdir: ../no-such-gitdir\n").unwrap();

    for folder in ["child", "pointer"] {
        let run = env.paper(&["init", folder]);

        run.assert_three_part_error(1);
        assert!(run.stderr.contains("damaged .git"), "{run:#?}");
        assert!(!run.stderr.contains("inside another"), "{run:#?}");
    }
}

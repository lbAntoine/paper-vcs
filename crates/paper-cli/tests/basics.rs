//! Flags, argument errors, discovery, identity and panics.

mod common;

use common::{Env, settings};

#[test]
fn version_prints_the_package_version() {
    let env = Env::new();
    let run = env.paper(&["--version"]);
    run.assert_code(0);
    assert_eq!(
        run.stdout.trim(),
        format!("paper {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_lists_the_four_commands() {
    let env = Env::new();
    let run = env.paper(&["--help"]);
    run.assert_code(0);
    for command in ["init", "save", "history", "show"] {
        assert!(run.stdout.contains(command), "{}", run.stdout);
    }
}

#[test]
fn init_help_describes_the_profile_option() {
    let env = Env::new();
    let run = env.paper(&["init", "--help"]);
    run.assert_code(0);
    assert!(run.stdout.contains("--profile"), "{}", run.stdout);
    assert!(run.stdout.contains(".gitignore"), "{}", run.stdout);
    assert!(!run.stdout.contains("no effect"), "{}", run.stdout);
}

#[test]
fn an_unknown_flag_exits_1_not_clap_2() {
    let env = Env::new();
    let run = env.paper(&["--no-such-flag", "history"]);
    run.assert_code(1);
    assert!(run.stderr.contains("error"), "{run:#?}");
}

#[test]
fn an_unknown_flag_on_a_subcommand_exits_1() {
    let env = Env::new();
    env.paper(&["save", "--no-such-flag"]).assert_code(1);
}

#[test]
fn a_missing_argument_exits_1_not_clap_2() {
    let env = Env::new();
    env.paper(&["show"]).assert_code(1);
}

#[test]
fn an_unknown_command_exits_1() {
    let env = Env::new();
    env.paper(&["frobnicate"]).assert_code(1);
}

#[test]
fn outside_any_git_repo_a_command_points_to_paper_init() {
    let env = Env::new();
    let run = env.paper(&["history"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("paper init"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("not_a_project_outside_git", env.redact(&run.stderr));
    });
}

#[test]
fn in_a_plain_git_repo_a_command_says_it_is_not_a_paper_project_yet() {
    let env = Env::new();
    env.plain_git_repo_with_side_branch();
    let run = env.paper(&["history"]);
    run.assert_three_part_error(1);
    assert!(
        run.stderr
            .contains("This is a git repo but not a paper project yet"),
        "{run:#?}"
    );
    assert!(run.stderr.contains("paper init"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("not_a_project_plain_git", env.redact(&run.stderr));
    });
}

#[test]
fn dash_c_runs_the_command_somewhere_else() {
    let env = Env::new();
    env.project_with_three_saves();
    let elsewhere = env.root.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let work = env.work.to_string_lossy().into_owned();

    let run = env.paper_in(&elsewhere, &[], &["-C", &work, "history"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Rewrite intro"), "{run:#?}");
}

#[test]
fn a_project_is_found_from_a_subfolder() {
    let env = Env::new();
    env.project_with_three_saves();

    let run = env.paper_in(&env.work.join("sections"), &[], &["history"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Start project"), "{run:#?}");
}

#[test]
fn without_an_identity_a_command_suggests_git_config() {
    let env = Env::new();
    env.project_with_three_saves();
    // Same project, but a global config with no identity (the repo has none of its own).
    let bare = Env::without_identity();
    let work = env.work.to_string_lossy().into_owned();

    let run = bare.paper_in(&bare.root, &[], &["-C", &work, "history"]);

    run.assert_three_part_error(1);
    assert!(
        run.stderr.contains("git config --global user.name"),
        "{run:#?}"
    );
    assert!(
        run.stderr.contains("git config --global user.email"),
        "{run:#?}"
    );
    settings().bind(|| {
        insta::assert_snapshot!("no_identity", env.redact(&run.stderr));
    });
}

#[test]
fn init_without_an_identity_suggests_git_config_and_writes_nothing() {
    let env = Env::without_identity();
    env.write("notes.txt", "hello\n");

    let run = env.paper(&["init"]);

    run.assert_three_part_error(1);
    assert!(
        run.stderr.contains("git config --global user.name"),
        "{run:#?}"
    );
    assert!(!env.work.join(".git").exists(), "init must not half-start");
}

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "PAPER_DEBUG_PANIC is compiled out of release builds"
)]
fn a_panic_exits_101_writes_a_crash_report_and_names_it() {
    let env = Env::new();
    env.project_with_three_saves();

    let run = env.paper_in(&env.work, &[("PAPER_DEBUG_PANIC", "1")], &["history"]);

    run.assert_code(101);
    let reports: Vec<_> = std::fs::read_dir(env.crash_dir())
        .unwrap_or_else(|err| panic!("no crash dir {:?}: {err}", env.crash_dir()))
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(reports.len(), 1, "{reports:?}");
    let name = reports[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.starts_with("crash-")
            && std::path::Path::new(&name)
                .extension()
                .is_some_and(|ext| ext == "txt"),
        "{name}"
    );
    assert!(
        run.stderr.contains(&name),
        "stderr must name the report: {run:#?}"
    );
    assert!(
        run.stderr
            .contains("https://github.com/lbAntoine/paper-vcs/issues"),
        "{run:#?}"
    );
    assert!(!std::fs::read_to_string(&reports[0]).unwrap().is_empty());
}

#[test]
fn output_has_no_colour_codes_when_piped() {
    let env = Env::new();
    env.project_with_three_saves();
    let run = env.paper(&["history"]);
    run.assert_code(0);
    assert!(!run.stdout.contains('\u{1b}'), "{run:#?}");
}

#[test]
fn color_never_has_no_colour_codes() {
    let env = Env::new();
    env.project_with_three_saves();
    let run = env.paper(&["--color", "never", "history"]);
    run.assert_code(0);
    assert!(!run.stdout.contains('\u{1b}'), "{run:#?}");
}

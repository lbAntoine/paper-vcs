//! `--json` errors: one object on stdout, nothing on stderr, the same exit codes.

mod common;

use common::{Env, settings};

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

fn assert_error_shape(run: &common::Run, code: i32, kind: &str) {
    run.assert_code(code);
    assert_eq!(run.stderr, "", "{run:#?}");
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["error"]["kind"], kind);
    for key in ["message", "why"] {
        assert!(
            !json["error"][key].as_str().unwrap().is_empty(),
            "{key}: {run:#?}"
        );
    }
    assert!(
        !json["error"]["next"].as_array().unwrap().is_empty(),
        "{run:#?}"
    );
}

#[test]
fn a_user_error_is_json_on_stdout() {
    let env = project();

    let run = env.paper(&["--json", "show", "nope"]);

    assert_error_shape(&run, 1, "user");
    settings().bind(|| {
        insta::assert_snapshot!("json_error_unknown_point", env.redact_json(&run.stdout));
    });
}

#[test]
fn not_a_project_is_a_json_user_error() {
    let env = Env::new();

    let run = env.paper(&["--json", "history"]);

    assert_error_shape(&run, 1, "user");
    settings().bind(|| {
        insta::assert_snapshot!("json_error_not_a_project", env.redact_json(&run.stdout));
    });
}

#[test]
fn a_project_error_is_json_with_exit_code_3() {
    let env = project();
    // A project written by a future version of paper.
    std::fs::write(env.work.join(".git/paper/version"), "99\n").unwrap();

    let run = env.paper(&["--json", "history"]);

    assert_error_shape(&run, 3, "project");
    settings().bind(|| {
        insta::assert_snapshot!("json_error_project", env.redact_json(&run.stdout));
    });
}

#[test]
fn a_project_error_has_all_three_parts_in_human_form_too() {
    let env = project();
    std::fs::write(env.work.join(".git/paper/version"), "99\n").unwrap();

    let run = env.paper(&["history"]);

    run.assert_three_part_error(3);
    settings().bind(|| {
        insta::assert_snapshot!("project_error_human", env.redact(&run.stderr));
    });
}

#[test]
fn an_argument_error_under_json_is_a_json_user_error() {
    let env = Env::new();

    let run = env.paper(&["--json", "--no-such-flag", "history"]);

    assert_error_shape(&run, 1, "user");
    settings().bind(|| {
        insta::assert_snapshot!("json_error_bad_argument", env.redact_json(&run.stdout));
    });
}

#[test]
fn a_missing_argument_under_json_is_a_json_user_error() {
    let env = Env::new();
    let run = env.paper(&["--json", "show"]);
    assert_error_shape(&run, 1, "user");
}

#[test]
fn a_project_error_puts_the_issue_url_in_command_and_the_prose_in_explanation() {
    let env = project();
    std::fs::write(env.work.join(".git/paper/version"), "99\n").unwrap();

    let run = env.paper(&["--json", "history"]);

    assert_error_shape(&run, 3, "project");
    let next = &run.json()["error"]["next"][0];
    assert_eq!(
        next["command"],
        "https://github.com/lbAntoine/paper-vcs/issues"
    );
    assert_eq!(
        next["explanation"],
        "report it, including what you ran and the message above"
    );
}

#[test]
fn the_json_scan_for_an_argument_error_stops_at_double_dash() {
    let env = Env::new();

    // `--json` after `--` is a value, not the flag, so the error is the human form.
    let run = env.paper(&["--no-such-flag", "--", "--json"]);

    run.assert_code(1);
    assert_eq!(run.stdout, "", "{run:#?}");
    assert!(!run.stderr.is_empty(), "{run:#?}");
    assert!(
        serde_json::from_str::<serde_json::Value>(&run.stderr).is_err(),
        "{run:#?}"
    );
}

#[test]
fn diff_status_and_restore_errors_are_json_too() {
    let env = project();
    for args in [
        &["--json", "diff", "--since", "nope"][..],
        &["--json", "diff", "nope.tex"],
        &["--json", "restore", "nope.tex"],
        &["--json", "restore", "--from", "nope", "main.tex"],
    ] {
        let run = env.paper(args);
        assert_error_shape(&run, 1, "user");
    }
}

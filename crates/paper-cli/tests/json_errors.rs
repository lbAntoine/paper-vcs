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

//! A busy project from the command line: exit 3 with a three-part error, and commands that
//! don't open a project never take the lock.

mod common;

use common::Env;

/// Short enough to keep the suite fast; honoured by debug builds only.
const WAIT_ENV: (&str, &str) = ("PAPER_DEBUG_LOCK_WAIT_MS", "200");

#[test]
fn status_on_a_busy_project_exits_3_with_a_three_part_error() {
    let env = Env::new();
    let _held = env.project_with_three_saves();

    let run = env.paper_in(&env.work, &[WAIT_ENV], &["status"]);

    run.assert_code(3);
    assert!(
        run.stderr
            .contains("Another paper command is running in this project"),
        "{run:#?}"
    );
    assert!(
        run.stderr.contains(&std::process::id().to_string()),
        "the holder's pid is named: {run:#?}"
    );
    assert!(!run.stderr.to_lowercase().contains("delete"), "{run:#?}");
    assert_eq!(run.stdout, "");
}

#[test]
fn json_status_on_a_busy_project_is_a_project_error_naming_the_pid() {
    let env = Env::new();
    let _held = env.project_with_three_saves();

    let run = env.paper_in(&env.work, &[WAIT_ENV], &["--json", "status"]);

    run.assert_code(3);
    assert_eq!(run.stderr, "", "{run:#?}");
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["error"]["kind"], "project");
    assert_eq!(
        json["error"]["message"],
        "Another paper command is running in this project"
    );
    assert!(!json["error"]["why"].as_str().unwrap().is_empty());
    assert!(!json["error"]["next"].as_array().unwrap().is_empty());
    assert!(
        run.stdout.contains(&std::process::id().to_string()),
        "the holder's pid is named: {run:#?}"
    );
}

#[test]
fn the_project_is_usable_again_once_the_holder_is_gone() {
    let env = Env::new();
    let held = env.project_with_three_saves();
    env.paper_in(&env.work, &[WAIT_ENV], &["status"])
        .assert_code(3);

    drop(held);

    env.paper_in(&env.work, &[WAIT_ENV], &["status"])
        .assert_code(0);
}

#[test]
fn version_does_not_take_the_lock() {
    let env = Env::new();
    let _held = env.project_with_three_saves();

    let run = env.paper_in(&env.work, &[WAIT_ENV], &["--version"]);

    run.assert_code(0);
    assert!(run.stdout.contains("paper"), "{run:#?}");
}

#[test]
fn reading_a_user_setting_does_not_take_the_lock() {
    let env = Env::new();
    env.paper(&["config", "set", "user.name", "Alex Doe"])
        .assert_code(0);
    let _held = env.project_with_three_saves();

    let run = env.paper_in(&env.work, &[WAIT_ENV], &["config", "get", "user.name"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Alex Doe"), "{run:#?}");
}

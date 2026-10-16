//! `paper restore`, cross-checked on disk and with the real `git` binary.

mod common;

use common::{Env, assert_keys, settings};

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

#[test]
fn restore_puts_back_the_last_saved_content() {
    let env = project();
    env.write("sections/intro.tex", "scribbles\n");

    let run = env.paper(&["restore", "sections/intro.tex"]);

    run.assert_code(0);
    assert_eq!(env.read("sections/intro.tex"), "Intro v2\n");
    assert!(run.stdout.contains("sections/intro.tex"), "{run:#?}");
    assert!(run.stdout.contains("modified"), "{run:#?}");
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    env.fsck();
    settings().bind(|| {
        insta::assert_snapshot!("restore_human", env.redact(&run.stdout));
    });
}

#[test]
fn restore_from_an_older_save_rewrites_a_folder() {
    let env = project();

    let run = env.paper(&["restore", "--from", "last~1", "sections"]);

    run.assert_code(0);
    assert_eq!(env.read("sections/intro.tex"), "Intro v1\n");
    assert_eq!(
        env.git_work(&["status", "--porcelain"]).trim(),
        "M sections/intro.tex"
    );
    env.fsck();
}

#[test]
fn restore_deletes_a_file_that_did_not_exist_at_the_point() {
    let env = project();

    let run = env.paper(&["restore", "--from", "last~2", "related.tex"]);

    run.assert_code(0);
    assert!(!env.work.join("related.tex").exists());
    assert!(run.stdout.contains("deleted"), "{run:#?}");
    assert!(run.stdout.contains("related.tex"), "{run:#?}");
    assert_eq!(
        env.git_work(&["status", "--porcelain"]).trim(),
        "D related.tex"
    );
}

#[test]
fn restore_takes_paths_relative_to_where_paper_runs() {
    let env = project();
    env.write("sections/intro.tex", "scribbles\n");

    let run = env.paper_in(&env.work.join("sections"), &[], &["restore", "intro.tex"]);

    run.assert_code(0);
    assert_eq!(env.read("sections/intro.tex"), "Intro v2\n");
}

#[cfg(unix)]
#[test]
fn restore_lists_the_files_it_skipped_and_why() {
    let env = project();
    std::fs::remove_file(env.work.join("main.tex")).unwrap();
    std::os::unix::fs::symlink("related.tex", env.work.join("main.tex")).unwrap();

    let run = env.paper(&["restore", "main.tex"]);

    run.assert_code(0);
    assert!(
        std::fs::symlink_metadata(env.work.join("main.tex"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the symlink stays"
    );
    assert!(run.stdout.contains("Skipped"), "{run:#?}");
    assert!(run.stdout.contains("main.tex"), "{run:#?}");
}

#[test]
fn restore_of_an_unknown_path_is_a_user_error_and_changes_nothing_on_disk() {
    let env = project();
    env.write("sections/intro.tex", "scribbles\n");

    let run = env.paper(&["restore", "sections/intro.tex", "nope.tex"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("nope.tex"), "{run:#?}");
    assert_eq!(env.read("sections/intro.tex"), "scribbles\n");
    settings().bind(|| {
        insta::assert_snapshot!("restore_unknown_path", env.redact(&run.stderr));
    });
}

#[test]
fn restore_without_paths_is_a_user_error() {
    let env = project();
    let run = env.paper(&["restore"]);
    run.assert_code(1);
    assert!(run.stdout.is_empty(), "{run:#?}");
    assert!(!run.stderr.is_empty(), "{run:#?}");
}

#[test]
fn restore_from_an_unknown_point_is_a_user_error() {
    let env = project();
    let run = env.paper(&["restore", "--from", "nope", "main.tex"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("nope"), "{run:#?}");
}

#[test]
fn quiet_restore_prints_nothing_on_success() {
    let env = project();
    env.write("main.tex", "scribbles\n");
    let run = env.paper(&["-q", "restore", "main.tex"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "");
    assert_eq!(env.read("main.tex"), "\\documentclass{article}\n");
}

#[test]
fn restore_json_has_a_fixed_key_set() {
    let env = project();
    env.write("sections/intro.tex", "scribbles\n");
    env.write("main.tex", "scribbles\n");

    let run = env.paper(&["--json", "restore", "sections", "main.tex"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_keys(&json, &["schema", "from", "changes", "skipped"]);
    assert_eq!(json["from"], env.git_work(&["rev-parse", "HEAD"]).trim());
    let changes = json["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2, "{run:#?}");
    for change in changes {
        assert_keys(change, &["path", "kind"]);
    }
    assert_eq!(changes[0]["path"], "main.tex");
    assert_eq!(changes[0]["kind"], "modified");
    assert_eq!(json["skipped"], serde_json::json!([]));
    settings().bind(|| {
        insta::assert_snapshot!("restore_json", env.redact_json(&run.stdout));
    });
}

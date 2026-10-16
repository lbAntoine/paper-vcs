//! `paper status`: the header, the unsaved changes with line counts or sizes, and what was
//! left out.

mod common;

use common::{Env, assert_keys, binary, settings};

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

/// Edits one file, adds a binary file and deletes another.
fn edit_draft(env: &Env) {
    env.write("sections/intro.tex", "Intro v3\nMore text\n");
    env.write_bytes("figures/plot.bin", &binary(2048, 7));
    std::fs::remove_file(env.work.join("related.tex")).unwrap();
}

#[test]
fn status_shows_the_header_and_each_change_with_counts_or_sizes_sorted() {
    let env = project();
    edit_draft(&env);

    let run = env.paper(&["status"]);

    run.assert_code(0);
    let id = env.change_id("HEAD");
    assert!(
        run.stdout
            .starts_with(&format!("On main \u{b7} last save {}", &id[..4])),
        "header: {run:#?}"
    );
    assert!(run.stdout.contains("\"Rewrite intro\""), "{run:#?}");
    assert!(run.stdout.contains("Draft (unsaved):"), "{run:#?}");
    let line_for = |needle: &str| {
        run.stdout
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no line for {needle}: {run:#?}"))
    };
    assert!(
        line_for("sections/intro.tex").contains("modified"),
        "{run:#?}"
    );
    assert!(
        line_for("sections/intro.tex").contains("+2 \u{2212}1 lines"),
        "{run:#?}"
    );
    assert!(line_for("figures/plot.bin").contains("new"), "{run:#?}");
    assert!(line_for("figures/plot.bin").contains("(2 KB)"), "{run:#?}");
    assert!(line_for("related.tex").contains("deleted"), "{run:#?}");
    let order: Vec<usize> = ["figures/plot.bin", "related.tex", "sections/intro.tex"]
        .iter()
        .map(|path| run.stdout.find(path).unwrap())
        .collect();
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "sorted by path: {run:#?}"
    );
    settings().bind(|| {
        insta::assert_snapshot!("status_human", env.redact(&run.stdout));
    });
}

#[test]
fn status_shows_both_sizes_of_a_changed_binary_file() {
    let env = project();
    env.write_bytes("figures/plot.bin", &binary(2048, 7));
    env.paper(&["save", "Add plot"]).assert_code(0);
    env.write_bytes("figures/plot.bin", &binary(4096, 9));

    let run = env.paper(&["status"]);

    run.assert_code(0);
    assert!(run.stdout.contains("2 KB \u{2192} 4 KB"), "{run:#?}");
}

#[test]
fn status_with_an_empty_draft_says_nothing_is_unsaved() {
    let env = project();

    let run = env.paper(&["status"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Nothing unsaved."), "{run:#?}");
    assert!(run.stdout.contains("On main"), "{run:#?}");
    assert!(!run.stdout.contains("Draft (unsaved):"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("status_nothing_unsaved", env.redact(&run.stdout));
    });
}

#[test]
fn status_with_no_snapshot_does_not_see_an_edit_made_since_the_last_command() {
    let env = project();
    env.write("edited.txt", "made outside paper\n");

    let run = env.paper(&["--no-snapshot", "status"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Nothing unsaved."), "{run:#?}");
    assert!(!run.stdout.contains("edited.txt"), "{run:#?}");
    // A normal run does see it.
    let normal = env.paper(&["status"]);
    assert!(normal.stdout.contains("edited.txt"), "{normal:#?}");
}

#[cfg(unix)]
#[test]
fn status_lists_files_paper_leaves_out() {
    let env = project();
    std::os::unix::fs::symlink("main.tex", env.work.join("alias.tex")).unwrap();

    let run = env.paper(&["status"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Skipped"), "{run:#?}");
    assert!(run.stdout.contains("alias.tex"), "{run:#?}");
}

#[test]
fn status_leaves_git_clean_and_the_saves_alone() {
    let env = project();
    edit_draft(&env);

    env.paper(&["status"]).assert_code(0);

    env.fsck();
    assert_eq!(env.subjects().len(), 3);
}

#[test]
fn quiet_status_prints_nothing() {
    let env = project();
    edit_draft(&env);
    let run = env.paper(&["-q", "status"]);
    run.assert_code(0);
    assert_eq!(run.stdout, "");
}

#[test]
fn status_outside_a_project_is_a_user_error() {
    let env = Env::new();
    env.paper(&["status"]).assert_three_part_error(1);
}

#[test]
fn status_json_has_a_fixed_key_set() {
    let env = project();
    edit_draft(&env);

    let run = env.paper(&["--json", "status"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_keys(
        &json,
        &["schema", "line", "last", "changes", "skipped", "large"],
    );
    assert_eq!(json["line"], "main");
    assert_eq!(json["last"]["message"], "Rewrite intro");
    let change_keys = [
        "path",
        "kind",
        "binary",
        "added",
        "removed",
        "old_bytes",
        "new_bytes",
    ];
    let changes = json["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 3, "{run:#?}");
    for change in changes {
        assert_keys(change, &change_keys);
    }
    assert_eq!(changes[0]["path"], "figures/plot.bin");
    assert_eq!(changes[0]["binary"], true);
    assert_eq!(changes[0]["old_bytes"], serde_json::Value::Null);
    assert_eq!(changes[0]["new_bytes"], 2048);
    assert_eq!(changes[0]["added"], serde_json::Value::Null);
    assert_eq!(changes[2]["path"], "sections/intro.tex");
    assert_eq!(changes[2]["added"], 2);
    assert_eq!(changes[2]["removed"], 1);
    assert_eq!(changes[2]["binary"], false);
    assert_eq!(changes[2]["old_bytes"], serde_json::Value::Null);
    settings().bind(|| {
        insta::assert_snapshot!("status_json", env.redact_json(&run.stdout));
    });
}

#[test]
fn status_json_for_an_empty_draft_has_every_key_with_empty_lists() {
    let env = project();

    let run = env.paper(&["--json", "status"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(
        &json,
        &["schema", "line", "last", "changes", "skipped", "large"],
    );
    for key in ["changes", "skipped", "large"] {
        assert_eq!(json[key], serde_json::json!([]), "{key}");
    }
}

#[test]
fn status_lists_a_nested_repository_as_skipped() {
    let env = project();
    env.git_work(&["init", "-q", "vendor"]);
    env.write("vendor/lib.tex", "nested\n");

    let run = env.paper(&["status"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains("vendor  (a separate git repository)"),
        "{run:#?}"
    );
    settings().bind(|| {
        insta::assert_snapshot!("status_skipped", env.redact(&run.stdout));
    });
}

const TWO_MB: usize = 2 * 1024 * 1024;

/// The warning threshold is lowered through config (env), so a 2 MB file is enough.
#[test]
fn status_lists_large_files_that_are_tracked() {
    let env = project();
    env.write_bytes("data/big.bin", &binary(TWO_MB, 3));

    let run = env.paper_in(
        &env.work,
        &[("PAPER_LARGE_FILES_WARN_MB", "1")],
        &["status"],
    );

    run.assert_code(0);
    assert!(run.stdout.contains("Large files"), "{run:#?}");
    assert!(run.stdout.contains("data/big.bin  (2.0 MB)"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("status_large", env.redact(&run.stdout));
    });
}

#[test]
fn status_skips_new_files_over_the_project_max_mb() {
    let env = project();
    let set = env.paper(&["config", "set", "--project", "large_files.max_mb", "1"]);
    set.assert_code(0);
    env.write_bytes("data/big.bin", &binary(TWO_MB, 3));

    let run = env.paper(&["status"]);

    run.assert_code(0);
    assert!(run.stdout.contains("data/big.bin"), "{run:#?}");
    assert!(run.stdout.contains("too large to track"), "{run:#?}");
    assert!(!run.stdout.contains("new       data/big.bin"), "{run:#?}");
}

#[test]
fn the_environment_overrides_the_project_max_mb() {
    let env = project();
    env.paper(&["config", "set", "--project", "large_files.max_mb", "1"])
        .assert_code(0);
    env.write_bytes("data/big.bin", &binary(TWO_MB, 3));

    let run = env.paper_in(
        &env.work,
        &[("PAPER_LARGE_FILES_MAX_MB", "50")],
        &["status"],
    );

    run.assert_code(0);
    assert!(run.stdout.contains("new       data/big.bin"), "{run:#?}");
}

#[test]
fn the_user_config_sets_the_limit_when_the_project_does_not() {
    let env = project();
    env.paper(&["config", "set", "large_files.max_mb", "1"])
        .assert_code(0);
    env.write_bytes("data/big.bin", &binary(TWO_MB, 3));

    let run = env.paper(&["status"]);

    run.assert_code(0);
    assert!(run.stdout.contains("too large to track"), "{run:#?}");
}

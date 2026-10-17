//! `paper undo`, `paper redo` and `paper undo --to`, driven through the real binary and
//! cross-checked with the real `git` binary and the files on disk.

mod common;

use std::fs;

use common::{Env, assert_keys, save_all, settings};
use paper_core::project::Project;
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;
use serde_json::json;

const WARNING_TAIL: &str = "were left as they are (see above); your next paper command will \
                            record them as edits, and that clears what you can redo.";

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

/// insta settings that blank op ids, which are random.
fn id_settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r#""(id|short_id)": "[0-9a-f]{4,32}""#, r#""$1": "[ID]""#);
    settings
}

/// `(id, short_id, description)` of every activity entry, newest first.
fn entries(env: &Env) -> Vec<(String, String, String)> {
    let run = env.paper(&["activity", "-n", "100", "--json"]);
    run.assert_code(0);
    run.json()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["id"].as_str().unwrap().to_owned(),
                e["short_id"].as_str().unwrap().to_owned(),
                e["description"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn short_id_of(env: &Env, description: &str) -> String {
    entries(env)
        .into_iter()
        .find(|(_, _, d)| d == description)
        .unwrap_or_else(|| panic!("no activity entry {description:?}"))
        .1
}

fn ops_dir(env: &Env) -> std::path::PathBuf {
    env.work.join(".git/paper/activity/ops")
}

fn op_count(env: &Env) -> usize {
    fs::read_dir(ops_dir(env)).unwrap().count()
}

/// Everything a refused or failed command must leave alone.
fn state(env: &Env) -> (Vec<String>, String, String, usize) {
    (
        env.subjects(),
        env.git_work(&["status", "--porcelain"]),
        env.read("sections/intro.tex"),
        op_count(env),
    )
}

// ---- undo and redo after a save ----

#[test]
fn undo_after_a_save_takes_the_save_off_the_line_and_keeps_the_edits_unsaved() {
    let env = project();

    let run = env.paper(&["undo"]);

    run.assert_code(0);
    assert!(
        run.stdout.starts_with("Undid: save \"Rewrite intro\"\n"),
        "{run:#?}"
    );
    assert_eq!(env.subjects(), ["Add related work", "Start project"]);
    assert_eq!(env.read("sections/intro.tex"), "Intro v2\n");
    let status = env.paper(&["status"]);
    status.assert_code(0);
    assert!(status.stdout.contains("sections/intro.tex"), "{status:#?}");
    assert_eq!(
        env.git_work(&["status", "--porcelain"]).trim(),
        "M sections/intro.tex"
    );
    env.fsck();
}

#[test]
fn redo_brings_the_undone_save_back() {
    let env = project();
    env.paper(&["undo"]).assert_code(0);

    let run = env.paper(&["redo"]);

    run.assert_code(0);
    assert!(
        run.stdout.starts_with("Redid: save \"Rewrite intro\"\n"),
        "{run:#?}"
    );
    assert_eq!(
        env.subjects(),
        ["Rewrite intro", "Add related work", "Start project"]
    );
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    env.fsck();
}

#[test]
fn a_second_undo_steps_back_through_the_edits_and_rewrites_the_files() {
    let env = project();
    env.paper(&["undo"]).assert_code(0);

    let run = env.paper(&["undo"]);

    run.assert_code(0);
    assert!(
        run.stdout.starts_with("Undid: edited sections/intro.tex\n"),
        "{run:#?}"
    );
    assert!(run.stdout.contains("modified"), "{run:#?}");
    assert!(run.stdout.contains("sections/intro.tex"), "{run:#?}");
    assert_eq!(env.read("sections/intro.tex"), "Intro v1\n");
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
}

// ---- undo with unsaved edits ----

#[test]
fn undo_with_unsaved_edits_takes_the_edits_back_first_and_lists_the_files() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");
    env.write("notes.tex", "scratch\n");

    let run = env.paper(&["undo"]);

    run.assert_code(0);
    assert!(run.stdout.starts_with("Undid: edited "), "{run:#?}");
    assert!(run.stdout.contains("sections/intro.tex"), "{run:#?}");
    assert!(run.stdout.contains("notes.tex"), "{run:#?}");
    assert_eq!(env.read("sections/intro.tex"), "Intro v2\n");
    assert!(!env.work.join("notes.tex").exists());
    assert_eq!(env.subjects().len(), 3, "the saves are untouched");
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    env.fsck();
}

// ---- undo --to ----

#[test]
fn undo_to_goes_back_to_the_entry_named_by_its_short_id_from_activity() {
    let env = project();
    let target = short_id_of(&env, "save \"Add related work\"");

    let run = env.paper(&["undo", "--to", &target]);

    run.assert_code(0);
    assert!(
        run.stdout.starts_with(&format!(
            "Went back to {target}: save \"Add related work\"\n"
        )),
        "{run:#?}"
    );
    assert_eq!(env.subjects(), ["Add related work", "Start project"]);
    assert_eq!(env.read("sections/intro.tex"), "Intro v1\n");
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    env.fsck();
}

#[test]
fn undo_to_an_unknown_id_is_a_user_error_and_changes_nothing() {
    let env = project();
    let before = state(&env);

    let run = env.paper(&["undo", "--to", "zzzz"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("zzzz"), "{run:#?}");
    assert_eq!(state(&env), before);
    settings().bind(|| {
        insta::assert_snapshot!("undo_to_unknown", env.redact(&run.stderr));
    });
}

#[test]
fn undo_to_an_empty_id_is_a_user_error_and_changes_nothing() {
    let env = project();
    let before = state(&env);

    let run = env.paper(&["undo", "--to", ""]);

    run.assert_three_part_error(1);
    assert_eq!(state(&env), before);
}

#[test]
fn undo_to_an_ambiguous_prefix_lists_the_candidates_and_changes_nothing() {
    let env = project();
    let head = &entries(&env)[0].0;
    let mut op: serde_json::Value =
        serde_json::from_slice(&fs::read(ops_dir(&env).join(format!("{head}.json"))).unwrap())
            .unwrap();
    let names: Vec<String> = fs::read_dir(ops_dir(&env))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let prefix = (0..16u32)
        .map(|n| format!("{n:x}fff"))
        .find(|p| !names.iter().any(|name| name.starts_with(p.as_str())))
        .unwrap();
    for (suffix, description) in [("1", "crafted one"), ("2", "crafted two")] {
        let id = format!("{prefix}{suffix}{}", "0".repeat(27));
        op["id"] = id.clone().into();
        op["position"] = id.clone().into();
        op["description"] = description.into();
        fs::write(
            ops_dir(&env).join(format!("{id}.json")),
            serde_json::to_vec_pretty(&op).unwrap(),
        )
        .unwrap();
    }
    let before = state(&env);

    let run = env.paper(&["undo", "--to", &prefix]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("crafted one"), "{run:#?}");
    assert!(run.stderr.contains("crafted two"), "{run:#?}");
    assert_eq!(state(&env), before);
}

// ---- nothing to do ----

#[test]
fn nothing_to_undo_exits_zero_and_changes_nothing() {
    let env = Env::new();
    env.write("main.tex", "x\n");
    Project::init(
        &env.work,
        Identity {
            name: "Mina Park".to_owned(),
            email: "mina@uni.example".to_owned(),
        },
        &SnapshotOptions::default(),
    )
    .unwrap();
    let ops = op_count(&env);

    let run = env.paper(&["undo"]);

    run.assert_code(0);
    assert_eq!(run.stdout, "Nothing to undo.\n");
    assert_eq!(op_count(&env), ops);
}

#[test]
fn nothing_to_redo_exits_zero_and_changes_nothing() {
    let env = project();
    let before = state(&env);

    let run = env.paper(&["redo"]);

    run.assert_code(0);
    assert_eq!(run.stdout, "Nothing to redo.\n");
    assert_eq!(state(&env), before);
}

#[test]
fn nothing_outcomes_in_json_have_every_key_with_null_and_empty_values() {
    let env = project();

    let redo = env.paper(&["--json", "redo"]);

    redo.assert_code(0);
    assert_eq!(
        redo.json(),
        json!({
            "schema": 1,
            "outcome": "nothing_to_redo",
            "entry": null,
            "changed": [],
            "skipped": [],
            "files_match": null
        })
    );
    // Walk back to the first entry, then ask once more.
    let mut steps = 0;
    while !env.paper(&["undo"]).stdout.starts_with("Nothing") {
        steps += 1;
        assert!(steps < 20, "undo never reached the first entry");
    }
    let undo = env.paper(&["undo", "--json"]);
    undo.assert_code(0);
    assert_eq!(
        undo.json(),
        json!({
            "schema": 1,
            "outcome": "nothing_to_undo",
            "entry": null,
            "changed": [],
            "skipped": [],
            "files_match": null
        })
    );
}

// ---- refusals after plain git ----

#[test]
fn undo_after_a_plain_git_commit_exits_with_saves_error_and_changes_nothing() {
    let env = project();
    env.git_work(&["commit", "--quiet", "--allow-empty", "-m", "Plain git"]);
    env.write("sections/intro.tex", "edit after the commit\n");
    let before = state(&env);
    let saved = env.paper(&["save", "Try"]);

    let run = env.paper(&["undo"]);

    run.assert_three_part_error(saved.code);
    assert_eq!(
        run.stderr.lines().next(),
        saved.stderr.lines().next(),
        "{run:#?} vs {saved:#?}"
    );
    assert_eq!(state(&env), before);
    for args in [&["redo"][..], &["undo", "--to", "0000"]] {
        env.paper(args).assert_three_part_error(saved.code);
    }
    assert_eq!(state(&env), before);
    env.fsck();
}

#[test]
fn undo_after_a_git_switch_exits_with_saves_error_and_changes_nothing() {
    let env = project();
    env.git_work(&["switch", "--quiet", "-c", "side"]);
    env.write("sections/intro.tex", "edit on side\n");
    let before = state(&env);
    let saved = env.paper(&["save", "Try"]);

    let run = env.paper(&["undo"]);

    run.assert_three_part_error(saved.code);
    assert_eq!(run.stderr.lines().next(), saved.stderr.lines().next());
    assert_eq!(state(&env), before);
}

// ---- skipped files ----

/// A project where undoing twice cannot restore `chap` (an ignored file is in the way).
fn blocked_project() -> Env {
    let env = Env::new();
    env.write(".gitignore", "*.log\n");
    env.write("chap", "a file\n");
    env.write("keep.tex", "keep\n");
    let (mut project, _) = Project::init(
        &env.work,
        Identity {
            name: "Mina Park".to_owned(),
            email: "mina@uni.example".to_owned(),
        },
        &SnapshotOptions::default(),
    )
    .unwrap();
    fs::remove_file(env.work.join("chap")).unwrap();
    env.write("chap/one.tex", "1\n");
    save_all(&mut project, "folder");
    env.write("chap/cache.log", "my ignored log\n");
    env
}

#[test]
fn a_checkout_that_skips_files_prints_each_with_its_reason_and_the_warning() {
    let env = blocked_project();
    env.paper(&["undo"]).assert_code(0);

    let run = env.paper(&["undo"]);

    run.assert_code(0);
    assert_eq!(env.read("chap/cache.log"), "my ignored log\n");
    assert!(run.stdout.contains("Skipped"), "{run:#?}");
    assert!(
        run.stdout
            .contains("chap  (a file paper doesn't track is in the way)"),
        "{run:#?}"
    );
    assert!(
        run.stdout
            .contains("chap/one.tex  (kept because chap can't be restored)"),
        "{run:#?}"
    );
    assert!(
        run.stdout.contains(&format!("2 files {WARNING_TAIL}")),
        "{run:#?}"
    );
    env.fsck();
}

#[test]
fn skipped_files_appear_in_json_with_files_match_false() {
    let env = blocked_project();
    env.paper(&["undo"]).assert_code(0);

    let run = env.paper(&["--json", "undo"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["outcome"], "undone");
    assert_eq!(json["files_match"], false);
    let skipped = json["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 2, "{run:#?}");
    assert_eq!(skipped[0]["path"], "chap");
    assert_eq!(skipped[0]["reason"], "blocked");
    assert_eq!(skipped[1]["path"], "chap/one.tex");
    assert_eq!(skipped[1]["reason"], "kept_for_swap");
}

#[test]
fn a_clean_checkout_prints_no_warning() {
    let env = project();

    let run = env.paper(&["undo"]);

    run.assert_code(0);
    assert!(run.stdout.starts_with("Undid: "), "{run:#?}");
    assert!(!run.stdout.contains("were left as they are"), "{run:#?}");
    assert!(!run.stdout.contains("Skipped"), "{run:#?}");
}

// ---- JSON ----

#[test]
fn undo_redo_and_undo_to_json_have_the_fixed_key_set() {
    let env = project();
    let target = short_id_of(&env, "save \"Add related work\"");

    let undone = env.paper(&["--json", "undo"]);
    let redone = env.paper(&["--json", "redo"]);
    let went_back = env.paper(&["--json", "undo", "--to", &target]);

    for (run, outcome) in [
        (&undone, "undone"),
        (&redone, "redone"),
        (&went_back, "went_back"),
    ] {
        run.assert_code(0);
        let json = run.json();
        assert_keys(
            &json,
            &[
                "schema",
                "outcome",
                "entry",
                "changed",
                "skipped",
                "files_match",
            ],
        );
        assert_eq!(json["schema"], 1);
        assert_eq!(json["outcome"], outcome);
        assert_keys(
            &json["entry"],
            &["id", "short_id", "time", "kind", "description"],
        );
        assert_eq!(json["files_match"], true);
        assert!(json["changed"].is_array() && json["skipped"].is_array());
    }
    assert_eq!(
        undone.json()["entry"]["description"],
        "save \"Rewrite intro\""
    );
    assert_eq!(
        redone.json()["entry"]["description"],
        "save \"Rewrite intro\""
    );
    assert_eq!(went_back.json()["entry"]["short_id"], target.as_str());
    settings().bind(|| {
        id_settings().bind(|| {
            insta::assert_snapshot!("undo_json", env.redact_json(&undone.stdout));
            insta::assert_snapshot!("redo_json", env.redact_json(&redone.stdout));
            insta::assert_snapshot!("undo_to_json", env.redact_json(&went_back.stdout));
        });
    });
}

#[test]
fn undo_json_lists_changed_files_in_the_shape_restore_uses() {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");

    let run = env.paper(&["--json", "undo"]);

    run.assert_code(0);
    let changed = run.json()["changed"].clone();
    let changed = changed.as_array().unwrap();
    assert_eq!(changed.len(), 1, "{run:#?}");
    assert_keys(&changed[0], &["path", "kind"]);
    assert_eq!(changed[0]["path"], "sections/intro.tex");
    assert_eq!(changed[0]["kind"], "modified");
}

// ---- activity ----

#[test]
fn activity_lists_undo_redo_and_go_back_entries() {
    let env = project();
    let target = short_id_of(&env, "save \"Add related work\"");
    env.paper(&["undo"]).assert_code(0);
    env.paper(&["redo"]).assert_code(0);
    env.paper(&["undo", "--to", &target]).assert_code(0);

    let descriptions: Vec<String> = entries(&env).into_iter().map(|e| e.2).collect();

    assert!(
        descriptions[0].starts_with(&format!("go back to {target}: ")),
        "{descriptions:?}"
    );
    assert_eq!(descriptions[1], "redo: save \"Rewrite intro\"");
    assert_eq!(descriptions[2], "undo: save \"Rewrite intro\"");
    let human = env.paper(&["activity"]);
    assert!(human.stdout.contains("undo: save \"Rewrite intro\""));
    assert!(human.stdout.contains("redo: save \"Rewrite intro\""));
    assert!(human.stdout.contains("go back to "));
}

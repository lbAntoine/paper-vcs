//! `paper gc` (PV-027), driven through the real binary: human and JSON output, `--dry-run`,
//! bad durations, and the notes `paper activity` and `paper undo` give once history was
//! cleaned up. Op files are dated by hand; the repository is cross-checked with `git`.
//!
//! The human layout and the JSON snapshot are not written here: GREEN accepts them.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use common::{Env, assert_keys};
use serde_json::Value;

const NOTE: &str = "older history was cleaned up by `paper gc`";

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

fn ops_dir(env: &Env) -> std::path::PathBuf {
    env.work.join(".git/paper/activity/ops")
}

fn read_op(env: &Env, id: &str) -> Value {
    serde_json::from_slice(&fs::read(ops_dir(env).join(format!("{id}.json"))).unwrap()).unwrap()
}

/// The op ids from the head back to the first op.
fn chain(env: &Env) -> Vec<String> {
    let head = fs::read_to_string(env.work.join(".git/paper/activity/head")).unwrap();
    let mut ids = Vec::new();
    let mut next = Some(head.trim().to_owned());
    while let Some(id) = next {
        next = read_op(env, &id)["parent"].as_str().map(str::to_owned);
        ids.push(id);
    }
    ids
}

/// Dates the oldest `count` ops of the chain `days` days ago.
fn backdate_oldest(env: &Env, count: usize, days: i64) {
    for id in chain(env).iter().rev().take(count) {
        let mut op = read_op(env, id);
        let then = jiff::Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_hours(24 * days))
            .unwrap();
        op["time"] = then.to_string().into();
        let mut json = serde_json::to_vec_pretty(&op).unwrap();
        json.push(b'\n');
        fs::write(ops_dir(env).join(format!("{id}.json")), json).unwrap();
    }
}

fn git_dir_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path.strip_prefix(root).unwrap().to_string_lossy();
                out.insert(name.replace('\\', "/"), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    let git_dir = dir.join(".git");
    walk(&git_dir, &git_dir, &mut out);
    out
}

fn activity_count(env: &Env) -> usize {
    let run = env.paper(&["activity", "-n", "100", "--json"]);
    run.assert_code(0);
    run.json()["entries"].as_array().unwrap().len()
}

// ---- the helpers themselves (these pass without gc) ----

#[test]
fn backdating_shows_in_activity_and_keeps_the_chain_walkable() {
    let env = project();
    let before = chain(&env);
    backdate_oldest(&env, 2, 200);
    assert_eq!(chain(&env), before);
    let run = env.paper(&["activity", "-n", "100", "--json"]);
    run.assert_code(0);
    let entries = run.json()["entries"].as_array().unwrap().clone();
    let oldest = entries.last().unwrap()["time"].as_str().unwrap();
    let then: jiff::Timestamp = oldest.parse().unwrap();
    assert!(jiff::Timestamp::now().duration_since(then).as_hours() >= 24 * 199);
    let newest = entries[0]["time"].as_str().unwrap();
    let newest: jiff::Timestamp = newest.parse().unwrap();
    assert!(jiff::Timestamp::now().duration_since(newest).as_hours() < 24);
}

#[test]
fn without_a_gc_activity_and_undo_carry_no_note() {
    let env = project();
    let run = env.paper(&["activity"]);
    run.assert_code(0);
    assert!(!run.stdout.contains("paper gc"), "{run:#?}");
    let mut last = env.paper(&["undo"]);
    for _ in 0..20 {
        if last.stdout.starts_with("Nothing to undo.") {
            break;
        }
        last = env.paper(&["undo"]);
    }
    assert_eq!(last.stdout, "Nothing to undo.\n");
}

// ---- output ----

#[test]
fn json_has_the_fixed_key_set_and_the_counts() {
    let env = project();
    let ops = chain(&env);
    backdate_oldest(&env, 3, 200);

    let run = env.paper(&["gc", "--older-than", "30d", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(
        &json,
        &[
            "schema",
            "dry_run",
            "removed_entries",
            "kept_entries",
            "horizon",
            "swept",
        ],
    );
    assert_eq!(json["schema"], 1);
    assert_eq!(json["dry_run"], false);
    assert_eq!(json["removed_entries"], 3);
    assert_eq!(json["kept_entries"], ops.len() - 3);
    assert_eq!(json["swept"], serde_json::json!([]));
    let date =
        &fs::read_to_string(ops_dir(&env).join(format!("{}.json", ops[ops.len() - 4]))).unwrap();
    let time = serde_json::from_str::<Value>(date).unwrap()["time"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        json["horizon"].as_str().unwrap().contains(&time[..10]),
        "the horizon names the date of the oldest kept entry: {json}"
    );
}

#[test]
fn the_human_output_says_what_was_removed_kept_and_that_git_gc_is_left_to_the_user() {
    let env = project();
    let ops = chain(&env);
    backdate_oldest(&env, 3, 200);

    let run = env.paper(&["gc", "--older-than", "30d"]);

    run.assert_code(0);
    assert!(run.stderr.is_empty(), "{run:#?}");
    assert!(run.stdout.contains('3'), "removed count: {run:#?}");
    assert!(
        run.stdout.contains(&(ops.len() - 3).to_string()),
        "kept count: {run:#?}"
    );
    assert!(
        run.stdout.contains("git gc"),
        "paper says it does not run git gc: {run:#?}"
    );
    env.fsck();
}

#[test]
fn the_default_window_is_90_days() {
    let env = project();
    let ops = chain(&env);
    backdate_oldest(&env, 4, 100);
    // The fifth-oldest is 50 days old: inside 90 days, outside 30.
    let mut op = read_op(&env, &ops[ops.len() - 5]);
    op["time"] = jiff::Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(24 * 50))
        .unwrap()
        .to_string()
        .into();
    let mut json = serde_json::to_vec_pretty(&op).unwrap();
    json.push(b'\n');
    fs::write(
        ops_dir(&env).join(format!("{}.json", ops[ops.len() - 5])),
        json,
    )
    .unwrap();

    let run = env.paper(&["gc", "--json"]);

    run.assert_code(0);
    assert_eq!(run.json()["removed_entries"], 4);
}

#[test]
fn weeks_are_accepted() {
    let env = project();
    backdate_oldest(&env, 2, 100);
    let run = env.paper(&["gc", "--older-than", "2w", "--json"]);
    run.assert_code(0);
    assert_eq!(run.json()["removed_entries"], 2);
}

// ---- effects ----

#[test]
fn gc_removes_old_ops_and_activity_stops_at_the_horizon_with_a_note() {
    let env = project();
    let ops = chain(&env);
    backdate_oldest(&env, 3, 200);

    env.paper(&["gc", "--older-than", "30d"]).assert_code(0);

    assert_eq!(fs::read_dir(ops_dir(&env)).unwrap().count(), ops.len() - 3);
    assert!(env.work.join(".git/paper/activity/horizon").is_file());
    let run = env.paper(&["activity", "-n", "100"]);
    run.assert_code(0);
    let entry_lines = run.stdout.lines().filter(|l| l.contains("  ")).count();
    assert!(entry_lines >= ops.len() - 3, "{run:#?}");
    assert!(
        run.stdout.to_lowercase().contains(NOTE),
        "activity says older history was cleaned up: {run:#?}"
    );
    assert_eq!(activity_count(&env), ops.len() - 3);
    env.fsck();
}

#[test]
fn undo_reaches_the_horizon_then_says_nothing_to_undo_with_the_note() {
    let env = project();
    let ops = chain(&env);
    backdate_oldest(&env, 3, 200);
    env.paper(&["gc", "--older-than", "30d"]).assert_code(0);

    for step in 0..(ops.len() - 3 - 1) {
        let run = env.paper(&["undo"]);
        run.assert_code(0);
        assert!(run.stdout.starts_with("Undid:"), "step {step}: {run:#?}");
    }
    let run = env.paper(&["undo"]);

    run.assert_code(0);
    assert!(run.stdout.starts_with("Nothing to undo."), "{run:#?}");
    assert!(run.stdout.to_lowercase().contains(NOTE), "{run:#?}");
    env.fsck();
}

#[test]
fn gc_records_no_activity_entry() {
    let env = project();
    let before = activity_count(&env);
    backdate_oldest(&env, 3, 200);
    env.paper(&["gc", "--older-than", "30d"]).assert_code(0);
    assert_eq!(activity_count(&env), before - 3);
    let run = env.paper(&["activity", "-n", "1", "--json"]);
    assert_ne!(run.json()["entries"][0]["description"], "gc");
}

#[test]
fn a_second_run_removes_nothing() {
    let env = project();
    backdate_oldest(&env, 3, 200);
    env.paper(&["gc", "--older-than", "30d"]).assert_code(0);

    let run = env.paper(&["gc", "--older-than", "30d", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["removed_entries"], 0);
    assert_eq!(json["swept"], serde_json::json!([]));
}

#[test]
fn leftovers_are_swept_and_listed() {
    let env = project();
    fs::write(ops_dir(&env).join(".tmp-op"), "x").unwrap();
    fs::write(env.work.join(".git/paper/view.json.v1"), "{}").unwrap();

    let run = env.paper(&["gc", "--json"]);

    run.assert_code(0);
    assert_eq!(
        run.json()["swept"],
        serde_json::json!(["activity/ops/.tmp-op", "view.json.v1"])
    );
    assert!(!ops_dir(&env).join(".tmp-op").exists());
    assert!(!env.work.join(".git/paper/view.json.v1").exists());
}

// ---- --dry-run ----

#[test]
fn a_dry_run_changes_nothing_and_says_so() {
    let env = project();
    backdate_oldest(&env, 3, 200);
    fs::write(ops_dir(&env).join(".tmp-op"), "x").unwrap();
    let before = git_dir_bytes(&env.work);
    let files = env.read("sections/intro.tex");

    let human = env.paper(&["gc", "--older-than", "30d", "--dry-run"]);
    let json = env.paper(&["gc", "--older-than", "30d", "--dry-run", "--json"]);

    human.assert_code(0);
    assert!(
        human.stdout.to_lowercase().contains("dry run"),
        "{human:#?}"
    );
    json.assert_code(0);
    assert_eq!(json.json()["dry_run"], true);
    assert_eq!(json.json()["removed_entries"], 3);
    assert_eq!(
        json.json()["swept"],
        serde_json::json!(["activity/ops/.tmp-op"])
    );
    assert_eq!(before, git_dir_bytes(&env.work), "every file and ref");
    assert_eq!(files, env.read("sections/intro.tex"));
}

// ---- bad durations ----

#[test]
fn a_duration_below_one_day_or_garbage_is_a_three_part_user_error() {
    let env = project();
    let before = git_dir_bytes(&env.work);
    for bad in ["0d", "0w", "soon", "1h", "-3d", "90", ""] {
        let run = env.paper(&["gc", "--older-than", bad]);
        run.assert_three_part_error(1);
    }
    assert_eq!(before, git_dir_bytes(&env.work), "nothing was touched");
}

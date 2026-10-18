//! `paper ignore` through the binary (PV-018): human and JSON output, refusals, listing, and
//! cross-checks with the real `git` binary and the files on disk.
//!
//! No snapshots are written by hand: output is checked on key phrases and parsed JSON.
//! GREEN adds the insta snapshots after reading the real output.

mod common;

use std::fs;

use common::{Env, assert_keys};
use paper_core::project::Project;
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;
use serde_json::json;

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

/// A project whose first save holds `main.tex`, `data/keep.csv` and two files under `data/raw`.
fn project() -> Env {
    let env = Env::new();
    env.write("main.tex", "\\documentclass{article}\n");
    env.write("data/keep.csv", "keep\n");
    env.write("data/raw/a.csv", "1,2\n");
    env.write("data/raw/b.csv", "3,4\n");
    Project::init(&env.work, identity(), &SnapshotOptions::default()).unwrap();
    env
}

fn ops(env: &Env) -> usize {
    fs::read_dir(env.work.join(".git/paper/activity/ops"))
        .unwrap()
        .count()
}

fn draft_files(env: &Env) -> Vec<String> {
    env.git_work(&["ls-tree", "-r", "--name-only", "refs/paper/draft/main"])
        .lines()
        .map(str::to_owned)
        .collect()
}

// ---- adding rules ----

#[test]
fn ignoring_a_saved_folder_says_what_it_did() {
    let env = project();

    let run = env.paper(&["ignore", "data/raw"]);

    run.assert_code(0);
    assert!(run.stderr.is_empty(), "{run:#?}");
    assert!(run.stdout.contains("/data/raw/"), "{run:#?}");
    assert!(run.stdout.contains("data/raw/a.csv"), "{run:#?}");
    assert!(run.stdout.contains("data/raw/b.csv"), "{run:#?}");
    let lower = run.stdout.to_lowercase();
    assert!(lower.contains("next save"), "{run:#?}");
    assert!(lower.contains("earlier saves"), "{run:#?}");
    assert_eq!(
        env.read("data/raw/a.csv"),
        "1,2\n",
        "the file stays on disk"
    );
    assert!(!draft_files(&env).iter().any(|f| f.starts_with("data/raw/")));
    assert!(
        env.read(".gitignore")
            .contains("# >>> paper-vcs: ignored >>>\n/data/raw/\n# <<< paper-vcs: ignored <<<\n")
    );
    env.fsck();
}

#[test]
fn the_next_save_records_the_removal_and_history_keeps_earlier_versions() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);

    env.paper(&["save", "Stop tracking raw data"])
        .assert_code(0);

    let tracked = env.git_work(&["ls-files"]);
    assert!(!tracked.contains("data/raw/"), "{tracked}");
    assert!(tracked.contains("data/keep.csv"), "{tracked}");
    let rule = env.git_work(&["check-ignore", "-v", "--no-index", "data/raw/a.csv"]);
    assert!(rule.contains("/data/raw/"), "{rule}");
    let earlier = env.git_work(&["ls-tree", "-r", "--name-only", "main~1"]);
    assert!(earlier.contains("data/raw/a.csv"), "{earlier}");
    let history = env.paper(&["history", "data/raw/a.csv"]);
    history.assert_code(0);
    assert!(history.stdout.contains("Start project"), "{history:#?}");
    env.fsck();
}

#[test]
fn a_pattern_is_written_as_typed_and_the_output_says_it_applies_to_the_whole_project() {
    let env = project();
    env.write("sections/x.aux", "x\n");
    env.paper(&["status"]).assert_code(0);

    let run = env.paper_in(&env.work.join("sections"), &[], &["ignore", "*.aux"]);

    run.assert_code(0);
    assert!(env.read(".gitignore").contains("\n*.aux\n"), "{run:#?}");
    assert!(
        run.stdout.to_lowercase().contains("whole project"),
        "{run:#?}"
    );
    assert!(run.stdout.contains("sections/x.aux"), "{run:#?}");
}

#[test]
fn paths_are_relative_to_where_paper_runs() {
    let env = project();

    let run = env.paper_in(&env.work.join("data"), &[], &["ignore", "raw"]);

    run.assert_code(0);
    assert!(env.read(".gitignore").contains("\n/data/raw/\n"));
}

#[test]
fn ignoring_adds_one_activity_entry_and_undo_takes_it_back() {
    let env = project();
    let gitignore_before = env.read(".gitignore");
    let ops_before = ops(&env);
    env.paper(&["ignore", "data/raw", "*.aux"]).assert_code(0);
    assert_eq!(ops(&env), ops_before + 1);
    let activity = env.paper(&["activity", "-n", "1"]);
    assert!(
        activity.stdout.contains("ignore /data/raw/, *.aux"),
        "{activity:#?}"
    );

    let undo = env.paper(&["undo"]);

    undo.assert_code(0);
    assert_eq!(env.read(".gitignore"), gitignore_before);
    assert!(draft_files(&env).contains(&"data/raw/a.csv".to_owned()));
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    env.fsck();
}

#[test]
fn changing_the_profile_afterwards_keeps_paper_block() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);

    env.paper(&["config", "set", "--project", "profile", "markdown"])
        .assert_code(0);

    let gitignore = env.read(".gitignore");
    assert!(
        gitignore.contains("paper-vcs profile: markdown"),
        "{gitignore}"
    );
    assert!(
        gitignore
            .contains("# >>> paper-vcs: ignored >>>\n/data/raw/\n# <<< paper-vcs: ignored <<<\n"),
        "{gitignore}"
    );
    assert!(
        gitignore.find("paper-vcs profile: markdown").unwrap()
            < gitignore.find("paper-vcs: ignored").unwrap()
    );
}

// ---- JSON ----

#[test]
fn json_for_an_add_has_every_key() {
    let env = project();

    let run = env.paper(&["ignore", "data/raw", "*.aux", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(&json, &["schema", "added", "already", "untracked", "entry"]);
    assert_eq!(json["schema"], 1);
    assert_eq!(json["added"], json!(["/data/raw/", "*.aux"]));
    assert_eq!(json["already"], json!([]));
    assert_eq!(
        json["untracked"],
        json!(["data/raw/a.csv", "data/raw/b.csv"])
    );
    let activity = env.paper(&["activity", "-n", "1", "--json"]).json();
    let listed = &activity["entries"][0];
    let keys: Vec<&str> = listed
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_keys(&json["entry"], &keys);
    assert_eq!(json["entry"]["description"], "ignore /data/raw/, *.aux");
    assert_eq!(json["entry"]["kind"], "ignore");
    assert_eq!(json["entry"]["id"], listed["id"]);
}

#[test]
fn json_for_a_no_op_has_a_null_entry() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let ops_before = ops(&env);

    let run = env.paper(&["ignore", "data/raw", "--json"]);

    run.assert_code(0);
    assert_eq!(
        run.json(),
        json!({
            "schema": 1,
            "added": [],
            "already": ["/data/raw/"],
            "untracked": [],
            "entry": null,
        })
    );
    assert_eq!(ops(&env), ops_before, "no entry recorded");
}

#[test]
fn json_for_a_finished_run_lists_the_rule_as_already_and_the_files_untracked() {
    let env = project();
    let mut gitignore = env.read(".gitignore");
    gitignore
        .push_str("\n# >>> paper-vcs: ignored >>>\n/data/raw/\n# <<< paper-vcs: ignored <<<\n");
    env.write(".gitignore", &gitignore);

    let run = env.paper(&["ignore", "data/raw", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["added"], json!([]));
    assert_eq!(json["already"], json!(["/data/raw/"]));
    assert_eq!(
        json["untracked"],
        json!(["data/raw/a.csv", "data/raw/b.csv"])
    );
    assert!(json["entry"].is_object());
}

// ---- nothing to do ----

#[test]
fn nothing_to_do_says_so_and_records_nothing() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let ops_before = ops(&env);
    let gitignore = env.read(".gitignore");

    let run = env.paper(&["ignore", "data/raw"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains("Nothing to do: already ignored."),
        "{run:#?}"
    );
    assert_eq!(ops(&env), ops_before);
    assert_eq!(env.read(".gitignore"), gitignore);
}

#[test]
fn rerunning_after_an_interruption_untracks_and_says_the_rule_was_already_there() {
    let env = project();
    let mut gitignore = env.read(".gitignore");
    gitignore
        .push_str("\n# >>> paper-vcs: ignored >>>\n/data/raw/\n# <<< paper-vcs: ignored <<<\n");
    env.write(".gitignore", &gitignore);

    let run = env.paper(&["ignore", "data/raw"]);

    run.assert_code(0);
    assert!(run.stdout.to_lowercase().contains("already"), "{run:#?}");
    assert!(run.stdout.contains("data/raw/a.csv"), "{run:#?}");
    assert!(!draft_files(&env).iter().any(|f| f.starts_with("data/raw/")));
}

// ---- the threshold ----

#[test]
fn more_than_twenty_files_need_yes() {
    let env = Env::new();
    for i in 0..21 {
        env.write(&format!("big/f{i:02}.txt"), &format!("file {i}\n"));
    }
    Project::init(&env.work, identity(), &SnapshotOptions::default()).unwrap();
    let gitignore = env.read(".gitignore");
    let ops_before = ops(&env);

    let refused = env.paper(&["ignore", "big"]);

    refused.assert_three_part_error(1);
    assert!(refused.stderr.contains("21"), "{refused:#?}");
    for i in 0..5 {
        assert!(
            refused.stderr.contains(&format!("big/f{i:02}.txt")),
            "{refused:#?}"
        );
    }
    assert!(!refused.stderr.contains("big/f05.txt"), "{refused:#?}");
    assert!(refused.stderr.contains("--yes"), "{refused:#?}");
    assert_eq!(env.read(".gitignore"), gitignore, "nothing written");
    assert_eq!(ops(&env), ops_before);

    let done = env.paper(&["ignore", "big", "--yes", "--json"]);
    done.assert_code(0);
    assert_eq!(done.json()["untracked"].as_array().unwrap().len(), 21);
    assert!(!draft_files(&env).iter().any(|f| f.starts_with("big/")));
}

// ---- refusals ----

#[test]
fn refused_arguments_are_three_part_errors_and_write_nothing() {
    let env = project();
    let gitignore = env.read(".gitignore");
    let ops_before = ops(&env);
    let elsewhere = env.root.join("elsewhere");
    let elsewhere = elsewhere.to_string_lossy().into_owned();

    for refused in [
        "",
        "!x",
        ".gitignore",
        "paper.toml",
        ".git/hooks",
        "../outside",
        elsewhere.as_str(),
    ] {
        let run = env.paper(&["ignore", refused]);
        run.assert_three_part_error(1);
        assert_eq!(env.read(".gitignore"), gitignore, "after {refused:?}");
        assert_eq!(ops(&env), ops_before, "after {refused:?}");
    }
}

#[test]
fn refusing_a_bang_points_to_unignore() {
    let env = project();

    let run = env.paper(&["ignore", "!x"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("unignore"), "{run:#?}");
}

// ---- listing ----

fn write_all_five_sources(env: &Env) {
    env.paper(&["ignore", "*.scratch"]).assert_code(0);
    let mut gitignore = env.read(".gitignore");
    gitignore.push_str("\n# my notes\nmine.txt\n");
    env.write(".gitignore", &gitignore);
    env.write("docs/.gitignore", "*.nested\n");
    let mut exclude = fs::read_to_string(env.work.join(".git/info/exclude")).unwrap();
    exclude.push_str("local.tmp\n");
    fs::write(env.work.join(".git/info/exclude"), exclude).unwrap();
}

fn point_global_excludes_at(env: &Env, name: &str, content: &str) {
    let path = env.root.join(name);
    fs::write(&path, content).unwrap();
    let mut config = fs::read_to_string(&env.global_config).unwrap();
    // Forward slashes are valid in git config paths on every platform.
    config.push_str("[core]\n\texcludesFile = ");
    config.push_str(&path.to_string_lossy().replace('\\', "/"));
    config.push('\n');
    fs::write(&env.global_config, config).unwrap();
}

#[test]
fn listing_json_has_fixed_keys_and_every_source() {
    let env = project();
    write_all_five_sources(&env);
    point_global_excludes_at(&env, "global-ignore", "# g\nglobal.tmp\n");

    let run = env.paper(&["ignore", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(&json, &["schema", "rules"]);
    assert_eq!(json["schema"], 1);
    let rules = json["rules"].as_array().unwrap();
    for rule in rules {
        assert_keys(rule, &["rule", "source", "file", "line"]);
        assert!(rule["line"].as_u64().unwrap() >= 1, "{rule}");
        assert!(!rule["rule"].as_str().unwrap().starts_with('#'), "{rule}");
    }
    let find = |source: &str, text: &str| {
        rules
            .iter()
            .find(|r| r["source"] == source && r["rule"] == text)
            .unwrap_or_else(|| panic!("no {source} rule {text}: {json}"))
    };
    assert_eq!(find("paper", "*.scratch")["file"], ".gitignore");
    assert_eq!(find("profile", ".DS_Store")["file"], ".gitignore");
    assert_eq!(find("yours", "mine.txt")["file"], ".gitignore");
    assert_eq!(find("yours", "*.nested")["file"], "docs/.gitignore");
    assert_eq!(find("yours", "*.nested")["line"], 1);
    assert_eq!(find("local", "local.tmp")["file"], ".git/info/exclude");
    let global = find("global", "global.tmp");
    assert!(global["file"].as_str().unwrap().ends_with("global-ignore"));
    assert_eq!(global["line"], 2);
    let gitignore = env.read(".gitignore");
    let line = gitignore.lines().position(|l| l == "mine.txt").unwrap() + 1;
    assert_eq!(find("yours", "mine.txt")["line"], line);
}

#[test]
fn listing_without_core_excludes_file_reads_the_xdg_default() {
    let env = project();
    let xdg = env.xdg_config.join("git");
    fs::create_dir_all(&xdg).unwrap();
    fs::write(xdg.join("ignore"), "xdg.tmp\n").unwrap();

    let run = env.paper(&["ignore", "--json"]);

    run.assert_code(0);
    let json = run.json();
    let rule = json["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["rule"] == "xdg.tmp")
        .unwrap_or_else(|| panic!("{json}"));
    assert_eq!(rule["source"], "global");
    assert!(rule["file"].as_str().unwrap().ends_with("ignore"));
    assert_eq!(rule["line"], 1);
}

#[test]
fn listing_in_words_names_each_source_rule_file_and_line() {
    let env = project();
    write_all_five_sources(&env);
    point_global_excludes_at(&env, "global-ignore", "global.tmp\n");

    let run = env.paper(&["ignore"]);

    run.assert_code(0);
    for phrase in [
        "paper",
        "profile",
        "yours",
        "local",
        "global",
        "*.scratch",
        ".DS_Store",
        "mine.txt",
        "*.nested",
        "local.tmp",
        "global.tmp",
        "docs/.gitignore",
        ".git/info/exclude",
    ] {
        assert!(run.stdout.contains(phrase), "{phrase:?} in {run:#?}");
    }
    assert!(
        !run.stdout.contains("my notes"),
        "comments are skipped: {run:#?}"
    );
    let order: Vec<usize> = [
        "*.scratch",
        ".DS_Store",
        "mine.txt",
        "local.tmp",
        "global.tmp",
    ]
    .iter()
    .map(|rule| run.stdout.find(rule).unwrap())
    .collect();
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(order, sorted, "grouped by source: {run:#?}");
}

#[test]
fn listing_with_no_rules_says_so() {
    let env = project();
    fs::remove_file(env.work.join(".gitignore")).unwrap();
    fs::write(env.work.join(".git/info/exclude"), "# none\n").unwrap();

    let run = env.paper(&["ignore"]);

    run.assert_code(0);
    assert_eq!(run.stdout.trim_end(), "No ignore rules.");
    let json = env.paper(&["ignore", "--json"]);
    json.assert_code(0);
    assert_eq!(json.json(), json!({"schema": 1, "rules": []}));
}

#[test]
fn listing_changes_nothing() {
    let env = project();
    let ops_before = ops(&env);
    let gitignore = env.read(".gitignore");

    env.paper(&["ignore"]).assert_code(0);

    assert_eq!(ops(&env), ops_before);
    assert_eq!(env.read(".gitignore"), gitignore);
}

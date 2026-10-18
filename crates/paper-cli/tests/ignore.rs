//! `paper ignore` through the binary (PV-018), and `paper unignore`, `--local` and `--why`
//! (PV-032): human and JSON output, refusals, listing, and
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
    env.write("sections/x.scratch", "x\n");
    env.paper(&["status"]).assert_code(0);

    let run = env.paper_in(&env.work.join("sections"), &[], &["ignore", "*.scratch"]);

    run.assert_code(0);
    assert!(env.read(".gitignore").contains("\n*.scratch\n"), "{run:#?}");
    assert!(
        run.stdout.to_lowercase().contains("whole project"),
        "{run:#?}"
    );
    assert!(run.stdout.contains("sections/x.scratch"), "{run:#?}");
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
    assert_keys(
        &json,
        &["schema", "added", "already", "untracked", "entry", "local"],
    );
    assert_eq!(json["schema"], 1);
    assert_eq!(json["local"], false);
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
            "local": false,
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

// ---- PV-032: unignore, --local and --why ----

/// A project whose own `.gitignore` starts with `lines`, saved with `main.tex` only.
fn project_with_gitignore(lines: &str) -> Env {
    let env = Env::new();
    env.write("main.tex", "\\documentclass{article}\n");
    env.write(".gitignore", lines);
    Project::init(&env.work, identity(), &SnapshotOptions::default()).unwrap();
    env
}

/// `*.log` is ignored by your own line; `notes/keep.log` and `notes/other.log` are on disk.
fn log_project() -> Env {
    let env = project_with_gitignore("*.log\n");
    env.write("notes/keep.log", "keep\n");
    env.write("notes/other.log", "other\n");
    env
}

fn exclude(env: &Env) -> String {
    fs::read_to_string(env.work.join(".git/info/exclude")).unwrap_or_default()
}

fn line_of(text: &str, rule: &str) -> usize {
    text.lines()
        .position(|line| line == rule)
        .unwrap_or_else(|| panic!("{rule:?} not in {text:?}"))
        + 1
}

/// The `file:line:pattern` git reports for a path that a rule matches.
fn git_rule(env: &Env, path: &str) -> (String, usize, String) {
    let out = env.git_work(&["check-ignore", "-v", "--no-index", path]);
    let source = out.split('\t').next().unwrap();
    let mut parts = source.rsplitn(3, ':');
    let pattern = parts.next().unwrap().to_owned();
    let line = parts.next().unwrap().parse().unwrap();
    (parts.next().unwrap().to_owned(), line, pattern)
}

fn assert_unchanged(env: &Env, gitignore: &str, exclude_text: &str, ops_before: usize) {
    assert_eq!(env.read(".gitignore"), gitignore);
    assert_eq!(exclude(env), exclude_text);
    assert_eq!(ops(env), ops_before);
}

#[test]
fn the_helpers_find_rules_and_lines() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let gitignore = env.read(".gitignore");

    let (file, line, pattern) = git_rule(&env, "data/raw/a.csv");

    assert_eq!(
        (file.as_str(), pattern.as_str()),
        (".gitignore", "/data/raw/")
    );
    assert_eq!(line, line_of(&gitignore, "/data/raw/"));
    assert!(!exclude(&env).contains("zzz"));
    assert_eq!(
        log_project().read(".gitignore").lines().next(),
        Some("*.log")
    );
}

// -- unignore --

#[test]
fn unignoring_a_folder_says_what_it_did_and_tracks_the_files_again() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let ops_before = ops(&env);

    let run = env.paper(&["unignore", "data/raw"]);

    run.assert_code(0);
    assert!(run.stderr.is_empty(), "{run:#?}");
    assert!(run.stdout.contains("/data/raw/"), "{run:#?}");
    assert!(run.stdout.contains("data/raw/a.csv"), "{run:#?}");
    assert!(run.stdout.contains("data/raw/b.csv"), "{run:#?}");
    assert!(run.stdout.contains("paper undo"), "{run:#?}");
    assert_eq!(ops(&env), ops_before + 1);
    assert!(!env.read(".gitignore").contains("/data/raw/"));
    assert!(draft_files(&env).contains(&"data/raw/a.csv".to_owned()));
    assert_eq!(env.read("data/raw/a.csv"), "1,2\n");
    let activity = env.paper(&["activity", "-n", "1"]);
    assert!(
        activity.stdout.contains("unignore /data/raw/"),
        "{activity:#?}"
    );
    env.fsck();
}

#[test]
fn unignore_json_for_a_removal_has_every_key() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);

    let run = env.paper(&["unignore", "data/raw", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(
        &json,
        &["schema", "removed", "negated", "tracked", "entry", "local"],
    );
    assert_eq!(json["schema"], 1);
    assert_eq!(json["removed"], json!(["/data/raw/"]));
    assert_eq!(json["negated"], json!([]));
    assert_eq!(json["tracked"], json!(["data/raw/a.csv", "data/raw/b.csv"]));
    assert_eq!(json["local"], false);
    let activity = env.paper(&["activity", "-n", "1", "--json"]).json();
    let listed = &activity["entries"][0];
    let keys: Vec<&str> = listed
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_keys(&json["entry"], &keys);
    assert_eq!(json["entry"]["kind"], "unignore");
    assert_eq!(json["entry"]["description"], "unignore /data/raw/");
    assert_eq!(json["entry"]["id"], listed["id"]);
}

#[test]
fn unignoring_a_file_your_own_line_ignores_adds_a_negation() {
    let env = log_project();

    let run = env.paper(&["unignore", "notes/keep.log", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["removed"], json!([]));
    assert_eq!(json["negated"], json!(["!/notes/keep.log"]));
    assert_eq!(json["tracked"], json!(["notes/keep.log"]));
    assert_eq!(json["entry"]["description"], "unignore /notes/keep.log");
    assert!(env.read(".gitignore").contains(
        "# >>> paper-vcs: ignored >>>\n!/notes/keep.log\n# <<< paper-vcs: ignored <<<\n"
    ));
    let tracked = draft_files(&env);
    assert!(
        tracked.contains(&"notes/keep.log".to_owned()),
        "{tracked:?}"
    );
    assert!(
        !tracked.contains(&"notes/other.log".to_owned()),
        "{tracked:?}"
    );
    let (_, _, pattern) = git_rule(&env, "notes/keep.log");
    assert_eq!(pattern, "!/notes/keep.log");
    env.fsck();
}

#[test]
fn unignoring_a_path_that_is_not_ignored_says_so_and_records_nothing() {
    let env = project();
    let gitignore = env.read(".gitignore");
    let ops_before = ops(&env);
    let exclude_before = exclude(&env);

    let human = env.paper(&["unignore", "data/keep.csv"]);
    let json = env.paper(&["unignore", "data/keep.csv", "--json"]);

    human.assert_code(0);
    assert!(
        human.stdout.contains("Nothing to do: not ignored."),
        "{human:#?}"
    );
    json.assert_code(0);
    assert_eq!(
        json.json(),
        json!({
            "schema": 1,
            "removed": [],
            "negated": [],
            "tracked": [],
            "entry": null,
            "local": false,
        })
    );
    assert_unchanged(&env, &gitignore, &exclude_before, ops_before);
}

#[test]
fn unignoring_a_file_in_an_excluded_folder_is_a_three_part_error_naming_the_rule() {
    let env = project_with_gitignore("/archive/\n");
    env.write("archive/old.txt", "old\n");
    let gitignore = env.read(".gitignore");
    let exclude_before = exclude(&env);
    let ops_before = ops(&env);

    let run = env.paper(&["unignore", "archive/old.txt"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("/archive/"), "{run:#?}");
    assert!(run.stderr.contains(".gitignore"), "{run:#?}");
    assert!(run.stderr.contains("unignore archive"), "{run:#?}");
    assert_unchanged(&env, &gitignore, &exclude_before, ops_before);

    let way_out = env.paper(&["unignore", "archive"]);
    way_out.assert_code(0);
    assert!(draft_files(&env).contains(&"archive/old.txt".to_owned()));
}

#[test]
fn unignoring_a_path_a_nested_gitignore_ignores_is_refused_naming_that_file() {
    let env = Env::new();
    env.write("main.tex", "m\n");
    env.write("docs/.gitignore", "*.tmp\n");
    Project::init(&env.work, identity(), &SnapshotOptions::default()).unwrap();
    env.write("docs/x.tmp", "x\n");
    let ops_before = ops(&env);

    let run = env.paper(&["unignore", "docs/x.tmp"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("docs/.gitignore"), "{run:#?}");
    assert!(run.stderr.contains("*.tmp"), "{run:#?}");
    assert_eq!(ops(&env), ops_before);
}

#[test]
fn unignore_needs_at_least_one_argument() {
    let env = project();

    let run = env.paper(&["unignore"]);

    run.assert_code(1);
    assert!(run.stdout.is_empty(), "{run:#?}");
    assert!(run.stderr.contains("required"), "{run:#?}");
    assert_eq!(ops(&env), 1);
}

#[test]
fn unignore_refuses_what_ignore_refuses_and_a_leading_bang() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let gitignore = env.read(".gitignore");
    let ops_before = ops(&env);
    let exclude_before = exclude(&env);

    for refused in [
        "",
        "!x",
        ".gitignore",
        "paper.toml",
        ".git/hooks",
        "../outside",
    ] {
        let run = env.paper(&["unignore", refused]);
        run.assert_three_part_error(1);
        assert_unchanged(&env, &gitignore, &exclude_before, ops_before);
    }
}

#[test]
fn undo_takes_an_unignore_back_and_redo_brings_it_again() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let gitignore_ignored = env.read(".gitignore");
    env.paper(&["unignore", "data/raw"]).assert_code(0);

    let undo = env.paper(&["undo"]);

    undo.assert_code(0);
    assert_eq!(env.read(".gitignore"), gitignore_ignored);
    assert!(!draft_files(&env).iter().any(|f| f.starts_with("data/raw/")));
    assert_eq!(
        env.read("data/raw/a.csv"),
        "1,2\n",
        "the files stay on disk"
    );

    env.paper(&["redo"]).assert_code(0);
    assert!(draft_files(&env).contains(&"data/raw/a.csv".to_owned()));
    env.fsck();
}

#[test]
fn ignore_after_unignore_takes_the_negation_out_again() {
    let env = log_project();
    env.paper(&["unignore", "notes/keep.log"]).assert_code(0);

    let run = env.paper(&["ignore", "notes/keep.log"]);

    run.assert_code(0);
    assert!(!env.read(".gitignore").contains("/notes/keep.log"));
    assert!(!draft_files(&env).contains(&"notes/keep.log".to_owned()));
    let (_, _, pattern) = git_rule(&env, "notes/keep.log");
    assert_eq!(pattern, "*.log");
}

// -- --local --

#[test]
fn ignore_local_writes_info_exclude_and_json_says_local() {
    let env = project();
    let gitignore = env.read(".gitignore");

    let run = env.paper(&["ignore", "data/raw", "--local", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(
        &json,
        &["schema", "added", "already", "untracked", "entry", "local"],
    );
    assert_eq!(json["local"], true);
    assert_eq!(json["added"], json!(["/data/raw/"]));
    assert_eq!(
        json["untracked"],
        json!(["data/raw/a.csv", "data/raw/b.csv"])
    );
    assert_eq!(json["entry"]["kind"], "ignore");
    assert_eq!(
        env.read(".gitignore"),
        gitignore,
        "co-authors see no change"
    );
    assert!(
        exclude(&env)
            .contains("# >>> paper-vcs: ignored >>>\n/data/raw/\n# <<< paper-vcs: ignored <<<\n"),
        "{}",
        exclude(&env)
    );
    let (file, _, pattern) = git_rule(&env, "data/raw/a.csv");
    assert_eq!(
        (file.as_str(), pattern.as_str()),
        (".git/info/exclude", "/data/raw/")
    );
    assert!(!draft_files(&env).iter().any(|f| f.starts_with("data/raw/")));
    env.fsck();
}

#[test]
fn ignore_local_in_words_still_lists_the_files_it_stopped_tracking() {
    let env = project();

    let run = env.paper(&["ignore", "--local", "data/raw"]);

    run.assert_code(0);
    assert!(run.stdout.contains("/data/raw/"), "{run:#?}");
    assert!(run.stdout.contains("data/raw/a.csv"), "{run:#?}");
    assert!(
        run.stdout.contains("exclude"),
        "it says where the rule went: {run:#?}"
    );
}

#[test]
fn unignore_local_removes_the_private_rule_and_json_says_local() {
    let env = project();
    env.paper(&["ignore", "--local", "data/raw"]).assert_code(0);
    let gitignore = env.read(".gitignore");

    let run = env.paper(&["unignore", "--local", "data/raw", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["local"], true);
    assert_eq!(json["removed"], json!(["/data/raw/"]));
    assert_eq!(json["tracked"], json!(["data/raw/a.csv", "data/raw/b.csv"]));
    assert!(!exclude(&env).contains("/data/raw/"), "{}", exclude(&env));
    assert_eq!(env.read(".gitignore"), gitignore);
    assert!(draft_files(&env).contains(&"data/raw/a.csv".to_owned()));
}

#[test]
fn unignore_local_of_a_shared_rule_is_refused_with_the_hint_to_run_it_without_local() {
    let env = log_project();
    let gitignore = env.read(".gitignore");
    let exclude_before = exclude(&env);
    let ops_before = ops(&env);

    let run = env.paper(&["unignore", "--local", "notes/keep.log"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("*.log"), "{run:#?}");
    assert!(run.stderr.contains("without"), "{run:#?}");
    assert!(run.stderr.contains("--local"), "{run:#?}");
    assert_unchanged(&env, &gitignore, &exclude_before, ops_before);
}

#[test]
fn undo_of_a_local_ignore_tracks_the_files_and_keeps_the_private_rule() {
    let env = project();
    env.paper(&["ignore", "--local", "data/raw"]).assert_code(0);
    let exclude_after = exclude(&env);

    let undo = env.paper(&["undo"]);

    undo.assert_code(0);
    assert_eq!(exclude(&env), exclude_after);
    assert!(draft_files(&env).contains(&"data/raw/a.csv".to_owned()));
    assert_eq!(env.read("data/raw/a.csv"), "1,2\n");
    env.fsck();
}

#[test]
fn undoing_a_local_entry_adds_the_note_about_info_exclude() {
    let env = project();
    env.paper(&["ignore", "--local", "data/raw"]).assert_code(0);

    let undo = env.paper(&["undo"]);

    undo.assert_code(0);
    assert!(
        undo.stdout.contains(
            "The rule in .git/info/exclude is unchanged; change it with \
             `paper ignore --local` or `paper unignore --local`."
        ),
        "{undo:#?}"
    );
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let shared = env.paper(&["undo"]);
    assert!(!shared.stdout.contains("info/exclude"), "{shared:#?}");
}

// -- --why --

#[test]
fn why_names_paper_block_rule_with_file_and_line() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let line = line_of(&env.read(".gitignore"), "/data/raw/");
    let ops_before = ops(&env);

    let run = env.paper(&["ignore", "--why", "data/raw/a.csv"]);

    run.assert_code(0);
    assert!(run.stderr.is_empty(), "{run:#?}");
    assert!(
        run.stdout.contains(&format!(
            "data/raw/a.csv is ignored by `/data/raw/` (paper's block, .gitignore line {line})."
        )),
        "{run:#?}"
    );
    assert!(!run.stdout.contains("still tracked"), "{run:#?}");
    assert_eq!(ops(&env), ops_before, "--why records nothing");
}

#[test]
fn why_json_has_fixed_keys_and_agrees_with_git_for_every_local_source() {
    let env = project();
    env.write(".DS_Store", "finder\n");
    let mut gitignore = env.read(".gitignore");
    gitignore.push_str("\n# my notes\nmine.txt\n");
    env.write(".gitignore", &gitignore);
    env.write("docs/.gitignore", "# nested\n*.nested\n");
    env.write("docs/x.nested", "n\n");
    env.write("mine.txt", "mine\n");
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    env.write("local.tmp", "l\n");
    let mut exclude_text = exclude(&env);
    exclude_text.push_str("# private\nlocal.tmp\n");
    fs::write(env.work.join(".git/info/exclude"), exclude_text).unwrap();

    for (path, source, rule, file) in [
        ("data/raw/a.csv", "paper", "/data/raw/", ".gitignore"),
        (".DS_Store", "profile", ".DS_Store", ".gitignore"),
        ("mine.txt", "yours", "mine.txt", ".gitignore"),
        ("docs/x.nested", "yours", "*.nested", "docs/.gitignore"),
        ("local.tmp", "local", "local.tmp", ".git/info/exclude"),
    ] {
        let run = env.paper(&["ignore", "--why", path, "--json"]);

        run.assert_code(0);
        let json = run.json();
        assert_keys(&json, &["schema", "path", "ignored", "tracked", "rule"]);
        assert_eq!(json["schema"], 1);
        assert_eq!(json["path"], path);
        assert_eq!(json["ignored"], true, "{path}");
        assert_eq!(json["tracked"], false, "{path}");
        assert_keys(
            &json["rule"],
            &["rule", "source", "file", "line", "negated"],
        );
        assert_eq!(json["rule"]["rule"], rule, "{path}");
        assert_eq!(json["rule"]["source"], source, "{path}");
        assert_eq!(json["rule"]["file"], file, "{path}");
        assert_eq!(json["rule"]["negated"], false, "{path}");
        let (git_file, git_line, git_pattern) = git_rule(&env, path);
        assert_eq!(json["rule"]["file"], git_file, "{path}");
        assert_eq!(json["rule"]["line"], git_line, "{path}");
        assert_eq!(json["rule"]["rule"], git_pattern, "{path}");
    }
}

#[test]
fn why_names_a_rule_from_git_global_excludes() {
    let env = project();
    point_global_excludes_at(&env, "global-ignore", "# g\nglobal.tmp\n");
    env.write("global.tmp", "g\n");

    let run = env.paper(&["ignore", "--why", "global.tmp", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["ignored"], true);
    assert_eq!(json["rule"]["source"], "global");
    assert_eq!(json["rule"]["rule"], "global.tmp");
    assert_eq!(json["rule"]["line"], 2);
    assert!(
        json["rule"]["file"]
            .as_str()
            .unwrap()
            .ends_with("global-ignore")
    );
    let human = env.paper(&["ignore", "--why", "global.tmp"]);
    assert!(
        human
            .stdout
            .contains("global.tmp is ignored by `global.tmp`"),
        "{human:#?}"
    );
    assert!(human.stdout.contains("line 2"), "{human:#?}");
}

#[test]
fn why_says_not_ignored_and_exits_zero() {
    let env = project();

    let human = env.paper(&["ignore", "--why", "data/keep.csv"]);
    let json = env.paper(&["ignore", "--why", "data/keep.csv", "--json"]);
    let absent = env.paper(&["ignore", "--why", "nothing/here.txt", "--json"]);

    human.assert_code(0);
    assert!(
        human.stdout.contains("data/keep.csv is not ignored."),
        "{human:#?}"
    );
    json.assert_code(0);
    assert_eq!(
        json.json(),
        json!({
            "schema": 1,
            "path": "data/keep.csv",
            "ignored": false,
            "tracked": true,
            "rule": null,
        })
    );
    absent.assert_code(0);
    assert_eq!(
        absent.json(),
        json!({
            "schema": 1,
            "path": "nothing/here.txt",
            "ignored": false,
            "tracked": false,
            "rule": null,
        })
    );
}

#[test]
fn why_says_which_negation_brings_a_file_back() {
    let env = log_project();
    env.paper(&["unignore", "notes/keep.log"]).assert_code(0);
    let line = line_of(&env.read(".gitignore"), "!/notes/keep.log");

    let human = env.paper(&["ignore", "--why", "notes/keep.log"]);
    let json = env.paper(&["ignore", "--why", "notes/keep.log", "--json"]);

    human.assert_code(0);
    assert!(
        human.stdout.contains(&format!(
            "notes/keep.log is not ignored: `!/notes/keep.log` (paper's block, .gitignore line {line}) brings it back."
        )),
        "{human:#?}"
    );
    let json = json.json();
    assert_eq!(json["ignored"], false);
    assert_eq!(json["tracked"], true);
    assert_eq!(json["rule"]["rule"], "!/notes/keep.log");
    assert_eq!(json["rule"]["negated"], true);
    assert_eq!(json["rule"]["source"], "paper");
    assert_eq!(json["rule"]["line"], line);
    let (_, git_line, git_pattern) = git_rule(&env, "notes/keep.log");
    assert_eq!((git_line, git_pattern.as_str()), (line, "!/notes/keep.log"));
}

#[test]
fn why_adds_that_a_saved_file_is_still_tracked() {
    let env = project();
    let mut gitignore = env.read(".gitignore");
    gitignore.push_str("*.csv\n");
    env.write(".gitignore", &gitignore);

    let human = env.paper(&["ignore", "--why", "data/keep.csv"]);
    let json = env.paper(&["ignore", "--why", "data/keep.csv", "--json"]);

    human.assert_code(0);
    assert!(
        human.stdout.contains("data/keep.csv is ignored by `*.csv`"),
        "{human:#?}"
    );
    assert!(
        human.stdout.contains(
            "It is still tracked, because it was saved before the rule; \
             paper ignore data/keep.csv stops tracking it."
        ),
        "{human:#?}"
    );
    let json = json.json();
    assert_eq!(json["ignored"], true);
    assert_eq!(json["tracked"], true);
}

#[test]
fn why_resolves_the_path_from_where_paper_runs_and_changes_nothing() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    env.write("main.tex", "an unsaved edit\n");
    let ops_before = ops(&env);
    let exclude_before = exclude(&env);
    let gitignore = env.read(".gitignore");

    let run = env.paper_in(
        &env.work.join("data"),
        &[],
        &["ignore", "--why", "raw/a.csv", "--json"],
    );

    run.assert_code(0);
    assert_eq!(run.json()["path"], "data/raw/a.csv");
    assert_eq!(run.json()["ignored"], true);
    assert_unchanged(&env, &gitignore, &exclude_before, ops_before);
    assert_eq!(env.read("main.tex"), "an unsaved edit\n");
}

#[test]
fn why_cannot_be_combined_with_other_arguments_or_flags() {
    let env = project();
    let control = env.paper(&["ignore", "--why", "data/keep.csv"]);
    control.assert_code(0);
    assert!(control.stdout.contains("not ignored"), "{control:#?}");
    let gitignore = env.read(".gitignore");
    let ops_before = ops(&env);
    let exclude_before = exclude(&env);

    for args in [
        &["ignore", "--why", "data/keep.csv", "main.tex"][..],
        &["ignore", "main.tex", "--why", "data/keep.csv"][..],
        &["ignore", "--why", "data/keep.csv", "--yes"][..],
        &["ignore", "--why", "data/keep.csv", "--local"][..],
        &["ignore", "--why"][..],
    ] {
        let run = env.paper(args);
        run.assert_code(1);
        assert!(run.stdout.is_empty(), "{args:?}: {run:#?}");
        assert!(run.stderr.starts_with("error:"), "{args:?}: {run:#?}");
        assert!(run.stderr.contains("--why"), "{args:?}: {run:#?}");
        assert_unchanged(&env, &gitignore, &exclude_before, ops_before);
    }
}

#[test]
fn why_refuses_an_empty_path_one_inside_git_and_one_outside_the_project() {
    let env = project();
    let control = env.paper(&["ignore", "--why", "data/keep.csv"]);
    control.assert_code(0);

    for refused in ["", ".git/config", "../outside"] {
        let run = env.paper(&["ignore", "--why", refused]);
        run.assert_three_part_error(1);
    }
}

// ---- snapshots (read and accepted by hand) ----

/// `common::settings()` plus filters for the random activity ids in `entry`.
fn snapshot_settings() -> insta::Settings {
    let mut settings = common::settings();
    settings.add_filter(r#""(id|short_id)": "[0-9a-f]{4,32}""#, r#""$1": "[ID]""#);
    settings
}

#[test]
fn snapshot_adding_and_untracking() {
    let env = project();
    let human = env.paper(&["ignore", "data/raw", "*.aux"]);
    human.assert_code(0);
    let env_json = project();
    let json = env_json.paper(&["ignore", "data/raw", "*.aux", "--json"]);
    json.assert_code(0);

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("ignore_add_human", env.redact(&human.stdout));
        insta::assert_snapshot!("ignore_add_json", env_json.redact_json(&json.stdout));
    });
}

#[test]
fn snapshot_a_rule_that_was_already_there() {
    let env = project();
    let mut gitignore = env.read(".gitignore");
    gitignore
        .push_str("\n# >>> paper-vcs: ignored >>>\n/data/raw/\n# <<< paper-vcs: ignored <<<\n");
    env.write(".gitignore", &gitignore);
    let human = env.paper(&["ignore", "data/raw"]);
    human.assert_code(0);

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("ignore_untrack_already_human", env.redact(&human.stdout));
    });
}

#[test]
fn snapshot_nothing_to_do() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let human = env.paper(&["ignore", "data/raw"]);
    let json = env.paper(&["ignore", "data/raw", "--json"]);
    human.assert_code(0);
    json.assert_code(0);

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("ignore_no_op_human", env.redact(&human.stdout));
        insta::assert_snapshot!("ignore_no_op_json", env.redact_json(&json.stdout));
    });
}

#[test]
fn snapshot_the_threshold_error() {
    let env = Env::new();
    for i in 0..21 {
        env.write(&format!("big/f{i:02}.txt"), &format!("file {i}\n"));
    }
    Project::init(&env.work, identity(), &SnapshotOptions::default()).unwrap();
    let refused = env.paper(&["ignore", "big"]);
    refused.assert_three_part_error(1);

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("ignore_threshold_error", env.redact(&refused.stderr));
    });
}

#[test]
fn snapshot_a_refused_argument() {
    let env = project();
    let refused = env.paper(&["ignore", "!data/raw"]);
    refused.assert_three_part_error(1);

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("ignore_unignore_error", env.redact(&refused.stderr));
    });
}

#[test]
fn snapshot_the_listing() {
    let env = project();
    write_all_five_sources(&env);
    point_global_excludes_at(&env, "global-ignore", "# g\nglobal.tmp\n");
    let human = env.paper(&["ignore"]);
    let json = env.paper(&["ignore", "--json"]);
    human.assert_code(0);
    json.assert_code(0);

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("ignore_listing_human", env.redact(&human.stdout));
        insta::assert_snapshot!("ignore_listing_json", env.redact_json(&json.stdout));
    });
}

// ---- redo of an ignore never deletes the files ----

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "PAPER_DEBUG_CRASH_AT is compiled out of release builds"
)]
fn a_crash_while_redoing_an_ignore_keeps_the_files_and_the_project_consistent() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    env.paper(&["undo"]).assert_code(0);

    let crashed = env.paper_in(
        &env.work,
        &[("PAPER_DEBUG_CRASH_AT", "mid-checkout")],
        &["redo"],
    );
    assert_ne!(crashed.code, 0, "{crashed:#?}");

    env.paper(&["status"]).assert_code(0);
    assert_eq!(env.read("data/raw/a.csv"), "1,2\n");
    assert_eq!(env.read("data/raw/b.csv"), "3,4\n");
    assert!(
        env.read(".gitignore").contains("/data/raw/"),
        "the redo finished"
    );
    assert!(!draft_files(&env).iter().any(|f| f.starts_with("data/raw/")));
    env.fsck();
}

#[test]
fn redo_after_undoing_an_ignore_leaves_the_files_on_disk() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    env.paper(&["undo"]).assert_code(0);

    env.paper(&["redo"]).assert_code(0);

    assert_eq!(env.read("data/raw/a.csv"), "1,2\n");
    assert_eq!(env.read("data/raw/b.csv"), "3,4\n");
    env.fsck();
}

// ---- PV-032 snapshots (read and accepted by hand) ----

#[test]
fn snapshot_unignore_remove_negate_and_refuse() {
    let env = project();
    env.paper(&["ignore", "data/raw"]).assert_code(0);
    let removed = env.paper(&["unignore", "data/raw"]);
    removed.assert_code(0);
    let removed_env = project();
    removed_env.paper(&["ignore", "data/raw"]).assert_code(0);
    let removed_json = removed_env.paper(&["unignore", "data/raw", "--json"]);
    removed_json.assert_code(0);

    let negate_env = log_project();
    let negated = negate_env.paper(&["unignore", "notes/keep.log"]);
    negated.assert_code(0);

    let refuse_env = project_with_gitignore("/archive/\n");
    refuse_env.write("archive/old.txt", "old\n");
    let refused = refuse_env.paper(&["unignore", "archive/old.txt"]);
    refused.assert_three_part_error(1);

    let nested_env = Env::new();
    nested_env.write("main.tex", "m\n");
    nested_env.write("docs/.gitignore", "*.tmp\n");
    Project::init(&nested_env.work, identity(), &SnapshotOptions::default()).unwrap();
    nested_env.write("docs/x.tmp", "x\n");
    let nested = nested_env.paper(&["unignore", "docs/x.tmp"]);
    nested.assert_three_part_error(1);

    let nothing = env.paper(&["unignore", "data/keep.csv"]);
    nothing.assert_code(0);

    let bang = env.paper(&["unignore", "!data/raw"]);
    bang.assert_three_part_error(1);

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("unignore_remove_human", env.redact(&removed.stdout));
        insta::assert_snapshot!(
            "unignore_remove_json",
            removed_env.redact_json(&removed_json.stdout)
        );
        insta::assert_snapshot!("unignore_negate_human", negate_env.redact(&negated.stdout));
        insta::assert_snapshot!(
            "unignore_refused_folder_error",
            refuse_env.redact(&refused.stderr)
        );
        insta::assert_snapshot!(
            "unignore_refused_nested_error",
            nested_env.redact(&nested.stderr)
        );
        insta::assert_snapshot!("unignore_nothing_human", env.redact(&nothing.stdout));
        insta::assert_snapshot!("unignore_bang_error", env.redact(&bang.stderr));
    });
}

#[test]
fn snapshot_local_rules() {
    let env = project();
    let ignored = env.paper(&["ignore", "--local", "data/raw"]);
    ignored.assert_code(0);
    let activity = env.paper(&["activity", "-n", "1"]);
    activity.assert_code(0);
    assert!(
        activity
            .stdout
            .contains("ignore /data/raw/ (in .git/info/exclude)"),
        "{activity:#?}"
    );
    let unignored = env.paper(&["unignore", "--local", "data/raw"]);
    unignored.assert_code(0);
    let json_env = project();
    json_env
        .paper(&["ignore", "--local", "data/raw"])
        .assert_code(0);
    let unignored_json = json_env.paper(&["unignore", "--local", "data/raw", "--json"]);
    unignored_json.assert_code(0);
    let shared = log_project();
    let refused = shared.paper(&["unignore", "--local", "notes/keep.log"]);
    refused.assert_three_part_error(1);

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("ignore_local_human", env.redact(&ignored.stdout));
        insta::assert_snapshot!("unignore_local_human", env.redact(&unignored.stdout));
        insta::assert_snapshot!(
            "unignore_local_json",
            json_env.redact_json(&unignored_json.stdout)
        );
        insta::assert_snapshot!(
            "unignore_local_refused_error",
            shared.redact(&refused.stderr)
        );
    });
}

#[test]
fn snapshot_why() {
    let paper_env = project();
    paper_env.paper(&["ignore", "data/raw"]).assert_code(0);
    let ignored = paper_env.paper(&["ignore", "--why", "data/raw/a.csv"]);
    let ignored_json = paper_env.paper(&["ignore", "--why", "data/raw/a.csv", "--json"]);
    let free = paper_env.paper(&["ignore", "--why", "main.tex"]);
    let free_json = paper_env.paper(&["ignore", "--why", "main.tex", "--json"]);
    let env = log_project();
    let ignored_by_yours = env.paper(&["ignore", "--why", "notes/keep.log"]);
    env.paper(&["unignore", "notes/keep.log"]).assert_code(0);
    let negated = env.paper(&["ignore", "--why", "notes/keep.log"]);
    let negated_json = env.paper(&["ignore", "--why", "notes/keep.log", "--json"]);
    let tracked_env = project();
    let mut gitignore = tracked_env.read(".gitignore");
    gitignore.push_str("*.csv\n");
    tracked_env.write(".gitignore", &gitignore);
    let tracked = tracked_env.paper(&["ignore", "--why", "data/keep.csv"]);
    for run in [
        &ignored,
        &ignored_json,
        &free,
        &free_json,
        &ignored_by_yours,
        &negated,
        &negated_json,
        &tracked,
    ] {
        run.assert_code(0);
    }

    snapshot_settings().bind(|| {
        insta::assert_snapshot!("why_ignored_human", paper_env.redact(&ignored.stdout));
        insta::assert_snapshot!(
            "why_ignored_json",
            paper_env.redact_json(&ignored_json.stdout)
        );
        insta::assert_snapshot!("why_free_human", paper_env.redact(&free.stdout));
        insta::assert_snapshot!("why_free_json", paper_env.redact_json(&free_json.stdout));
        insta::assert_snapshot!("why_yours_human", env.redact(&ignored_by_yours.stdout));
        insta::assert_snapshot!("why_negated_human", env.redact(&negated.stdout));
        insta::assert_snapshot!("why_negated_json", env.redact_json(&negated_json.stdout));
        insta::assert_snapshot!("why_tracked_human", tracked_env.redact(&tracked.stdout));
    });
}

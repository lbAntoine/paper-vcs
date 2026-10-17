//! The export step, `exported.json` and the keep-alive chain (PV-022). Real temp repositories,
//! cross-checked with the `git` binary.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use paper_core::profile::Profile;
use paper_core::project::{
    PathSelection, Project, RestoreRequest, SaveError, SaveRequest, StatusOptions,
};
use paper_core::snapshot::SnapshotOptions;
use paper_core::view::View;
use paper_git::Identity;
use serde_json::{Value, json};

const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

/// Runs `git` in `dir` with a fixed identity, failing the test if git fails.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn write(dir: &Path, path: &str, content: &str) {
    let absolute = dir.join(path);
    fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    fs::write(absolute, content).unwrap();
}

fn init(dir: &Path) -> Project {
    Project::init_with_profile(
        dir,
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap()
    .0
}

fn open(dir: &Path) -> Project {
    Project::open(dir, identity()).unwrap()
}

/// A new project holding `a.tex`, closed again.
fn new_project_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.tex", "one\n");
    drop(init(dir.path()));
    dir
}

/// A plain-git repository with a save on `main` and a second branch `other`, adopted by `init`.
fn adopted_project_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    write(dir.path(), "a.tex", "one\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "first"]);
    git(dir.path(), &["branch", "other"]);
    drop(init(dir.path()));
    dir
}

fn save_all(project: &mut Project, message: &str) {
    project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
}

fn restore_last(project: &mut Project, path: &str) {
    project
        .restore(&RestoreRequest {
            paths: vec![path.to_owned()],
            from: None,
        })
        .unwrap();
}

fn paper(dir: &Path) -> PathBuf {
    dir.join(".git/paper")
}

fn exported_path(dir: &Path) -> PathBuf {
    paper(dir).join("exported.json")
}

fn exported(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(exported_path(dir)).unwrap()).unwrap()
}

fn index_bytes(dir: &Path) -> Vec<u8> {
    fs::read(dir.join(".git/index")).unwrap()
}

/// Every ref git has, by name.
fn refs(dir: &Path) -> BTreeMap<String, String> {
    git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"])
        .lines()
        .map(|line| {
            let (name, id) = line.split_once(' ').unwrap();
            (name.to_owned(), id.to_owned())
        })
        .collect()
}

fn keep(dir: &Path) -> String {
    git(dir, &["rev-parse", "refs/paper/keep"])
        .trim()
        .to_owned()
}

/// The refs, HEAD and `exported.json` the view calls for.
fn expected_refs(dir: &Path, view: &View) -> BTreeMap<String, String> {
    let mut expected = BTreeMap::new();
    for (name, line) in view.lines() {
        expected.insert(format!("refs/heads/{name}"), line.tip.as_str().to_owned());
        expected.insert(
            format!("refs/paper/draft/{name}"),
            line.draft.as_str().to_owned(),
        );
    }
    expected.insert("refs/paper/keep".to_owned(), keep(dir));
    expected
}

/// After a command: git's refs and HEAD follow the view, and `exported.json` says so.
fn assert_exported(dir: &Path, project: &Project) {
    let view = project.view();
    let expected = expected_refs(dir, view);
    let actual = refs(dir);
    for (name, id) in &expected {
        assert_eq!(actual.get(name), Some(id), "{name}");
    }
    assert_eq!(
        git(dir, &["symbolic-ref", "HEAD"]).trim(),
        format!("refs/heads/{}", view.current_line())
    );
    assert_eq!(
        exported(dir),
        json!({
            "schema": 1,
            "head": view.current_line().as_str(),
            "refs": expected,
        })
    );
}

fn view_commits(view: &View) -> BTreeSet<String> {
    view.lines()
        .flat_map(|(_, line)| [line.tip.as_str().to_owned(), line.draft.as_str().to_owned()])
        .collect()
}

/// The parents of the keep ref's commit.
fn keep_parents(dir: &Path) -> BTreeSet<String> {
    git(
        dir,
        &["rev-list", "--parents", "-n", "1", "refs/paper/keep"],
    )
    .split_whitespace()
    .skip(1)
    .map(str::to_owned)
    .collect()
}

/// How many keep-alive commits the chain holds.
fn keep_commits(dir: &Path) -> usize {
    git(dir, &["log", "--format=%s", "refs/paper/keep"])
        .lines()
        .filter(|subject| *subject == "paper keep-alive")
        .count()
}

fn op_count(dir: &Path) -> usize {
    fs::read_dir(paper(dir).join("activity/ops"))
        .unwrap()
        .count()
}

/// Every commit any op ever named, read from the op files.
fn commits_named_by_ops(dir: &Path) -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    for entry in fs::read_dir(paper(dir).join("activity/ops")).unwrap() {
        let op: Value = serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
        for line in op["view"]["lines"].as_object().unwrap().values() {
            all.insert(line["tip"].as_str().unwrap().to_owned());
            all.insert(line["draft"].as_str().unwrap().to_owned());
        }
    }
    all
}

/// `(mode, id, path)` of every file in the index.
fn index_entries(dir: &Path) -> BTreeSet<(String, String, String)> {
    git(dir, &["ls-files", "-s"])
        .lines()
        .map(|line| {
            let (meta, path) = line.split_once('\t').unwrap();
            let mut parts = meta.split(' ');
            (
                parts.next().unwrap().to_owned(),
                parts.next().unwrap().to_owned(),
                path.to_owned(),
            )
        })
        .collect()
}

/// `(mode, id, path)` of every file in `rev`'s tree.
fn tree_entries(dir: &Path, rev: &str) -> BTreeSet<(String, String, String)> {
    git(dir, &["ls-tree", "-r", rev])
        .lines()
        .map(|line| {
            let (meta, path) = line.split_once('\t').unwrap();
            let mut parts = meta.split(' ');
            let mode = parts.next().unwrap().to_owned();
            let _kind = parts.next();
            (mode, parts.next().unwrap().to_owned(), path.to_owned())
        })
        .collect()
}

// ---- refs, HEAD and exported.json follow the head op ----

#[test]
fn a_new_project_exports_its_refs_head_and_exported_json() {
    let dir = new_project_dir();
    let project = open(dir.path());
    assert_exported(dir.path(), &project);
}

#[test]
fn exported_json_has_the_documented_shape() {
    let dir = new_project_dir();
    let value = exported(dir.path());
    assert_eq!(value["schema"], 1);
    assert_eq!(value["head"], "main");
    let names: Vec<&str> = value["refs"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        names,
        [
            "refs/heads/main",
            "refs/paper/draft/main",
            "refs/paper/keep"
        ]
    );
}

#[test]
fn an_adopted_project_exports_every_line() {
    let dir = adopted_project_dir();
    let project = open(dir.path());
    assert!(project.view().lines().count() >= 2);
    assert_exported(dir.path(), &project);
    let value = exported(dir.path());
    assert!(value["refs"]["refs/heads/other"].is_string());
    assert!(value["refs"]["refs/paper/draft/other"].is_string());
}

#[test]
fn refs_head_and_exported_json_follow_every_command() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    project.snapshot().unwrap();
    assert_exported(dir.path(), &project);
    write(dir.path(), "a.tex", "three\n");
    save_all(&mut project, "Third");
    assert_exported(dir.path(), &project);
    write(dir.path(), "a.tex", "dirty\n");
    restore_last(&mut project, "a.tex");
    assert_exported(dir.path(), &project);
    project.status(&StatusOptions::default()).unwrap();
    assert_exported(dir.path(), &project);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn the_exported_state_survives_reopening() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "Second");
    drop(project);
    assert_exported(dir.path(), &open(dir.path()));
}

// ---- the index ----

#[test]
fn a_snapshot_leaves_the_index_untouched() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "edited\n");
    let before = index_bytes(dir.path());
    project.snapshot().unwrap();
    assert_eq!(index_bytes(dir.path()), before);
}

#[test]
fn a_restore_leaves_the_index_untouched() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "Second");
    write(dir.path(), "a.tex", "dirty\n");
    let before = index_bytes(dir.path());
    restore_last(&mut project, "a.tex");
    assert_eq!(index_bytes(dir.path()), before);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.tex")).unwrap(),
        "two\n"
    );
}

#[test]
fn a_save_updates_the_index_to_the_new_tips_tree() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    write(dir.path(), "b.tex", "new\n");
    save_all(&mut project, "Second");
    assert_eq!(index_entries(dir.path()), tree_entries(dir.path(), "main"));
    let paths: Vec<String> = index_entries(dir.path()).into_iter().map(|e| e.2).collect();
    assert!(paths.contains(&"a.tex".to_owned()) && paths.contains(&"b.tex".to_owned()));
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

#[test]
fn a_project_started_by_init_has_an_index_matching_its_tip() {
    let dir = new_project_dir();
    assert_eq!(index_entries(dir.path()), tree_entries(dir.path(), "main"));
}

// ---- the keep-alive chain ----

#[test]
fn the_first_ops_keep_commit_names_every_commit_in_its_view() {
    let dir = new_project_dir();
    let project = open(dir.path());
    assert_eq!(keep_parents(dir.path()), view_commits(project.view()));
    assert_eq!(keep_commits(dir.path()), 1);
}

#[test]
fn an_adopted_projects_first_keep_commit_names_every_lines_commits() {
    let dir = adopted_project_dir();
    let project = open(dir.path());
    assert_eq!(keep_parents(dir.path()), view_commits(project.view()));
}

#[test]
fn a_keep_commit_has_the_empty_tree_the_keep_message_and_papers_identity() {
    let dir = new_project_dir();
    let out = git(
        dir.path(),
        &["log", "-1", "--format=%T%n%s%n%an%n%ae", "refs/paper/keep"],
    );
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            EMPTY_TREE,
            "paper keep-alive",
            "Mina Park",
            "mina@uni.example"
        ]
    );
}

#[test]
fn each_op_adds_exactly_one_keep_commit() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    assert_eq!(keep_commits(dir.path()), op_count(dir.path()));
    write(dir.path(), "a.tex", "two\n");
    project.snapshot().unwrap();
    assert_eq!(keep_commits(dir.path()), op_count(dir.path()));
    write(dir.path(), "a.tex", "three\n");
    save_all(&mut project, "Third");
    assert_eq!(keep_commits(dir.path()), op_count(dir.path()));
    write(dir.path(), "a.tex", "dirty\n");
    restore_last(&mut project, "a.tex");
    assert_eq!(keep_commits(dir.path()), op_count(dir.path()));
    assert!(op_count(dir.path()) >= 5);
}

#[test]
fn a_single_op_keep_commit_is_the_previous_keep_plus_the_ops_new_commits() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    let previous_keep = keep(dir.path());
    let previous_view = view_commits(project.view());
    write(dir.path(), "a.tex", "two\n");
    project.snapshot().unwrap();
    assert_eq!(op_count(dir.path()), 2);
    let mut expected: BTreeSet<String> = view_commits(project.view())
        .difference(&previous_view)
        .cloned()
        .collect();
    assert_eq!(expected.len(), 1, "a snapshot rewrites the draft only");
    expected.insert(previous_keep);
    assert_eq!(keep_parents(dir.path()), expected);
}

#[test]
fn a_save_op_keeps_the_save_and_the_new_draft_alive() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    project.snapshot().unwrap();
    let previous_keep = keep(dir.path());
    let previous_view = view_commits(project.view());
    save_all(&mut project, "Second");
    let mut expected: BTreeSet<String> = view_commits(project.view())
        .difference(&previous_view)
        .cloned()
        .collect();
    expected.insert(previous_keep);
    assert_eq!(keep_parents(dir.path()), expected);
    assert!(expected.contains(project.view().current().tip.as_str()));
    assert!(expected.contains(project.view().current().draft.as_str()));
}

#[test]
fn a_command_that_records_nothing_adds_no_keep_commit() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    let before = (keep(dir.path()), op_count(dir.path()));
    project.snapshot().unwrap();
    project.status(&StatusOptions::default()).unwrap();
    assert_eq!((keep(dir.path()), op_count(dir.path())), before);
    assert_eq!(keep_commits(dir.path()), 1);
}

#[test]
fn old_drafts_and_saves_survive_gc() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    for round in 0..3 {
        write(dir.path(), "a.tex", &format!("draft {round}a\n"));
        project.snapshot().unwrap();
        write(dir.path(), "a.tex", &format!("draft {round}b\n"));
        project.snapshot().unwrap();
        write(dir.path(), "a.tex", &format!("saved {round}\n"));
        save_all(&mut project, &format!("Round {round}"));
        write(dir.path(), "a.tex", "scribble\n");
        restore_last(&mut project, "a.tex");
    }
    let named = commits_named_by_ops(dir.path());
    assert!(named.len() >= 12, "{}", named.len());
    git(dir.path(), &["reflog", "expire", "--expire=now", "--all"]);
    git(dir.path(), &["gc", "--prune=now", "--quiet"]);
    for id in &named {
        git(dir.path(), &["cat-file", "-e", id]);
    }
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn the_migration_op_gets_a_keep_commit_and_exported_json() {
    let dir = new_project_dir();
    {
        let mut project = open(dir.path());
        write(dir.path(), "a.tex", "two\n");
        save_all(&mut project, "Second");
    }
    // Rewrite the project as v0.1.0 left it: view.json, version 1, no activity, no keep chain.
    let head = fs::read_to_string(paper(dir.path()).join("activity/head")).unwrap();
    let op: Value = serde_json::from_slice(
        &fs::read(
            paper(dir.path())
                .join("activity/ops")
                .join(format!("{}.json", head.trim())),
        )
        .unwrap(),
    )
    .unwrap();
    let mut json = serde_json::to_vec_pretty(&op["view"]).unwrap();
    json.push(b'\n');
    fs::write(paper(dir.path()).join("view.json"), json).unwrap();
    fs::remove_dir_all(paper(dir.path()).join("activity")).unwrap();
    fs::remove_file(exported_path(dir.path())).ok();
    fs::write(paper(dir.path()).join("version"), "1\n").unwrap();
    git(dir.path(), &["update-ref", "-d", "refs/paper/keep"]);

    let project = open(dir.path());

    assert_eq!(keep_commits(dir.path()), 1);
    assert_eq!(keep_parents(dir.path()), view_commits(project.view()));
    assert_exported(dir.path(), &project);
}

// ---- never pushed ----

#[test]
fn pushing_a_line_sends_no_paper_refs() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "Second");
    write(dir.path(), "a.tex", "three\n");
    project.snapshot().unwrap();

    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "-b", "main"]);
    let url = remote.path().to_str().unwrap();
    git(dir.path(), &["push", url, "main"]);
    git(dir.path(), &["push", url, "refs/heads/*:refs/heads/*"]);

    let listed = git(dir.path(), &["ls-remote", url]);
    assert!(listed.contains("refs/heads/main"), "{listed}");
    assert!(!listed.contains("refs/paper"), "{listed}");
    assert!(
        listed.contains(project.view().current().tip.as_str()),
        "{listed}"
    );
}

// ---- detecting plain-git changes ----

fn save_error(project: &mut Project, message: &str) -> SaveError {
    project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap_err()
}

#[test]
fn save_refuses_after_a_plain_git_commit_on_the_line() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    git(
        dir.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "Plain git"],
    );
    write(dir.path(), "a.tex", "two\n");
    assert!(matches!(
        save_error(&mut project, "Too late"),
        SaveError::OutOfSyncWithGit { .. }
    ));
}

#[test]
fn save_refuses_after_git_switch_to_another_branch() {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    git(dir.path(), &["switch", "--quiet", "-c", "side"]);
    write(dir.path(), "a.tex", "two\n");
    assert!(matches!(
        save_error(&mut project, "Wrong branch"),
        SaveError::HeadOnAnotherBranch { .. }
    ));
}

#[test]
fn save_refuses_after_a_plain_git_commit_even_without_exported_json() {
    let dir = new_project_dir();
    fs::remove_file(exported_path(dir.path())).ok();
    let mut project = open(dir.path());
    git(
        dir.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "Plain git"],
    );
    write(dir.path(), "a.tex", "two\n");
    assert!(matches!(
        save_error(&mut project, "Too late"),
        SaveError::OutOfSyncWithGit { .. }
    ));
}

#[test]
fn a_project_without_exported_json_still_saves_and_gets_one() {
    let dir = new_project_dir();
    fs::remove_file(exported_path(dir.path())).ok();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "Second");
    assert_exported(dir.path(), &project);
}

#[test]
fn a_project_without_exported_json_gets_one_after_a_snapshot() {
    let dir = new_project_dir();
    fs::remove_file(exported_path(dir.path())).ok();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    project.snapshot().unwrap();
    assert_exported(dir.path(), &project);
}

#[test]
fn save_compares_the_line_with_exported_json_not_with_the_view() {
    // A crash between the op (step 6) and the export (step 7) leaves the refs behind the head
    // op, and exported.json still names what paper last wrote. That is not a plain-git change.
    let dir = new_project_dir();
    let mut project = open(dir.path());
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "Second");
    let behind = git(dir.path(), &["rev-parse", "main~1"]).trim().to_owned();
    git(dir.path(), &["update-ref", "refs/heads/main", &behind]);
    let mut value = exported(dir.path());
    value["refs"]["refs/heads/main"] = Value::String(behind);
    fs::write(
        exported_path(dir.path()),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();

    write(dir.path(), "a.tex", "three\n");
    save_all(&mut project, "Third");

    assert_exported(dir.path(), &project);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

// ---- what plain git moved is left alone ----

/// Rewrites a finished project as v0.1.0 left it: `view.json`, version 1, no activity.
fn into_format_1(dir: &Path) {
    let head = fs::read_to_string(paper(dir).join("activity/head")).unwrap();
    let op: Value = serde_json::from_slice(
        &fs::read(
            paper(dir)
                .join("activity/ops")
                .join(format!("{}.json", head.trim())),
        )
        .unwrap(),
    )
    .unwrap();
    let mut json = serde_json::to_vec_pretty(&op["view"]).unwrap();
    json.push(b'\n');
    fs::write(paper(dir).join("view.json"), json).unwrap();
    fs::remove_dir_all(paper(dir).join("activity")).unwrap();
    fs::remove_file(exported_path(dir)).ok();
    fs::write(paper(dir).join("version"), "1\n").unwrap();
}

fn rev(dir: &Path, name: &str) -> String {
    git(dir, &["rev-parse", name]).trim().to_owned()
}

fn plain_git_commit(dir: &Path) -> String {
    git(
        dir,
        &["commit", "--quiet", "--allow-empty", "-m", "Plain git"],
    );
    rev(dir, "main")
}

#[test]
fn migrating_after_a_plain_git_commit_leaves_the_line_alone() {
    let dir = new_project_dir();
    let paper_tip = rev(dir.path(), "main");
    into_format_1(dir.path());
    let theirs = plain_git_commit(dir.path());
    let index = index_bytes(dir.path());

    let mut project = open(dir.path());

    assert_eq!(rev(dir.path(), "main"), theirs);
    assert_eq!(index_bytes(dir.path()), index);
    assert_eq!(exported(dir.path())["refs"]["refs/heads/main"], paper_tip);
    assert!(matches!(
        save_error(&mut project, "Too late"),
        SaveError::OutOfSyncWithGit { .. }
    ));
    assert_eq!(rev(dir.path(), "main"), theirs);
}

#[test]
fn migrating_after_a_git_switch_leaves_head_alone() {
    let dir = new_project_dir();
    into_format_1(dir.path());
    git(dir.path(), &["switch", "--quiet", "-c", "side"]);
    let index = index_bytes(dir.path());

    let mut project = open(dir.path());

    assert_eq!(
        git(dir.path(), &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/side"
    );
    assert_eq!(index_bytes(dir.path()), index);
    assert_eq!(exported(dir.path())["head"], "main");
    assert!(matches!(
        save_error(&mut project, "Wrong branch"),
        SaveError::HeadOnAnotherBranch { .. }
    ));
}

/// Runs a command that records an op after plain git moved the line.
fn after_plain_commit(command: impl FnOnce(&mut Project, &Path)) {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    let paper_tip = project.view().current().tip.as_str().to_owned();
    let theirs = plain_git_commit(dir.path());
    let index = index_bytes(dir.path());

    write(dir.path(), "a.tex", "edited\n");
    command(&mut project, dir.path());

    assert_eq!(rev(dir.path(), "main"), theirs);
    assert_eq!(index_bytes(dir.path()), index);
    assert_eq!(exported(dir.path())["refs"]["refs/heads/main"], paper_tip);
    assert_eq!(
        rev(dir.path(), "refs/paper/draft/main"),
        project.view().current().draft.as_str()
    );
    assert!(matches!(
        save_error(&mut project, "Too late"),
        SaveError::OutOfSyncWithGit { .. }
    ));
}

#[test]
fn a_snapshot_after_a_plain_git_commit_leaves_the_line_alone() {
    after_plain_commit(|project, _| {
        project.snapshot().unwrap();
    });
}

#[test]
fn a_restore_after_a_plain_git_commit_leaves_the_line_alone() {
    after_plain_commit(|project, _| restore_last(project, "a.tex"));
}

fn after_git_switch(command: impl FnOnce(&mut Project)) -> (tempfile::TempDir, Project) {
    let dir = new_project_dir();
    let mut project = open(dir.path());
    git(dir.path(), &["switch", "--quiet", "-c", "side"]);
    let index = index_bytes(dir.path());

    write(dir.path(), "a.tex", "edited\n");
    command(&mut project);

    assert_eq!(
        git(dir.path(), &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/side"
    );
    assert_eq!(index_bytes(dir.path()), index);
    assert_eq!(exported(dir.path())["head"], "main");
    assert_eq!(
        rev(dir.path(), "refs/paper/draft/main"),
        project.view().current().draft.as_str()
    );
    assert!(matches!(
        save_error(&mut project, "Wrong branch"),
        SaveError::HeadOnAnotherBranch { .. }
    ));
    (dir, project)
}

#[test]
fn a_snapshot_after_a_git_switch_leaves_head_alone() {
    after_git_switch(|project| {
        project.snapshot().unwrap();
    });
}

#[test]
fn a_restore_after_a_git_switch_leaves_head_alone() {
    after_git_switch(|project| restore_last(project, "a.tex"));
}

#[test]
fn switching_back_to_the_line_does_not_reset_the_index() {
    let (dir, mut project) = after_git_switch(|project| {
        project.snapshot().unwrap();
    });
    git(dir.path(), &["switch", "--quiet", "main"]);
    let index = index_bytes(dir.path());
    write(dir.path(), "a.tex", "edited again\n");
    project.snapshot().unwrap();
    assert_eq!(index_bytes(dir.path()), index);
    assert_exported(dir.path(), &project);
}

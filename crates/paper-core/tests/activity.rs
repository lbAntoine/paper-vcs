//! The operation log and head pointer (PV-020): ops written by `init`, `save` and `restore`,
//! immutability, format 2, migration from format 1, and damaged state. Real temp repositories,
//! cross-checked with the `git` binary.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use paper_core::activity::{ActivityError, ActivityLog, MIN_SHORT_ID, OpId, OpKind, short_ids};
use paper_core::name::LineName;
use paper_core::profile::Profile;
use paper_core::project::{
    ActivityEntry, FORMAT_VERSION, LockOptions, OpenError, PathSelection, Project, RestoreRequest,
    SaveRequest, StatusOptions,
};
use paper_core::snapshot::SnapshotOptions;
use paper_core::view::{Line, View};
use paper_core::{ErrorKind, PaperError};
use paper_git::Identity;
use serde_json::Value;

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

/// A new project holding `a.tex`, closed again (so the lock is free).
fn new_project_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.tex", "one\n");
    drop(init(dir.path()));
    dir
}

/// An existing plain-git repository with a save on `main` and a second branch `other`, then
/// adopted by `init` and closed again.
fn adopted_project_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    write(dir.path(), "a.tex", "one\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "first"]);
    git(dir.path(), &["branch", "other"]);
    write(dir.path(), "a.tex", "uncommitted\n");
    let (project, initialized) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    assert!(initialized.adopted);
    drop(project);
    dir
}

fn paper(dir: &Path) -> PathBuf {
    dir.join(".git/paper")
}

fn ops_dir(dir: &Path) -> PathBuf {
    paper(dir).join("activity/ops")
}

fn version(dir: &Path) -> String {
    fs::read_to_string(paper(dir).join("version"))
        .unwrap()
        .trim()
        .to_owned()
}

fn head_id(dir: &Path) -> String {
    fs::read_to_string(paper(dir).join("activity/head"))
        .unwrap()
        .trim()
        .to_owned()
}

/// Every op file by name, with its exact bytes.
fn op_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(ops_dir(dir))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

fn read_op(dir: &Path, id: &str) -> Value {
    let bytes = fs::read(ops_dir(dir).join(format!("{id}.json"))).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// The ops from the head back to the first one.
fn chain(dir: &Path) -> Vec<Value> {
    let mut ops = Vec::new();
    let mut next = Some(head_id(dir));
    while let Some(id) = next {
        let op = read_op(dir, &id);
        assert_eq!(op["id"], id.as_str(), "an op file is named after its id");
        next = op["parent"].as_str().map(str::to_owned);
        ops.push(op);
        assert!(ops.len() < 1000, "the parent chain loops");
    }
    ops
}

fn view_value(project: &Project) -> Value {
    serde_json::to_value(project.view()).unwrap()
}

fn is_op_id(text: &str) -> bool {
    text.len() == 32 && text.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn save_all(project: &mut Project, message: &str) {
    project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
}

fn project_error(error: OpenError) -> PaperError {
    PaperError::from(error)
}

fn assert_clear_project_error(error: OpenError) {
    let error = project_error(error);
    assert!(
        error.next.iter().all(|s| !s.command.contains("paper init")),
        "init can't repair this, so it must not be suggested: {error:?}"
    );
    assert_eq!(error.kind, ErrorKind::Project, "{error:?}");
    assert!(!error.message.trim().is_empty(), "{error:?}");
    assert!(!error.why.trim().is_empty(), "{error:?}");
    assert!(!error.next.is_empty(), "{error:?}");
}

// ---- init ----

#[test]
fn format_version_is_2() {
    assert_eq!(FORMAT_VERSION, 2);
}

#[test]
fn a_new_project_starts_with_one_init_op_and_a_head() {
    let dir = new_project_dir();

    assert_eq!(version(dir.path()), "2");
    assert!(
        !paper(dir.path()).join("view.json").exists(),
        "view.json is no longer written"
    );
    let files = op_files(dir.path());
    assert_eq!(files.len(), 1, "{:?}", files.keys());
    let head = head_id(dir.path());
    assert!(is_op_id(&head), "head holds an op id, got {head:?}");
    assert!(files.contains_key(&format!("{head}.json")));

    let op = read_op(dir.path(), &head);
    assert_eq!(op["id"], head.as_str());
    assert_eq!(op["parent"], Value::Null);
    assert_eq!(op["kind"], "init");
    assert_eq!(op["position"], head.as_str());
    assert_eq!(op["redo"], Value::Array(vec![]));
    assert!(
        !op["description"].as_str().unwrap().trim().is_empty(),
        "{op}"
    );
    let time = op["time"].as_str().unwrap();
    assert!(
        time.parse::<jiff::Timestamp>().is_ok(),
        "not RFC 3339: {time}"
    );
    let project = Project::open(dir.path(), identity()).unwrap();
    assert_eq!(op["view"], view_value(&project));
}

#[test]
fn an_adopted_project_starts_with_one_init_op_and_keeps_every_line() {
    let dir = adopted_project_dir();

    assert_eq!(version(dir.path()), "2");
    assert!(!paper(dir.path()).join("view.json").exists());
    let ops = chain(dir.path());
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["kind"], "init");
    assert_eq!(ops[0]["parent"], Value::Null);
    let project = Project::open(dir.path(), identity()).unwrap();
    assert_eq!(project.view().lines().count(), 2);
    assert_eq!(ops[0]["view"], view_value(&project));
}

#[test]
fn op_ids_are_distinct_32_character_lowercase_hex() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "two");
    write(dir.path(), "a.tex", "three\n");
    save_all(&mut project, "three");
    drop(project);

    let ids: Vec<String> = chain(dir.path())
        .iter()
        .map(|op| op["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(ids.len() >= 3);
    for id in &ids {
        assert!(is_op_id(id), "{id}");
    }
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len());
}

// ---- save and restore ----

#[test]
fn save_appends_an_op_whose_view_is_the_projects_view() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let before = chain(dir.path()).len();
    write(dir.path(), "a.tex", "two\n");

    save_all(&mut project, "two");

    let ops = chain(dir.path());
    assert!(ops.len() > before, "save must add at least one op");
    assert_eq!(ops[0]["view"], view_value(&project));
    assert!(ops.iter().any(|op| op["kind"] == "save"), "{ops:?}");
    let root = ops.last().unwrap();
    assert_eq!(root["kind"], "init");
    assert_eq!(root["parent"], Value::Null);
    for pair in ops.windows(2) {
        assert_eq!(pair[0]["parent"], pair[1]["id"]);
    }
    for op in &ops {
        assert_eq!(op["position"], op["id"]);
        assert_eq!(op["redo"], Value::Array(vec![]));
    }
}

#[test]
fn snapshot_appends_an_op_only_when_the_draft_changed() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let start = chain(dir.path()).len();

    project.snapshot().unwrap();
    assert_eq!(chain(dir.path()).len(), start, "nothing changed, no op");

    write(dir.path(), "a.tex", "edited\n");
    project.snapshot().unwrap();
    let ops = chain(dir.path());
    assert_eq!(ops.len(), start + 1);
    assert_eq!(ops[0]["kind"], "snapshot");
    assert_eq!(ops[0]["view"], view_value(&project));
}

#[test]
fn restore_appends_ops_that_chain_back_to_the_first_one() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "two");
    write(dir.path(), "a.tex", "an edit to keep\n");
    let before = chain(dir.path()).len();

    project
        .restore(&RestoreRequest {
            paths: vec!["a.tex".to_owned()],
            from: None,
        })
        .unwrap();

    let ops = chain(dir.path());
    assert!(ops.len() > before, "restore records the draft it replaces");
    assert_eq!(ops[0]["view"], view_value(&project));
    assert_eq!(ops.last().unwrap()["parent"], Value::Null);
    for pair in ops.windows(2) {
        assert_eq!(pair[0]["parent"], pair[1]["id"]);
    }
}

#[test]
fn reopening_gives_the_view_in_the_head_op() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "two");
    write(dir.path(), "b.tex", "new file\n");
    project.snapshot().unwrap();
    let view = project.view().clone();
    drop(project);

    let reopened = Project::open(dir.path(), identity()).unwrap();

    assert_eq!(reopened.view(), &view);
    assert_eq!(
        read_op(dir.path(), &head_id(dir.path()))["view"],
        view_value(&reopened)
    );
}

#[test]
fn op_files_are_never_rewritten() {
    let dir = new_project_dir();
    let first = op_files(dir.path());
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "two");
    let second = op_files(dir.path());
    write(dir.path(), "a.tex", "three\n");
    project.snapshot().unwrap();
    project
        .restore(&RestoreRequest {
            paths: vec!["a.tex".to_owned()],
            from: None,
        })
        .unwrap();
    drop(project);
    let last = op_files(dir.path());

    assert!(last.len() > second.len() && second.len() > first.len());
    for (earlier, later) in [(&first, &last), (&second, &last)] {
        for (name, bytes) in earlier {
            assert_eq!(later.get(name), Some(bytes), "{name} changed");
        }
    }
}

#[test]
fn plain_git_still_agrees_after_a_session_of_commands() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "two");
    drop(project);

    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert!(git(dir.path(), &["log", "--format=%s", "main"]).contains("two"));
}

// ---- ActivityLog ----

fn sample_view(c: char) -> View {
    let commit = c.to_string().repeat(40).parse().unwrap();
    View::new(
        LineName::main(),
        Line {
            tip: commit,
            draft: c.to_string().repeat(40).parse().unwrap(),
        },
    )
}

#[test]
fn an_empty_log_has_no_head() {
    let dir = tempfile::tempdir().unwrap();
    let log = ActivityLog::open(dir.path());
    assert_eq!(log.head().unwrap(), None);
}

#[test]
fn append_writes_the_op_file_then_moves_the_head() {
    let dir = tempfile::tempdir().unwrap();
    let log = ActivityLog::open(dir.path());

    let first = log
        .append(OpKind::Init, &sample_view('a'), "start")
        .unwrap();
    assert_eq!(first.parent, None);
    assert_eq!(first.kind, OpKind::Init);
    assert_eq!(first.position, first.id);
    assert!(first.redo.is_empty());
    assert_eq!(first.view, sample_view('a'));
    assert_eq!(first.description, "start");
    assert_eq!(log.head().unwrap(), Some(first.id.clone()));
    assert!(
        dir.path()
            .join(format!("activity/ops/{}.json", first.id))
            .is_file()
    );

    let second = log
        .append(OpKind::Save, &sample_view('b'), "second")
        .unwrap();
    assert_eq!(second.parent, Some(first.id.clone()));
    assert_ne!(second.id, first.id);
    assert_eq!(log.head().unwrap(), Some(second.id.clone()));
    assert_eq!(log.read(&second.id).unwrap(), second);
    assert_eq!(log.read(&first.id).unwrap(), first);
}

#[test]
fn reading_an_op_that_is_not_there_names_the_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = ActivityLog::open(dir.path());
    log.append(OpKind::Init, &sample_view('a'), "start")
        .unwrap();
    let ghost: OpId = "0".repeat(32).parse().unwrap();

    let error = log.read(&ghost).unwrap_err();

    assert!(
        matches!(error, ActivityError::MissingOp { .. }),
        "{error:?}"
    );
}

#[test]
fn an_op_id_is_exactly_32_lowercase_hex_characters() {
    let good = "0123456789abcdef0123456789abcdef";
    assert_eq!(good.parse::<OpId>().unwrap().to_string(), good);
    for bad in [
        "",
        "abc",
        "0123456789ABCDEF0123456789ABCDEF",
        "0123456789abcdef0123456789abcdeg",
        "0123456789abcdef0123456789abcdef0",
        "../../etc/passwd000000000000000",
    ] {
        assert!(bad.parse::<OpId>().is_err(), "{bad:?} should be rejected");
    }
}

#[test]
fn random_op_ids_are_valid_and_distinct() {
    let a = OpId::random();
    let b = OpId::random();
    assert_ne!(a, b);
    assert!(is_op_id(a.as_str()));
}

// ---- migration from format 1 ----

/// Rewrites a finished project as v0.1.0 left it: `view.json` (pretty JSON plus a newline)
/// and `version` = 1, with no `activity/`. Returns the view's JSON.
fn make_format_1(dir: &Path) -> Value {
    let view = read_op(dir, &head_id(dir))["view"].clone();
    let mut json = serde_json::to_vec_pretty(&view).unwrap();
    json.push(b'\n');
    fs::write(paper(dir).join("view.json"), json).unwrap();
    fs::remove_dir_all(paper(dir).join("activity")).unwrap();
    fs::write(paper(dir).join("version"), "1\n").unwrap();
    view
}

/// What migration must leave alone: refs, history and the working tree's status.
fn git_state(dir: &Path) -> (String, String, String) {
    (
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        git(dir, &["log", "--all", "--format=%H %s"]),
        git(dir, &["status", "--porcelain"]),
    )
}

fn assert_migrated(dir: &Path, original_view: &Value) {
    assert_eq!(version(dir), "2");
    assert!(!paper(dir).join("view.json").exists());
    assert!(paper(dir).join("view.json.v1").is_file());
    let ops = chain(dir);
    let root = ops.last().unwrap();
    assert_eq!(root["kind"], "migrate");
    assert_eq!(root["parent"], Value::Null);
    assert_eq!(&ops[0]["view"], original_view);
}

#[test]
fn opening_a_format_1_project_migrates_it_in_place() {
    let dir = new_project_dir();
    let original = make_format_1(dir.path());
    let json_bytes = fs::read(paper(dir.path()).join("view.json")).unwrap();
    let state = git_state(dir.path());

    let project = Project::open(dir.path(), identity()).unwrap();

    assert_migrated(dir.path(), &original);
    assert_eq!(chain(dir.path()).len(), 1, "exactly one migrate op");
    assert_eq!(op_files(dir.path()).len(), 1);
    assert_eq!(view_value(&project), original);
    assert_eq!(
        fs::read(paper(dir.path()).join("view.json.v1")).unwrap(),
        json_bytes,
        "the old view is kept as it was"
    );
    drop(project);
    assert_eq!(git_state(dir.path()), state);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn migration_keeps_every_lines_tip_and_draft() {
    let dir = adopted_project_dir();
    let original = make_format_1(dir.path());
    assert_eq!(original["lines"].as_object().unwrap().len(), 2);
    let state = git_state(dir.path());

    let project = Project::open(dir.path(), identity()).unwrap();

    assert_eq!(view_value(&project), original);
    assert_eq!(project.view().lines().count(), 2);
    drop(project);
    assert_migrated(dir.path(), &original);
    assert_eq!(git_state(dir.path()), state);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn a_migrated_project_keeps_working_and_is_not_migrated_twice() {
    let dir = new_project_dir();
    let original = make_format_1(dir.path());
    drop(Project::open(dir.path(), identity()).unwrap());
    let migrated = op_files(dir.path());

    let mut project = Project::open(dir.path(), identity()).unwrap();
    assert_eq!(
        op_files(dir.path()),
        migrated,
        "opening again writes nothing"
    );
    write(dir.path(), "a.tex", "after migration\n");
    save_all(&mut project, "after migration");
    drop(project);

    let ops = chain(dir.path());
    assert!(ops.len() >= 2);
    assert_eq!(ops.last().unwrap()["kind"], "migrate");
    for (name, bytes) in &migrated {
        assert_eq!(op_files(dir.path()).get(name), Some(bytes));
    }
    assert_ne!(ops[0]["view"], original);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

/// Turns a finished migration back into the files a crash at `step` would have left.
/// Steps in order: the migrate op, `head`, `version` = 2, `view.json` renamed.
#[derive(Clone, Copy, Debug)]
enum CrashAfter {
    /// The op file is written, `head` is not.
    Op,
    /// `head` is written, `version` is still 1.
    Head,
    /// `version` is 2, `view.json` has not been renamed yet.
    Version,
}

fn interrupted_migration(crash: CrashAfter) -> (tempfile::TempDir, Value) {
    let dir = new_project_dir();
    let original = make_format_1(dir.path());
    drop(Project::open(dir.path(), identity()).unwrap());
    let paper = paper(dir.path());
    fs::rename(paper.join("view.json.v1"), paper.join("view.json")).unwrap();
    match crash {
        CrashAfter::Op => {
            fs::remove_file(paper.join("activity/head")).unwrap();
            fs::write(paper.join("version"), "1\n").unwrap();
        }
        CrashAfter::Head => fs::write(paper.join("version"), "1\n").unwrap(),
        CrashAfter::Version => {}
    }
    (dir, original)
}

fn assert_next_open_completes(crash: CrashAfter) {
    let (dir, original) = interrupted_migration(crash);
    let state = git_state(dir.path());

    let project = Project::open(dir.path(), identity()).unwrap();

    assert_eq!(view_value(&project), original, "{crash:?}");
    drop(project);
    assert_migrated(dir.path(), &original);
    assert_eq!(git_state(dir.path()), state);
    // And the project is fully usable afterwards.
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "later\n");
    save_all(&mut project, "later");
    drop(project);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn a_migration_interrupted_after_the_op_is_finished_by_the_next_open() {
    assert_next_open_completes(CrashAfter::Op);
}

#[test]
fn a_migration_interrupted_after_the_head_is_finished_by_the_next_open() {
    assert_next_open_completes(CrashAfter::Head);
}

#[test]
fn a_migration_interrupted_after_the_version_is_finished_by_the_next_open() {
    assert_next_open_completes(CrashAfter::Version);
}

#[test]
fn a_format_1_project_with_a_damaged_view_json_is_a_clear_error_and_stays_format_1() {
    let dir = new_project_dir();
    make_format_1(dir.path());
    fs::write(paper(dir.path()).join("view.json"), "{ not json").unwrap();

    let error = Project::open(dir.path(), identity()).unwrap_err();

    assert_clear_project_error(error);
    assert_eq!(version(dir.path()), "1");
    assert!(paper(dir.path()).join("view.json").is_file());
}

// ---- unsupported and damaged state ----

#[test]
fn a_format_3_project_is_unsupported() {
    let dir = new_project_dir();
    assert_eq!(version(dir.path()), "2", "starts as a format 2 project");
    fs::write(paper(dir.path()).join("version"), "3\n").unwrap();

    let error = Project::open(dir.path(), identity()).unwrap_err();

    assert!(
        matches!(&error, OpenError::UnsupportedFormat { found } if found == "3"),
        "{error:?}"
    );
}

#[test]
fn a_missing_head_is_a_clear_project_error() {
    let dir = new_project_dir();
    fs::remove_file(paper(dir.path()).join("activity/head")).unwrap();

    assert_clear_project_error(Project::open(dir.path(), identity()).unwrap_err());
}

#[test]
fn a_garbled_head_is_a_clear_project_error() {
    let dir = new_project_dir();
    head_id(dir.path());
    fs::write(paper(dir.path()).join("activity/head"), "not an op id\n").unwrap();

    assert_clear_project_error(Project::open(dir.path(), identity()).unwrap_err());
}

#[test]
fn a_head_naming_an_absent_op_is_a_clear_project_error() {
    let dir = new_project_dir();
    head_id(dir.path());
    fs::write(
        paper(dir.path()).join("activity/head"),
        format!("{}\n", "0".repeat(32)),
    )
    .unwrap();

    assert_clear_project_error(Project::open(dir.path(), identity()).unwrap_err());
}

#[test]
fn a_missing_head_op_file_is_a_clear_project_error() {
    let dir = new_project_dir();
    let head = head_id(dir.path());
    fs::remove_file(ops_dir(dir.path()).join(format!("{head}.json"))).unwrap();

    assert_clear_project_error(Project::open(dir.path(), identity()).unwrap_err());
}

#[test]
fn a_damaged_head_op_file_is_a_clear_project_error() {
    let dir = new_project_dir();
    let head = head_id(dir.path());
    fs::write(
        ops_dir(dir.path()).join(format!("{head}.json")),
        "{ not json",
    )
    .unwrap();

    assert_clear_project_error(Project::open(dir.path(), identity()).unwrap_err());
}

#[test]
fn a_busy_project_still_reports_busy_while_migration_is_pending() {
    // Migration happens under the lock: a second opener waits and reports Busy, it does not
    // migrate behind the first one's back.
    let dir = new_project_dir();
    make_format_1(dir.path());
    let first = Project::open(dir.path(), identity()).unwrap();

    let second = Project::open_with(
        dir.path(),
        identity(),
        &LockOptions {
            wait: std::time::Duration::from_millis(100),
        },
    );

    assert!(matches!(second, Err(OpenError::Busy { .. })));
    drop(first);
    assert_eq!(version(dir.path()), "2");
}

#[test]
fn a_format_2_project_without_a_head_names_the_head_file_and_does_not_suggest_init() {
    let dir = new_project_dir();
    fs::remove_file(paper(dir.path()).join("activity/head")).unwrap();

    let error = Project::open(dir.path(), identity()).unwrap_err();

    assert!(error.to_string().contains("head"), "{error}");
    assert_clear_project_error(error);
}

#[test]
fn a_format_1_project_with_neither_head_nor_view_json_is_a_clear_error() {
    let dir = new_project_dir();
    make_format_1(dir.path());
    fs::remove_file(paper(dir.path()).join("view.json")).unwrap();

    let error = Project::open(dir.path(), identity()).unwrap_err();

    assert!(error.to_string().contains("view.json"), "{error}");
    assert_clear_project_error(error);
    assert_eq!(version(dir.path()), "1");
}

#[test]
fn init_will_not_delete_a_real_activity_log_whose_version_file_is_gone() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "two");
    drop(project);
    let ops = op_files(dir.path());
    let head = head_id(dir.path());
    fs::remove_file(paper(dir.path()).join("version")).unwrap();

    let error = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap_err();

    assert!(
        matches!(
            error,
            paper_core::project::InitError::ActivityLogInTheWay { .. }
        ),
        "{error:?}"
    );
    assert_eq!(op_files(dir.path()), ops);
    assert_eq!(head_id(dir.path()), head);
    let converted = PaperError::from(error);
    assert_eq!(converted.kind, ErrorKind::User, "{converted:?}");
}

#[test]
fn init_restarts_the_log_when_only_a_lone_init_op_was_left_behind() {
    let dir = new_project_dir();
    let leftover = op_files(dir.path());
    fs::remove_file(paper(dir.path()).join("version")).unwrap();

    drop(init(dir.path()));

    let ops = op_files(dir.path());
    assert_eq!(ops.len(), 1);
    assert!(leftover.keys().all(|name| !ops.contains_key(name)));
    assert_eq!(chain(dir.path())[0]["parent"], Value::Null);
}

#[test]
fn restore_records_its_result_as_a_restore_op_after_a_snapshot_op() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "an edit to keep\n");

    project
        .restore(&RestoreRequest {
            paths: vec!["a.tex".to_owned()],
            from: None,
        })
        .unwrap();

    let ops = chain(dir.path());
    assert_eq!(ops[0]["kind"], "restore", "{ops:?}");
    assert_eq!(ops[1]["kind"], "snapshot", "{ops:?}");
    assert_eq!(ops[0]["view"], view_value(&project));
}

#[test]
fn init_will_not_delete_several_op_files_even_without_a_head() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "two");
    drop(project);
    let ops = op_files(dir.path());
    assert!(ops.len() >= 2);
    fs::remove_file(paper(dir.path()).join("version")).unwrap();
    fs::remove_file(paper(dir.path()).join("activity/head")).unwrap();

    let error = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap_err();

    assert!(
        matches!(
            error,
            paper_core::project::InitError::ActivityLogInTheWay { .. }
        ),
        "{error:?}"
    );
    assert_eq!(op_files(dir.path()), ops);
}

// ---- PV-021: entries and descriptions ----

fn descriptions(dir: &Path) -> Vec<String> {
    chain(dir)
        .iter()
        .map(|op| op["description"].as_str().unwrap().to_owned())
        .collect()
}

fn kinds(dir: &Path) -> Vec<String> {
    chain(dir)
        .iter()
        .map(|op| op["kind"].as_str().unwrap().to_owned())
        .collect()
}

/// A project with `a.tex` saved twice ("one", then "two"), closed again.
fn project_dir_with_two_saves() -> tempfile::TempDir {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    save_all(&mut project, "Second");
    dir
}

fn restore(project: &mut Project, path: &str, from: Option<&str>) {
    project
        .restore(&RestoreRequest {
            paths: vec![path.to_owned()],
            from: from.map(|point| point.parse().unwrap()),
        })
        .unwrap();
}

/// The description the edits entry gets after changing the project as `change` says.
fn edits_description(change: impl FnOnce(&Path)) -> String {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let before = chain(dir.path()).len();
    change(dir.path());
    project.snapshot().unwrap();
    let ops = chain(dir.path());
    assert_eq!(ops.len(), before + 1, "{ops:?}");
    assert_eq!(ops[0]["kind"], "snapshot");
    ops[0]["description"].as_str().unwrap().to_owned()
}

#[test]
fn init_describes_a_new_project_as_starting_it() {
    let dir = new_project_dir();
    assert_eq!(descriptions(dir.path()), ["start the project"]);
    assert_eq!(kinds(dir.path()), ["init"]);
}

#[test]
fn init_describes_an_adopted_repository_as_adopting_it_and_records_one_entry() {
    let dir = adopted_project_dir();
    assert_eq!(descriptions(dir.path()), ["adopt the git repository"]);
    assert_eq!(kinds(dir.path()), ["init"]);
}

#[test]
fn a_migrated_project_keeps_the_upgrade_entry_at_the_bottom() {
    let dir = new_project_dir();
    make_format_1(dir.path());
    let mut project = Project::open(dir.path(), identity()).unwrap();
    assert_eq!(descriptions(dir.path()), ["upgrade to paper format 2"]);
    write(dir.path(), "a.tex", "later\n");
    save_all(&mut project, "Later");

    let entries = project.activity(20).unwrap();

    let bottom = entries.last().unwrap();
    assert_eq!(bottom.kind, OpKind::Migrate);
    assert_eq!(bottom.description, "upgrade to paper format 2");
    assert_eq!(entries.len(), 3, "{entries:?}");
}

#[test]
fn editing_files_then_saving_records_an_edits_entry_and_a_save_entry() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let before = chain(dir.path()).len();
    write(dir.path(), "a.tex", "two\n");
    write(dir.path(), "b.tex", "related\n");

    save_all(&mut project, "Rewrite related work");

    let ops = chain(dir.path());
    assert_eq!(ops.len(), before + 2, "{ops:?}");
    assert_eq!(ops[0]["kind"], "save");
    assert_eq!(ops[0]["description"], "save \"Rewrite related work\"");
    assert_eq!(ops[1]["kind"], "snapshot");
    assert_eq!(ops[1]["description"], "edited a.tex, b.tex");
    assert_eq!(ops[0]["view"], view_value(&project));
}

#[test]
fn a_save_of_edits_already_recorded_adds_only_the_save_entry() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "two\n");
    project.snapshot().unwrap();
    let before = chain(dir.path()).len();

    save_all(&mut project, "Second");

    let ops = chain(dir.path());
    assert_eq!(ops.len(), before + 1, "{ops:?}");
    assert_eq!(ops[0]["kind"], "save");
    assert_eq!(ops[0]["description"], "save \"Second\"");
}

#[test]
fn a_save_that_finds_nothing_to_save_records_nothing() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let before = op_files(dir.path());

    let outcome = project
        .save(&SaveRequest {
            message: "nothing".to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();

    assert!(matches!(
        outcome,
        paper_core::project::SaveOutcome::NothingToSave
    ));
    assert_eq!(op_files(dir.path()), before);
}

#[test]
fn the_edits_description_names_one_file() {
    let text = edits_description(|dir| write(dir, "a.tex", "changed\n"));
    assert_eq!(text, "edited a.tex");
}

#[test]
fn the_edits_description_lists_three_files_sorted_in_full() {
    let text = edits_description(|dir| {
        write(dir, "z.tex", "z\n");
        write(dir, "sections/m.tex", "m\n");
        write(dir, "a.tex", "changed\n");
    });
    assert_eq!(text, "edited a.tex, sections/m.tex, z.tex");
}

#[test]
fn the_edits_description_says_and_one_more_for_a_fourth_file() {
    let text = edits_description(|dir| {
        for name in ["d.tex", "c.tex", "b.tex"] {
            write(dir, name, "x\n");
        }
        write(dir, "a.tex", "changed\n");
    });
    assert_eq!(text, "edited a.tex, b.tex, c.tex and 1 more");
}

#[test]
fn the_edits_description_counts_the_files_beyond_the_first_three_after_sorting() {
    let text = edits_description(|dir| {
        for name in ["f.tex", "e.tex", "d.tex", "c.tex", "b.tex"] {
            write(dir, name, "x\n");
        }
        write(dir, "a.tex", "changed\n");
    });
    assert_eq!(text, "edited a.tex, b.tex, c.tex and 3 more");
}

#[test]
fn added_and_deleted_files_count_as_edited() {
    let text = edits_description(|dir| {
        fs::remove_file(dir.join("a.tex")).unwrap();
        write(dir, "new.tex", "new\n");
    });
    assert_eq!(text, "edited a.tex, new.tex");
}

#[test]
fn restore_after_edits_records_an_edits_entry_then_a_restore_entry() {
    let dir = project_dir_with_two_saves();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "an edit to keep\n");
    let before = chain(dir.path()).len();

    restore(&mut project, "a.tex", None);

    let ops = chain(dir.path());
    assert_eq!(ops.len(), before + 2, "{ops:?}");
    assert_eq!(ops[0]["kind"], "restore");
    assert_eq!(ops[0]["description"], "restore a.tex from last");
    assert_eq!(ops[1]["kind"], "snapshot");
    assert_eq!(ops[1]["description"], "edited a.tex");
}

#[test]
fn restore_from_an_older_save_names_the_point_as_given() {
    let dir = project_dir_with_two_saves();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let before = chain(dir.path()).len();

    restore(&mut project, "a.tex", Some("last~1"));

    let ops = chain(dir.path());
    assert_eq!(ops.len(), before + 1, "no edits to record: {ops:?}");
    assert_eq!(ops[0]["kind"], "restore");
    assert_eq!(ops[0]["description"], "restore a.tex from last~1");
}

#[test]
fn a_restore_that_changes_nothing_records_nothing() {
    let dir = project_dir_with_two_saves();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let before = op_files(dir.path());

    restore(&mut project, "a.tex", None);

    assert_eq!(op_files(dir.path()), before);
}

#[test]
fn a_restore_that_changes_nothing_still_records_the_edits_it_found() {
    let dir = project_dir_with_two_saves();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "b.tex", "an edit elsewhere\n");
    let before = chain(dir.path()).len();

    restore(&mut project, "a.tex", None);

    let ops = chain(dir.path());
    assert_eq!(ops.len(), before + 1, "{ops:?}");
    assert_eq!(ops[0]["kind"], "snapshot");
    assert_eq!(ops[0]["description"], "edited b.tex");
}

#[test]
fn status_records_an_edits_entry_only_when_files_changed() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let start = chain(dir.path()).len();

    project.status(&StatusOptions::default()).unwrap();
    assert_eq!(chain(dir.path()).len(), start, "nothing changed, no entry");

    write(dir.path(), "a.tex", "edited\n");
    project.status(&StatusOptions::default()).unwrap();
    let ops = chain(dir.path());
    assert_eq!(ops.len(), start + 1);
    assert_eq!(ops[0]["kind"], "snapshot");
    assert_eq!(ops[0]["description"], "edited a.tex");

    project.status(&StatusOptions::default()).unwrap();
    assert_eq!(
        chain(dir.path()).len(),
        start + 1,
        "edits are recorded once"
    );
}

#[test]
fn status_without_a_snapshot_records_nothing() {
    let dir = new_project_dir();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    let before = op_files(dir.path());
    write(dir.path(), "a.tex", "edited\n");

    project.status(&StatusOptions { snapshot: false }).unwrap();

    assert_eq!(op_files(dir.path()), before);
}

// ---- PV-021: the listing ----

fn entry_ids(entries: &[ActivityEntry]) -> Vec<String> {
    entries.iter().map(|e| e.id.to_string()).collect()
}

/// A project with an init entry and `extra` more ops appended straight to the log.
fn project_with_many_ops(extra: usize) -> (tempfile::TempDir, Project) {
    let dir = new_project_dir();
    let project = Project::open(dir.path(), identity()).unwrap();
    let log = ActivityLog::open(project.paper_dir());
    for n in 0..extra {
        log.append(
            OpKind::Snapshot,
            project.view(),
            &format!("edited file{n}.tex"),
        )
        .unwrap();
    }
    (dir, project)
}

/// Every id is prefixed by its short id, which is at least 4 characters, hex, shared with no
/// other listed id, and no longer than needed.
fn assert_short_ids_are_the_shortest_unique_prefixes(entries: &[ActivityEntry]) {
    for entry in entries {
        let short = &entry.short_id;
        let full = entry.id.as_str();
        assert!(short.len() >= MIN_SHORT_ID, "{short}");
        assert_eq!(MIN_SHORT_ID, 4);
        assert!(full.starts_with(short.as_str()), "{short} of {full}");
        let sharing = |prefix: &str| {
            entries
                .iter()
                .filter(|other| other.id.as_str().starts_with(prefix))
                .count()
        };
        assert_eq!(sharing(short), 1, "{short} is not unique");
        if short.len() > MIN_SHORT_ID {
            assert!(
                sharing(&short[..short.len() - 1]) > 1,
                "{short} is longer than it needs to be"
            );
        }
    }
}

#[test]
fn activity_lists_the_head_chain_newest_first_with_every_field() {
    let dir = project_dir_with_two_saves();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "three\n");
    save_all(&mut project, "Third");
    let ops = chain(dir.path());

    let entries = project.activity(20).unwrap();

    assert_eq!(entries.len(), ops.len());
    for (entry, op) in entries.iter().zip(&ops) {
        assert_eq!(entry.id.as_str(), op["id"].as_str().unwrap());
        assert_eq!(entry.time, op["time"].as_str().unwrap());
        assert_eq!(entry.description, op["description"].as_str().unwrap());
        let kind = serde_json::to_value(entry.kind).unwrap();
        assert_eq!(kind, op["kind"]);
    }
    assert_eq!(entries[0].description, "save \"Third\"");
    assert_eq!(entries.last().unwrap().description, "start the project");
}

#[test]
fn activity_limit_keeps_the_newest_entries() {
    let dir = project_dir_with_two_saves();
    let project = Project::open(dir.path(), identity()).unwrap();
    let all = entry_ids(&project.activity(20).unwrap());
    assert_eq!(all.len(), 3);

    let two = project.activity(2).unwrap();

    assert_eq!(entry_ids(&two), all[..2]);
    assert_eq!(project.activity(100).unwrap().len(), 3);
}

#[test]
fn activity_reads_only_the_head_chain_and_ignores_other_files_in_ops() {
    let dir = project_dir_with_two_saves();
    let expected = entry_ids(
        &Project::open(dir.path(), identity())
            .unwrap()
            .activity(20)
            .unwrap(),
    );
    let ops = ops_dir(dir.path());
    let a_real_op = fs::read(ops.join(format!("{}.json", head_id(dir.path())))).unwrap();
    let stray = OpId::random();
    fs::write(ops.join(format!("{stray}.json")), &a_real_op).unwrap();
    fs::write(ops.join("garbage.json"), "{ not json").unwrap();
    fs::write(ops.join(".tmp1234"), "half written").unwrap();
    let project = Project::open(dir.path(), identity()).unwrap();

    let entries = project.activity(20).unwrap();

    assert_eq!(entry_ids(&entries), expected);
}

#[test]
fn activity_neither_snapshots_nor_records_anything() {
    let dir = new_project_dir();
    let project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "unrecorded edit\n");
    let before = op_files(dir.path());
    let head = head_id(dir.path());

    let entries = project.activity(20).unwrap();

    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(op_files(dir.path()), before);
    assert_eq!(head_id(dir.path()), head);
}

#[test]
fn short_ids_of_many_random_ops_are_unique_and_as_short_as_possible() {
    let (_dir, project) = project_with_many_ops(400);

    let entries = project.activity(1000).unwrap();

    assert_eq!(entries.len(), 401);
    assert_short_ids_are_the_shortest_unique_prefixes(&entries);
    let mut shorts: Vec<&str> = entries.iter().map(|e| e.short_id.as_str()).collect();
    shorts.sort_unstable();
    shorts.dedup();
    assert_eq!(shorts.len(), entries.len(), "short ids are all different");
}

#[test]
fn short_ids_are_unique_among_the_listed_entries_only() {
    let (_dir, project) = project_with_many_ops(300);

    let entries = project.activity(40).unwrap();

    assert_eq!(entries.len(), 40);
    assert_short_ids_are_the_shortest_unique_prefixes(&entries);
}

fn op_id(prefix: &str) -> OpId {
    format!("{prefix}{}", "0".repeat(32 - prefix.len()))
        .parse()
        .unwrap()
}

#[test]
fn short_ids_are_four_characters_when_nothing_clashes() {
    let ids = [op_id("1234"), op_id("abcd"), op_id("ffff")];
    assert_eq!(short_ids(&ids), ["1234", "abcd", "ffff"]);
    assert_eq!(short_ids(&[op_id("9876")]), ["9876"]);
    assert!(short_ids(&[]).is_empty());
}

#[test]
fn short_ids_grow_until_ids_sharing_a_prefix_differ() {
    let ids = [
        op_id("abcdef"),
        op_id("1234"),
        op_id("abcdee"),
        op_id("abcd0"),
        op_id("abcd1"),
    ];
    assert_eq!(
        short_ids(&ids),
        ["abcdef", "1234", "abcdee", "abcd0", "abcd1"]
    );
}

#[test]
fn short_ids_use_all_32_characters_for_ids_differing_only_in_the_last_one() {
    let a: OpId = format!("{}0", "c".repeat(31)).parse().unwrap();
    let b: OpId = format!("{}1", "c".repeat(31)).parse().unwrap();
    assert_eq!(
        short_ids(&[a.clone(), b.clone()]),
        [a.to_string(), b.to_string()]
    );
}

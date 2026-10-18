//! Recovery on open (PV-025): a `checkout-pending` marker is repaired by the rule in
//! 03-architecture.md § Command lifecycle, and refs that lag the head op are exported again.
//! The marker states are made by hand, on real repositories, and cross-checked with `git`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use paper_core::activity::{ActivityLog, OpId};
use paper_core::profile::Profile;
use paper_core::project::*;
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;

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

fn save_all(project: &mut Project, message: &str) {
    let outcome = project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
    assert!(matches!(outcome, SaveOutcome::Saved(_)), "{outcome:?}");
}

fn paper_dir(dir: &Path) -> PathBuf {
    dir.join(".git/paper")
}

fn marker_path(dir: &Path) -> PathBuf {
    paper_dir(dir).join("checkout-pending")
}

fn log(dir: &Path) -> ActivityLog {
    ActivityLog::open(&paper_dir(dir))
}

fn head_id(dir: &Path) -> OpId {
    log(dir).head().unwrap().unwrap()
}

fn draft_tree(project: &Project) -> String {
    let draft = &project.view().current().draft;
    project
        .repo()
        .read_commit(draft)
        .unwrap()
        .tree
        .as_str()
        .to_owned()
}

/// Writes a `checkout-pending` marker by hand (schema 1: `from` op id, `tree` id).
fn write_marker(dir: &Path, from: &OpId, tree: &str) {
    let json = format!(
        "{{\n  \"schema\": 1,\n  \"from\": \"{}\",\n  \"tree\": \"{tree}\"\n}}\n",
        from.as_str()
    );
    fs::write(marker_path(dir), json).unwrap();
}

fn text(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

/// A project whose last command was an `undo` of an edits entry, with the files still as they
/// were before the undo: the state a crash right after the op leaves.
struct Crashed {
    dir: tempfile::TempDir,
    /// The head op before the undo (what the marker names as `from`).
    from: OpId,
    /// The draft tree the undo moves to (what the marker names as `tree`).
    tree: String,
    /// A tree that is not `tree`: the first save's.
    other_tree: String,
}

/// Files `a.tex` and `b.tex` are saved as v2; then edited to v3 with a new `c.tex`, and
/// recorded (an edits entry); then `undo` takes the edits back (a and b to v2, c deleted).
/// The files are then put back as they were before the undo, as if it crashed after the op.
fn crashed_after_the_undo_op() -> Crashed {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.tex", "a1\n");
    write(dir.path(), "b.tex", "b1\n");
    let (mut project, _) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    let other_tree = draft_tree(&project);
    write(dir.path(), "a.tex", "a2\n");
    write(dir.path(), "b.tex", "b2\n");
    save_all(&mut project, "Second");
    write(dir.path(), "a.tex", "a3\n");
    write(dir.path(), "b.tex", "b3\n");
    write(dir.path(), "c.tex", "c3\n");
    project.snapshot().unwrap();
    let from = head_id(dir.path());

    project.undo().unwrap();
    let tree = draft_tree(&project);
    drop(project);
    assert_eq!(text(dir.path(), "a.tex"), "a2\n");
    assert!(!dir.path().join("c.tex").exists());

    write(dir.path(), "a.tex", "a3\n");
    write(dir.path(), "b.tex", "b3\n");
    write(dir.path(), "c.tex", "c3\n");
    Crashed {
        dir,
        from,
        tree,
        other_tree,
    }
}

fn open(dir: &Path) -> Project {
    Project::open(dir, identity()).unwrap()
}

/// Bytes of every file under `root` (not following links), by relative path.
fn tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
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
    walk(root, root, &mut out);
    out
}

// ---- the marker: the three branches of the repair rule ----

#[test]
fn a_marker_whose_op_never_committed_is_dropped_and_the_files_are_untouched() {
    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    // Rule 1: head is `from`. Make that true by writing the marker for the undo's own head
    // parent state: the op the project is at is named, so the command never committed.
    let head = head_id(dir);
    write_marker(dir, &head, &crashed.other_tree);
    let files_before = (text(dir, "a.tex"), text(dir, "b.tex"), text(dir, "c.tex"));
    let ops_before = log(dir).op_file_count();

    let project = open(dir);

    assert!(!marker_path(dir).exists(), "the stale marker is dropped");
    assert_eq!(
        (text(dir, "a.tex"), text(dir, "b.tex"), text(dir, "c.tex")),
        files_before
    );
    assert_eq!(log(dir).op_file_count(), ops_before);
    assert_eq!(head_id(dir), head);
    drop(project);
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn a_marker_whose_op_committed_gets_its_checkout_finished() {
    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    write_marker(dir, &crashed.from, &crashed.tree);

    let project = open(dir);

    assert!(!marker_path(dir).exists(), "the finished marker is removed");
    assert_eq!(text(dir, "a.tex"), "a2\n");
    assert_eq!(text(dir, "b.tex"), "b2\n");
    assert!(
        !dir.join("c.tex").exists(),
        "c.tex did not exist at the target"
    );
    drop(project);
    assert_eq!(git(dir, &["status", "--porcelain"]), "");
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn finishing_a_checkout_leaves_the_files_already_rewritten_as_they_are() {
    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    // The crash came after the first file: a.tex is already at the target.
    write(dir, "a.tex", "a2\n");
    write_marker(dir, &crashed.from, &crashed.tree);

    let project = open(dir);

    assert!(!marker_path(dir).exists());
    assert_eq!(text(dir, "a.tex"), "a2\n");
    assert_eq!(text(dir, "b.tex"), "b2\n");
    assert!(!dir.join("c.tex").exists());
    drop(project);
}

#[test]
fn a_marker_naming_an_unrelated_op_is_dropped_and_the_files_are_left_alone() {
    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    // Rule 3: head is not `from`, and head's parent is not `from` either (the files have been
    // snapshotted since): the marker is only dropped.
    let older = log(dir).read(&crashed.from).unwrap().parent.unwrap();
    write_marker(dir, &older, &crashed.tree);
    let ops_before = log(dir).op_file_count();

    let project = open(dir);

    assert!(!marker_path(dir).exists());
    assert_eq!(text(dir, "a.tex"), "a3\n", "nothing was rewritten");
    assert_eq!(text(dir, "b.tex"), "b3\n");
    assert_eq!(text(dir, "c.tex"), "c3\n");
    assert_eq!(log(dir).op_file_count(), ops_before);
    drop(project);
}

#[test]
fn a_marker_whose_tree_is_not_the_head_drafts_is_dropped_and_the_files_are_left_alone() {
    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    // Head's parent is `from`, but head's draft tree is not `tree`: rule 3.
    write_marker(dir, &crashed.from, &crashed.other_tree);

    let project = open(dir);

    assert!(!marker_path(dir).exists());
    assert_eq!(text(dir, "a.tex"), "a3\n");
    assert_eq!(text(dir, "b.tex"), "b3\n");
    assert_eq!(text(dir, "c.tex"), "c3\n");
    drop(project);
}

// ---- an edit made after the crash is never overwritten ----

#[test]
fn a_file_edited_after_the_crash_is_never_overwritten_by_the_repair() {
    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    write_marker(dir, &crashed.from, &crashed.tree);
    write(dir, "a.tex", "my edit after the crash\n");

    let mut project = open(dir);

    assert!(!marker_path(dir).exists());
    assert_eq!(
        text(dir, "a.tex"),
        "my edit after the crash\n",
        "an edited file is left alone"
    );
    assert_eq!(text(dir, "b.tex"), "b2\n", "the other files are finished");
    assert!(!dir.join("c.tex").exists());

    // The edit ends up as an edits entry, not as a loss.
    let state = project.snapshot().unwrap();
    assert!(
        state.changes.iter().any(|c| c.path == "a.tex"),
        "{:?}",
        state.changes
    );
    let log = log(dir);
    let top = log.read(&log.head().unwrap().unwrap()).unwrap();
    assert!(
        top.description.starts_with("edited a.tex"),
        "{}",
        top.description
    );
    assert_eq!(text(dir, "a.tex"), "my edit after the crash\n");
}

#[test]
fn a_file_that_would_be_deleted_but_was_edited_after_the_crash_is_kept() {
    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    write_marker(dir, &crashed.from, &crashed.tree);
    write(dir, "c.tex", "c is mine now\n");

    let project = open(dir);

    assert!(!marker_path(dir).exists());
    assert_eq!(text(dir, "c.tex"), "c is mine now\n");
    assert_eq!(text(dir, "a.tex"), "a2\n");
    drop(project);
}

// ---- recovery is silent and cheap ----

#[test]
fn recovery_records_no_activity_entry() {
    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    write_marker(dir, &crashed.from, &crashed.tree);
    let head = head_id(dir);
    let ops_before = log(dir).op_file_count();

    drop(open(dir));

    assert_eq!(head_id(dir), head);
    assert_eq!(log(dir).op_file_count(), ops_before);
}

#[test]
fn opening_a_project_with_nothing_to_repair_writes_nothing() {
    // Modification times of exported.json and of every ref file (loose or packed).
    fn mtimes(dir: &Path) -> Vec<(String, std::time::SystemTime)> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut paths = vec![
            paper_dir(dir).join("exported.json"),
            dir.join(".git/packed-refs"),
        ];
        walk(&dir.join(".git/refs"), &mut paths);
        let mut found: Vec<_> = paths
            .into_iter()
            .filter_map(|path| {
                let modified = fs::metadata(&path).ok()?.modified().ok()?;
                Some((path.to_string_lossy().into_owned(), modified))
            })
            .collect();
        found.sort();
        found
    }

    let crashed = crashed_after_the_undo_op();
    let dir = crashed.dir.path();
    // Bring the project to rest first: the next command snapshots and settles everything.
    drop(open(dir));
    open(dir).snapshot().unwrap();

    let snapshot = |dir: &Path| {
        let mut paper = tree_bytes(&paper_dir(dir));
        // The lock and its holder hint are written by every open.
        paper.retain(|name, _| name != "lock" && name != "lock-holder");
        (
            paper,
            mtimes(dir),
            git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
            git(dir, &["ls-files", "--stage"]),
            fs::read(dir.join(".git/HEAD")).unwrap(),
            fs::read(dir.join(".git/index")).unwrap(),
        )
    };
    let before = snapshot(dir);

    drop(open(dir));

    assert_eq!(snapshot(dir), before);
}

// ---- refs that lag the head op ----

/// A saved project, and what a crash between the op and the export leaves: the refs, index and
/// `exported.json` as they were before the save, with the op already committed.
struct Lagging {
    dir: tempfile::TempDir,
    new_tip: String,
    new_exported: Vec<u8>,
    old_exported: Vec<u8>,
    old_tip: String,
    old_draft: String,
}

fn after_a_save_whose_export_never_ran() -> Lagging {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.tex", "a1\n");
    let (mut project, _) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    let exported = paper_dir(dir.path()).join("exported.json");
    let old_exported = fs::read(&exported).unwrap();
    let old_tip = git(dir.path(), &["rev-parse", "refs/heads/main"]);
    let old_draft = git(dir.path(), &["rev-parse", "refs/paper/draft/main"]);
    write(dir.path(), "a.tex", "a2\n");
    save_all(&mut project, "Second");
    drop(project);
    let new_tip = git(dir.path(), &["rev-parse", "refs/heads/main"]);
    let new_exported = fs::read(&exported).unwrap();
    assert_ne!(old_tip, new_tip);

    // Rewind git to before the save, as if the export never ran.
    git(
        dir.path(),
        &["update-ref", "refs/heads/main", old_tip.trim()],
    );
    git(
        dir.path(),
        &["update-ref", "refs/paper/draft/main", old_draft.trim()],
    );
    git(dir.path(), &["read-tree", old_tip.trim()]);
    fs::write(&exported, &old_exported).unwrap();
    Lagging {
        dir,
        new_tip,
        new_exported,
        old_exported,
        old_tip,
        old_draft,
    }
}

#[test]
fn refs_that_lag_the_head_op_are_exported_on_open() {
    let lagging = after_a_save_whose_export_never_ran();
    let dir = lagging.dir.path();
    assert_eq!(
        git(dir, &["rev-parse", "refs/heads/main"]),
        lagging.old_tip,
        "the fixture lags"
    );
    assert_ne!(lagging.old_draft, "");

    drop(open(dir));

    assert_eq!(git(dir, &["rev-parse", "refs/heads/main"]), lagging.new_tip);
    assert_eq!(
        git(dir, &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/main"
    );
    assert_eq!(
        fs::read(paper_dir(dir).join("exported.json")).unwrap(),
        lagging.new_exported
    );
    assert_eq!(git(dir, &["status", "--porcelain"]), "");
    assert_eq!(
        git(dir, &["log", "--format=%s", "main"]).lines().next(),
        Some("Second")
    );
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn an_exported_json_that_lags_the_head_op_is_rewritten_on_open() {
    let lagging = after_a_save_whose_export_never_ran();
    let dir = lagging.dir.path();
    // Only exported.json lags: the refs and the index were written.
    git(
        dir,
        &["update-ref", "refs/heads/main", lagging.new_tip.trim()],
    );
    git(
        dir,
        &["reset", "--quiet", "--mixed", lagging.new_tip.trim()],
    );
    assert_ne!(lagging.old_exported, lagging.new_exported);

    drop(open(dir));

    assert_eq!(
        fs::read(paper_dir(dir).join("exported.json")).unwrap(),
        lagging.new_exported
    );
    assert_eq!(git(dir, &["rev-parse", "refs/heads/main"]), lagging.new_tip);
}

#[test]
fn recovering_twice_changes_nothing_the_second_time() {
    let lagging = after_a_save_whose_export_never_ran();
    let dir = lagging.dir.path();

    drop(open(dir));
    let once = (
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        git(dir, &["ls-files", "--stage"]),
        fs::read(paper_dir(dir).join("exported.json")).unwrap(),
        log(dir).op_file_count(),
    );
    drop(open(dir));
    let twice = (
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        git(dir, &["ls-files", "--stage"]),
        fs::read(paper_dir(dir).join("exported.json")).unwrap(),
        log(dir).op_file_count(),
    );

    assert_eq!(once, twice);
    assert_eq!(once.2, lagging.new_exported);
}

//! PV-031: undo, redo and `undo --to` keep walking when the checkout had to leave a file
//! alone (an ignored file or a symlink in the way). The leftovers are remembered in
//! `.git/paper/checkout-left.json` and ignored by the undo family's own snapshot only.

use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::profile::Profile;
use paper_core::project::*;
use paper_core::snapshot::SnapshotOptions;
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

fn text(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

fn project_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        write(dir.path(), path, content);
    }
    let (project, _) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    (dir, project)
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

fn undone(outcome: UndoOutcome) -> Undone {
    match outcome {
        UndoOutcome::Undone(undone) => undone,
        other => panic!("expected an undone entry, got {other:?}"),
    }
}

fn undo(project: &mut Project) -> Undone {
    undone(project.undo().unwrap())
}

fn redo(project: &mut Project) -> Undone {
    undone(project.redo().unwrap())
}

fn head_id(dir: &Path) -> String {
    fs::read_to_string(dir.join(".git/paper/activity/head"))
        .unwrap()
        .trim()
        .to_owned()
}

fn head_op(dir: &Path) -> Value {
    let bytes =
        fs::read(dir.join(format!(".git/paper/activity/ops/{}.json", head_id(dir)))).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// The id of the op the project is at.
fn position(dir: &Path) -> String {
    head_op(dir)["position"].as_str().unwrap().to_owned()
}

/// How many "edited ..." entries the activity log holds.
fn edited_count(project: &Project) -> usize {
    project
        .activity(1000)
        .unwrap()
        .iter()
        .filter(|e| e.description.starts_with("edited"))
        .count()
}

fn left_path(dir: &Path) -> std::path::PathBuf {
    dir.join(".git/paper/checkout-left.json")
}

fn left_json(dir: &Path) -> Value {
    serde_json::from_slice(&fs::read(left_path(dir)).unwrap()).unwrap()
}

fn assert_git_consistent(dir: &Path) {
    git(dir, &["fsck", "--strict", "--no-progress"]);
    assert!(!dir.join(".git/paper/checkout-pending").exists());
}

/// The ops, oldest first, as they stand in the log.
fn op_ids_oldest_first(project: &Project) -> Vec<String> {
    let mut ids: Vec<String> = project
        .activity(1000)
        .unwrap()
        .iter()
        .map(|e| e.id.to_string())
        .collect();
    ids.reverse();
    ids
}

/// A project with an ignored file `chap/cache.log` that blocks bringing the file `chap`
/// back. Ops, oldest first: 0 start, 1 edited keep.tex, 2 save "one", 3 edited chap(+),
/// 4 save "folder". The project is at 4; the ignored log is on disk.
fn ignored_scenario() -> (tempfile::TempDir, Project, Vec<String>) {
    let (dir, mut project) = project_with(&[
        (".gitignore", "*.log\n"),
        ("chap", "a file\n"),
        ("keep.tex", "v0\n"),
    ]);
    write(dir.path(), "keep.tex", "v1\n");
    save_all(&mut project, "one");
    fs::remove_file(dir.path().join("chap")).unwrap();
    write(dir.path(), "chap/one.tex", "1\n");
    save_all(&mut project, "folder");
    write(dir.path(), "chap/cache.log", "my ignored log\n");
    let ids = op_ids_oldest_first(&project);
    assert_eq!(ids.len(), 5, "{ids:?}");
    (dir, project, ids)
}

/// [`ignored_scenario`] after the two undos that reach op 2, where `chap` can't come back.
fn at_the_leftover() -> (tempfile::TempDir, Project, Vec<String>) {
    let (dir, mut project, ids) = ignored_scenario();
    undo(&mut project);
    let second = undo(&mut project);
    assert!(!second.files_match);
    assert_eq!(position(dir.path()), ids[2]);
    (dir, project, ids)
}

// ---- sanity of the scenario builders (passes on main) ----

#[test]
fn the_ignored_scenario_leaves_chap_and_its_file_behind_at_the_second_undo() {
    let (dir, mut project, ids) = ignored_scenario();
    assert_eq!(position(dir.path()), ids[4]);

    let first = undo(&mut project);
    assert!(first.files_match, "the first undo only moves the line");
    assert_eq!(position(dir.path()), ids[3]);
    let second = undo(&mut project);

    assert!(!second.files_match);
    let skipped: Vec<&str> = second.skipped.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(skipped, ["chap", "chap/one.tex"]);
    assert_eq!(position(dir.path()), ids[2]);
    assert_eq!(text(dir.path(), "chap/cache.log"), "my ignored log\n");
    assert_eq!(text(dir.path(), "chap/one.tex"), "1\n");
    assert_git_consistent(dir.path());
}

// ---- the leftovers do not hold undo back ----

#[test]
fn an_ignored_file_in_the_way_does_not_stop_undo_from_walking_back_and_records_no_edit() {
    let (dir, mut project, ids) = at_the_leftover();
    let edited = edited_count(&project);

    let third = undo(&mut project);
    assert_eq!(position(dir.path()), ids[1], "undo walks one entry back");
    let fourth = undo(&mut project);
    assert_eq!(position(dir.path()), ids[0]);

    assert_eq!(edited_count(&project), edited, "no edits entry is recorded");
    assert!(!third.entry.description.contains("chap"), "{third:?}");
    assert!(!fourth.entry.description.contains("chap"), "{fourth:?}");
    assert_eq!(text(dir.path(), "keep.tex"), "v0\n");
    assert_eq!(text(dir.path(), "chap/cache.log"), "my ignored log\n");
    assert_eq!(text(dir.path(), "chap/one.tex"), "1\n");
    assert_git_consistent(dir.path());
    assert!(matches!(
        project.undo().unwrap(),
        UndoOutcome::NothingToUndo
    ));
}

#[test]
fn redo_brings_every_entry_back_in_order_after_undos_that_left_a_file_alone() {
    let (dir, mut project, ids) = at_the_leftover();
    undo(&mut project);
    undo(&mut project);

    for expected in [&ids[1], &ids[2], &ids[3], &ids[4]] {
        redo(&mut project);
        assert_eq!(&position(dir.path()), expected);
    }

    assert!(matches!(
        project.redo().unwrap(),
        UndoOutcome::NothingToRedo
    ));
    assert_eq!(text(dir.path(), "keep.tex"), "v1\n");
    assert_eq!(text(dir.path(), "chap/one.tex"), "1\n");
    assert_eq!(text(dir.path(), "chap/cache.log"), "my ignored log\n");
    assert!(
        !left_path(dir.path()).exists(),
        "the files match again, so nothing is remembered"
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert_git_consistent(dir.path());
}

#[test]
fn redo_right_after_a_checkout_that_left_a_file_alone_records_no_edit() {
    let (dir, mut project, ids) = at_the_leftover();
    let edited = edited_count(&project);

    redo(&mut project);

    assert_eq!(position(dir.path()), ids[3]);
    assert_eq!(edited_count(&project), edited);
}

#[test]
fn undo_to_goes_on_from_a_checkout_that_left_a_file_alone_without_recording_an_edit() {
    let (dir, mut project, ids) = at_the_leftover();
    let edited = edited_count(&project);
    let target = project
        .activity(1000)
        .unwrap()
        .into_iter()
        .find(|e| e.id.to_string() == ids[1])
        .unwrap()
        .short_id;

    undone(project.undo_to(&target).unwrap());

    assert_eq!(position(dir.path()), ids[1]);
    assert_eq!(edited_count(&project), edited, "no edits entry");
    assert_eq!(text(dir.path(), "chap/cache.log"), "my ignored log\n");
    assert_git_consistent(dir.path());
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_way_does_not_stop_undo_from_walking_back_and_never_gets_touched() {
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "secret.txt", "outside\n");
    let target = outside.path().join("secret.txt");
    let (dir, mut project) = project_with(&[("a.tex", "a\n"), ("b.tex", "b\n")]);
    write(dir.path(), "b.tex", "b2\n");
    save_all(&mut project, "two");
    fs::remove_file(dir.path().join("a.tex")).unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("a.tex")).unwrap();
    let before = op_ids_oldest_first(&project);
    assert_eq!(before.len(), 3, "start, edited b.tex, save two: {before:?}");

    // The first undo records the symlink as an edit (it is one) and takes it back; the
    // checkout then can't put the regular file back.
    let first = undo(&mut project);
    assert!(!first.files_match, "{first:?}");
    let ids = op_ids_oldest_first(&project);
    assert_eq!(position(dir.path()), ids[2]);
    let edited = edited_count(&project);

    undo(&mut project);
    assert_eq!(position(dir.path()), ids[1]);
    undo(&mut project);
    assert_eq!(position(dir.path()), ids[0]);

    assert_eq!(edited_count(&project), edited, "no further edits entry");
    let meta = fs::symlink_metadata(dir.path().join("a.tex")).unwrap();
    assert!(meta.file_type().is_symlink(), "the symlink stays");
    assert_eq!(fs::read_link(dir.path().join("a.tex")).unwrap(), target);
    assert_eq!(text(outside.path(), "secret.txt"), "outside\n");
    assert_eq!(text(dir.path(), "b.tex"), "b\n");

    redo(&mut project);
    assert_eq!(position(dir.path()), ids[1]);
    redo(&mut project);
    assert_eq!(position(dir.path()), ids[2]);
    assert!(
        fs::symlink_metadata(dir.path().join("a.tex"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_git_consistent(dir.path());
}

// ---- what is remembered ----

#[test]
fn the_checkout_remembers_what_it_left_alone_in_a_file_named_after_the_op_it_recorded() {
    let (dir, _project, _) = at_the_leftover();

    let remembered = left_json(dir.path());

    assert_eq!(remembered["schema"], 1);
    assert_eq!(remembered["op"], head_id(dir.path()).as_str());
    let paths = remembered["paths"].as_object().unwrap();
    assert_eq!(
        paths.keys().map(String::as_str).collect::<Vec<_>>(),
        ["chap", "chap/one.tex"]
    );
    assert!(paths["chap"].is_null(), "the snapshot sees nothing at chap");
    assert!(!paths["chap/one.tex"].is_null());
    // Written atomically: no temporary file is left next to it.
    let stray: Vec<String> = fs::read_dir(dir.path().join(".git/paper"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("checkout-left") && name != "checkout-left.json")
        .collect();
    assert!(stray.is_empty(), "{stray:?}");
    assert!(
        !git(dir.path(), &["status", "--porcelain"]).contains("checkout-left"),
        "plain git does not see the file"
    );
    assert_git_consistent(dir.path());
}

#[test]
fn the_remembered_file_is_removed_when_a_later_checkout_leaves_nothing_alone() {
    let (dir, mut project, ids) = at_the_leftover();
    assert!(left_path(dir.path()).exists());

    // The draft at op 3 holds chap/one.tex, which is on disk: the files match.
    let back = redo(&mut project);

    assert!(back.files_match);
    assert_eq!(position(dir.path()), ids[3]);
    assert!(!left_path(dir.path()).exists());
    assert_git_consistent(dir.path());
}

// ---- real edits are still recorded (I1) ----

#[test]
fn editing_a_remembered_path_again_records_the_normal_edits_entry() {
    let (dir, mut project, _) = at_the_leftover();
    let edited = edited_count(&project);
    let before = head_id(dir.path());
    write(dir.path(), "chap/one.tex", "changed\n");

    let outcome = undo(&mut project);

    assert_eq!(edited_count(&project), edited + 1, "I1 holds");
    assert!(
        outcome.entry.description.starts_with("edited"),
        "{outcome:?}"
    );
    assert_eq!(
        position(dir.path()),
        before,
        "the edits entry was taken back"
    );
    assert_git_consistent(dir.path());
}

#[test]
fn an_edit_to_a_remembered_path_taken_back_by_undo_can_be_redone() {
    let (dir, mut project, _) = at_the_leftover();
    write(dir.path(), "chap/one.tex", "changed\n");
    undo(&mut project);

    let back = redo(&mut project);

    assert!(back.entry.description.starts_with("edited"), "{back:?}");
    assert_eq!(text(dir.path(), "chap/one.tex"), "changed\n");
    assert_git_consistent(dir.path());
}

#[test]
fn editing_another_file_before_undo_records_every_difference_and_loses_nothing() {
    let (dir, mut project, _) = at_the_leftover();
    let edited = edited_count(&project);
    let before = head_id(dir.path());
    write(dir.path(), "keep.tex", "typed\n");

    let outcome = undo(&mut project);

    assert_eq!(edited_count(&project), edited + 1, "I1 holds");
    assert!(
        outcome.entry.description.starts_with("edited"),
        "{outcome:?}"
    );
    assert!(
        outcome.entry.description.contains("keep.tex"),
        "{outcome:?}"
    );
    assert_eq!(
        position(dir.path()),
        before,
        "the edits entry was taken back"
    );
    assert_eq!(
        text(dir.path(), "keep.tex"),
        "v1\n",
        "the edit was taken back"
    );
    assert_git_consistent(dir.path());
}

#[test]
fn an_edit_to_another_file_taken_back_by_undo_can_be_redone() {
    let (dir, mut project, _) = at_the_leftover();
    write(dir.path(), "keep.tex", "typed\n");
    undo(&mut project);

    redo(&mut project);

    assert_eq!(text(dir.path(), "keep.tex"), "typed\n");
    assert_git_consistent(dir.path());
}

#[test]
fn status_between_undos_records_the_leftovers_and_the_next_undo_still_gets_past_them() {
    let (dir, mut project, _) = at_the_leftover();
    let edited = edited_count(&project);
    let second_undo = head_id(dir.path());
    let first_undo = head_op(dir.path())["parent"].as_str().unwrap().to_owned();

    project.status(&StatusOptions::default()).unwrap();
    assert_eq!(edited_count(&project), edited + 1, "status records them");

    // One extra step: the edits entry is taken back, which lands on the second undo.
    undo(&mut project);
    assert_eq!(position(dir.path()), second_undo);
    let after_the_extra_step = edited_count(&project);
    // Then it walks on: the second undo is itself an entry that can be taken back.
    undo(&mut project);
    assert_eq!(position(dir.path()), first_undo, "never stuck");
    assert_eq!(edited_count(&project), after_the_extra_step);
    assert_git_consistent(dir.path());
}

// ---- stale, unreadable or foreign files ----

/// A well-formed remembered file for the leftovers of [`ignored_scenario`], with the given
/// schema and op. (The value of a path is whatever a snapshot sees there; the files below are
/// ignored by the reader in every variant that uses this, so only the shape matters.)
fn remembered_for(dir: &Path, schema: u64, op: &str) -> Value {
    let one = git(dir, &["hash-object", "chap/one.tex"]);
    serde_json::json!({
        "schema": schema,
        "op": op,
        "paths": { "chap": null, "chap/one.tex": { "id": one.trim(), "mode": "100644" } },
    })
}

fn assert_ignored_and_replaced(variant: &str, damage: &dyn Fn(&Path)) {
    let (dir, mut project, _) = at_the_leftover();
    let edited = edited_count(&project);
    let before = head_id(dir.path());
    damage(dir.path());

    let outcome = project
        .undo()
        .unwrap_or_else(|e| panic!("{variant}: a bad file is never an error: {e}"));

    assert!(matches!(outcome, UndoOutcome::Undone(_)), "{variant}");
    assert_eq!(
        edited_count(&project),
        edited + 1,
        "{variant}: today's behaviour, the leftovers are recorded as edits"
    );
    assert_eq!(position(dir.path()), before, "{variant}");
    let remembered = left_json(dir.path());
    assert_eq!(remembered["schema"], 1, "{variant}: replaced");
    assert_eq!(remembered["op"], head_id(dir.path()).as_str(), "{variant}");
    assert_git_consistent(dir.path());
}

#[test]
fn a_remembered_file_that_is_not_json_changes_nothing_and_is_replaced() {
    assert_ignored_and_replaced("not json", &|dir| {
        fs::write(left_path(dir), "this is {not json").unwrap();
    });
}

#[test]
fn an_empty_remembered_file_changes_nothing_and_is_replaced() {
    assert_ignored_and_replaced("empty", &|dir| {
        fs::write(left_path(dir), "").unwrap();
    });
}

#[test]
fn a_remembered_file_with_an_unknown_schema_changes_nothing_and_is_replaced() {
    assert_ignored_and_replaced("schema 2", &|dir| {
        let json = remembered_for(dir, 2, &head_id(dir));
        fs::write(left_path(dir), serde_json::to_vec(&json).unwrap()).unwrap();
    });
}

#[test]
fn a_remembered_file_naming_another_op_changes_nothing_and_is_replaced() {
    assert_ignored_and_replaced("foreign op", &|dir| {
        let json = remembered_for(dir, 1, "00000000000000000000000000000000");
        fs::write(left_path(dir), serde_json::to_vec(&json).unwrap()).unwrap();
    });
}

#[test]
fn a_remembered_file_of_the_wrong_shape_changes_nothing_and_is_replaced() {
    assert_ignored_and_replaced("paths is a list", &|dir| {
        let mut json = remembered_for(dir, 1, &head_id(dir));
        json["paths"] = Value::from(vec!["chap"]);
        fs::write(left_path(dir), serde_json::to_vec(&json).unwrap()).unwrap();
    });
}

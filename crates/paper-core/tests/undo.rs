//! `Project::{undo, redo, undo_to}`, cross-checked with the real `git` binary and compared
//! byte for byte on the working files.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::activity::OpKind;
use paper_core::error::{ErrorKind, PaperError};
use paper_core::point::Point;
use paper_core::profile::Profile;
use paper_core::project::*;
use paper_core::snapshot::{ChangeKind, SnapshotOptions};
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

/// Every file outside `.git`, as bytes, by relative path. Symlinks are not followed.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if name == ".git" {
                continue;
            }
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                walk(root, &path, out);
            } else if kind.is_file() {
                out.insert(name, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn text(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

fn tip(dir: &Path) -> String {
    git(dir, &["rev-parse", "refs/heads/main"])
        .trim()
        .to_owned()
}

fn status(dir: &Path) -> String {
    git(dir, &["status", "--porcelain"])
}

fn log(dir: &Path) -> String {
    git(dir, &["log", "--format=%s", "main"])
}

/// What every undo-family command must leave true (invariant I6) and finished (no marker).
fn assert_git_consistent(dir: &Path) {
    git(dir, &["fsck", "--strict", "--no-progress"]);
    assert_eq!(
        git(dir, &["symbolic-ref", "HEAD"]).trim(),
        "refs/heads/main"
    );
    assert!(
        !dir.join(".git/paper/checkout-pending").exists(),
        "the checkout marker is deleted when the checkout is done"
    );
}

/// Everything a refused or empty command must leave alone.
fn stored(dir: &Path) -> (String, String, String, String, BTreeMap<String, Vec<u8>>) {
    (
        // refs/paper/keep moves with every op by design (PV-022).
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"])
            .lines()
            .filter(|line| !line.starts_with("refs/paper/keep "))
            .map(|line| format!("{line}\n"))
            .collect(),
        git(dir, &["ls-files", "--stage"]),
        // Read the file: `symbolic-ref` fails on a detached HEAD.
        fs::read_to_string(dir.join(".git/HEAD")).unwrap(),
        fs::read_to_string(dir.join(".git/paper/activity/head")).unwrap(),
        files(dir),
    )
}

fn op_count(dir: &Path) -> usize {
    fs::read_dir(dir.join(".git/paper/activity/ops"))
        .unwrap()
        .count()
}

fn op_json(dir: &Path, id: &str) -> serde_json::Value {
    let bytes = fs::read(dir.join(format!(".git/paper/activity/ops/{id}.json"))).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn head_op(dir: &Path) -> serde_json::Value {
    let head = fs::read_to_string(dir.join(".git/paper/activity/head")).unwrap();
    op_json(dir, head.trim())
}

fn ids(list: &serde_json::Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
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

fn descriptions(project: &Project) -> Vec<String> {
    project
        .activity(100)
        .unwrap()
        .into_iter()
        .map(|e| e.description)
        .collect()
}

/// The activity entry whose description is exactly `description` (newest first).
fn entry(project: &Project, description: &str) -> ActivityEntry {
    project
        .activity(100)
        .unwrap()
        .into_iter()
        .find(|e| e.description == description)
        .unwrap_or_else(|| panic!("no entry '{description}' in {:?}", descriptions(project)))
}

// ---- undo and redo of a save ----

#[test]
fn undo_after_a_save_moves_the_line_back_and_keeps_the_edits_as_the_draft() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    let start = tip(dir.path());
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    let saved_files = files(dir.path());
    assert_ne!(tip(dir.path()), start);

    let undone = undo(&mut project);

    assert_eq!(undone.entry.description, "save \"one\"");
    assert!(undone.changed.is_empty(), "{:?}", undone.changed);
    assert!(undone.skipped.is_empty());
    assert_eq!(tip(dir.path()), start);
    assert_eq!(log(dir.path()), "Start project\n");
    assert_eq!(files(dir.path()), saved_files, "files are byte-identical");
    assert_eq!(
        status(dir.path()),
        " M a.tex\n",
        "the saved edits are the draft again"
    );
    assert_git_consistent(dir.path());
}

#[test]
fn a_second_undo_takes_the_edits_out_of_the_files_and_redo_twice_brings_everything_back() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    let start_files = files(dir.path());
    let start = tip(dir.path());
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    let saved_files = files(dir.path());
    let saved_tip = tip(dir.path());

    undo(&mut project);
    let second = undo(&mut project);

    assert_eq!(second.entry.description, "edited a.tex");
    assert_eq!(files(dir.path()), start_files);
    assert_eq!(tip(dir.path()), start);
    assert_eq!(status(dir.path()), "");
    assert_eq!(second.changed.len(), 1);
    assert_eq!(second.changed[0].path, "a.tex");
    assert_eq!(second.changed[0].kind, ChangeKind::Modified);
    assert_git_consistent(dir.path());

    let first_redo = redo(&mut project);
    assert_eq!(first_redo.entry.description, "edited a.tex");
    assert_eq!(files(dir.path()), saved_files);
    assert_eq!(tip(dir.path()), start);
    assert_eq!(status(dir.path()), " M a.tex\n");
    assert_git_consistent(dir.path());

    let second_redo = redo(&mut project);
    assert_eq!(second_redo.entry.description, "save \"one\"");
    assert_eq!(files(dir.path()), saved_files);
    assert_eq!(tip(dir.path()), saved_tip);
    assert_eq!(log(dir.path()), "one\nStart project\n");
    assert_eq!(status(dir.path()), "");
    assert_git_consistent(dir.path());
}

#[test]
fn undo_with_unsaved_edits_records_them_first_then_takes_them_back() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    let start_files = files(dir.path());
    write(dir.path(), "a.tex", "unsaved\n");
    let edited_files = files(dir.path());

    let undone = undo(&mut project);

    assert_eq!(undone.entry.description, "edited a.tex");
    assert_eq!(files(dir.path()), start_files, "the edit leaves the files");
    assert_eq!(status(dir.path()), "");
    let kept = descriptions(&project);
    assert_eq!(kept[0], "undo: edited a.tex");
    assert_eq!(
        kept[1], "edited a.tex",
        "the edits are an entry of their own"
    );
    assert_git_consistent(dir.path());

    redo(&mut project);
    assert_eq!(files(dir.path()), edited_files, "the edit can be redone");
    assert_eq!(status(dir.path()), " M a.tex\n");
    assert_git_consistent(dir.path());
}

#[test]
fn undo_and_redo_add_and_remove_files_and_the_folders_they_leave_empty() {
    let (dir, mut project) = project_with(&[("b.tex", "b\n")]);
    let start_files = files(dir.path());
    fs::remove_file(dir.path().join("b.tex")).unwrap();
    write(dir.path(), "sec/new.tex", "new\n");
    save_all(&mut project, "swap");
    let saved_files = files(dir.path());

    undo(&mut project);
    let second = undo(&mut project);

    assert_eq!(files(dir.path()), start_files);
    assert!(
        !dir.path().join("sec").exists(),
        "an emptied folder goes away"
    );
    let kinds: Vec<(&str, ChangeKind)> = second
        .changed
        .iter()
        .map(|c| (c.path.as_str(), c.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("b.tex", ChangeKind::Added),
            ("sec/new.tex", ChangeKind::Deleted)
        ]
    );
    assert_git_consistent(dir.path());

    redo(&mut project);
    redo(&mut project);
    assert_eq!(files(dir.path()), saved_files);
    assert_eq!(status(dir.path()), "");
    assert_git_consistent(dir.path());
}

#[test]
fn a_new_command_after_an_undo_clears_the_redo_stack() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    undo(&mut project);
    write(dir.path(), "a.tex", "v2\n");
    save_all(&mut project, "two");
    let before = stored(dir.path());

    assert_eq!(project.redo().unwrap(), UndoOutcome::NothingToRedo);

    assert_eq!(stored(dir.path()), before);
    assert_eq!(head_op(dir.path())["redo"], serde_json::json!([]));
    assert_eq!(text(dir.path(), "a.tex"), "v2\n");
}

#[test]
fn new_edits_after_an_undo_make_redo_have_nothing_and_keep_the_edits() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    undo(&mut project);
    write(dir.path(), "a.tex", "v2\n");

    // The edits are recorded as a normal entry, which clears the redo stack.
    assert_eq!(project.redo().unwrap(), UndoOutcome::NothingToRedo);

    assert_eq!(text(dir.path(), "a.tex"), "v2\n");
    assert_eq!(descriptions(&project)[0], "edited a.tex");
    assert_git_consistent(dir.path());
}

#[test]
fn undo_at_the_first_entry_and_redo_with_an_empty_stack_record_nothing() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    let before = stored(dir.path());
    let ops = op_count(dir.path());

    assert_eq!(project.undo().unwrap(), UndoOutcome::NothingToUndo);
    assert_eq!(project.redo().unwrap(), UndoOutcome::NothingToRedo);

    assert_eq!(stored(dir.path()), before);
    assert_eq!(op_count(dir.path()), ops);
    assert_git_consistent(dir.path());
}

#[test]
fn undoing_all_the_way_back_ends_in_nothing_to_undo_and_redoing_all_the_way_in_nothing_to_redo() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    let latest = stored(dir.path());

    let mut undos = 0;
    while let UndoOutcome::Undone(_) = project.undo().unwrap() {
        undos += 1;
        assert!(undos < 10, "undo never ends");
    }
    assert_eq!(undos, 2, "the edits entry and the save");
    let mut redos = 0;
    while let UndoOutcome::Undone(_) = project.redo().unwrap() {
        redos += 1;
        assert!(redos < 10, "redo never ends");
    }

    assert_eq!(redos, 2);
    // Refs, index, HEAD and files are back; only the log head moved on.
    let (refs, index, head_ref, _, bytes) = stored(dir.path());
    assert_eq!(
        (refs, index, head_ref, bytes),
        (latest.0, latest.1, latest.2, latest.4)
    );
}

// ---- the op bookkeeping ----

#[test]
fn undo_and_redo_record_ops_with_the_position_and_redo_stack_of_the_algorithm() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    let entries = project.activity(10).unwrap();
    let (saved, edited, init) = (
        entries[0].id.to_string(),
        entries[1].id.to_string(),
        entries[2].id.to_string(),
    );

    undo(&mut project);
    let first = head_op(dir.path());
    assert_eq!(first["kind"], "undo");
    assert_eq!(first["parent"], saved.as_str());
    assert_eq!(first["position"], edited.as_str());
    assert_eq!(ids(&first["redo"]), [saved.as_str()]);
    assert_eq!(first["view"], op_json(dir.path(), &edited)["view"]);

    undo(&mut project);
    let second = head_op(dir.path());
    assert_eq!(second["position"], init.as_str());
    assert_eq!(ids(&second["redo"]), [edited.clone(), saved.clone()]);
    assert_eq!(second["view"], op_json(dir.path(), &init)["view"]);

    redo(&mut project);
    let third = head_op(dir.path());
    assert_eq!(third["kind"], "redo");
    assert_eq!(third["position"], edited.as_str());
    assert_eq!(ids(&third["redo"]), [saved.as_str()]);
    assert_eq!(third["view"], op_json(dir.path(), &edited)["view"]);
}

#[test]
fn activity_lists_undo_redo_and_go_back_entries_in_researcher_words() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    undo(&mut project);
    redo(&mut project);
    let target = entry(&project, "edited a.tex");
    project.undo_to(&target.short_id).unwrap();

    let entries = project.activity(10).unwrap();

    assert!(
        entries[0]
            .description
            .starts_with(&format!("go back to {}", target.short_id))
            && entries[0].description.ends_with(": edited a.tex"),
        "{:?}",
        entries[0]
    );
    assert_eq!(entries[0].kind, OpKind::Undo);
    assert_eq!(entries[1].description, "redo: save \"one\"");
    assert_eq!(entries[1].kind, OpKind::Redo);
    assert_eq!(entries[2].description, "undo: save \"one\"");
    assert_eq!(entries[2].kind, OpKind::Undo);
}

// ---- undo --to ----

fn three_saves() -> (tempfile::TempDir, Project) {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    for (content, message) in [("v1\n", "one"), ("v2\n", "two"), ("v3\n", "three")] {
        write(dir.path(), "a.tex", content);
        save_all(&mut project, message);
    }
    (dir, project)
}

#[test]
fn undo_to_an_entry_on_the_chain_restores_its_files_and_refs() {
    let (dir, mut project) = three_saves();
    let target = entry(&project, "save \"one\"");
    let log_at_one = "one\nStart project\n";

    let undone = undone(project.undo_to(&target.short_id).unwrap());

    assert_eq!(undone.entry.id, target.id);
    assert_eq!(text(dir.path(), "a.tex"), "v1\n");
    assert_eq!(log(dir.path()), log_at_one);
    assert_eq!(status(dir.path()), "");
    let head = head_op(dir.path());
    assert_eq!(head["kind"], "undo");
    assert_eq!(head["position"], target.id.as_str());
    assert_eq!(head["redo"], serde_json::json!([]));
    assert!(
        head["description"]
            .as_str()
            .unwrap()
            .starts_with(&format!("go back to {}", target.short_id)),
        "{head}"
    );
    assert!(
        head["description"]
            .as_str()
            .unwrap()
            .ends_with(": save \"one\"")
    );
    assert_git_consistent(dir.path());
    assert_eq!(project.redo().unwrap(), UndoOutcome::NothingToRedo);
}

#[test]
fn undo_to_an_entry_that_exists_only_on_a_redo_stack_goes_forward_to_it() {
    let (dir, mut project) = three_saves();
    let three = entry(&project, "save \"three\"");
    // Undoing the save of "three" and its edits leaves the position at "two".
    undo(&mut project);
    undo(&mut project);
    assert_eq!(text(dir.path(), "a.tex"), "v2\n");
    assert_eq!(log(dir.path()), "two\none\nStart project\n");

    // "three" was undone: it is on the redo stack and not at the position.
    undone(project.undo_to(&three.short_id).unwrap());

    assert_eq!(text(dir.path(), "a.tex"), "v3\n");
    assert_eq!(log(dir.path()), "three\ntwo\none\nStart project\n");
    assert_eq!(status(dir.path()), "");
    assert_eq!(head_op(dir.path())["redo"], serde_json::json!([]));
    assert_git_consistent(dir.path());
    assert_eq!(project.redo().unwrap(), UndoOutcome::NothingToRedo);
}

#[test]
fn undo_to_takes_the_full_id_too() {
    let (dir, mut project) = three_saves();
    let target = entry(&project, "save \"two\"");

    project.undo_to(target.id.as_str()).unwrap();

    assert_eq!(text(dir.path(), "a.tex"), "v2\n");
}

#[test]
fn undo_to_with_unsaved_edits_records_them_first_so_they_stay_reachable() {
    let (dir, mut project) = three_saves();
    let first = entry(&project, "save \"one\"");
    write(dir.path(), "a.tex", "unsaved\n");

    project.undo_to(&first.short_id).unwrap();

    assert_eq!(text(dir.path(), "a.tex"), "v1\n");
    let edits = entry(&project, "edited a.tex");
    project.undo_to(&edits.short_id).unwrap();
    assert_eq!(text(dir.path(), "a.tex"), "unsaved\n");
    assert_git_consistent(dir.path());
}

/// Writes a copy of the head op under another id, so two op files share a prefix.
fn craft_op(dir: &Path, id: &str, description: &str) {
    let mut op = head_op(dir);
    op["id"] = id.into();
    op["position"] = id.into();
    op["description"] = description.into();
    let path = dir.join(format!(".git/paper/activity/ops/{id}.json"));
    fs::write(path, serde_json::to_vec_pretty(&op).unwrap()).unwrap();
}

/// A 4-character prefix no real op id starts with.
fn unused_prefix(dir: &Path) -> String {
    let names: Vec<String> = fs::read_dir(dir.join(".git/paper/activity/ops"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    (0..16u32)
        .map(|n| format!("{n:x}fff"))
        .find(|prefix| !names.iter().any(|name| name.starts_with(prefix.as_str())))
        .unwrap()
}

#[test]
fn an_ambiguous_prefix_lists_the_candidates_and_changes_nothing() {
    let (dir, mut project) = three_saves();
    let prefix = unused_prefix(dir.path());
    craft_op(
        dir.path(),
        &format!("{prefix}1{}", "0".repeat(27)),
        "crafted one",
    );
    craft_op(
        dir.path(),
        &format!("{prefix}2{}", "0".repeat(27)),
        "crafted two",
    );
    let before = stored(dir.path());
    let ops = op_count(dir.path());

    let err = project.undo_to(&prefix).unwrap_err();

    match &err {
        UndoError::AmbiguousEntry {
            prefix: p,
            candidates,
        } => {
            assert_eq!(*p, prefix);
            let mut found: Vec<&str> = candidates.iter().map(|c| c.description.as_str()).collect();
            found.sort_unstable();
            assert_eq!(found, ["crafted one", "crafted two"]);
            for candidate in candidates {
                assert!(candidate.short_id.len() >= 5, "{candidate:?}");
                assert!(candidate.id.as_str().starts_with(&prefix));
            }
        }
        other => panic!("expected AmbiguousEntry, got {other:?}"),
    }
    let taught = PaperError::from(err);
    assert_eq!(taught.kind, ErrorKind::User);
    let everything = format!("{} {} {:?}", taught.message, taught.why, taught.next);
    assert!(
        everything.contains("crafted one") && everything.contains("crafted two"),
        "{everything}"
    );
    assert!(!taught.next.is_empty());
    assert_eq!(stored(dir.path()), before);
    assert_eq!(op_count(dir.path()), ops);
}

#[test]
fn an_unknown_prefix_is_a_teaching_error_and_changes_nothing() {
    let (dir, mut project) = three_saves();
    let prefix = unused_prefix(dir.path());
    let before = stored(dir.path());
    let ops = op_count(dir.path());

    for text in [prefix.as_str(), "zzzz"] {
        let err = project.undo_to(text).unwrap_err();
        assert!(
            matches!(&err, UndoError::UnknownEntry { prefix: p } if p == text),
            "{err:?}"
        );
        let taught = PaperError::from(err);
        assert_eq!(taught.kind, ErrorKind::User);
        assert!(taught.message.contains(text), "{}", taught.message);
        assert!(!taught.why.is_empty() && !taught.next.is_empty());
    }

    assert_eq!(stored(dir.path()), before);
    assert_eq!(op_count(dir.path()), ops);
}

// ---- restore ----

#[test]
fn undo_of_a_restore_puts_the_files_back_as_they_were_before_it() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n"), ("b.tex", "b0\n")]);
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "v2 unsaved\n");
    write(dir.path(), "b.tex", "b1 unsaved\n");
    let before_restore = files(dir.path());
    project
        .restore(&RestoreRequest {
            paths: vec!["a.tex".to_owned()],
            from: Some("last".parse::<Point>().unwrap()),
        })
        .unwrap();
    assert_eq!(text(dir.path(), "a.tex"), "v1\n");

    let undone = undo(&mut project);

    assert!(
        undone.entry.description.starts_with("restore a.tex"),
        "{:?}",
        undone.entry
    );
    assert_eq!(files(dir.path()), before_restore);
    assert_git_consistent(dir.path());

    redo(&mut project);
    assert_eq!(text(dir.path(), "a.tex"), "v1\n");
    assert_eq!(text(dir.path(), "b.tex"), "b1 unsaved\n");
}

// ---- plain git moved ----

fn refusal_setup() -> (tempfile::TempDir, Project) {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n")]);
    write(dir.path(), "a.tex", "v1\n");
    save_all(&mut project, "one");
    undo(&mut project);
    write(dir.path(), "a.tex", "edit made after the undo\n");
    (dir, project)
}

fn assert_refused_untouched(
    dir: &Path,
    project: &mut Project,
    expected: impl Fn(&UndoError) -> bool,
) {
    let before = stored(dir);
    let ops = op_count(dir);
    let results = [
        project.undo().unwrap_err(),
        project.redo().unwrap_err(),
        project.undo_to("0000").unwrap_err(),
    ];
    for err in results {
        assert!(expected(&err), "{err:?}");
    }
    assert_eq!(stored(dir), before, "nothing was written");
    assert_eq!(op_count(dir), ops);
}

#[test]
fn undo_redo_and_undo_to_refuse_after_a_plain_git_commit() {
    let (dir, mut project) = refusal_setup();
    git(
        dir.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "Plain git"],
    );

    assert_refused_untouched(
        dir.path(),
        &mut project,
        |err| matches!(err, UndoError::Save(SaveError::OutOfSyncWithGit { line }) if line.as_str() == "main"),
    );
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn undo_redo_and_undo_to_refuse_when_git_is_on_another_branch() {
    let (dir, mut project) = refusal_setup();
    git(dir.path(), &["checkout", "--quiet", "-b", "side"]);

    assert_refused_untouched(
        dir.path(),
        &mut project,
        |err| matches!(err, UndoError::Save(SaveError::HeadOnAnotherBranch { found: Some(b), .. }) if b == "side"),
    );
}

#[test]
fn undo_refuses_when_git_detached_head() {
    let (dir, mut project) = refusal_setup();
    git(dir.path(), &["checkout", "--quiet", "--detach"]);

    assert_refused_untouched(dir.path(), &mut project, |err| {
        matches!(
            err,
            UndoError::Save(SaveError::HeadOnAnotherBranch { found: None, .. })
        )
    });
}

// ---- files paper does not record ----

#[test]
fn the_checkout_never_overwrites_an_ignored_file_and_reports_it_as_skipped() {
    let (dir, mut project) = project_with(&[
        (".gitignore", "*.log\n"),
        ("chap", "a file\n"),
        ("keep.tex", "keep\n"),
    ]);
    fs::remove_file(dir.path().join("chap")).unwrap();
    write(dir.path(), "chap/one.tex", "1\n");
    save_all(&mut project, "folder");
    write(dir.path(), "chap/cache.log", "my ignored log\n");

    undo(&mut project);
    let second = undo(&mut project);

    assert_eq!(text(dir.path(), "chap/cache.log"), "my ignored log\n");
    assert_eq!(text(dir.path(), "chap/one.tex"), "1\n");
    let reported: Vec<(&str, &RestoreSkip)> = second
        .skipped
        .iter()
        .map(|s| (s.path.as_str(), &s.reason))
        .collect();
    assert_eq!(
        reported,
        vec![
            ("chap", &RestoreSkip::Blocked),
            (
                "chap/one.tex",
                &RestoreSkip::KeptForSwap {
                    target: "chap".to_owned()
                }
            ),
        ]
    );
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
    assert!(!dir.path().join(".git/paper/checkout-pending").exists());
}

#[cfg(unix)]
#[test]
fn the_checkout_never_writes_through_or_over_a_symlink() {
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "secret.txt", "outside\n");
    let (dir, mut project) = project_with(&[("a.tex", "a\n"), ("b.tex", "b\n")]);
    write(dir.path(), "b.tex", "b2\n");
    save_all(&mut project, "two");
    fs::remove_file(dir.path().join("a.tex")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), dir.path().join("a.tex"))
        .unwrap();

    undo(&mut project);
    undo(&mut project);

    let meta = fs::symlink_metadata(dir.path().join("a.tex")).unwrap();
    assert!(meta.file_type().is_symlink(), "the symlink stays");
    assert_eq!(text(outside.path(), "secret.txt"), "outside\n");
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

// ---- the scripted session ----

struct Expect {
    a: &'static str,
    b: &'static str,
    /// Whether the line is at the save "one" (else at the starting save).
    saved: bool,
    status: &'static str,
}

fn assert_state(dir: &Path, step: &str, expect: &Expect, start_tip: &str, saved_tip: &str) {
    assert_eq!(text(dir, "a.tex"), expect.a, "{step}: a.tex");
    assert_eq!(text(dir, "b.tex"), expect.b, "{step}: b.tex");
    let want_tip = if expect.saved { saved_tip } else { start_tip };
    assert_eq!(tip(dir), want_tip, "{step}: line tip");
    assert_eq!(status(dir), expect.status, "{step}: status");
    assert_eq!(
        log(dir),
        if expect.saved {
            "one\nStart project\n"
        } else {
            "Start project\n"
        },
        "{step}: log"
    );
    assert_git_consistent(dir);
}

#[test]
fn a_scripted_session_of_edits_saves_restore_undos_and_redos_has_the_exact_state_at_every_step() {
    let (dir, mut project) = project_with(&[("a.tex", "v0\n"), ("b.tex", "b0\n")]);
    let d = dir.path();
    let start_tip = tip(d);

    write(d, "a.tex", "v1\n");
    save_all(&mut project, "one");
    let saved_tip = tip(d);
    project
        .restore(&RestoreRequest {
            paths: vec!["a.tex".to_owned()],
            from: Some("last~1".parse::<Point>().unwrap()),
        })
        .unwrap();
    write(d, "b.tex", "b1\n");

    // The state before the first undo, once the pending edit is recorded: a v0, b b1.
    let st0 = Expect {
        a: "v0\n",
        b: "b1\n",
        saved: true,
        status: " M a.tex\n M b.tex\n",
    };
    let st1 = Expect {
        a: "v0\n",
        b: "b0\n",
        saved: true,
        status: " M a.tex\n",
    };
    let st2 = Expect {
        a: "v1\n",
        b: "b0\n",
        saved: true,
        status: "",
    };
    let st3 = Expect {
        a: "v1\n",
        b: "b0\n",
        saved: false,
        status: " M a.tex\n",
    };
    let st4 = Expect {
        a: "v0\n",
        b: "b0\n",
        saved: false,
        status: "",
    };

    let u1 = undo(&mut project);
    assert_eq!(u1.entry.description, "edited b.tex");
    assert_state(d, "undo 1", &st1, &start_tip, &saved_tip);
    let u2 = undo(&mut project);
    assert!(
        u2.entry.description.starts_with("restore a.tex"),
        "{:?}",
        u2.entry
    );
    assert_state(d, "undo 2", &st2, &start_tip, &saved_tip);
    let u3 = undo(&mut project);
    assert_eq!(u3.entry.description, "save \"one\"");
    assert_state(d, "undo 3", &st3, &start_tip, &saved_tip);
    let u4 = undo(&mut project);
    assert_eq!(u4.entry.description, "edited a.tex");
    assert_state(d, "undo 4", &st4, &start_tip, &saved_tip);
    assert_eq!(project.undo().unwrap(), UndoOutcome::NothingToUndo);
    assert_state(d, "undo 5 (nothing)", &st4, &start_tip, &saved_tip);

    assert_eq!(redo(&mut project).entry.description, "edited a.tex");
    assert_state(d, "redo 1", &st3, &start_tip, &saved_tip);
    assert_eq!(redo(&mut project).entry.description, "save \"one\"");
    assert_state(d, "redo 2", &st2, &start_tip, &saved_tip);
    assert!(
        redo(&mut project)
            .entry
            .description
            .starts_with("restore a.tex")
    );
    assert_state(d, "redo 3", &st1, &start_tip, &saved_tip);
    assert_eq!(redo(&mut project).entry.description, "edited b.tex");
    assert_state(d, "redo 4", &st0, &start_tip, &saved_tip);
    assert_eq!(project.redo().unwrap(), UndoOutcome::NothingToRedo);
    assert_state(d, "redo 5 (nothing)", &st0, &start_tip, &saved_tip);

    drop(project);
    let reopened = Project::open(d, identity()).unwrap();
    assert_eq!(reopened.view().current().tip.to_string(), saved_tip);
}

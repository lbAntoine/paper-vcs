//! A name that the filesystem folds to another (Unicode normalisation on APFS, case on
//! case-insensitive volumes) is the same file: `undo` and `restore` must never delete the
//! only copy of it (PV-035, issue #67).
//!
//! The planner (`restore_plan`) is private to `paper-core`, so the invariant "no planned
//! deletion removes a file that a blocked write refers to" is covered end to end here, by
//! asserting that a file always survives. Every test skips unless the filesystem folds the
//! pair of spellings it uses.

#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::profile::Profile;
use paper_core::project::*;
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;

/// "été.tex" with precomposed accents.
const NFC_FILE: &str = "\u{e9}t\u{e9}.tex";
/// The same name with combining accents: different bytes, same name to APFS.
const NFD_FILE: &str = "e\u{301}te\u{301}.tex";
const NFC_DIR: &str = "R\u{e9}sum\u{e9}";
const NFD_DIR: &str = "Re\u{301}sume\u{301}";

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

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
    project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
}

/// Whether the filesystem treats `a` and `b` as the same name: write `a`, look up `b`.
fn folds(a: &str, b: &str) -> bool {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), a, "probe");
    let folded = fs::symlink_metadata(dir.path().join(b)).is_ok();
    if !folded {
        eprintln!("skipped: this filesystem does not fold {a:?} and {b:?}");
    }
    folded
}

/// A project whose first save holds `old_file` and whose second save holds the same bytes
/// under `new_file`, in the working folder as well.
fn renamed_project(old_file: &str, new_file: &str, old_top: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), old_file, "the content\n");
    let (mut project, _) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    // A separate delete and create, so the new spelling is the one kept on disk.
    fs::remove_file(dir.path().join(old_file)).unwrap();
    if old_top != old_file {
        // Remove the now-empty folder so the new spelling of the folder is created.
        fs::remove_dir(dir.path().join(old_top)).unwrap();
    }
    write(dir.path(), new_file, "the content\n");
    save_all(&mut project, "renamed");
    (dir, project)
}

/// How many entries of `parent` are spelled `a` or `b` (exact bytes).
fn spellings_present(parent: &Path, a: &str, b: &str) -> usize {
    fs::read_dir(parent)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            name == a || name == b
        })
        .count()
}

/// The file survives with its content, under exactly one of its two spellings.
fn assert_file_survives(dir: &Path, file: &str, other: &str, content: &str) {
    assert_eq!(
        spellings_present(dir, file, other),
        1,
        "the file is on disk under exactly one spelling"
    );
    assert_eq!(fs::read_to_string(dir.join(file)).unwrap(), content);
    assert_eq!(fs::read_to_string(dir.join(other)).unwrap(), content);
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

fn assert_nested_survives(dir: &Path, top: &str, other_top: &str, name: &str) {
    assert_eq!(
        spellings_present(dir, top, other_top),
        1,
        "the folder is on disk under exactly one spelling"
    );
    assert_eq!(
        fs::read_to_string(dir.join(top).join(name)).unwrap(),
        "the content\n"
    );
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

/// `restore --from last~1` naming both spellings, as a researcher would.
fn restore_previous(project: &mut Project, spellings: &[&str]) {
    project
        .restore(&RestoreRequest {
            paths: spellings.iter().map(|p| (*p).to_owned()).collect(),
            from: Some("last~1".parse().unwrap()),
        })
        .unwrap();
}

#[test]
fn undoing_a_unicode_form_rename_twice_keeps_the_file() {
    if !folds(NFC_FILE, NFD_FILE) {
        return;
    }
    let (dir, mut project) = renamed_project(NFC_FILE, NFD_FILE, NFC_FILE);
    project.undo().unwrap();
    assert_file_survives(dir.path(), NFC_FILE, NFD_FILE, "the content\n");
    project.undo().unwrap();
    assert_file_survives(dir.path(), NFC_FILE, NFD_FILE, "the content\n");
}

#[test]
fn restoring_the_previous_save_after_a_unicode_form_rename_keeps_the_file() {
    if !folds(NFC_FILE, NFD_FILE) {
        return;
    }
    let (dir, mut project) = renamed_project(NFC_FILE, NFD_FILE, NFC_FILE);
    restore_previous(&mut project, &[NFC_FILE, NFD_FILE]);
    assert_file_survives(dir.path(), NFC_FILE, NFD_FILE, "the content\n");
}

#[test]
fn undoing_a_unicode_form_rename_of_a_folder_twice_keeps_the_file_inside() {
    if !folds(NFC_DIR, NFD_DIR) {
        return;
    }
    let old = format!("{NFC_DIR}/cv.tex");
    let new = format!("{NFD_DIR}/cv.tex");
    let (dir, mut project) = renamed_project(&old, &new, NFC_DIR);
    project.undo().unwrap();
    assert_nested_survives(dir.path(), NFC_DIR, NFD_DIR, "cv.tex");
    project.undo().unwrap();
    assert_nested_survives(dir.path(), NFC_DIR, NFD_DIR, "cv.tex");
}

#[test]
fn restoring_the_previous_save_after_a_unicode_form_folder_rename_keeps_the_file_inside() {
    if !folds(NFC_DIR, NFD_DIR) {
        return;
    }
    let old = format!("{NFC_DIR}/cv.tex");
    let new = format!("{NFD_DIR}/cv.tex");
    let (dir, mut project) = renamed_project(&old, &new, NFC_DIR);
    restore_previous(&mut project, &[&old, &new]);
    assert_nested_survives(dir.path(), NFC_DIR, NFD_DIR, "cv.tex");
}

#[test]
fn undoing_a_case_only_rename_twice_keeps_the_file() {
    if !folds("Notes.tex", "notes.tex") {
        return;
    }
    let (dir, mut project) = renamed_project("Notes.tex", "notes.tex", "Notes.tex");
    project.undo().unwrap();
    assert_file_survives(dir.path(), "Notes.tex", "notes.tex", "the content\n");
    project.undo().unwrap();
    assert_file_survives(dir.path(), "Notes.tex", "notes.tex", "the content\n");
}

#[test]
fn restoring_the_previous_save_after_a_case_only_rename_keeps_the_file() {
    if !folds("Notes.tex", "notes.tex") {
        return;
    }
    let (dir, mut project) = renamed_project("Notes.tex", "notes.tex", "Notes.tex");
    restore_previous(&mut project, &["Notes.tex", "notes.tex"]);
    assert_file_survives(dir.path(), "Notes.tex", "notes.tex", "the content\n");
}

/// Whatever is at `top` after the round trip: the folder's `cv.tex` or the file itself.
fn assert_folder_or_file_survives(dir: &Path, top: &str) {
    let at = dir.join(top);
    let content = if at.is_dir() {
        fs::read_to_string(at.join("cv.tex")).unwrap()
    } else {
        fs::read_to_string(&at).unwrap()
    };
    assert_eq!(content, "the content\n");
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn undoing_a_folder_turned_into_a_unicode_form_file_keeps_the_content() {
    if !folds(NFC_DIR, NFD_DIR) {
        return;
    }
    let old = format!("{NFC_DIR}/cv.tex");
    let (dir, mut project) = renamed_project(&old, NFD_DIR, NFC_DIR);
    project.undo().unwrap();
    assert_folder_or_file_survives(dir.path(), NFC_DIR);
    project.undo().unwrap();
    assert_folder_or_file_survives(dir.path(), NFC_DIR);
}

#[test]
fn restoring_a_folder_turned_into_a_unicode_form_file_keeps_the_content() {
    if !folds(NFC_DIR, NFD_DIR) {
        return;
    }
    let old = format!("{NFC_DIR}/cv.tex");
    let (dir, mut project) = renamed_project(&old, NFD_DIR, NFC_DIR);
    restore_previous(&mut project, &[&old, NFD_DIR]);
    assert_folder_or_file_survives(dir.path(), NFC_DIR);
}

#[test]
fn undoing_a_file_turned_into_a_unicode_form_folder_keeps_the_content() {
    if !folds(NFC_DIR, NFD_DIR) {
        return;
    }
    let new = format!("{NFD_DIR}/cv.tex");
    let (dir, mut project) = renamed_project(NFC_DIR, &new, NFC_DIR);
    project.undo().unwrap();
    assert_folder_or_file_survives(dir.path(), NFC_DIR);
    project.undo().unwrap();
    assert_folder_or_file_survives(dir.path(), NFC_DIR);
}

#[test]
fn restoring_one_spelling_names_the_tracked_other_spelling_in_the_skip() {
    if !folds(NFC_FILE, NFD_FILE) {
        return;
    }
    let (dir, mut project) = renamed_project(NFC_FILE, NFD_FILE, NFC_FILE);
    let restored = project
        .restore(&RestoreRequest {
            paths: vec![NFC_FILE.to_owned()],
            from: Some("last~1".parse().unwrap()),
        })
        .unwrap();
    assert_eq!(
        restored.skipped,
        vec![SkippedPath {
            path: NFC_FILE.to_owned(),
            reason: RestoreSkip::CaseAlias {
                other: NFD_FILE.to_owned()
            },
        }]
    );
    assert_file_survives(dir.path(), NFC_FILE, NFD_FILE, "the content\n");
}

#[test]
fn a_hard_link_under_a_folded_name_is_not_overwritten_by_undo() {
    if !folds(NFC_FILE, NFD_FILE) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), NFC_FILE, "the content\n");
    let (mut project, _) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    write(dir.path(), "b.tex", "precious\n");
    save_all(&mut project, "add b");
    let ignore = |project: &mut Project, arg: &str| {
        project
            .ignore(&IgnoreRequest {
                args: vec![arg.to_owned()],
                from: String::new(),
                yes: false,
                local: false,
            })
            .unwrap();
    };
    ignore(&mut project, NFC_FILE);
    // The ignored file is replaced by a hard link to b.tex under the other spelling.
    fs::remove_file(dir.path().join(NFC_FILE)).unwrap();
    fs::hard_link(dir.path().join("b.tex"), dir.path().join(NFD_FILE)).unwrap();
    ignore(&mut project, "e*.tex");
    let start = project.activity(1000).unwrap().pop().unwrap().short_id;

    project.undo_to(&start).unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join(NFC_FILE)).unwrap(),
        "precious\n",
        "the file that is not tracked is left alone"
    );
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

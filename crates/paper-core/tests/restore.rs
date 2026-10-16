//! `Project::restore`, cross-checked with the real `git` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::point::Point;
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

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

fn project_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Project) {
    project_with_options(files, &SnapshotOptions::default())
}

fn project_with_options(
    files: &[(&str, &str)],
    options: &SnapshotOptions,
) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        write(dir.path(), path, content);
    }
    let (project, _) = Project::init(dir.path(), identity(), options).unwrap();
    (dir, project)
}

/// Saves the whole draft. `Project::init` already saved the starting files, so a draft
/// without edits is a no-op here.
fn save_all(project: &mut Project, message: &str) {
    project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
}

fn point(text: &str) -> Point {
    text.parse().unwrap()
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

use std::os::unix::fs::PermissionsExt;

fn restore(
    project: &mut Project,
    only: &[&str],
    from: Option<&str>,
) -> Result<Restored, RestoreError> {
    project.restore(&RestoreRequest {
        paths: paths(only),
        from: from.map(point),
    })
}

fn fsck(dir: &Path) {
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

fn exists(dir: &Path, path: &str) -> bool {
    dir.join(path).exists()
}

/// Everything a refused restore must leave alone.
fn stored(dir: &Path) -> (String, String, String) {
    (
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        git(dir, &["ls-files", "--stage"]),
        fs::read_to_string(dir.join(".git/paper/view.json")).unwrap(),
    )
}

#[test]
fn restore_puts_back_the_saved_content_and_keeps_the_edit_in_the_old_draft() {
    let (dir, mut project) = project_with(&[("a.tex", "saved\n"), ("b.tex", "b\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "my precious edit\n");
    write(dir.path(), "b.tex", "b2\n");
    let before = project.snapshot().unwrap();
    assert_eq!(before.changes.len(), 2);
    let old_draft = project.view().current().draft.clone();
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let index = git(dir.path(), &["ls-files", "--stage"]);

    let restored = restore(&mut project, &["a.tex"], None).unwrap();

    assert_eq!(read(dir.path(), "a.tex"), "saved\n");
    assert_eq!(read(dir.path(), "b.tex"), "b2\n");
    assert_eq!(restored.from, project.view().current().tip);
    assert_eq!(restored.changes.len(), 1);
    assert_eq!(restored.changes[0].path, "a.tex");
    assert_eq!(restored.changes[0].kind, ChangeKind::Modified);
    // The edit is still reachable from the commit that was the draft before the restore.
    let shown = git(dir.path(), &["show", &format!("{old_draft}:a.tex")]);
    assert_eq!(shown, "my precious edit\n");
    assert_ne!(project.view().current().draft, old_draft);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir.path(), &["ls-files", "--stage"]), index);
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), " M b.tex\n");
    fsck(dir.path());
}

#[test]
fn restore_from_an_older_save_rewrites_a_directory() {
    let (dir, mut project) = project_with(&[
        ("sec/keep.tex", "k1\n"),
        ("sec/recreate.tex", "r\n"),
        ("sec/deep/x.tex", "x\n"),
        ("other.tex", "o\n"),
    ]);
    save_all(&mut project, "one");
    write(dir.path(), "sec/keep.tex", "k2\n");
    fs::remove_file(dir.path().join("sec/recreate.tex")).unwrap();
    write(dir.path(), "sec/added/new.tex", "n\n");
    save_all(&mut project, "two");
    write(dir.path(), "sec/keep.tex", "k3\n");
    write(dir.path(), "other.tex", "o2\n");
    fs::remove_file(dir.path().join("sec/deep/x.tex")).unwrap();
    save_all(&mut project, "three");

    restore(&mut project, &["sec"], Some("last~2")).unwrap();

    assert_eq!(read(dir.path(), "sec/keep.tex"), "k1\n");
    assert_eq!(read(dir.path(), "sec/recreate.tex"), "r\n");
    assert_eq!(read(dir.path(), "sec/deep/x.tex"), "x\n");
    assert!(!exists(dir.path(), "sec/added/new.tex"));
    assert!(!exists(dir.path(), "sec/added"), "empty directory removed");
    assert_eq!(read(dir.path(), "other.tex"), "o2\n");
    fsck(dir.path());
}

#[test]
fn restore_deletes_a_file_that_did_not_exist_at_the_point() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "new/draft.tex", "d\n");

    let restored = restore(&mut project, &["new/draft.tex"], None).unwrap();

    assert!(!exists(dir.path(), "new/draft.tex"));
    assert!(!exists(dir.path(), "new"));
    assert_eq!(restored.changes.len(), 1);
    assert_eq!(restored.changes[0].kind, ChangeKind::Deleted);
}

#[test]
fn restore_with_an_unknown_path_writes_nothing_and_lists_every_one() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "edited\n");
    project.snapshot().unwrap();
    let before = stored(dir.path());

    let err = restore(
        &mut project,
        &["a.tex", "nope.tex", "../x", "/etc/passwd"],
        None,
    )
    .unwrap_err();

    match err {
        RestoreError::UnknownPaths { paths } => {
            assert_eq!(paths, self::paths(&["nope.tex", "../x", "/etc/passwd"]));
        }
        other => panic!("expected UnknownPaths, got {other:?}"),
    }
    assert_eq!(read(dir.path(), "a.tex"), "edited\n");
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn restore_without_paths_is_refused() {
    let (_dir, mut project) = project_with(&[("a.tex", "a\n")]);
    save_all(&mut project, "one");
    assert!(matches!(
        restore(&mut project, &[], None),
        Err(RestoreError::NoPaths)
    ));
}

#[test]
fn restore_keeps_the_executable_bit() {
    let (dir, mut project) = project_with(&[("run.sh", "#!/bin/sh\n")]);
    fs::set_permissions(dir.path().join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    save_all(&mut project, "one");
    write(dir.path(), "run.sh", "changed\n");
    fs::set_permissions(dir.path().join("run.sh"), fs::Permissions::from_mode(0o644)).unwrap();

    restore(&mut project, &["run.sh"], None).unwrap();

    assert_eq!(read(dir.path(), "run.sh"), "#!/bin/sh\n");
    let mode = fs::metadata(dir.path().join("run.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert_ne!(mode & 0o111, 0, "still executable");
}

#[test]
fn restore_flips_between_file_and_directory_both_ways() {
    let (dir, mut project) = project_with(&[("notes", "a file\n"), ("chap/one.tex", "1\n")]);
    save_all(&mut project, "one");
    fs::remove_file(dir.path().join("notes")).unwrap();
    write(dir.path(), "notes/day1.txt", "d\n");
    fs::remove_dir_all(dir.path().join("chap")).unwrap();
    write(dir.path(), "chap", "now a file\n");

    restore(&mut project, &["notes", "chap"], None).unwrap();

    assert_eq!(read(dir.path(), "notes"), "a file\n");
    assert_eq!(read(dir.path(), "chap/one.tex"), "1\n");
    fsck(dir.path());
}

#[test]
fn restoring_a_file_that_already_matches_is_a_no_op() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n")]);
    save_all(&mut project, "one");
    let draft = project.view().current().draft.clone();

    let restored = restore(&mut project, &["a.tex"], None).unwrap();

    assert_eq!(restored.changes, vec![]);
    assert_eq!(read(dir.path(), "a.tex"), "a\n");
    assert_eq!(project.view().current().draft, draft);
}

#[test]
fn restore_never_touches_symlinks_and_reports_symlinks_in_the_from_tree() {
    let (dir, project) = project_with(&[("a.tex", "a\n")]);
    // A symlink committed with plain git is in that save's tree. Only paper's view decides
    // what `last` is, so point the line's tip at that commit (a later version adopts it).
    let old_tip = project.view().current().tip.to_string();
    std::os::unix::fs::symlink("a.tex", dir.path().join("link")).unwrap();
    git(dir.path(), &["add", "link"]);
    git(dir.path(), &["commit", "-m", "link"]);
    let link_save = git(dir.path(), &["rev-parse", "HEAD"]);
    let view_path = dir.path().join(".git/paper/view.json");
    let view = fs::read_to_string(&view_path).unwrap();
    assert!(view.contains(&old_tip));
    fs::write(&view_path, view.replace(&old_tip, link_save.trim())).unwrap();
    let mut project = Project::open(dir.path(), identity()).unwrap();
    fs::remove_file(dir.path().join("link")).unwrap();
    write(dir.path(), "link", "plain file now\n");

    let restored = restore(&mut project, &["link"], Some("last")).unwrap();

    // Skipped files are never overwritten; the symlink entry is reported.
    assert_eq!(read(dir.path(), "link"), "plain file now\n");
    assert_eq!(restored.skipped, vec!["link".to_owned()]);
    assert_eq!(restored.changes, vec![]);
}

#[test]
fn after_restore_the_draft_matches_the_working_files() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n"), ("d/b.tex", "b\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "edit\n");
    write(dir.path(), "d/c.tex", "c\n");
    fs::remove_file(dir.path().join("d/b.tex")).unwrap();

    restore(&mut project, &["a.tex", "d"], None).unwrap();

    let stored = project.status(&StatusOptions { snapshot: false }).unwrap();
    let fresh = project.status(&StatusOptions { snapshot: true }).unwrap();
    assert_eq!(stored.changes, fresh.changes);
    assert_eq!(fresh.changes, vec![]);
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    fsck(dir.path());
}

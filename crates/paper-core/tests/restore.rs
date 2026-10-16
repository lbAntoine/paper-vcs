//! `Project::restore`, cross-checked with the real `git` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

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

fn skipped(path: &str, reason: RestoreSkip) -> SkippedPath {
    SkippedPath {
        path: path.to_owned(),
        reason,
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
    let (project, _) =
        Project::init_with_profile(dir.path(), identity(), options, Profile::builtin("generic"))
            .unwrap();
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

#[cfg(unix)]
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

    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let index = git(dir.path(), &["ls-files", "--stage"]);

    restore(&mut project, &["sec"], Some("last~2")).unwrap();

    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir.path(), &["ls-files", "--stage"]), index);
    assert_eq!(
        git(dir.path(), &["status", "--porcelain"]),
        " D sec/added/new.tex\n M sec/keep.tex\n?? sec/deep/\n?? sec/recreate.tex\n"
    );

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

#[cfg(unix)]
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

#[cfg(unix)]
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
    assert_eq!(
        restored.skipped,
        vec![skipped("link", RestoreSkip::SavedSymlink)]
    );
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

#[test]
fn a_refused_restore_keeps_the_unsaved_edit_recorded_but_changes_no_file() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n")]);
    write(dir.path(), "a.tex", "unsaved edit\n");
    let draft = project.view().current().draft.clone();

    let err = restore(&mut project, &["a.tex", "nope.tex"], None).unwrap_err();

    assert!(matches!(err, RestoreError::UnknownPaths { .. }));
    assert_eq!(read(dir.path(), "a.tex"), "unsaved edit\n");
    // I1: the edit is recorded in the draft even though the restore was refused.
    assert_ne!(project.view().current().draft, draft);
    let stored = project.status(&StatusOptions { snapshot: false }).unwrap();
    assert_eq!(stored.changes.len(), 1);
    assert_eq!(stored.changes[0].path, "a.tex");
}

#[test]
fn restore_never_overwrites_an_ignored_file_sitting_at_a_saved_path() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n"), ("out.log", "saved log\n")]);
    // The file is saved, then becomes ignored and untracked in the draft.
    write(dir.path(), ".gitignore", "out.log\n");
    save_all(&mut project, "two");
    git(dir.path(), &["rm", "--cached", "-q", "out.log"]);
    // Delete it from the draft by recording its removal, then bring back different content.
    fs::remove_file(dir.path().join("out.log")).unwrap();
    project.snapshot().unwrap();
    write(dir.path(), "out.log", "my local log\n");

    let restored = restore(&mut project, &["out.log"], Some("last")).unwrap();

    assert_eq!(read(dir.path(), "out.log"), "my local log\n");
    assert_eq!(
        restored.skipped,
        vec![skipped("out.log", RestoreSkip::Blocked)]
    );
    assert_eq!(restored.changes, vec![]);
}

#[cfg(unix)]
#[test]
fn restore_does_not_write_through_a_symlinked_parent_directory() {
    let outside = tempfile::tempdir().unwrap();
    let (dir, mut project) = project_with(&[("sec/one.tex", "1\n"), ("keep.tex", "k\n")]);
    fs::remove_dir_all(dir.path().join("sec")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("sec")).unwrap();

    let restored = restore(&mut project, &["sec/one.tex"], None).unwrap();

    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    assert_eq!(
        restored.skipped,
        vec![skipped("sec/one.tex", RestoreSkip::SymlinkedFolder)]
    );
}

#[cfg(unix)]
#[test]
fn restore_never_overwrites_a_symlink_in_the_working_files() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n"), ("b.tex", "b\n")]);
    fs::remove_file(dir.path().join("a.tex")).unwrap();
    std::os::unix::fs::symlink("b.tex", dir.path().join("a.tex")).unwrap();

    // `from` has a regular file at a.tex; the user's symlink stays.
    let restored = restore(&mut project, &["a.tex"], None).unwrap();

    assert!(
        fs::symlink_metadata(dir.path().join("a.tex"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        restored.skipped,
        vec![skipped("a.tex", RestoreSkip::Symlink)]
    );
}

#[cfg(unix)]
#[test]
fn restore_never_overwrites_an_oversize_untracked_file() {
    let options = SnapshotOptions {
        warn_file_bytes: 50,
        max_file_bytes: 100,
    };
    let (dir, mut project) = project_with_options(&[("big.dat", "small\n")], &options);
    // Removed from the draft, then recreated too big to track: the draft doesn't know it.
    fs::remove_file(dir.path().join("big.dat")).unwrap();
    project.snapshot().unwrap();
    let big = "x".repeat(500);
    write(dir.path(), "big.dat", &big);

    let restored = restore(&mut project, &["big.dat"], None).unwrap();

    assert_eq!(read(dir.path(), "big.dat"), big);
    assert_eq!(
        restored.skipped,
        vec![skipped("big.dat", RestoreSkip::TooLarge { bytes: 500 })]
    );
    assert_eq!(restored.changes, vec![]);
}

#[cfg(unix)]
#[test]
fn restore_never_touches_files_under_a_nested_repository() {
    let (dir, mut project) = project_with(&[("vendor/lib.tex", "saved\n")]);
    git(dir.path(), &["init", "-q", "vendor"]);
    write(dir.path(), "vendor/lib.tex", "nested edit\n");

    let restored = restore(&mut project, &["vendor"], None).unwrap();

    assert_eq!(read(dir.path(), "vendor/lib.tex"), "nested edit\n");
    assert_eq!(
        restored.skipped,
        vec![skipped("vendor/lib.tex", RestoreSkip::NestedRepository)]
    );
}

// ---- PV-012: case-only renames and all-or-nothing flips ----
//
// Case-only tests assert an end state that is the same on case-sensitive and
// case-insensitive filesystems, and check it by listing directory entries
// (`Path::exists` ignores case on macOS and Windows).

/// The sorted names of the entries directly inside `dir/sub`, ignoring dot-entries.
fn names(dir: &Path, sub: &str) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir.join(sub))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();
    found.sort();
    found
}

/// Replaces `from` with `to` on disk as a separate delete and create, so the result is
/// the same on every filesystem.
fn rename_on_disk(dir: &Path, from: &str, to: &str) {
    let content = fs::read_to_string(dir.join(from)).unwrap();
    fs::remove_file(dir.join(from)).unwrap();
    write(dir, to, &content);
}

/// Restores `only`, then checks what every case-only scenario must keep true.
fn restore_and_check_untouched_history(dir: &Path, project: &mut Project, only: &[&str]) {
    let head = git(dir, &["rev-parse", "HEAD"]);
    let index = git(dir, &["ls-files", "--stage"]);

    restore(project, only, None).unwrap();

    assert_eq!(git(dir, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir, &["ls-files", "--stage"]), index);
    assert_eq!(project.snapshot().unwrap().changes, vec![], "draft matches");
    fsck(dir);
}

#[test]
fn restore_gives_the_saved_spelling_when_only_the_case_of_a_file_changed() {
    let (dir, mut project) = project_with(&[("Notes.tex", "saved\n")]);
    save_all(&mut project, "one");
    rename_on_disk(dir.path(), "Notes.tex", "notes.tex");
    fs::write(dir.path().join("notes.tex"), "edited\n").unwrap();
    project.snapshot().unwrap();

    restore_and_check_untouched_history(dir.path(), &mut project, &["Notes.tex", "notes.tex"]);

    assert_eq!(names(dir.path(), ""), vec!["Notes.tex"]);
    assert_eq!(read(dir.path(), "Notes.tex"), "saved\n");
}

#[test]
fn restore_gives_the_saved_spelling_for_the_reverse_case_only_rename() {
    let (dir, mut project) = project_with(&[("notes.tex", "saved\n")]);
    save_all(&mut project, "one");
    rename_on_disk(dir.path(), "notes.tex", "Notes.tex");
    fs::write(dir.path().join("Notes.tex"), "edited\n").unwrap();
    project.snapshot().unwrap();

    restore_and_check_untouched_history(dir.path(), &mut project, &["Notes.tex", "notes.tex"]);

    assert_eq!(names(dir.path(), ""), vec!["notes.tex"]);
    assert_eq!(read(dir.path(), "notes.tex"), "saved\n");
}

#[test]
fn restore_gives_the_saved_spelling_when_only_the_case_of_a_folder_changed() {
    let (dir, mut project) = project_with(&[("Sections/a.tex", "saved\n")]);
    save_all(&mut project, "one");
    fs::remove_dir_all(dir.path().join("Sections")).unwrap();
    write(dir.path(), "sections/a.tex", "edited\n");
    project.snapshot().unwrap();

    restore_and_check_untouched_history(dir.path(), &mut project, &["Sections", "sections"]);

    assert_eq!(names(dir.path(), ""), vec!["Sections"]);
    assert_eq!(names(dir.path(), "Sections"), vec!["a.tex"]);
    assert_eq!(read(dir.path(), "Sections/a.tex"), "saved\n");
}

#[test]
fn a_blocked_file_folder_swap_leaves_both_sides_alone_but_still_does_an_unrelated_deletion() {
    let (dir, mut project) = project_with(&[
        (".gitignore", "*.log\n"),
        ("chap", "a file\n"),
        ("keep.tex", "keep\n"),
    ]);
    save_all(&mut project, "one");
    // The draft turns `chap` into a folder holding a tracked file and an ignored one.
    fs::remove_file(dir.path().join("chap")).unwrap();
    write(dir.path(), "chap/one.tex", "1\n");
    write(dir.path(), "chap/cache.log", "ignored\n");
    write(dir.path(), "extra.tex", "extra\n");
    project.snapshot().unwrap();
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let index = git(dir.path(), &["ls-files", "--stage"]);

    let restored = restore(&mut project, &["chap", "extra.tex"], None).unwrap();

    assert_eq!(names(dir.path(), "chap"), vec!["cache.log", "one.tex"]);
    assert_eq!(read(dir.path(), "chap/one.tex"), "1\n");
    assert_eq!(read(dir.path(), "chap/cache.log"), "ignored\n");
    assert_eq!(
        restored.skipped,
        vec![
            skipped("chap", RestoreSkip::Blocked),
            skipped("chap/one.tex", RestoreSkip::Blocked),
        ]
    );
    let changes: Vec<(String, ChangeKind)> = restored
        .changes
        .iter()
        .map(|change| (change.path.clone(), change.kind))
        .collect();
    assert_eq!(changes, vec![("extra.tex".to_owned(), ChangeKind::Deleted)]);
    assert_eq!(
        names(dir.path(), ""),
        vec![".gitignore", "chap", "keep.tex"]
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(dir.path(), &["ls-files", "--stage"]), index);
    assert_eq!(project.snapshot().unwrap().changes, vec![], "draft matches");
    fsck(dir.path());
}

//! `Project::status`, with real repositories.

use std::fs;
use std::path::Path;

use paper_core::project::*;
use paper_core::snapshot::{ChangeKind, SnapshotOptions};
use paper_git::Identity;

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

fn write(dir: &Path, path: &str, content: &str) {
    let absolute = dir.join(path);
    fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    fs::write(absolute, content).unwrap();
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

fn stat_of(status: &Status) -> Vec<(String, ChangeKind, ChangeStat)> {
    status
        .changes
        .iter()
        .map(|e| (e.path.clone(), e.kind, e.stat))
        .collect()
}

fn with_snapshot() -> StatusOptions {
    StatusOptions { snapshot: true }
}

fn without_snapshot() -> StatusOptions {
    StatusOptions { snapshot: false }
}

#[test]
fn status_reports_line_last_save_and_sorted_changes_with_counts() {
    let (dir, mut project) = project_with(&[
        ("b.tex", "1\n2\n"),
        ("fig.bin", "a\0b"),
        ("gone.tex", "x\n"),
    ]);
    write(dir.path(), "gone.tex", "y\n");
    save_all(&mut project, "first draft");
    write(dir.path(), "b.tex", "1\n3\n4\n");
    write(dir.path(), "a.tex", "new\n");
    fs::write(dir.path().join("fig.bin"), b"abc\0de").unwrap();
    fs::remove_file(dir.path().join("gone.tex")).unwrap();

    let status = project.status(&StatusOptions::default()).unwrap();
    assert_eq!(
        status.line.to_string(),
        project.view().current_line().to_string()
    );
    assert_eq!(status.last.message, "first draft");
    assert_eq!(status.last.commit, project.view().current().tip);
    assert_eq!(
        stat_of(&status),
        vec![
            (
                "a.tex".to_owned(),
                ChangeKind::Added,
                ChangeStat::Lines {
                    added: 1,
                    removed: 0
                }
            ),
            (
                "b.tex".to_owned(),
                ChangeKind::Modified,
                ChangeStat::Lines {
                    added: 2,
                    removed: 1
                }
            ),
            (
                "fig.bin".to_owned(),
                ChangeKind::Modified,
                ChangeStat::Bytes {
                    old: Some(3),
                    new: Some(6)
                }
            ),
            (
                "gone.tex".to_owned(),
                ChangeKind::Deleted,
                ChangeStat::Lines {
                    added: 0,
                    removed: 1
                }
            ),
        ]
    );
}

#[test]
fn status_snapshots_first_unless_told_not_to() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "edited\n");

    let stale = project.status(&without_snapshot()).unwrap();
    assert_eq!(stale.changes, vec![]);
    assert_eq!(stale.skipped, vec![]);
    assert_eq!(stale.large, vec![]);

    let fresh = project.status(&with_snapshot()).unwrap();
    assert_eq!(fresh.changes.len(), 1);
    assert_eq!(fresh.changes[0].path, "a.tex");

    let stored = project.status(&without_snapshot()).unwrap();
    assert_eq!(stored.changes, fresh.changes);
}

#[test]
fn status_of_an_empty_draft_has_no_changes() {
    let (_dir, mut project) = project_with(&[("a.tex", "a\n")]);
    save_all(&mut project, "one");
    let status = project.status(&StatusOptions::default()).unwrap();
    assert_eq!(status.changes, vec![]);
}

#[test]
fn status_reports_large_and_skipped_files() {
    let options = SnapshotOptions {
        warn_file_bytes: 50,
        max_file_bytes: 1000,
    };
    let (dir, mut project) = project_with_options(&[("a.tex", "a\n")], &options);
    save_all(&mut project, "one");
    write(dir.path(), "big.txt", &"x".repeat(100));
    write(dir.path(), "huge.txt", &"y".repeat(2000));

    let status = project.status(&StatusOptions::default()).unwrap();
    assert!(
        status
            .large
            .iter()
            .any(|(p, n)| p == "big.txt" && *n == 100)
    );
    assert!(status.skipped.iter().any(|s| s.path == "huge.txt"));
    let changed: Vec<&str> = status.changes.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(changed, vec!["big.txt"]);
}

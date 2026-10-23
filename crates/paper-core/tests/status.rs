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
fn status_reports_line_last_save_and_sorted_changes_with_word_counts() {
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
                ChangeStat::Words {
                    added: 1,
                    removed: 0,
                    layout_only: false
                }
            ),
            (
                "b.tex".to_owned(),
                ChangeKind::Modified,
                ChangeStat::Words {
                    added: 2,
                    removed: 1,
                    layout_only: false
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
                ChangeStat::Words {
                    added: 0,
                    removed: 1,
                    layout_only: false
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

// ---- PV-029: status says when the line moved outside paper ----

use std::process::Command;

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

fn main_line() -> paper_core::name::LineName {
    "main".parse().unwrap()
}

/// Everything status must leave alone: refs and HEAD.
fn refs_and_head(dir: &Path) -> (String, String) {
    (
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        git(dir, &["rev-parse", "HEAD"]) + &git(dir, &["branch", "--show-current"]),
    )
}

#[test]
fn git_helper_commits_and_switches_in_the_temp_repository() {
    let (dir, _project) = project_with(&[("a.tex", "1\n")]);
    git(dir.path(), &["commit", "--allow-empty", "-m", "plain"]);
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%s"]).trim(),
        "plain"
    );
    git(dir.path(), &["switch", "-c", "other"]);
    assert_eq!(
        git(dir.path(), &["branch", "--show-current"]).trim(),
        "other"
    );
}

#[test]
fn a_healthy_project_has_no_outside_paper_state() {
    let (_dir, mut project) = project_with(&[("a.tex", "1\n")]);
    for options in [with_snapshot(), without_snapshot()] {
        assert_eq!(project.status(&options).unwrap().outside_paper, None);
    }
}

#[test]
fn status_reports_the_line_moved_after_a_plain_git_commit() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    git(dir.path(), &["commit", "--allow-empty", "-m", "plain"]);
    let before = refs_and_head(dir.path());

    let status = project.status(&with_snapshot()).unwrap();

    assert_eq!(
        status.outside_paper,
        Some(OutsidePaper::LineMoved { line: main_line() })
    );
    assert_eq!(refs_and_head(dir.path()), before, "status writes no refs");
}

#[test]
fn status_reports_the_line_moved_without_a_snapshot() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    git(dir.path(), &["commit", "--allow-empty", "-m", "plain"]);

    let status = project.status(&without_snapshot()).unwrap();

    assert_eq!(
        status.outside_paper,
        Some(OutsidePaper::LineMoved { line: main_line() })
    );
}

#[test]
fn status_reports_another_branch_after_git_switch() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    git(dir.path(), &["switch", "-c", "other"]);
    let before = refs_and_head(dir.path());

    let status = project.status(&with_snapshot()).unwrap();

    assert_eq!(
        status.outside_paper,
        Some(OutsidePaper::OnAnotherBranch {
            line: main_line(),
            found: Some("other".to_owned()),
        })
    );
    assert_eq!(refs_and_head(dir.path()), before, "status writes no HEAD");
}

#[test]
fn status_reports_another_branch_without_a_snapshot() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    git(dir.path(), &["switch", "-c", "other"]);

    let status = project.status(&without_snapshot()).unwrap();

    assert_eq!(
        status.outside_paper,
        Some(OutsidePaper::OnAnotherBranch {
            line: main_line(),
            found: Some("other".to_owned()),
        })
    );
}

#[test]
fn status_reports_a_detached_head_as_found_none() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    git(dir.path(), &["checkout", "--detach"]);

    let status = project.status(&with_snapshot()).unwrap();

    assert_eq!(
        status.outside_paper,
        Some(OutsidePaper::OnAnotherBranch {
            line: main_line(),
            found: None,
        })
    );
}

#[test]
fn status_reports_another_branch_when_the_line_also_moved() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    git(dir.path(), &["switch", "-c", "other"]);
    git(dir.path(), &["commit", "--allow-empty", "-m", "plain"]);
    git(dir.path(), &["branch", "-f", "main", "HEAD"]);

    let status = project.status(&with_snapshot()).unwrap();

    assert_eq!(
        status.outside_paper,
        Some(OutsidePaper::OnAnotherBranch {
            line: main_line(),
            found: Some("other".to_owned()),
        })
    );
}

#[test]
fn the_draft_is_still_compared_with_papers_last_save_when_the_line_moved() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    git(dir.path(), &["commit", "--allow-empty", "-m", "plain"]);
    write(dir.path(), "a.tex", "1\n2\n");

    let status = project.status(&with_snapshot()).unwrap();

    assert_eq!(status.changes.len(), 1);
    assert!(status.outside_paper.is_some());
}

#[test]
fn status_reports_the_line_moved_after_git_reset_hard() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    write(dir.path(), "a.tex", "2\n");
    save_all(&mut project, "Second");
    git(dir.path(), &["reset", "--hard", "HEAD~1"]);

    let status = project.status(&with_snapshot()).unwrap();

    assert_eq!(
        status.outside_paper,
        Some(OutsidePaper::LineMoved { line: main_line() })
    );
}

#[test]
fn status_reports_the_line_moved_after_git_commit_amend() {
    let (dir, mut project) = project_with(&[("a.tex", "1\n")]);
    git(
        dir.path(),
        &["commit", "--amend", "--allow-empty", "-m", "amended"],
    );

    let status = project.status(&without_snapshot()).unwrap();

    assert_eq!(
        status.outside_paper,
        Some(OutsidePaper::LineMoved { line: main_line() })
    );
}

#[test]
fn status_marks_a_rewrap_as_layout_only() {
    let (dir, mut project) = project_with(&[("a.tex", "one two three\nfour five\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "one two\nthree  four five\n");

    let status = project.status(&StatusOptions::default()).unwrap();

    assert_eq!(
        stat_of(&status),
        vec![(
            "a.tex".to_owned(),
            ChangeKind::Modified,
            ChangeStat::Words {
                added: 0,
                removed: 0,
                layout_only: true
            }
        )]
    );
}

#[test]
fn status_counts_only_words_not_latex_commands() {
    let (dir, mut project) = project_with(&[("a.tex", "\\textbf{big} result\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "\\emph{big} result\n");

    let status = project.status(&StatusOptions::default()).unwrap();

    assert_eq!(
        status.changes[0].stat,
        ChangeStat::Words {
            added: 0,
            removed: 0,
            layout_only: false
        }
    );
}

#[test]
fn status_counts_words_of_other_text_files_too() {
    let (dir, mut project) = project_with(&[("notes.md", "alpha beta\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "notes.md", "alpha beta gamma delta\n");

    let status = project.status(&StatusOptions::default()).unwrap();

    assert_eq!(
        status.changes[0].stat,
        ChangeStat::Words {
            added: 2,
            removed: 0,
            layout_only: false
        }
    );
}

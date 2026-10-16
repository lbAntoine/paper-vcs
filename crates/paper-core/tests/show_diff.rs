//! `Project::show_diff`: a save (or the draft) against its first parent.

use std::fs;

use paper_core::point::Point;
use paper_core::project::{FileDiffEntry, PathSelection, Project, SaveRequest};
use paper_core::snapshot::{ChangeKind, SnapshotOptions};
use paper_git::Identity;

fn project_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        let absolute = dir.path().join(path);
        fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        fs::write(absolute, content).unwrap();
    }
    let identity = Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    };
    let (project, _) = Project::init(dir.path(), identity, &SnapshotOptions::default()).unwrap();
    (dir, project)
}

fn point(text: &str) -> Point {
    text.parse().unwrap()
}

fn summary(entries: &[FileDiffEntry]) -> Vec<(&str, ChangeKind)> {
    entries.iter().map(|e| (e.path.as_str(), e.kind)).collect()
}

#[test]
fn a_first_save_shows_every_file_as_added() {
    let (_dir, project) = project_with(&[("a.tex", "a\n"), ("d/b.tex", "b\n")]);

    let entries = project.show_diff(&point("last")).unwrap();

    assert_eq!(
        summary(&entries),
        vec![
            (".gitignore", ChangeKind::Added),
            ("a.tex", ChangeKind::Added),
            ("d/b.tex", ChangeKind::Added),
            ("paper.toml", ChangeKind::Added)
        ]
    );
}

#[test]
fn a_later_save_shows_exactly_what_it_changed() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n"), ("b.tex", "b\n")]);
    fs::write(dir.path().join("a.tex"), "a2\n").unwrap();
    fs::write(dir.path().join("c.tex"), "c\n").unwrap();
    project
        .save(&SaveRequest {
            message: "Edit".to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();

    let entries = project.show_diff(&point("last")).unwrap();

    assert_eq!(
        summary(&entries),
        vec![
            ("a.tex", ChangeKind::Modified),
            ("c.tex", ChangeKind::Added)
        ]
    );
}

#[test]
fn the_draft_is_shown_against_the_tip() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n"), ("b.tex", "b\n")]);
    fs::write(dir.path().join("a.tex"), "a2\n").unwrap();
    fs::remove_file(dir.path().join("b.tex")).unwrap();
    project.snapshot().unwrap();

    let entries = project.show_diff(&point("@")).unwrap();

    assert_eq!(
        summary(&entries),
        vec![
            ("a.tex", ChangeKind::Modified),
            ("b.tex", ChangeKind::Deleted)
        ]
    );
}

#[test]
fn an_unknown_point_is_an_error() {
    let (_dir, project) = project_with(&[("a.tex", "a\n")]);
    assert!(project.show_diff(&point("nope")).is_err());
}

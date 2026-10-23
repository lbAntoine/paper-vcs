//! `Project::show_diff`: a save (or the draft) against its first parent.

use std::fs;

use paper_core::point::Point;
use paper_core::profile::Profile;
use paper_core::project::{
    DiffMode, EntryDiff, FileDiffEntry, PathSelection, Project, SaveRequest,
};
use paper_core::snapshot::{ChangeKind, SnapshotOptions};
use paper_diff::{FileDiff, Segment, WordDiff};
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
    let (project, _) = Project::init_with_profile(
        dir.path(),
        identity,
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
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

    let entries = project.show_diff(&point("last"), DiffMode::Lines).unwrap();

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

    let entries = project.show_diff(&point("last"), DiffMode::Lines).unwrap();

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

    let entries = project.show_diff(&point("@"), DiffMode::Lines).unwrap();

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
    assert!(project.show_diff(&point("nope"), DiffMode::Lines).is_err());
}

fn words(entry: &FileDiffEntry) -> &WordDiff {
    match &entry.diff {
        EntryDiff::Words(diff) => diff,
        other @ EntryDiff::Lines(_) => {
            panic!("{}: expected a word diff, got {other:?}", entry.path)
        }
    }
}

fn save(project: &mut Project, message: &str) {
    project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
}

#[test]
fn words_mode_counts_the_words_a_save_changed() {
    let (dir, mut project) = project_with(&[("a.tex", "one two three\n")]);
    fs::write(dir.path().join("a.tex"), "one 2 three four\n").unwrap();
    save(&mut project, "Edit");

    let entries = project.show_diff(&point("last"), DiffMode::Words).unwrap();

    assert_eq!(summary(&entries), vec![("a.tex", ChangeKind::Modified)]);
    let diff = words(&entries[0]);
    assert_eq!((diff.added, diff.removed), (2, 1));
    assert!(!diff.layout_only);
    assert_eq!(diff.paragraphs.len(), 1);
    assert_eq!(diff.paragraphs[0].number, 1);
}

#[test]
fn words_mode_shows_a_first_save_as_words_added_to_nothing() {
    let (_dir, project) = project_with(&[("a.tex", "alpha beta\n\ngamma\n")]);

    let entries = project.show_diff(&point("last"), DiffMode::Words).unwrap();

    let a = entries.iter().find(|e| e.path == "a.tex").unwrap();
    assert_eq!(a.kind, ChangeKind::Added);
    let diff = words(a);
    assert_eq!((diff.added, diff.removed), (3, 0));
    assert_eq!(diff.paragraphs.len(), 2);
}

#[test]
fn words_mode_shows_the_draft_with_deleted_files_as_words_removed() {
    let (dir, mut project) = project_with(&[("a.tex", "a\n"), ("b.tex", "bee sea\n")]);
    fs::write(dir.path().join("a.tex"), "a2\n").unwrap();
    fs::remove_file(dir.path().join("b.tex")).unwrap();
    project.snapshot().unwrap();

    let entries = project.show_diff(&point("@"), DiffMode::Words).unwrap();

    assert_eq!(
        summary(&entries),
        vec![
            ("a.tex", ChangeKind::Modified),
            ("b.tex", ChangeKind::Deleted)
        ]
    );
    assert_eq!(
        (words(&entries[1]).added, words(&entries[1]).removed),
        (0, 2)
    );
}

#[test]
fn words_mode_flags_a_save_that_only_moved_line_breaks() {
    let (dir, mut project) = project_with(&[("a.tex", "one two three\n")]);
    fs::write(dir.path().join("a.tex"), "one\ntwo  three\n").unwrap();
    save(&mut project, "Rewrap");

    let entries = project.show_diff(&point("last"), DiffMode::Words).unwrap();

    let diff = words(&entries[0]);
    assert!(diff.layout_only);
    assert!(diff.paragraphs.is_empty());
    assert_eq!((diff.added, diff.removed), (0, 0));
}

#[test]
fn words_mode_leaves_binary_files_as_a_size_change() {
    let (dir, mut project) = project_with(&[("fig.bin", "ab\0cd")]);
    fs::write(dir.path().join("fig.bin"), b"abc\0defgh").unwrap();
    save(&mut project, "Change figure");

    let entries = project.show_diff(&point("last"), DiffMode::Words).unwrap();

    assert_eq!(
        entries[0].diff,
        EntryDiff::Lines(FileDiff::Binary {
            old_size: Some(5),
            new_size: Some(9)
        })
    );
}

#[test]
fn words_mode_cuts_a_long_unchanged_run() {
    let body: String = (0..120)
        .map(|i| ["w", &i.to_string(), " "].concat())
        .collect();
    let (dir, mut project) = project_with(&[("a.tex", &format!("first {body}last\n"))]);
    fs::write(dir.path().join("a.tex"), format!("FIRST {body}LAST\n")).unwrap();
    save(&mut project, "Edit ends");

    let entries = project.show_diff(&point("last"), DiffMode::Words).unwrap();

    let segments = &words(&entries[0]).paragraphs[0].segments;
    assert!(segments.contains(&Segment::Cut), "{segments:?}");
}

#[test]
fn lines_mode_still_gives_hunks() {
    let (dir, mut project) = project_with(&[("a.tex", "a\nb\n")]);
    fs::write(dir.path().join("a.tex"), "a\nc\n").unwrap();
    save(&mut project, "Edit");

    let entries = project.show_diff(&point("last"), DiffMode::Lines).unwrap();

    let EntryDiff::Lines(FileDiff::Text(text)) = &entries[0].diff else {
        panic!("{:?}", entries[0]);
    };
    assert_eq!((text.added, text.removed), (1, 1));
    assert!(!text.hunks.is_empty());
}

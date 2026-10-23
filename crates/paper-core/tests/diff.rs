//! `Project::diff`, with real repositories.

use std::fs;
use std::path::Path;

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

fn point(text: &str) -> Point {
    text.parse().unwrap()
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

use paper_diff::{FileDiff, Segment, WordDiff};

fn diff(
    project: &mut Project,
    from: Option<&str>,
    to: Option<&str>,
    only: &[&str],
) -> Vec<FileDiffEntry> {
    diff_in(project, DiffMode::Lines, from, to, only)
}

fn diff_in(
    project: &mut Project,
    mode: DiffMode,
    from: Option<&str>,
    to: Option<&str>,
    only: &[&str],
) -> Vec<FileDiffEntry> {
    project.snapshot().unwrap();
    project
        .diff(&DiffRequest {
            mode,
            from: from.map(point),
            to: to.map(point),
            paths: paths(only),
        })
        .unwrap()
}

fn words(entry: &FileDiffEntry) -> &WordDiff {
    match &entry.diff {
        EntryDiff::Words(diff) => diff,
        other @ EntryDiff::Lines(_) => {
            panic!("{}: expected a word diff, got {other:?}", entry.path)
        }
    }
}

fn summary(entries: &[FileDiffEntry]) -> Vec<(String, ChangeKind)> {
    entries.iter().map(|e| (e.path.clone(), e.kind)).collect()
}

fn counts(entry: &FileDiffEntry) -> (usize, usize) {
    match &entry.diff {
        EntryDiff::Lines(FileDiff::Text(text)) => (text.added, text.removed),
        other => panic!("expected a text diff, got {other:?}"),
    }
}

/// Saves `one` then leaves an edit, an addition and a deletion in the draft.
fn project_with_changes() -> (tempfile::TempDir, Project) {
    let (dir, mut project) = project_with(&[("a.tex", "x\ny\n"), ("gone.tex", "bye\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "x\nz\nw\n");
    write(dir.path(), "new.tex", "hello\n");
    fs::remove_file(dir.path().join("gone.tex")).unwrap();
    (dir, project)
}

#[test]
fn diff_without_points_compares_last_with_the_draft() {
    let (_dir, mut project) = project_with_changes();
    let entries = diff(&mut project, None, None, &[]);
    assert_eq!(
        summary(&entries),
        vec![
            ("a.tex".to_owned(), ChangeKind::Modified),
            ("gone.tex".to_owned(), ChangeKind::Deleted),
            ("new.tex".to_owned(), ChangeKind::Added),
        ]
    );
    assert_eq!(counts(&entries[0]), (2, 1));
    assert_eq!(counts(&entries[1]), (0, 1));
    assert_eq!(counts(&entries[2]), (1, 0));
    let EntryDiff::Lines(FileDiff::Text(text)) = &entries[0].diff else {
        unreachable!()
    };
    assert!(!text.hunks.is_empty());
}

#[test]
fn diff_with_one_point_compares_it_with_the_draft() {
    let (_dir, mut project) = project_with_changes();
    let entries = diff(&mut project, Some("last"), None, &[]);
    assert_eq!(entries.len(), 3);
}

#[test]
fn diff_between_two_saves_and_its_reverse() {
    let (dir, mut project) = project_with(&[("a.tex", "one\n")]);
    save_all(&mut project, "first");
    write(dir.path(), "a.tex", "one\ntwo\n");
    write(dir.path(), "b.tex", "b\n");
    save_all(&mut project, "second");

    let forward = diff(&mut project, Some("last~1"), Some("last"), &[]);
    assert_eq!(
        summary(&forward),
        vec![
            ("a.tex".to_owned(), ChangeKind::Modified),
            ("b.tex".to_owned(), ChangeKind::Added),
        ]
    );
    assert_eq!(counts(&forward[0]), (1, 0));

    let backward = diff(&mut project, Some("last"), Some("last~1"), &[]);
    assert_eq!(
        summary(&backward),
        vec![
            ("a.tex".to_owned(), ChangeKind::Modified),
            ("b.tex".to_owned(), ChangeKind::Deleted),
        ]
    );
    assert_eq!(counts(&backward[0]), (0, 1));
}

#[test]
fn diff_of_a_binary_file_gives_both_sizes() {
    let (dir, mut project) = project_with(&[("fig.bin", "ab\0cd")]);
    save_all(&mut project, "one");
    fs::write(dir.path().join("fig.bin"), b"abc\0defgh").unwrap();
    let entries = diff(&mut project, None, None, &[]);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].diff,
        EntryDiff::Lines(FileDiff::Binary {
            old_size: Some(5),
            new_size: Some(9)
        })
    );
}

#[test]
fn diff_paths_restrict_by_file_and_by_directory_components() {
    let (dir, mut project) = project_with(&[
        ("sections/a.tex", "a\n"),
        ("sections2/b.tex", "b\n"),
        ("top.tex", "t\n"),
    ]);
    save_all(&mut project, "one");
    write(dir.path(), "sections/a.tex", "A\n");
    write(dir.path(), "sections2/b.tex", "B\n");
    write(dir.path(), "top.tex", "T\n");

    let by_dir = diff(&mut project, None, None, &["sections/"]);
    assert_eq!(
        summary(&by_dir),
        vec![("sections/a.tex".to_owned(), ChangeKind::Modified)]
    );
    let by_file = diff(&mut project, None, None, &["top.tex"]);
    assert_eq!(
        summary(&by_file),
        vec![("top.tex".to_owned(), ChangeKind::Modified)]
    );
}

#[test]
fn diff_lists_every_unknown_path() {
    let (_dir, mut project) = project_with_changes();
    project.snapshot().unwrap();
    let err = project
        .diff(&DiffRequest {
            mode: DiffMode::Lines,
            from: None,
            to: None,
            paths: paths(&["a.tex", "nope.tex", "typo/"]),
        })
        .unwrap_err();
    match err {
        DiffError::UnknownPaths { paths } => {
            assert_eq!(paths, self::paths(&["nope.tex", "typo/"]));
        }
        other => panic!("expected UnknownPaths, got {other:?}"),
    }
}

#[test]
fn diff_of_an_unchanged_existing_path_is_empty_not_an_error() {
    let (_dir, mut project) = project_with(&[("a.tex", "a\n"), ("b.tex", "b\n")]);
    save_all(&mut project, "one");
    let entries = diff(&mut project, None, None, &["b.tex"]);
    assert_eq!(entries, vec![]);
}

#[test]
fn diff_with_an_unknown_point_is_a_resolve_error() {
    let (_dir, project) = project_with(&[("a.tex", "a\n")]);
    let err = project
        .diff(&DiffRequest {
            mode: DiffMode::Lines,
            from: Some(point("nowhere")),
            to: None,
            paths: vec![],
        })
        .unwrap_err();
    assert!(matches!(
        err,
        DiffError::Resolve(paper_core::project::ResolveError::UnknownPoint { .. })
    ));
}

#[test]
fn words_are_the_default_mode() {
    assert_eq!(DiffMode::default(), DiffMode::Words);
    assert_eq!(DiffRequest::default().mode, DiffMode::Words);
}

#[test]
fn words_mode_counts_words_for_modified_added_and_deleted_files() {
    let (_dir, mut project) = project_with_changes();
    let entries = diff_in(&mut project, DiffMode::Words, None, None, &[]);
    assert_eq!(
        summary(&entries),
        vec![
            ("a.tex".to_owned(), ChangeKind::Modified),
            ("gone.tex".to_owned(), ChangeKind::Deleted),
            ("new.tex".to_owned(), ChangeKind::Added),
        ]
    );
    let counts = |e: &FileDiffEntry| (words(e).added, words(e).removed);
    assert_eq!(counts(&entries[0]), (2, 1));
    assert_eq!(counts(&entries[1]), (0, 1));
    assert_eq!(counts(&entries[2]), (1, 0));
    assert!(entries.iter().all(|e| !words(e).layout_only));
}

#[test]
fn words_mode_numbers_the_changed_paragraphs_on_the_new_side() {
    let (dir, mut project) = project_with(&[(
        "p.tex",
        "First paragraph here.\n\nSecond paragraph here.\n\nThird paragraph here.\n",
    )]);
    save_all(&mut project, "one");
    write(
        dir.path(),
        "p.tex",
        "First paragraph here.\n\nSecond paragraph changed.\n\nThird paragraph here.\n",
    );

    let entries = diff_in(&mut project, DiffMode::Words, None, None, &[]);

    let diff = words(&entries[0]);
    assert_eq!((diff.added, diff.removed), (1, 1));
    assert_eq!(diff.paragraphs.len(), 1);
    assert_eq!(diff.paragraphs[0].number, 2);
    assert!(
        diff.paragraphs[0]
            .segments
            .contains(&Segment::Removed("here".to_owned())),
        "{diff:?}"
    );
    assert!(
        diff.paragraphs[0]
            .segments
            .contains(&Segment::Added("changed".to_owned())),
        "{diff:?}"
    );
}

#[test]
fn words_mode_reports_a_rewrap_as_layout_only() {
    let (dir, mut project) = project_with(&[("a.tex", "one two three\nfour five\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "one two\nthree four  five\n");

    let entries = diff_in(&mut project, DiffMode::Words, None, None, &[]);

    assert_eq!(entries.len(), 1);
    let diff = words(&entries[0]);
    assert!(diff.layout_only);
    assert!(diff.paragraphs.is_empty());
    assert_eq!((diff.added, diff.removed), (0, 0));
}

#[test]
fn words_mode_uses_the_latex_tokeniser_for_tex_files_only() {
    let (dir, mut project) = project_with(&[
        ("a.tex", "\\textbf{big} result\n"),
        ("a.txt", "\\textbf{big} result\n"),
    ]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "\\emph{big} result\n");
    write(dir.path(), "a.txt", "\\emph{big} result\n");

    let entries = diff_in(&mut project, DiffMode::Words, None, None, &[]);

    // A LaTeX command is not a word; in a plain text file `textbf` is one.
    assert_eq!(entries[0].path, "a.tex");
    assert_eq!(
        (words(&entries[0]).added, words(&entries[0]).removed),
        (0, 0)
    );
    assert!(!words(&entries[0]).layout_only);
    assert!(!words(&entries[0]).paragraphs.is_empty());
    assert_eq!(entries[1].path, "a.txt");
    assert_eq!(
        (words(&entries[1]).added, words(&entries[1]).removed),
        (1, 1)
    );
}

#[test]
fn words_mode_cuts_long_unchanged_runs() {
    let body: String = (0..120)
        .map(|i| ["w", &i.to_string(), " "].concat())
        .collect();
    let (dir, mut project) = project_with(&[("a.tex", &format!("first {body}last\n"))]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", &format!("FIRST {body}LAST\n"));

    let entries = diff_in(&mut project, DiffMode::Words, None, None, &[]);

    assert!(
        words(&entries[0]).paragraphs[0]
            .segments
            .contains(&Segment::Cut)
    );
}

#[test]
fn words_mode_leaves_binary_files_as_sizes() {
    let (dir, mut project) = project_with(&[("fig.bin", "ab\0cd")]);
    save_all(&mut project, "one");
    fs::write(dir.path().join("fig.bin"), b"abc\0defgh").unwrap();

    let entries = diff_in(&mut project, DiffMode::Words, None, None, &[]);

    assert_eq!(
        entries[0].diff,
        EntryDiff::Lines(FileDiff::Binary {
            old_size: Some(5),
            new_size: Some(9)
        })
    );
}

#[test]
fn words_mode_honours_paths_and_unknown_path_errors() {
    let (dir, mut project) = project_with(&[("a.tex", "a b\n"), ("b.tex", "c d\n")]);
    save_all(&mut project, "one");
    write(dir.path(), "a.tex", "a x\n");
    write(dir.path(), "b.tex", "c y\n");

    let only = diff_in(&mut project, DiffMode::Words, None, None, &["b.tex"]);
    assert_eq!(
        summary(&only),
        vec![("b.tex".to_owned(), ChangeKind::Modified)]
    );
    assert!(matches!(only[0].diff, EntryDiff::Words(_)));

    let err = project
        .diff(&DiffRequest {
            mode: DiffMode::Words,
            paths: paths(&["nope.tex"]),
            ..DiffRequest::default()
        })
        .unwrap_err();
    assert!(matches!(err, DiffError::UnknownPaths { .. }));
}

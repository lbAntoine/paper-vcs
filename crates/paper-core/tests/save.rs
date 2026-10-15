//! `Project::snapshot` and `Project::save`, cross-checked with the real `git` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::name::LineName;
use paper_core::project::{PathSelection, Project, SaveError, SaveOutcome, SaveRequest, Saved};
use paper_core::snapshot::{Change, ChangeKind, SnapshotOptions};
use paper_core::view::View;
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

fn fsck(dir: &Path) {
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

fn write(dir: &Path, path: &str, content: &str) {
    let absolute = dir.join(path);
    fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    fs::write(absolute, content).unwrap();
}

/// A project created with `files`, plus the temp directory that owns it.
fn project_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        write(dir.path(), path, content);
    }
    let (project, _) = Project::init(dir.path(), identity(), &SnapshotOptions::default()).unwrap();
    (dir, project)
}

fn line(name: &str) -> LineName {
    name.parse().unwrap()
}

fn request(message: &str, paths: PathSelection) -> SaveRequest {
    SaveRequest {
        message: message.to_owned(),
        paths,
    }
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

fn change(path: &str, kind: ChangeKind) -> Change {
    Change {
        path: path.to_owned(),
        kind,
    }
}

fn saved(outcome: SaveOutcome) -> Saved {
    match outcome {
        SaveOutcome::Saved(saved) => saved,
        SaveOutcome::NothingToSave => panic!("expected a save, got NothingToSave"),
    }
}

fn save_all(project: &mut Project, message: &str) -> Saved {
    saved(project.save(&request(message, PathSelection::All)).unwrap())
}

/// The `change-id` header of `rev`, as printed by `git cat-file`.
fn change_id_header(dir: &Path, rev: &str) -> String {
    git(dir, &["cat-file", "-p", rev])
        .lines()
        .find_map(|line| line.strip_prefix("change-id "))
        .unwrap_or_else(|| panic!("{rev} has no change-id header"))
        .to_owned()
}

/// Everything a refused or empty save must leave alone.
#[derive(Debug, PartialEq, Eq)]
struct Stored {
    refs: String,
    view: View,
    view_json: String,
    index_tree: String,
}

fn stored(dir: &Path, project: &Project) -> Stored {
    Stored {
        refs: git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        view: project.view().clone(),
        view_json: fs::read_to_string(dir.join(".git/paper/view.json")).unwrap(),
        index_tree: git(dir, &["ls-files", "--stage"]),
    }
}

fn tree_of(project: &Project, commit: &paper_git::CommitId) -> paper_git::TreeId {
    project.repo().read_commit(commit).unwrap().tree
}

fn draft_files(project: &Project) -> Vec<String> {
    let tree = tree_of(project, &project.view().current().draft);
    project
        .repo()
        .tree_files(&tree)
        .unwrap()
        .into_keys()
        .collect()
}

// ---------------------------------------------------------------- Project::snapshot

#[test]
fn snapshot_of_unchanged_files_keeps_the_same_draft_commit() {
    let (dir, mut project) = project_with(&[("main.tex", "one")]);
    let before = project.view().current().draft.clone();

    let state = project.snapshot().unwrap();
    assert_eq!(state.changes, vec![]);
    assert_eq!(project.view().current().draft, before);
    assert_eq!(project.snapshot().unwrap().changes, vec![]);
    assert_eq!(project.view().current().draft, before);
    fsck(dir.path());
}

#[test]
fn snapshot_after_an_edit_rewrites_the_draft_keeping_its_change_id_and_parent() {
    let (dir, mut project) = project_with(&[("main.tex", "one"), ("refs.bib", "r")]);
    let before = project.view().current().clone();
    let old_draft = project.repo().read_commit(&before.draft).unwrap();

    write(dir.path(), "main.tex", "two");
    write(dir.path(), "abstract.tex", "new");
    fs::remove_file(dir.path().join("refs.bib")).unwrap();
    let state = project.snapshot().unwrap();

    assert_eq!(
        state.changes,
        vec![
            change("abstract.tex", ChangeKind::Added),
            change("main.tex", ChangeKind::Modified),
            change("refs.bib", ChangeKind::Deleted),
        ]
    );
    let after = project.view().current().clone();
    assert_ne!(after.draft, before.draft);
    assert_eq!(after.tip, before.tip);
    let new_draft = project.repo().read_commit(&after.draft).unwrap();
    assert_eq!(new_draft.change_id, old_draft.change_id);
    assert_eq!(new_draft.parents, old_draft.parents);
    assert_ne!(new_draft.tree, old_draft.tree);
    assert_eq!(
        draft_files(&project),
        vec!["abstract.tex".to_owned(), "main.tex".to_owned()]
    );
    fsck(dir.path());
}

#[test]
fn snapshot_persists_the_new_draft_in_the_ref_and_the_view() {
    let (dir, mut project) = project_with(&[("main.tex", "one")]);
    write(dir.path(), "main.tex", "two");
    project.snapshot().unwrap();

    let draft = project.view().current().draft.clone();
    assert_eq!(
        git(dir.path(), &["rev-parse", "refs/paper/draft/main"]).trim(),
        draft.as_str()
    );
    let reopened = Project::open(dir.path(), identity()).unwrap();
    assert_eq!(reopened.view(), project.view());
    fsck(dir.path());
}

#[test]
fn snapshot_reports_changes_relative_to_the_last_save() {
    let (dir, mut project) = project_with(&[("a.tex", "a"), ("b.tex", "b")]);
    write(dir.path(), "a.tex", "a2");
    save_all(&mut project, "Edit a");
    write(dir.path(), "b.tex", "b2");

    let state = project.snapshot().unwrap();
    assert_eq!(state.changes, vec![change("b.tex", ChangeKind::Modified)]);
    fsck(dir.path());
}

#[cfg(unix)]
#[test]
fn snapshot_reports_what_it_skipped() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    std::os::unix::fs::symlink("/somewhere/data", dir.path().join("data")).unwrap();

    let state = project.snapshot().unwrap();
    assert_eq!(state.skipped.len(), 1);
    assert_eq!(state.skipped[0].path, "data");
    assert_eq!(state.changes, vec![]);
}

// ---------------------------------------------------------------- Project::save

#[test]
fn saving_with_no_changes_returns_nothing_to_save_and_writes_nothing() {
    let (dir, mut project) = project_with(&[("main.tex", "one")]);
    let before = stored(dir.path(), &project);

    let outcome = project
        .save(&request("Nothing happened", PathSelection::All))
        .unwrap();

    assert_eq!(outcome, SaveOutcome::NothingToSave);
    assert_eq!(stored(dir.path(), &project), before);
    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "main"]),
        "Start project\n"
    );
    fsck(dir.path());
}

#[test]
fn saving_everything_creates_one_commit_on_the_line() {
    let (dir, mut project) = project_with(&[("main.tex", "one")]);
    let old = project.view().current().clone();
    let old_draft_change_id = project.repo().read_commit(&old.draft).unwrap().change_id;
    write(dir.path(), "main.tex", "two");

    let result = save_all(&mut project, "  Rewrite the intro \n\n");

    assert_eq!(result.line, line("main"));
    let commit = project.repo().read_commit(&result.save).unwrap();
    assert_eq!(commit.parents, vec![old.tip.clone()]);
    assert_eq!(commit.message, "Rewrite the intro");
    assert_eq!(commit.author.identity, identity());
    assert_eq!(commit.committer.identity, identity());
    assert_eq!(commit.change_id, old_draft_change_id);
    assert_eq!(Some(result.change_id), old_draft_change_id);
    assert_eq!(project.view().current().tip, result.save);
    assert_eq!(result.saved, vec![change("main.tex", ChangeKind::Modified)]);
    assert_eq!(result.remaining, vec![]);

    assert_eq!(
        git(dir.path(), &["rev-parse", "main"]).trim(),
        result.save.as_str()
    );
    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "main"]),
        "Rewrite the intro\nStart project\n"
    );
    assert_eq!(git(dir.path(), &["show", "main:main.tex"]), "two");
    assert_eq!(
        change_id_header(dir.path(), "main"),
        result.change_id.to_string()
    );
    assert_eq!(
        git(dir.path(), &["symbolic-ref", "HEAD"]),
        "refs/heads/main\n"
    );
    fsck(dir.path());
}

#[test]
fn after_a_full_save_the_new_draft_is_empty_and_git_is_clean() {
    let (dir, mut project) = project_with(&[("main.tex", "one")]);
    write(dir.path(), "main.tex", "two");

    let result = save_all(&mut project, "Edit");

    let current = project.view().current().clone();
    assert_eq!(current.tip, result.save);
    let save = project.repo().read_commit(&current.tip).unwrap();
    let draft = project.repo().read_commit(&current.draft).unwrap();
    assert_eq!(draft.parents, vec![save.id.clone()]);
    assert_eq!(draft.tree, save.tree);
    assert_eq!(draft.message, "");
    assert!(draft.change_id.is_some());
    assert_ne!(draft.change_id, save.change_id);
    assert_eq!(
        git(dir.path(), &["rev-parse", "refs/paper/draft/main"]).trim(),
        current.draft.as_str()
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert_eq!(project.snapshot().unwrap().changes, vec![]);
    fsck(dir.path());
}

#[test]
fn new_modified_and_deleted_files_are_all_saved() {
    let (dir, mut project) = project_with(&[("a.tex", "a"), ("b.tex", "b")]);
    write(dir.path(), "a.tex", "a2");
    fs::remove_file(dir.path().join("b.tex")).unwrap();
    write(dir.path(), "sections/c.tex", "c");

    let result = save_all(&mut project, "Mixed edits");

    assert_eq!(
        result.saved,
        vec![
            change("a.tex", ChangeKind::Modified),
            change("b.tex", ChangeKind::Deleted),
            change("sections/c.tex", ChangeKind::Added),
        ]
    );
    assert_eq!(
        git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]),
        "a.tex\nsections/c.tex\n"
    );
    assert_eq!(git(dir.path(), &["show", "main:a.tex"]), "a2");
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    fsck(dir.path());
}

#[test]
fn saving_only_some_paths_keeps_the_rest_in_the_new_draft() {
    let (dir, mut project) = project_with(&[("a.tex", "a"), ("b.tex", "b")]);
    write(dir.path(), "a.tex", "a2");
    write(dir.path(), "b.tex", "b2");
    write(dir.path(), "c.tex", "c");

    let result = saved(
        project
            .save(&request("Just a", PathSelection::Only(paths(&["a.tex"]))))
            .unwrap(),
    );

    assert_eq!(result.saved, vec![change("a.tex", ChangeKind::Modified)]);
    assert_eq!(
        result.remaining,
        vec![
            change("b.tex", ChangeKind::Modified),
            change("c.tex", ChangeKind::Added),
        ]
    );
    assert_eq!(
        git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]),
        "a.tex\nb.tex\n"
    );
    assert_eq!(git(dir.path(), &["show", "main:a.tex"]), "a2");
    assert_eq!(git(dir.path(), &["show", "main:b.tex"]), "b");
    assert_eq!(
        git(dir.path(), &["status", "--porcelain"]),
        " M b.tex\n?? c.tex\n"
    );
    assert_eq!(
        draft_files(&project),
        vec!["a.tex".to_owned(), "b.tex".to_owned(), "c.tex".to_owned()]
    );
    let draft = project
        .repo()
        .read_commit(&project.view().current().draft)
        .unwrap();
    assert_eq!(draft.parents, vec![result.save.clone()]);
    assert_eq!(
        project.snapshot().unwrap().changes,
        vec![
            change("b.tex", ChangeKind::Modified),
            change("c.tex", ChangeKind::Added),
        ]
    );
    fsck(dir.path());
}

#[test]
fn saving_only_a_deletion_removes_just_that_file() {
    let (dir, mut project) = project_with(&[("a.tex", "a"), ("b.tex", "b")]);
    fs::remove_file(dir.path().join("a.tex")).unwrap();
    write(dir.path(), "b.tex", "b2");

    let result = saved(
        project
            .save(&request("Drop a", PathSelection::Only(paths(&["a.tex"]))))
            .unwrap(),
    );

    assert_eq!(result.saved, vec![change("a.tex", ChangeKind::Deleted)]);
    assert_eq!(
        git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]),
        "b.tex\n"
    );
    assert_eq!(git(dir.path(), &["show", "main:b.tex"]), "b");
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), " M b.tex\n");
    fsck(dir.path());
}

#[test]
fn a_directory_path_selects_every_changed_file_under_it() {
    for directory in ["sections", "sections/"] {
        let (dir, mut project) = project_with(&[
            ("main.tex", "m"),
            ("sections/intro.tex", "i"),
            ("sections/deep/method.tex", "x"),
        ]);
        write(dir.path(), "main.tex", "m2");
        write(dir.path(), "sections/intro.tex", "i2");
        write(dir.path(), "sections/deep/method.tex", "x2");
        write(dir.path(), "sections/new.tex", "n");

        let result = saved(
            project
                .save(&request(
                    "Sections",
                    PathSelection::Only(paths(&[directory])),
                ))
                .unwrap(),
        );

        assert_eq!(
            result.saved,
            vec![
                change("sections/deep/method.tex", ChangeKind::Modified),
                change("sections/intro.tex", ChangeKind::Modified),
                change("sections/new.tex", ChangeKind::Added),
            ],
            "directory spelled {directory:?}"
        );
        assert_eq!(
            result.remaining,
            vec![change("main.tex", ChangeKind::Modified)]
        );
        assert_eq!(git(dir.path(), &["status", "--porcelain"]), " M main.tex\n");
        fsck(dir.path());
    }
}

#[test]
fn a_directory_path_only_matches_whole_path_components() {
    let (dir, mut project) = project_with(&[("sections/intro.tex", "i")]);
    write(dir.path(), "sections/intro.tex", "i2");
    project.snapshot().unwrap();
    let before = stored(dir.path(), &project);

    let err = project
        .save(&request("Nope", PathSelection::Only(paths(&["sec"]))))
        .unwrap_err();

    assert!(matches!(err, SaveError::NoChangesAt { .. }), "{err:?}");
    assert_eq!(stored(dir.path(), &project), before);
    fsck(dir.path());
}

#[test]
fn all_but_saves_everything_except_the_matches() {
    let (dir, mut project) = project_with(&[
        ("main.tex", "m"),
        ("sections/intro.tex", "i"),
        ("notes.txt", "n"),
    ]);
    write(dir.path(), "main.tex", "m2");
    write(dir.path(), "sections/intro.tex", "i2");
    write(dir.path(), "notes.txt", "n2");

    let result = saved(
        project
            .save(&request(
                "All but notes",
                PathSelection::AllBut(paths(&["notes.txt", "sections"])),
            ))
            .unwrap(),
    );

    assert_eq!(result.saved, vec![change("main.tex", ChangeKind::Modified)]);
    assert_eq!(
        result.remaining,
        vec![
            change("notes.txt", ChangeKind::Modified),
            change("sections/intro.tex", ChangeKind::Modified),
        ]
    );
    assert_eq!(
        git(dir.path(), &["status", "--porcelain"]),
        " M notes.txt\n M sections/intro.tex\n"
    );
    fsck(dir.path());
}

#[test]
fn an_empty_selection_is_nothing_to_save_and_writes_nothing() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    write(dir.path(), "a.tex", "a2");
    project.snapshot().unwrap();
    let before = stored(dir.path(), &project);

    let outcome = project
        .save(&request(
            "Everything but a",
            PathSelection::AllBut(paths(&["a.tex"])),
        ))
        .unwrap();

    assert_eq!(outcome, SaveOutcome::NothingToSave);
    assert_eq!(stored(dir.path(), &project), before);
    fsck(dir.path());
}

#[test]
fn paths_matching_no_changed_file_are_listed_and_nothing_is_written() {
    let (dir, mut project) = project_with(&[("a.tex", "a"), ("b.tex", "b")]);
    write(dir.path(), "a.tex", "a2");
    project.snapshot().unwrap();
    let before = stored(dir.path(), &project);

    let err = project
        .save(&request(
            "Wrong paths",
            PathSelection::Only(paths(&["a.tex", "missing.tex", "b.tex", "gone/dir"])),
        ))
        .unwrap_err();

    let SaveError::NoChangesAt {
        paths: mut unmatched,
    } = err
    else {
        panic!("expected NoChangesAt, got {err:?}");
    };
    unmatched.sort();
    // b.tex exists but has no unsaved changes, so it matches nothing either.
    assert_eq!(unmatched, vec!["b.tex", "gone/dir", "missing.tex"]);
    assert_eq!(stored(dir.path(), &project), before);

    let err = project
        .save(&request(
            "Wrong exclusion",
            PathSelection::AllBut(paths(&["typo.tex"])),
        ))
        .unwrap_err();
    assert!(
        matches!(&err, SaveError::NoChangesAt { paths } if *paths == vec!["typo.tex"]),
        "{err:?}"
    );
    assert_eq!(stored(dir.path(), &project), before);
    fsck(dir.path());
}

#[test]
fn an_empty_or_blank_message_is_refused_and_nothing_is_written() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    write(dir.path(), "a.tex", "a2");
    let before = stored(dir.path(), &project);

    for message in ["", "   ", " \n\t \n"] {
        let err = project
            .save(&request(message, PathSelection::All))
            .unwrap_err();
        assert!(
            matches!(err, SaveError::MissingMessage),
            "{message:?}: {err:?}"
        );
        assert_eq!(stored(dir.path(), &project), before, "{message:?}");
    }
    fsck(dir.path());
}

#[test]
fn the_message_is_checked_before_looking_for_changes() {
    let (_dir, mut project) = project_with(&[("a.tex", "a")]);
    let err = project.save(&request("", PathSelection::All)).unwrap_err();
    assert!(matches!(err, SaveError::MissingMessage), "{err:?}");
}

#[test]
fn a_commit_made_with_plain_git_on_the_line_is_refused() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    write(dir.path(), "a.tex", "a2");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "--quiet", "-m", "Plain git"]);
    write(dir.path(), "b.tex", "b");
    let before = stored(dir.path(), &project);

    let err = project
        .save(&request("Too late", PathSelection::All))
        .unwrap_err();

    assert!(
        matches!(&err, SaveError::OutOfSyncWithGit { line: l } if *l == line("main")),
        "{err:?}"
    );
    assert_eq!(stored(dir.path(), &project), before);
    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "main"]),
        "Plain git\nStart project\n"
    );
    fsck(dir.path());
}

#[test]
fn head_attached_to_another_branch_is_refused() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    git(dir.path(), &["checkout", "--quiet", "-b", "side"]);
    write(dir.path(), "a.tex", "a2");
    let before = stored(dir.path(), &project);

    let err = project
        .save(&request("On the wrong branch", PathSelection::All))
        .unwrap_err();

    assert!(
        matches!(&err, SaveError::HeadOnAnotherBranch { found: Some(b), .. } if b == "side"),
        "{err:?}"
    );
    assert!(err.to_string().contains("git switch main"), "{err}");
    assert_eq!(stored(dir.path(), &project), before);
    fsck(dir.path());
}

#[test]
fn git_moving_the_line_is_reported_before_a_missing_message() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    git(
        dir.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "Plain git"],
    );

    let err = project.save(&request("", PathSelection::All)).unwrap_err();

    assert!(matches!(err, SaveError::OutOfSyncWithGit { .. }), "{err:?}");
}

#[test]
fn two_saves_in_a_row_give_a_linear_history() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    write(dir.path(), "a.tex", "a2");
    let first = save_all(&mut project, "First");
    write(dir.path(), "a.tex", "a3");
    let second = save_all(&mut project, "Second");

    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "main"]),
        "Second\nFirst\nStart project\n"
    );
    let second_commit = project.repo().read_commit(&second.save).unwrap();
    assert_eq!(second_commit.parents, vec![first.save.clone()]);
    assert_ne!(first.change_id, second.change_id);
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert_eq!(git(dir.path(), &["show", "main:a.tex"]), "a3");
    fsck(dir.path());
}

#[test]
fn opening_the_project_after_a_save_sees_the_new_tip_and_draft() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    write(dir.path(), "a.tex", "a2");
    let result = save_all(&mut project, "Edit");

    let reopened = Project::open(dir.path(), identity()).unwrap();

    assert_eq!(reopened.view(), project.view());
    assert_eq!(reopened.view().current().tip, result.save);
    let draft = reopened
        .repo()
        .read_commit(&reopened.view().current().draft)
        .unwrap();
    assert_eq!(draft.parents, vec![result.save]);
    fsck(dir.path());
}

#[test]
fn a_reopened_project_can_save_again() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    write(dir.path(), "a.tex", "a2");
    save_all(&mut project, "First");
    let mut reopened = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.tex", "a3");

    save_all(&mut reopened, "Second");

    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "main"]),
        "Second\nFirst\nStart project\n"
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    fsck(dir.path());
}

#[test]
fn a_file_turning_into_a_directory_takes_its_deletion_along() {
    let (dir, mut project) = project_with(&[("notes", "text"), ("x.tex", "x")]);
    fs::remove_file(dir.path().join("notes")).unwrap();
    write(dir.path(), "notes/a.tex", "a");
    write(dir.path(), "x.tex", "x2");

    let result = saved(
        project
            .save(&request(
                "Notes become a folder",
                PathSelection::Only(paths(&["notes/a.tex"])),
            ))
            .unwrap(),
    );

    assert_eq!(
        result.saved,
        vec![
            change("notes", ChangeKind::Deleted),
            change("notes/a.tex", ChangeKind::Added),
        ]
    );
    assert_eq!(
        result.remaining,
        vec![change("x.tex", ChangeKind::Modified)]
    );
    assert_eq!(
        git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]),
        "notes/a.tex\nx.tex\n"
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), " M x.tex\n");
    fsck(dir.path());
}

#[test]
fn a_directory_turning_into_a_file_takes_its_deletions_along() {
    let (dir, mut project) = project_with(&[("notes/a.tex", "a"), ("x.tex", "x")]);
    fs::remove_dir_all(dir.path().join("notes")).unwrap();
    write(dir.path(), "notes", "text");
    write(dir.path(), "x.tex", "x2");

    let result = saved(
        project
            .save(&request(
                "Notes become a file",
                PathSelection::AllBut(paths(&["notes/a.tex", "x.tex"])),
            ))
            .unwrap(),
    );

    assert_eq!(
        result.saved,
        vec![
            change("notes", ChangeKind::Added),
            change("notes/a.tex", ChangeKind::Deleted),
        ]
    );
    assert_eq!(
        result.remaining,
        vec![change("x.tex", ChangeKind::Modified)]
    );
    assert_eq!(
        git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]),
        "notes\nx.tex\n"
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), " M x.tex\n");
    fsck(dir.path());
}

// ---------------------------------------------------------------- PV-007 hardening

fn ls_main(dir: &Path) -> String {
    git(dir, &["ls-tree", "-r", "--name-only", "main"])
}

#[test]
fn saving_a_deletion_inside_a_folder_that_became_a_file_forces_nothing() {
    let (dir, mut project) =
        project_with(&[("notes/a.tex", "a"), ("notes/b.tex", "b"), ("x.tex", "x")]);
    fs::remove_dir_all(dir.path().join("notes")).unwrap();
    write(dir.path(), "notes", "text");

    let result = saved(
        project
            .save(&request(
                "Drop a",
                PathSelection::Only(paths(&["notes/a.tex"])),
            ))
            .unwrap(),
    );

    assert_eq!(
        result.saved,
        vec![change("notes/a.tex", ChangeKind::Deleted)]
    );
    assert_eq!(
        result.remaining,
        vec![
            change("notes", ChangeKind::Added),
            change("notes/b.tex", ChangeKind::Deleted),
        ]
    );
    assert_eq!(ls_main(dir.path()), "notes/b.tex\nx.tex\n");
    fsck(dir.path());
}

#[test]
fn saving_the_file_that_replaced_a_folder_takes_every_deletion_under_it() {
    let (dir, mut project) =
        project_with(&[("notes/a.tex", "a"), ("notes/b.tex", "b"), ("x.tex", "x")]);
    fs::remove_dir_all(dir.path().join("notes")).unwrap();
    write(dir.path(), "notes", "text");

    let result = saved(
        project
            .save(&request(
                "Notes file",
                PathSelection::Only(paths(&["notes"])),
            ))
            .unwrap(),
    );

    assert_eq!(
        result.saved,
        vec![
            change("notes", ChangeKind::Added),
            change("notes/a.tex", ChangeKind::Deleted),
            change("notes/b.tex", ChangeKind::Deleted),
        ]
    );
    assert_eq!(result.remaining, vec![]);
    assert_eq!(ls_main(dir.path()), "notes\nx.tex\n");
    fsck(dir.path());
}

#[test]
fn saving_one_file_of_a_new_folder_takes_only_the_old_file_it_replaces() {
    let (dir, mut project) = project_with(&[("notes", "text")]);
    fs::remove_file(dir.path().join("notes")).unwrap();
    write(dir.path(), "notes/a.tex", "a");
    write(dir.path(), "notes/b.tex", "b");

    let result = saved(
        project
            .save(&request(
                "First note",
                PathSelection::Only(paths(&["notes/a.tex"])),
            ))
            .unwrap(),
    );

    assert_eq!(
        result.saved,
        vec![
            change("notes", ChangeKind::Deleted),
            change("notes/a.tex", ChangeKind::Added),
        ]
    );
    assert_eq!(
        result.remaining,
        vec![change("notes/b.tex", ChangeKind::Added)]
    );
    assert_eq!(ls_main(dir.path()), "notes/a.tex\n");
    fsck(dir.path());
}

#[test]
fn all_but_still_saves_an_excluded_path_that_a_selected_change_forces() {
    let (dir, mut project) =
        project_with(&[("notes/a.tex", "a"), ("notes/b.tex", "b"), ("x.tex", "x")]);
    fs::remove_dir_all(dir.path().join("notes")).unwrap();
    write(dir.path(), "notes", "text");
    write(dir.path(), "x.tex", "x2");

    let result = saved(
        project
            .save(&request(
                "Notes file",
                PathSelection::AllBut(paths(&["notes/b.tex", "x.tex"])),
            ))
            .unwrap(),
    );

    assert_eq!(
        result.saved,
        vec![
            change("notes", ChangeKind::Added),
            change("notes/a.tex", ChangeKind::Deleted),
            change("notes/b.tex", ChangeKind::Deleted),
        ]
    );
    assert_eq!(
        result.remaining,
        vec![change("x.tex", ChangeKind::Modified)]
    );
    assert_eq!(ls_main(dir.path()), "notes\nx.tex\n");
    fsck(dir.path());
}

#[test]
fn a_git_reset_of_the_line_is_refused_without_claiming_new_commits() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    write(dir.path(), "a.tex", "a2");
    save_all(&mut project, "Second");
    git(dir.path(), &["reset", "--quiet", "--hard", "HEAD~1"]);
    write(dir.path(), "b.tex", "b");
    let before = stored(dir.path(), &project);

    let err = project
        .save(&request("After reset", PathSelection::All))
        .unwrap_err();

    assert!(
        matches!(&err, SaveError::OutOfSyncWithGit { line: l } if *l == line("main")),
        "{err:?}"
    );
    let message = err.to_string();
    assert!(!message.contains("new commits"), "{message}");
    assert!(message.contains("main"), "{message}");
    assert_eq!(stored(dir.path(), &project), before);
    fsck(dir.path());
}

#[test]
fn a_deleted_line_branch_is_out_of_sync_with_git() {
    let (dir, mut project) = project_with(&[("a.tex", "a")]);
    git(dir.path(), &["update-ref", "-d", "refs/heads/main"]);
    write(dir.path(), "a.tex", "a2");
    let before = stored(dir.path(), &project);

    let err = project
        .save(&request("No branch", PathSelection::All))
        .unwrap_err();

    assert!(
        matches!(&err, SaveError::OutOfSyncWithGit { line: l } if *l == line("main")),
        "{err:?}"
    );
    let message = err.to_string();
    assert!(!message.contains("new commits"), "{message}");
    assert!(message.contains("main"), "{message}");
    assert_eq!(stored(dir.path(), &project), before);
}

#[cfg(unix)]
#[test]
fn saving_an_executable_bit_change_keeps_mode_100755() {
    use std::os::unix::fs::PermissionsExt;
    let chmod = |dir: &Path, path: &str| {
        let file = dir.join(path);
        let mut perms = fs::metadata(&file).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(file, perms).unwrap();
    };

    // Fully.
    let (dir, mut project) = project_with(&[("build.sh", "make"), ("b.tex", "b")]);
    chmod(dir.path(), "build.sh");
    let result = save_all(&mut project, "Make it executable");
    assert_eq!(result.saved, vec![change("build.sh", ChangeKind::Modified)]);
    assert!(git(dir.path(), &["ls-tree", "main"]).contains("100755 blob"));
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    fsck(dir.path());

    // Partially.
    let (dir, mut project) = project_with(&[("build.sh", "make"), ("b.tex", "b")]);
    chmod(dir.path(), "build.sh");
    write(dir.path(), "b.tex", "b2");
    let result = saved(
        project
            .save(&request(
                "Executable only",
                PathSelection::Only(paths(&["build.sh"])),
            ))
            .unwrap(),
    );
    assert_eq!(result.saved, vec![change("build.sh", ChangeKind::Modified)]);
    assert_eq!(
        git(dir.path(), &["ls-tree", "main", "build.sh"])
            .split_whitespace()
            .next(),
        Some("100755")
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), " M b.tex\n");
    fsck(dir.path());
}

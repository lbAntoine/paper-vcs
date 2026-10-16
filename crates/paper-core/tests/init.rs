//! `Project::init` and `Project::open`, cross-checked with the real `git` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::name::LineName;
use paper_core::project::{InitError, OpenError, Project};
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

fn init(path: &Path) -> Result<(Project, paper_core::project::Initialized), InitError> {
    Project::init(path, identity(), &SnapshotOptions::default())
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

fn line(name: &str) -> LineName {
    name.parse().unwrap()
}

fn draft_files(project: &Project, name: &str) -> Vec<String> {
    let draft = &project.view().line(&line(name)).unwrap().draft;
    let tree = project.repo().read_commit(draft).unwrap().tree;
    project
        .repo()
        .tree_files(&tree)
        .unwrap()
        .into_keys()
        .collect()
}

#[test]
fn init_in_an_empty_folder_creates_a_first_save_and_an_empty_draft() {
    let dir = tempfile::tempdir().unwrap();
    let (project, initialized) = init(dir.path()).unwrap();
    assert!(!initialized.adopted);
    // PV-004: the generic profile's .gitignore and paper.toml are in the first save.
    assert_eq!(initialized.tracked_files, 2);

    let view = project.view();
    assert_eq!(view.current_line(), &LineName::main());
    let tip = project.repo().read_commit(&view.current().tip).unwrap();
    let draft = project.repo().read_commit(&view.current().draft).unwrap();
    assert_eq!(tip.message, "Start project");
    assert!(tip.parents.is_empty());
    assert!(tip.change_id.is_some());
    assert_eq!(draft.parents, vec![tip.id.clone()]);
    assert_eq!(draft.tree, tip.tree);
    assert_ne!(draft.change_id, tip.change_id);

    assert_eq!(
        fs::read_to_string(dir.path().join(".git/paper/version")).unwrap(),
        "1\n"
    );
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn init_with_existing_files_saves_them_and_leaves_git_clean() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "\\documentclass{article}");
    write(dir.path(), "sections/intro.tex", "Intro");
    write(dir.path(), ".gitignore", "*.aux\n");
    write(dir.path(), "main.aux", "junk");

    let (_project, initialized) = init(dir.path()).unwrap();
    // PV-004: paper.toml joins the first save (the .gitignore now also holds the profile block).
    assert_eq!(initialized.tracked_files, 4);
    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "main"]),
        "Start project\n"
    );
    assert_eq!(
        git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]),
        ".gitignore\nmain.tex\npaper.toml\nsections/intro.tex\n"
    );
    assert_eq!(
        git(dir.path(), &["symbolic-ref", "HEAD"]),
        "refs/heads/main\n"
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

#[test]
fn init_creates_the_folder_if_needed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new-paper");
    init(&path).unwrap();
    assert!(path.join(".git/paper/version").exists());
}

#[test]
fn init_twice_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path()).unwrap();
    assert!(matches!(
        init(dir.path()),
        Err(InitError::AlreadyInitialized { .. })
    ));
}

#[test]
fn init_inside_another_repository_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet"]);
    let sub = dir.path().join("chapter-2");
    fs::create_dir_all(&sub).unwrap();
    assert!(matches!(
        init(&sub),
        Err(InitError::InsideAnotherRepository { .. })
    ));
}

#[test]
fn adopting_keeps_history_and_turns_uncommitted_work_into_the_draft() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet", "--initial-branch=main"]);
    write(dir.path(), "paper.tex", "v1");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "--quiet", "-m", "First draft"]);
    git(dir.path(), &["branch", "journal-version"]);
    let main_before = git(dir.path(), &["rev-parse", "main"]);
    write(dir.path(), "paper.tex", "v2, not committed");
    write(dir.path(), "notes.md", "untracked");

    let (project, initialized) = init(dir.path()).unwrap();
    assert!(initialized.adopted);
    assert!(initialized.skipped_branches.is_empty());

    // Nothing rewritten: git history and branches are untouched.
    assert_eq!(git(dir.path(), &["rev-parse", "main"]), main_before);
    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "main"]),
        "First draft\n"
    );

    let view = project.view();
    assert_eq!(view.current_line(), &line("main"));
    assert_eq!(view.lines().count(), 2);
    assert_eq!(view.current().tip.as_str(), main_before.trim());
    // PV-004: the profile block and paper.toml are unsaved changes in the draft.
    assert_eq!(
        draft_files(&project, "main"),
        [".gitignore", "notes.md", "paper.tex", "paper.toml"]
    );
    assert_eq!(draft_files(&project, "journal-version"), ["paper.tex"]);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn adopting_a_repository_without_commits_starts_on_its_current_branch() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet", "--initial-branch=thesis"]);
    write(dir.path(), "chapter1.md", "# Introduction");

    let (project, initialized) = init(dir.path()).unwrap();
    assert!(initialized.adopted);
    assert_eq!(project.view().current_line(), &line("thesis"));
    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "thesis"]),
        "Start project\n"
    );
}

#[test]
fn adopting_skips_branches_that_cannot_be_line_names() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet", "--initial-branch=main"]);
    write(dir.path(), "a.txt", "a");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "--quiet", "-m", "a"]);
    git(dir.path(), &["branch", "last"]);

    let (project, initialized) = init(dir.path()).unwrap();
    assert_eq!(initialized.skipped_branches, ["last"]);
    assert_eq!(project.view().lines().count(), 1);
}

#[test]
fn adopting_with_a_detached_head_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet", "--initial-branch=main"]);
    write(dir.path(), "a.txt", "a");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "--quiet", "-m", "a"]);
    git(dir.path(), &["checkout", "--quiet", "--detach"]);
    assert!(matches!(init(dir.path()), Err(InitError::DetachedHead)));
}

#[test]
fn open_reads_back_the_project_init_created() {
    let dir = tempfile::tempdir().unwrap();
    let (created, _) = init(dir.path()).unwrap();
    let opened = Project::open(dir.path(), identity()).unwrap();
    assert_eq!(opened.view(), created.view());
}

#[test]
fn open_refuses_a_plain_git_repository() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet"]);
    assert!(matches!(
        Project::open(dir.path(), identity()),
        Err(OpenError::NotAPaperProject { .. })
    ));
}

#[test]
fn an_interrupted_init_can_be_run_again() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path()).unwrap();
    // Simulate a crash before the version file, the last thing init writes.
    fs::remove_file(dir.path().join(".git/paper/version")).unwrap();
    assert!(matches!(
        Project::open(dir.path(), identity()),
        Err(OpenError::NotAPaperProject { .. })
    ));
    let (project, initialized) = init(dir.path()).unwrap();
    assert!(initialized.adopted);
    assert_eq!(project.view().current_line(), &LineName::main());
}

// ---- a damaged .git at the target (PV-014 item 5) ----

fn assert_damaged(target: &Path) {
    let error = init(target).unwrap_err();
    assert!(
        matches!(error, InitError::DamagedGit { .. }),
        "expected DamagedGit, got {error:?}"
    );
}

#[test]
fn an_empty_dot_git_at_the_target_is_damaged() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    assert_damaged(dir.path());
    assert!(!dir.path().join("paper.toml").exists());
}

#[test]
fn a_damaged_dot_git_inside_a_parent_repository_is_still_damaged_not_nested() {
    let parent = tempfile::tempdir().unwrap();
    git(parent.path(), &["init", "--quiet"]);
    let child = parent.path().join("child");
    fs::create_dir_all(child.join(".git")).unwrap();
    assert_damaged(&child);

    let file_child = parent.path().join("pointer");
    fs::create_dir(&file_child).unwrap();
    fs::write(file_child.join(".git"), "gitdir: ../no-such-gitdir\n").unwrap();
    assert_damaged(&file_child);
}

#[test]
fn a_dot_git_file_pointing_at_a_missing_gitdir_is_damaged_without_a_parent() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".git"), "gitdir: ./missing\n").unwrap();
    assert_damaged(dir.path());
}

#[test]
fn a_worktree_style_dot_git_file_still_adopts() {
    let main = tempfile::tempdir().unwrap();
    git(main.path(), &["init", "--quiet", "--initial-branch=main"]);
    fs::write(main.path().join("a.txt"), "a\n").unwrap();
    git(main.path(), &["add", "."]);
    git(main.path(), &["commit", "--quiet", "-m", "First"]);
    let holder = tempfile::tempdir().unwrap();
    let worktree = holder.path().join("wt");
    git(
        main.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "other",
            worktree.to_str().unwrap(),
        ],
    );
    assert!(worktree.join(".git").is_file());

    let (_project, initialized) = init(&worktree).unwrap();

    assert!(initialized.adopted);
}

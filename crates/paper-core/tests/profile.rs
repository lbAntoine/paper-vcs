//! Profiles in `Project::init_with_profile`, cross-checked with the real `git` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::profile::{Profile, gitignore};
use paper_core::project::{InitError, Initialized, Project};
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

fn init(path: &Path, choice: Option<&str>) -> Result<(Project, Initialized), InitError> {
    let profile = choice.map(|name| Profile::builtin(name).unwrap());
    Project::init_with_profile(path, identity(), &SnapshotOptions::default(), profile)
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

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

fn tree(dir: &Path, rev: &str) -> Vec<String> {
    git(dir, &["ls-tree", "-r", "--name-only", rev])
        .lines()
        .map(str::to_owned)
        .collect()
}

fn draft_files(project: &Project) -> Vec<String> {
    let draft = &project.view().current().draft;
    let tree = project.repo().read_commit(draft).unwrap().tree;
    project
        .repo()
        .tree_files(&tree)
        .unwrap()
        .into_keys()
        .collect()
}

fn latex_folder(dir: &Path) {
    write(dir, "main.tex", "\\documentclass{article}\n");
    write(dir, "sections/intro.tex", "Intro\n");
    write(dir, "main.aux", "junk");
    write(dir, "main.log", "junk");
    write(dir, "main.pdf", "%PDF compiled");
}

fn fsck(dir: &Path) {
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

fn block_start(name: &str) -> String {
    format!("# >>> paper-vcs profile: {name} >>>")
}

// ---- new project ----

#[test]
fn init_in_a_new_latex_folder_saves_the_block_and_paper_toml_and_none_of_the_junk() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());

    let (_project, initialized) = init(dir.path(), None).unwrap();

    assert_eq!(initialized.profile, "latex");
    assert!(initialized.detected);
    assert!(!initialized.adopted);
    assert_eq!(initialized.written, [".gitignore", "paper.toml"]);
    assert!(!initialized.tips.is_empty());
    assert_eq!(initialized.tracked_files, 5);
    assert_eq!(
        tree(dir.path(), "main"),
        [
            ".gitignore",
            "main.pdf",
            "main.tex",
            "paper.toml",
            "sections/intro.tex"
        ]
    );
    assert!(read(dir.path(), ".gitignore").contains(&block_start("latex")));
    assert!(read(dir.path(), "paper.toml").contains("profile = \"latex\""));
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    git(dir.path(), &["check-ignore", "main.aux"]);
    fsck(dir.path());
}

#[test]
fn init_in_a_new_folder_keeps_the_users_own_gitignore_lines() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());
    write(dir.path(), ".gitignore", "secret-notes.txt\n");
    write(dir.path(), "secret-notes.txt", "private");

    init(dir.path(), None).unwrap();

    let gitignore = read(dir.path(), ".gitignore");
    assert!(gitignore.starts_with("secret-notes.txt\n\n"), "{gitignore}");
    assert!(gitignore.contains(&block_start("latex")));
    assert!(!tree(dir.path(), "main").contains(&"secret-notes.txt".to_owned()));
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

#[test]
fn a_chosen_profile_beats_detection_and_is_reported_as_chosen() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());

    let (_project, initialized) = init(dir.path(), Some("markdown")).unwrap();

    assert_eq!(initialized.profile, "markdown");
    assert!(!initialized.detected);
    let gitignore = read(dir.path(), ".gitignore");
    assert!(gitignore.contains(&block_start("markdown")));
    assert!(!gitignore.contains(&block_start("latex")));
    assert!(read(dir.path(), "paper.toml").contains("profile = \"markdown\""));
    // markdown does not ignore LaTeX junk, so it is saved.
    assert!(tree(dir.path(), "main").contains(&"main.aux".to_owned()));
}

#[test]
fn detection_only_looks_at_files_the_walk_sees() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".gitignore", "*.tex\n");
    write(dir.path(), "main.tex", "ignored by the user");
    write(dir.path(), "notes.md", "# Notes\n");

    let (_project, initialized) = init(dir.path(), None).unwrap();

    assert_eq!(initialized.profile, "markdown");
    assert!(initialized.detected);
}

#[test]
fn an_empty_folder_gets_the_generic_profile() {
    let dir = tempfile::tempdir().unwrap();

    let (_project, initialized) = init(dir.path(), None).unwrap();

    assert_eq!(initialized.profile, "generic");
    assert!(initialized.detected);
    assert_eq!(tree(dir.path(), "main"), [".gitignore", "paper.toml"]);
    fsck(dir.path());
}

#[test]
fn a_malformed_block_stops_init_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());
    let broken = "# >>> paper-vcs profile: latex >>>\n*.aux\n";
    write(dir.path(), ".gitignore", broken);

    let error = init(dir.path(), None).unwrap_err();

    assert!(matches!(error, InitError::Gitignore(_)), "{error:?}");
    assert_eq!(read(dir.path(), ".gitignore"), broken);
    assert!(!dir.path().join("paper.toml").exists());
    assert!(!dir.path().join(".git/paper/version").exists());
}

// ---- adoption ----

fn committed_repo(dir: &Path) {
    git(dir, &["init", "--quiet", "--initial-branch=main"]);
    write(dir, "main.tex", "v1\n");
    write(
        dir,
        "build/out.aux",
        "committed before the profile existed\n",
    );
    git(dir, &["add", "."]);
    git(dir, &["commit", "--quiet", "-m", "First draft"]);
}

#[test]
fn adoption_puts_the_block_and_paper_toml_in_the_draft_and_leaves_history_alone() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    let log_before = git(dir.path(), &["log", "--format=%H %s", "--branches"]);

    let (project, initialized) = init(dir.path(), None).unwrap();

    assert!(initialized.adopted);
    assert_eq!(initialized.profile, "latex");
    assert!(initialized.detected);
    assert_eq!(initialized.written, [".gitignore", "paper.toml"]);
    assert_eq!(
        git(dir.path(), &["log", "--format=%H %s", "--branches"]),
        log_before
    );
    // Committed files that match the new ignores stay tracked.
    assert_eq!(tree(dir.path(), "main"), ["build/out.aux", "main.tex"]);
    let draft = draft_files(&project);
    for expected in [".gitignore", "paper.toml", "build/out.aux", "main.tex"] {
        assert!(draft.contains(&expected.to_owned()), "{draft:?}");
    }
    // The two files are unsaved changes, as far as git can tell too.
    let status = git(dir.path(), &["status", "--porcelain"]);
    assert!(status.contains(".gitignore"), "{status}");
    assert!(status.contains("paper.toml"), "{status}");
    fsck(dir.path());
}

#[test]
fn adoption_keeps_an_existing_profile_in_paper_toml_and_does_not_rewrite_the_file() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    let toml = "# chosen by my supervisor\nprofile = \"typst\"\n";
    write(dir.path(), "paper.toml", toml);

    let (_project, initialized) = init(dir.path(), None).unwrap();

    assert_eq!(initialized.profile, "typst");
    assert_eq!(initialized.written, [".gitignore"]);
    assert_eq!(read(dir.path(), "paper.toml"), toml);
    assert!(read(dir.path(), ".gitignore").contains(&block_start("typst")));
}

#[test]
fn adoption_agreeing_with_paper_toml_is_fine() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    write(dir.path(), "paper.toml", "profile = \"typst\"\n");

    let (_project, initialized) = init(dir.path(), Some("typst")).unwrap();

    assert_eq!(initialized.profile, "typst");
}

#[test]
fn adoption_with_a_conflicting_choice_is_an_error_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    let toml = "profile = \"typst\"\n";
    write(dir.path(), "paper.toml", toml);

    let error = init(dir.path(), Some("latex")).unwrap_err();

    assert!(
        matches!(&error, InitError::ProfileConflict { requested, existing }
            if requested == "latex" && existing == "typst"),
        "{error:?}"
    );
    assert_eq!(read(dir.path(), "paper.toml"), toml);
    assert!(!dir.path().join(".gitignore").exists());
    assert!(!dir.path().join(".git/paper/version").exists());
}

#[test]
fn adoption_adds_the_profile_to_a_paper_toml_that_has_none_keeping_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    write(
        dir.path(),
        "paper.toml",
        "# my settings\n[large_files]\nwarn_mb = 5\n",
    );

    let (_project, initialized) = init(dir.path(), Some("markdown")).unwrap();

    assert_eq!(initialized.written, [".gitignore", "paper.toml"]);
    let toml = read(dir.path(), "paper.toml");
    assert!(toml.contains("# my settings"), "{toml}");
    assert!(toml.contains("warn_mb = 5"), "{toml}");
    assert!(toml.contains("profile = \"markdown\""), "{toml}");
}

// ---- gitignore::apply_to_file against real git ----

#[test]
fn apply_to_file_keeps_git_happy_and_ignores_the_junk() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet"]);
    write(dir.path(), ".gitignore", "mine.txt\n");
    let latex = Profile::builtin("latex").unwrap();
    let typst = Profile::builtin("typst").unwrap();

    assert!(gitignore::apply_to_file(dir.path(), latex).unwrap());
    git(dir.path(), &["check-ignore", "main.aux"]);
    git(dir.path(), &["check-ignore", "mine.txt"]);
    assert!(read(dir.path(), ".gitignore").starts_with("mine.txt\n"));

    assert!(gitignore::apply_to_file(dir.path(), typst).unwrap());
    let text = read(dir.path(), ".gitignore");
    assert!(text.contains(&block_start("typst")), "{text}");
    assert!(!text.contains(&block_start("latex")), "{text}");
    assert!(text.starts_with("mine.txt\n"));
    assert!(!gitignore::apply_to_file(dir.path(), typst).unwrap());
}

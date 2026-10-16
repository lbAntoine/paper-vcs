//! `configured_identity` against configs written by the real `git` binary.
//!
//! The process environment is shared between tests, so these tests never change it. They
//! read the repository's own config (which wins over global config) and, for the global
//! case, run the check in a child process with an isolated environment.

use std::path::Path;
use std::process::Command;

use paper_git::configured_identity;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet"]);
    dir
}

#[test]
fn reads_name_and_email_from_the_repository_config() {
    let dir = repo();
    git(dir.path(), &["config", "user.name", "Mina Park"]);
    git(dir.path(), &["config", "user.email", "mina@uni.example"]);

    let identity = configured_identity(dir.path()).unwrap().unwrap();

    assert_eq!(identity.name, "Mina Park");
    assert_eq!(identity.email, "mina@uni.example");
}

#[test]
fn reads_the_config_of_the_repository_enclosing_a_subfolder() {
    let dir = repo();
    git(dir.path(), &["config", "user.name", "Mina Park"]);
    git(dir.path(), &["config", "user.email", "mina@uni.example"]);
    let sub = dir.path().join("sections");
    std::fs::create_dir(&sub).unwrap();

    let identity = configured_identity(&sub).unwrap().unwrap();

    assert_eq!(identity.name, "Mina Park");
}

#[test]
fn reads_the_global_config_named_by_git_config_global() {
    // Child process: `GIT_CONFIG_GLOBAL` points at a temp file with the identity.
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("gitconfig");
    std::fs::write(
        &global,
        "[user]\n\tname = Global Gina\n\temail = gina@lab.example\n",
    )
    .unwrap();
    let project = dir.path().join("work");
    std::fs::create_dir(&project).unwrap();

    let identity = identity_in_child(&project, &global);

    assert_eq!(identity.as_deref(), Some("Global Gina <gina@lab.example>"));
}

#[test]
fn returns_none_when_the_email_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("gitconfig");
    std::fs::write(&global, "[user]\n\tname = Only Name\n").unwrap();

    assert_eq!(identity_in_child(dir.path(), &global), None);
}

#[test]
fn returns_none_when_the_name_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("gitconfig");
    std::fs::write(&global, "[user]\n\temail = only@email.example\n").unwrap();

    assert_eq!(identity_in_child(dir.path(), &global), None);
}

#[test]
fn returns_none_when_nothing_is_configured() {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("gitconfig");
    std::fs::write(&global, "").unwrap();

    assert_eq!(identity_in_child(dir.path(), &global), None);
}

#[test]
fn the_repository_config_wins_over_the_global_one() {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("gitconfig");
    std::fs::write(&global, "[user]\n\tname = Global\n\temail = g@x.example\n").unwrap();
    let project = dir.path().join("work");
    std::fs::create_dir(&project).unwrap();
    git(&project, &["init", "--quiet"]);
    git(&project, &["config", "user.name", "Local Lou"]);
    git(&project, &["config", "user.email", "lou@lab.example"]);

    let identity = identity_in_child(&project, &global);

    assert_eq!(identity.as_deref(), Some("Local Lou <lou@lab.example>"));
}

#[test]
fn a_damaged_repository_is_an_error_not_a_silent_fallback() {
    let dir = tempfile::tempdir().unwrap();
    // A `.git` file that points nowhere: there is a repository here, but it can't be opened.
    std::fs::write(dir.path().join(".git"), "gitdir: does/not/exist\n").unwrap();

    assert!(configured_identity(dir.path()).is_err());
}

/// Runs this test binary again as a child with an isolated environment; the child body
/// is `child_prints_identity`.
fn identity_in_child(path: &Path, global: &Path) -> Option<String> {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_prints_identity", "--nocapture"])
        .env("PAPER_TEST_CHILD_PATH", path)
        .env("GIT_CONFIG_GLOBAL", global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", path)
        .env("XDG_CONFIG_HOME", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("IDENTITY:"))
        .and_then(|rest| (rest != "none").then(|| rest.to_owned()))
}

/// Not a test of its own: the body the child process runs. It does nothing unless the
/// parent set `PAPER_TEST_CHILD_PATH`.
#[test]
fn child_prints_identity() {
    let Some(path) = std::env::var_os("PAPER_TEST_CHILD_PATH") else {
        return;
    };
    match configured_identity(Path::new(&path)).unwrap() {
        Some(identity) => println!("IDENTITY:{} <{}>", identity.name, identity.email),
        None => println!("IDENTITY:none"),
    }
}

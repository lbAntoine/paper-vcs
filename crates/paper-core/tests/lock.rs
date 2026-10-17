//! The project lock: one paper command at a time (03-architecture.md § Command lifecycle).
//!
//! Holders are `Project` values, in this process, in a thread, or in a child process (this test
//! binary re-run with `PAPER_TEST_LOCK_HOLDER` set). Waits are short and nothing asserts on
//! timing beyond "eventually succeeds / fails".

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use paper_core::ErrorKind;
use paper_core::PaperError;
use paper_core::project::{InitError, LockOptions, OpenError, Project};
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;

const HOLDER_ENV: &str = "PAPER_TEST_LOCK_HOLDER";
const SHORT: Duration = Duration::from_millis(300);

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

fn short() -> LockOptions {
    LockOptions { wait: SHORT }
}

fn new_project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    std::fs::write(path.join("notes.md"), "hello\n").unwrap();
    let (project, _) = Project::init(&path, identity(), &SnapshotOptions::default()).unwrap();
    drop(project);
    (dir, path)
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap()
}

/// Not a test: the body of the child process in the crash test. It takes the lock, says so on
/// stdout, and waits until its stdin closes (or a generous limit passes).
#[test]
fn lock_holder_helper() {
    let Some(path) = std::env::var_os(HOLDER_ENV) else {
        return;
    };
    let project = Project::open(Path::new(&path), identity()).unwrap();
    println!("LOCKED");
    let mut sink = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut sink);
    drop(project);
}

#[test]
fn the_default_wait_is_five_seconds() {
    assert_eq!(LockOptions::default().wait, Duration::from_secs(5));
}

#[test]
fn opening_a_held_project_fails_busy_after_the_wait_and_works_once_it_is_dropped() {
    let (_dir, path) = new_project();
    let first = Project::open_with(&path, identity(), &short()).unwrap();

    let second = Project::open_with(&path, identity(), &short());
    assert!(
        matches!(second, Err(OpenError::Busy { .. })),
        "expected Busy, got {second:?}"
    );

    drop(first);
    Project::open_with(&path, identity(), &short()).expect("free again after the drop");
}

#[test]
fn the_busy_error_names_the_holder_process() {
    let (_dir, path) = new_project();
    let _first = Project::open_with(&path, identity(), &short()).unwrap();

    let Err(OpenError::Busy { holder }) = Project::open_with(&path, identity(), &short()) else {
        panic!("expected Busy");
    };

    assert_eq!(holder, Some(std::process::id()));
}

#[test]
fn busy_converts_to_a_three_part_project_error_with_the_pid() {
    let (_dir, path) = new_project();
    let _first = Project::open_with(&path, identity(), &short()).unwrap();
    let Err(error) = Project::open_with(&path, identity(), &short()) else {
        panic!("expected Busy");
    };

    let error = PaperError::from(error);

    assert_eq!(error.kind, ErrorKind::Project);
    assert_eq!(
        error.message,
        "Another paper command is running in this project"
    );
    assert!(!error.why.is_empty());
    assert!(!error.next.is_empty());
    let pid = std::process::id().to_string();
    let text = format!("{} {} {:?}", error.message, error.why, error.next);
    assert!(text.contains(&pid), "the pid is named: {text}");
    assert!(
        !text.to_lowercase().contains("delete"),
        "never advise deleting the lock file: {text}"
    );
}

#[test]
fn init_on_a_held_project_is_refused_busy() {
    let (_dir, path) = new_project();
    let _held = Project::open_with(&path, identity(), &short()).unwrap();

    let result = Project::init_with(
        &path,
        identity(),
        &SnapshotOptions::default(),
        None,
        &short(),
    );

    assert!(
        matches!(result, Err(InitError::Busy { .. })),
        "expected Busy (not AlreadyInitialized), got {:?}",
        result.map(|_| ())
    );
}

#[test]
fn init_converts_busy_to_the_same_project_error() {
    let (_dir, path) = new_project();
    let _held = Project::open_with(&path, identity(), &short()).unwrap();
    let Err(error) = Project::init_with(
        &path,
        identity(),
        &SnapshotOptions::default(),
        None,
        &short(),
    ) else {
        panic!("expected an error");
    };

    let error = PaperError::from(error);

    assert_eq!(error.kind, ErrorKind::Project);
    assert_eq!(
        error.message,
        "Another paper command is running in this project"
    );
}

#[test]
fn init_keeps_the_lock_until_the_project_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    std::fs::write(path.join("notes.md"), "hello\n").unwrap();

    let (project, _) = Project::init(&path, identity(), &SnapshotOptions::default()).unwrap();

    let busy = Project::open_with(&path, identity(), &short());
    assert!(
        matches!(busy, Err(OpenError::Busy { .. })),
        "init's project holds the lock, got {busy:?}"
    );
    drop(project);
    Project::open_with(&path, identity(), &short()).unwrap();
}

#[test]
fn a_waiting_open_succeeds_once_the_holder_lets_go() {
    let (_dir, path) = new_project();
    let released = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel();
    let holder = {
        let path = path.clone();
        let released = Arc::clone(&released);
        thread::spawn(move || {
            let project = Project::open_with(&path, identity(), &short()).unwrap();
            ready_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(100));
            released.store(true, Ordering::SeqCst);
            drop(project);
        })
    };
    ready_rx.recv().unwrap();

    let waiting = Project::open_with(
        &path,
        identity(),
        &LockOptions {
            wait: Duration::from_millis(500),
        },
    );

    assert!(waiting.is_ok(), "the wait should outlast the holder");
    assert!(
        released.load(Ordering::SeqCst),
        "open must have waited for the holder to release"
    );
    holder.join().unwrap();
}

#[test]
fn a_killed_holder_leaves_the_project_open_at_once() {
    let (_dir, path) = new_project();
    let exe = std::env::current_exe().unwrap();
    let mut child = Command::new(exe)
        .args([
            "--exact",
            "lock_holder_helper",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HOLDER_ENV, &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let locked = BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .any(|line| line.trim_end().ends_with("LOCKED"));
    assert!(locked, "the child never reported taking the lock");

    let while_alive = Project::open_with(&path, identity(), &short());
    let busy = matches!(while_alive, Err(OpenError::Busy { .. }));
    drop(while_alive);

    child.kill().unwrap();
    child.wait().unwrap();

    assert!(busy, "the lock is held while the child lives");
    Project::open_with(&path, identity(), &short())
        .expect("a killed holder leaves no stale lock behind");
}

#[test]
fn locking_leaves_the_repository_valid_for_plain_git() {
    let (_dir, path) = new_project();
    {
        let _held = Project::open_with(&path, identity(), &short()).unwrap();
        let _ = Project::open_with(&path, identity(), &short());
    }

    git(&path, &["fsck", "--strict", "--no-progress"]);
    assert!(
        git(&path, &["log", "--format=%s"]).contains("Start"),
        "history is intact"
    );
    assert_eq!(
        git(&path, &["status", "--porcelain"]).trim(),
        "",
        "lock file is inside .git"
    );
    assert!(path.join(".git/paper/lock").exists());
}

#[test]
fn the_lock_file_is_never_removed_after_a_failed_init_or_a_dropped_project() {
    let (_dir, path) = new_project();
    let lock_file = path.join(".git/paper/lock");
    assert!(lock_file.exists(), "dropping a project keeps the lock file");

    let again = Project::init_with(
        &path,
        identity(),
        &SnapshotOptions::default(),
        None,
        &short(),
    );

    assert!(matches!(again, Err(InitError::AlreadyInitialized { .. })));
    assert!(lock_file.exists(), "a refused init keeps the lock file");
}

#[test]
fn a_refused_adoption_creates_no_paper_folder_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    git(path, &["init", "--quiet", "--initial-branch=main"]);
    std::fs::write(path.join("a.txt"), "a").unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "--quiet", "-m", "a"]);
    git(path, &["checkout", "--quiet", "--detach"]);

    let result = Project::init_with(
        path,
        identity(),
        &SnapshotOptions::default(),
        None,
        &short(),
    );

    assert!(matches!(result, Err(InitError::DetachedHead)));
    assert!(!path.join(".git/paper").exists());
}

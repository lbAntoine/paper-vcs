//! Integration tests for `GitRepo`. Results are cross-checked with the real `git` binary,
//! because plain-git compatibility is a hard promise (ADR-0002).

use std::path::Path;
use std::process::Command;

use paper_git::{
    ChangeId, CommitId, FileKind, GitRepo, GitTime, Identity, NewCommit, Signature, TreeEdit,
    TreeFile, TreeId,
};
use tempfile::TempDir;

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

fn fresh_repo() -> (TempDir, GitRepo) {
    let dir = tempfile::tempdir().unwrap();
    let repo = GitRepo::init(dir.path(), &identity()).unwrap();
    (dir, repo)
}

/// Runs `git` in `dir` and returns stdout, failing the test if git fails.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
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

fn signature() -> Signature {
    Signature {
        identity: identity(),
        time: GitTime {
            seconds: 1_790_000_000,
            offset_seconds: 2 * 3600,
        },
    }
}

fn tree_with(repo: &GitRepo, files: &[(&str, &[u8])]) -> TreeId {
    let edits: Vec<_> = files
        .iter()
        .map(|(path, content)| TreeEdit::Upsert {
            path: (*path).to_owned(),
            blob: repo.write_blob(content).unwrap(),
            executable: false,
        })
        .collect();
    repo.edit_tree(&repo.empty_tree().unwrap(), &edits).unwrap()
}

fn new_commit(tree: TreeId, parents: Vec<CommitId>, change_id: Option<ChangeId>) -> NewCommit {
    NewCommit {
        tree,
        parents,
        author: signature(),
        committer: signature(),
        message: "Start project".to_owned(),
        change_id,
    }
}

#[test]
fn init_creates_a_repository_with_a_working_directory() {
    let (dir, repo) = fresh_repo();
    let canonical = |p: &Path| p.canonicalize().unwrap();
    assert_eq!(canonical(repo.workdir()), canonical(dir.path()));
    assert_eq!(
        canonical(repo.git_dir()),
        canonical(&dir.path().join(".git"))
    );
}

#[test]
fn open_finds_an_existing_repository() {
    let (dir, _) = fresh_repo();
    assert!(GitRepo::open(dir.path(), &identity()).is_ok());
}

#[test]
fn open_rejects_bare_repositories() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--bare", "--quiet"]);
    assert!(matches!(
        GitRepo::open(dir.path(), &identity()),
        Err(paper_git::GitError::Bare { .. })
    ));
}

#[test]
fn blobs_round_trip() {
    let (_dir, repo) = fresh_repo();
    let id = repo.write_blob("Résumé 논문\n".as_bytes()).unwrap();
    assert_eq!(repo.read_blob(&id).unwrap(), "Résumé 논문\n".as_bytes());
}

#[test]
fn edit_tree_builds_nested_paths_and_prunes_empty_directories() {
    let (dir, repo) = fresh_repo();
    let blob = repo.write_blob(b"x").unwrap();
    let full = repo
        .edit_tree(
            &repo.empty_tree().unwrap(),
            &[
                TreeEdit::Upsert {
                    path: "sections/intro.tex".to_owned(),
                    blob: blob.clone(),
                    executable: false,
                },
                TreeEdit::Upsert {
                    path: "build.sh".to_owned(),
                    blob,
                    executable: true,
                },
            ],
        )
        .unwrap();
    let listing = git(dir.path(), &["ls-tree", "-r", full.as_str()]);
    assert!(
        listing.contains("100755 blob") && listing.contains("\tbuild.sh"),
        "{listing}"
    );
    assert!(
        listing.contains("100644 blob") && listing.contains("\tsections/intro.tex"),
        "{listing}"
    );

    let pruned = repo
        .edit_tree(
            &full,
            &[TreeEdit::Remove {
                path: "sections/intro.tex".to_owned(),
            }],
        )
        .unwrap();
    let listing = git(dir.path(), &["ls-tree", pruned.as_str()]);
    assert!(
        !listing.contains("sections"),
        "empty directory should disappear: {listing}"
    );
}

#[test]
fn commits_round_trip_with_their_change_id() {
    let (_dir, repo) = fresh_repo();
    let change_id = ChangeId::random();
    let tree = tree_with(&repo, &[("main.tex", b"\\documentclass{article}\n")]);
    let parent = repo
        .write_commit(&new_commit(tree.clone(), vec![], None))
        .unwrap();
    let written = new_commit(tree.clone(), vec![parent.clone()], Some(change_id));
    let id = repo.write_commit(&written).unwrap();

    let read = repo.read_commit(&id).unwrap();
    assert_eq!(read.id, id);
    assert_eq!(read.tree, tree);
    assert_eq!(read.parents, vec![parent]);
    assert_eq!(read.author, signature());
    assert_eq!(read.committer, signature());
    assert_eq!(read.message, "Start project");
    assert_eq!(read.change_id, Some(change_id));
}

#[test]
fn commits_made_by_plain_git_have_no_change_id() {
    let (_dir, repo) = fresh_repo();
    let id = repo
        .write_commit(&new_commit(repo.empty_tree().unwrap(), vec![], None))
        .unwrap();
    assert_eq!(repo.read_commit(&id).unwrap().change_id, None);
}

#[test]
fn git_sees_the_change_id_header_and_accepts_the_commit() {
    let (dir, repo) = fresh_repo();
    let change_id = ChangeId::random();
    let id = repo
        .write_commit(&new_commit(
            repo.empty_tree().unwrap(),
            vec![],
            Some(change_id),
        ))
        .unwrap();
    repo.set_ref("refs/heads/main", &id, "test").unwrap();

    let raw = git(dir.path(), &["cat-file", "-p", id.as_str()]);
    assert!(raw.contains(&format!("\nchange-id {change_id}\n")), "{raw}");
    assert_eq!(
        git(dir.path(), &["log", "--format=%s", "main"]),
        "Start project\n"
    );
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn refs_can_be_set_found_and_deleted() {
    let (_dir, repo) = fresh_repo();
    let id = repo
        .write_commit(&new_commit(repo.empty_tree().unwrap(), vec![], None))
        .unwrap();
    assert_eq!(repo.find_ref("refs/paper/draft/main").unwrap(), None);

    repo.set_ref("refs/paper/draft/main", &id, "paper: snapshot")
        .unwrap();
    assert_eq!(repo.find_ref("refs/paper/draft/main").unwrap(), Some(id));

    repo.delete_ref("refs/paper/draft/main").unwrap();
    assert_eq!(repo.find_ref("refs/paper/draft/main").unwrap(), None);
    repo.delete_ref("refs/paper/draft/main").unwrap();
}

#[test]
fn head_can_be_attached_to_a_branch() {
    let (dir, repo) = fresh_repo();
    repo.set_head_branch("try/intro", "paper: switch").unwrap();
    assert_eq!(repo.head_branch().unwrap().as_deref(), Some("try/intro"));
    assert_eq!(
        git(dir.path(), &["symbolic-ref", "HEAD"]),
        "refs/heads/try/intro\n"
    );
}

#[test]
fn find_ref_follows_head_to_its_commit() {
    let (_dir, repo) = fresh_repo();
    let id = repo
        .write_commit(&new_commit(repo.empty_tree().unwrap(), vec![], None))
        .unwrap();
    repo.set_ref("refs/heads/main", &id, "test").unwrap();
    repo.set_head_branch("main", "test").unwrap();
    assert_eq!(repo.find_ref("HEAD").unwrap(), Some(id));
}

#[test]
fn head_is_none_when_detached() {
    let (dir, repo) = fresh_repo();
    let id = repo
        .write_commit(&new_commit(repo.empty_tree().unwrap(), vec![], None))
        .unwrap();
    git(
        dir.path(),
        &["update-ref", "--no-deref", "HEAD", id.as_str()],
    );
    let reopened = GitRepo::open(dir.path(), &identity()).unwrap();
    assert_eq!(reopened.head_branch().unwrap(), None);
}

#[test]
fn rejects_branch_names_git_does_not_accept() {
    let (_dir, repo) = fresh_repo();
    assert!(matches!(
        repo.set_head_branch("bad..name", "test"),
        Err(paper_git::GitError::InvalidBranchName { .. })
    ));
}

#[test]
fn tree_files_lists_every_file_recursively_with_its_kind() {
    let (_dir, repo) = fresh_repo();
    let blob = repo.write_blob(b"x").unwrap();
    let upsert = |path: &str, executable| TreeEdit::Upsert {
        path: path.to_owned(),
        blob: blob.clone(),
        executable,
    };
    let tree = repo
        .edit_tree(
            &repo.empty_tree().unwrap(),
            &[
                upsert("sections/intro.tex", false),
                upsert("build.sh", true),
            ],
        )
        .unwrap();

    let files = repo.tree_files(&tree).unwrap();
    let file = |kind| TreeFile {
        blob: blob.clone(),
        kind,
    };
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        vec![
            ("build.sh".to_owned(), file(FileKind::Executable)),
            ("sections/intro.tex".to_owned(), file(FileKind::Regular)),
        ]
    );
}

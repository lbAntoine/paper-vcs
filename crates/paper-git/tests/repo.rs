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

#[test]
fn local_branches_lists_every_branch_with_its_commit() {
    let (_dir, repo) = fresh_repo();
    let first = repo
        .write_commit(&new_commit(repo.empty_tree().unwrap(), vec![], None))
        .unwrap();
    let second = repo
        .write_commit(&new_commit(
            repo.empty_tree().unwrap(),
            vec![first.clone()],
            None,
        ))
        .unwrap();
    repo.set_ref("refs/heads/main", &second, "test").unwrap();
    repo.set_ref("refs/heads/try/intro", &first, "test")
        .unwrap();
    repo.set_ref("refs/paper/draft/main", &first, "test")
        .unwrap();

    assert_eq!(
        repo.local_branches().unwrap(),
        vec![("main".to_owned(), second), ("try/intro".to_owned(), first),]
    );
}

#[test]
fn reset_index_to_makes_git_status_clean() {
    let (dir, repo) = fresh_repo();
    std::fs::write(dir.path().join("main.tex"), "hello").unwrap();
    let tree = tree_with(&repo, &[("main.tex", b"hello")]);
    let id = repo
        .write_commit(&new_commit(tree.clone(), vec![], None))
        .unwrap();
    repo.set_ref("refs/heads/main", &id, "test").unwrap();
    repo.set_head_branch("main", "test").unwrap();

    repo.reset_index_to(&tree).unwrap();
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

#[test]
fn enclosing_workdir_finds_the_repository_root_from_a_subfolder() {
    let (dir, _repo) = fresh_repo();
    let nested = dir.path().join("sections/figures");
    std::fs::create_dir_all(&nested).unwrap();
    let found = GitRepo::enclosing_workdir(&nested).unwrap();
    assert_eq!(
        found.canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
}

#[test]
fn enclosing_workdir_is_none_outside_any_repository() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(GitRepo::enclosing_workdir(dir.path()), None);
}

/// A commit at `seconds` (committer time) on top of `parents`, holding `tree`.
fn commit_at(repo: &GitRepo, tree: &TreeId, parents: &[&CommitId], seconds: i64) -> CommitId {
    let mut signature = signature();
    signature.time.seconds = seconds;
    repo.write_commit(&NewCommit {
        tree: tree.clone(),
        parents: parents.iter().map(|id| (*id).clone()).collect(),
        author: signature.clone(),
        committer: signature,
        message: format!("commit at {seconds}"),
        change_id: None,
    })
    .unwrap()
}

/// root(100) <- a(200) <- merge(500); root <- b(300) <- merge; tip c(400) on top of b.
fn history_with_a_merge(repo: &GitRepo) -> (Vec<CommitId>, CommitId, CommitId) {
    let tree = tree_with(repo, &[("a.txt", b"a")]);
    let root = commit_at(repo, &tree, &[], 100);
    let a = commit_at(repo, &tree, &[&root], 200);
    let b = commit_at(repo, &tree, &[&root], 300);
    let c = commit_at(repo, &tree, &[&b], 400);
    let merge = commit_at(repo, &tree, &[&a, &b], 500);
    (vec![root, a, b, c.clone(), merge.clone()], merge, c)
}

#[test]
fn ancestors_lists_every_reachable_commit_once_newest_first_including_both_merge_parents() {
    let (dir, repo) = fresh_repo();
    let (all, merge, _c) = history_with_a_merge(&repo);
    repo.set_ref("refs/heads/main", &merge, "test").unwrap();

    let found = repo.ancestors(std::slice::from_ref(&merge)).unwrap();
    let ids: Vec<_> = found.iter().map(|commit| commit.id.clone()).collect();

    // merge(500), b(300), a(200), root(100): c is not reachable from the merge.
    let expected: Vec<_> = [4, 2, 1, 0].iter().map(|&i| all[i].clone()).collect();
    assert_eq!(ids, expected);

    let counted = git(dir.path(), &["rev-list", "--count", merge.as_str()]);
    assert_eq!(counted.trim(), found.len().to_string());
}

#[test]
fn ancestors_from_several_tips_visits_shared_history_once() {
    let (dir, repo) = fresh_repo();
    let (all, merge, c) = history_with_a_merge(&repo);
    repo.set_ref("refs/heads/main", &merge, "test").unwrap();
    repo.set_ref("refs/heads/side", &c, "test").unwrap();

    let found = repo.ancestors(&[merge.clone(), c.clone()]).unwrap();
    let ids: Vec<_> = found.iter().map(|commit| commit.id.clone()).collect();

    let expected: Vec<_> = [4, 3, 2, 1, 0].iter().map(|&i| all[i].clone()).collect();
    assert_eq!(ids, expected);
    let counted = git(dir.path(), &["rev-list", "--count", "--all"]);
    assert_eq!(counted.trim(), "5");
}

#[test]
fn ancestors_breaks_time_ties_by_commit_id() {
    let (_dir, repo) = fresh_repo();
    let tree = tree_with(&repo, &[("a.txt", b"a")]);
    let root = commit_at(&repo, &tree, &[], 100);
    let left = commit_at(&repo, &tree_with(&repo, &[("l", b"l")]), &[&root], 200);
    let right = commit_at(&repo, &tree_with(&repo, &[("r", b"r")]), &[&root], 200);

    let found = repo.ancestors(&[left.clone(), right.clone()]).unwrap();
    let ids: Vec<_> = found.iter().map(|commit| commit.id.clone()).collect();

    let mut tied = [left, right];
    tied.sort();
    assert_eq!(ids, [tied[0].clone(), tied[1].clone(), root]);
}

#[test]
fn ancestors_of_nothing_is_empty() {
    let (_dir, repo) = fresh_repo();
    assert!(repo.ancestors(&[]).unwrap().is_empty());
}

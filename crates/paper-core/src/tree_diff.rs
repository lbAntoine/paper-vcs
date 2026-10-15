//! Differences between two trees (used by `save` and `status`).

use paper_git::{GitError, GitRepo, TreeId};

use crate::snapshot::{Change, ChangeKind};

/// The files that differ between `from` and `to`, sorted by path.
///
/// A file whose content or executable bit changed is `Modified`.
///
/// # Errors
///
/// [`GitError`] if either tree can't be read.
pub fn diff_trees(repo: &GitRepo, from: &TreeId, to: &TreeId) -> Result<Vec<Change>, GitError> {
    let old = repo.tree_files(from)?;
    let new = repo.tree_files(to)?;
    let mut changes = Vec::new();
    for (path, file) in &new {
        match old.get(path) {
            None => changes.push(Change {
                path: path.clone(),
                kind: ChangeKind::Added,
            }),
            Some(before) if before != file => changes.push(Change {
                path: path.clone(),
                kind: ChangeKind::Modified,
            }),
            Some(_) => {}
        }
    }
    for path in old.keys().filter(|path| !new.contains_key(*path)) {
        changes.push(Change {
            path: path.clone(),
            kind: ChangeKind::Deleted,
        });
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use paper_git::{Identity, TreeEdit};

    use super::*;

    fn repo() -> (tempfile::TempDir, GitRepo) {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity {
            name: "Mina Park".to_owned(),
            email: "mina@uni.example".to_owned(),
        };
        let repo = GitRepo::init(dir.path(), &identity).unwrap();
        (dir, repo)
    }

    fn upsert(repo: &GitRepo, path: &str, content: &str, executable: bool) -> TreeEdit {
        TreeEdit::Upsert {
            path: path.to_owned(),
            blob: repo.write_blob(content.as_bytes()).unwrap(),
            executable,
        }
    }

    fn change(path: &str, kind: ChangeKind) -> Change {
        Change {
            path: path.to_owned(),
            kind,
        }
    }

    #[test]
    fn identical_trees_have_no_differences() {
        let (_dir, repo) = repo();
        let tree = repo
            .edit_tree(
                &repo.empty_tree().unwrap(),
                &[upsert(&repo, "a.tex", "a", false)],
            )
            .unwrap();
        assert_eq!(diff_trees(&repo, &tree, &tree).unwrap(), vec![]);
    }

    #[test]
    fn added_modified_and_deleted_files_are_reported_sorted_by_path() {
        let (_dir, repo) = repo();
        let from = repo
            .edit_tree(
                &repo.empty_tree().unwrap(),
                &[
                    upsert(&repo, "keep.tex", "same", false),
                    upsert(&repo, "sections/edit.tex", "old", false),
                    upsert(&repo, "gone.tex", "bye", false),
                ],
            )
            .unwrap();
        let to = repo
            .edit_tree(
                &from,
                &[
                    upsert(&repo, "sections/edit.tex", "new", false),
                    TreeEdit::Remove {
                        path: "gone.tex".to_owned(),
                    },
                    upsert(&repo, "abstract.tex", "fresh", false),
                ],
            )
            .unwrap();
        assert_eq!(
            diff_trees(&repo, &from, &to).unwrap(),
            vec![
                change("abstract.tex", ChangeKind::Added),
                change("gone.tex", ChangeKind::Deleted),
                change("sections/edit.tex", ChangeKind::Modified),
            ]
        );
    }

    #[test]
    fn becoming_executable_counts_as_modified() {
        let (_dir, repo) = repo();
        let from = repo
            .edit_tree(
                &repo.empty_tree().unwrap(),
                &[upsert(&repo, "build.sh", "make", false)],
            )
            .unwrap();
        let to = repo
            .edit_tree(&from, &[upsert(&repo, "build.sh", "make", true)])
            .unwrap();
        assert_eq!(
            diff_trees(&repo, &from, &to).unwrap(),
            vec![change("build.sh", ChangeKind::Modified)]
        );
    }

    #[test]
    fn the_diff_is_directional() {
        let (_dir, repo) = repo();
        let empty = repo.empty_tree().unwrap();
        let one = repo
            .edit_tree(&empty, &[upsert(&repo, "a.tex", "a", false)])
            .unwrap();
        assert_eq!(
            diff_trees(&repo, &empty, &one).unwrap(),
            vec![change("a.tex", ChangeKind::Added)]
        );
        assert_eq!(
            diff_trees(&repo, &one, &empty).unwrap(),
            vec![change("a.tex", ChangeKind::Deleted)]
        );
    }
}

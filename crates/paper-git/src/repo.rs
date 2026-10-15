//! [`GitRepo`]: every git read and write paper does goes through here

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gix::bstr::{BString, ByteSlice};
use gix::objs::tree::EntryKind;
use gix::refs::Target;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};

use crate::error::ResultExt;
use crate::{
    BlobId, ChangeId, Commit, CommitId, FileKind, GitError, GitTime, Identity, NewCommit,
    Signature, TreeEdit, TreeFile, TreeId,
};

const CHANGE_ID_HEADER: &str = "change-id";

/// A git repository with a working directory (paper never works on bare repositories).
#[derive(Debug)]
pub struct GitRepo {
    repo: gix::Repository,
}

impl GitRepo {
    /// Creates a new repository in `path`
    ///
    /// `identity` is used for reflog entries when git itself has no identity configured
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if the repository can't be created
    pub fn init(path: &Path, identity: &Identity) -> Result<Self, GitError> {
        let repo = gix::init(path).context("create a repository")?;
        Self::with_identity(repo, identity)
    }

    /// Opens the repository whose working directory is exactly `path`
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if there is no repository at `path`
    /// [`GitError::Bare`] if it has no working directory
    pub fn open(path: &Path, identity: &Identity) -> Result<Self, GitError> {
        let repo = gix::open(path).context("open the repository")?;
        if repo.workdir().is_none() {
            return Err(GitError::Bare {
                path: path.to_owned(),
            });
        }
        Self::with_identity(repo, identity)
    }

    fn with_identity(mut repo: gix::Repository, identity: &Identity) -> Result<Self, GitError> {
        // Reflog entries need a committer; researchers often never ran `git config user.name`
        repo.committer_or_set_fallback(identity.name.as_str(), identity.email.as_str())
            .context("set the committer identity")?;
        Ok(Self { repo })
    }

    /// The working directory of the repository that contains `path`, searching upwards.
    /// `None` if `path` isn't inside a repository with a working directory.
    pub fn enclosing_workdir(path: &Path) -> Option<PathBuf> {
        let repo = gix::discover(path).ok()?;
        repo.workdir().map(Path::to_path_buf)
    }

    /// # Panics
    ///
    /// Never: [`GitRepo::open`] rejects repositories without a working directory
    pub fn workdir(&self) -> &Path {
        self.repo.workdir().expect("checked on open")
    }

    pub fn git_dir(&self) -> &Path {
        self.repo.git_dir()
    }

    /// # Errors
    ///
    /// [`GitError::Gix`] if the object can't be written
    pub fn write_blob(&self, data: &[u8]) -> Result<BlobId, GitError> {
        let id = self
            .repo
            .write_blob(data)
            .context("write a file's content")?;
        Ok(BlobId::from_gix(id))
    }

    /// # Errors
    ///
    /// [`GitError::Gix`] if the blob doesn't exist
    pub fn read_blob(&self, id: &BlobId) -> Result<Vec<u8>, GitError> {
        let blob = self
            .repo
            .find_blob(id.to_gix())
            .context(format!("read file content {id}"))?;
        Ok(blob.data.clone())
    }

    /// The empty tree, written to the object database
    ///
    /// gix treats the empty tree as virtual, but `git fsck` requires every tree a commit
    /// references to exist on disk, so it's written explicitly
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if the object can't be written
    pub fn empty_tree(&self) -> Result<TreeId, GitError> {
        let id = self
            .repo
            .write_object(gix::objs::Tree::empty())
            .context("write the empty tree")?;
        Ok(TreeId::from_gix(id))
    }

    /// Applies `edits` to the tree `base` and returns the new tree. Intermediate directories
    /// are created as needed, and directories left empty by removals disappear
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if `base` doesn't exist, a path is invalid, or writing fails
    pub fn edit_tree(&self, base: &TreeId, edits: &[TreeEdit]) -> Result<TreeId, GitError> {
        let mut editor = self.repo.edit_tree(base.to_gix()).context("read a tree")?;
        for edit in edits {
            match edit {
                TreeEdit::Upsert {
                    path,
                    blob,
                    executable,
                } => {
                    let kind = if *executable {
                        EntryKind::BlobExecutable
                    } else {
                        EntryKind::Blob
                    };
                    editor
                        .upsert(path.as_str(), kind, blob.to_gix())
                        .context(format!("add {path} to a tree"))?;
                }
                TreeEdit::Remove { path } => {
                    editor
                        .remove(path.as_str())
                        .context(format!("remove {path} from a tree"))?;
                }
            }
        }
        let id = editor.write().context("write a tree")?;
        Ok(TreeId::from_gix(id))
    }

    /// Every file in `tree`, recursively, keyed by `/`-separated path.
    ///
    /// Submodules are left out: paper doesn't manage them, and snapshots never touch them.
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if the tree or one of its subtrees can't be read.
    pub fn tree_files(&self, tree: &TreeId) -> Result<BTreeMap<String, TreeFile>, GitError> {
        let mut recorder = gix::traverse::tree::Recorder::default();
        self.repo
            .find_tree(tree.to_gix())
            .context(format!("find tree {tree}"))?
            .traverse()
            .breadthfirst(&mut recorder)
            .context(format!("walk tree {tree}"))?;
        let files = recorder
            .records
            .into_iter()
            .filter_map(|entry| {
                let kind = match entry.mode.kind() {
                    EntryKind::Blob => FileKind::Regular,
                    EntryKind::BlobExecutable => FileKind::Executable,
                    EntryKind::Link => FileKind::Symlink,
                    EntryKind::Tree | EntryKind::Commit => return None,
                };
                let file = TreeFile {
                    blob: BlobId::from_gix(entry.oid),
                    kind,
                };
                Some((entry.filepath.to_str_lossy().into_owned(), file))
            })
            .collect();
        Ok(files)
    }

    /// # Errors
    ///
    /// [`GitError::Gix`] if the commit can't be written.
    pub fn write_commit(&self, commit: &NewCommit) -> Result<CommitId, GitError> {
        let extra_headers = commit
            .change_id
            .iter()
            .map(|id| {
                (
                    BString::from(CHANGE_ID_HEADER),
                    BString::from(id.to_string()),
                )
            })
            .collect();
        let object = gix::objs::Commit {
            tree: commit.tree.to_gix(),
            parents: commit.parents.iter().map(CommitId::to_gix).collect(),
            author: to_gix_signature(&commit.author),
            committer: to_gix_signature(&commit.committer),
            encoding: None,
            // Git convention: messages end with exactly one newline.
            message: format!("{}\n", commit.message.trim_end()).into(),
            extra_headers,
        };
        let id = self.repo.write_object(&object).context("write a commit")?;
        Ok(CommitId::from_gix(id))
    }

    /// # Errors
    ///
    /// [`GitError::Gix`] if the commit doesn't exist or can't be decoded,
    /// [`GitError::BadChangeId`] / [`GitError::NonUtf8Message`] for malformed content.
    pub fn read_commit(&self, id: &CommitId) -> Result<Commit, GitError> {
        let found = self
            .repo
            .find_commit(id.to_gix())
            .context(format!("find commit {id}"))?;
        let object = found
            .decode()
            .and_then(gix::objs::CommitRef::into_owned)
            .context(format!("decode commit {id}"))?;
        let change_id = object
            .extra_headers()
            .find(CHANGE_ID_HEADER)
            .map(|value| {
                value
                    .to_str_lossy()
                    .parse::<ChangeId>()
                    .map_err(|source| GitError::BadChangeId {
                        id: id.clone(),
                        source,
                    })
            })
            .transpose()?;
        let message = object
            .message
            .to_str()
            .map_err(|_| GitError::NonUtf8Message { id: id.clone() })?
            .trim_end()
            .to_owned();
        Ok(Commit {
            id: id.clone(),
            tree: TreeId::from_gix(object.tree),
            parents: object
                .parents
                .iter()
                .copied()
                .map(CommitId::from_gix)
                .collect(),
            author: from_gix_signature(&object.author),
            committer: from_gix_signature(&object.committer),
            message,
            change_id,
        })
    }

    /// The commit a reference points to, following symbolic references. `None` if it doesn't exist.
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if the reference store can't be read.
    pub fn find_ref(&self, name: &str) -> Result<Option<CommitId>, GitError> {
        let Some(reference) = self
            .repo
            .try_find_reference(name)
            .context(format!("read {name}"))?
        else {
            return Ok(None);
        };
        let id = reference
            .into_fully_peeled_id()
            .context(format!("resolve {name}"))?;
        Ok(Some(CommitId::from_gix(id)))
    }

    /// Points `name` at `target`, creating it if needed. `reason` goes into the reflog.
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if the name is invalid or the reference can't be written.
    pub fn set_ref(&self, name: &str, target: &CommitId, reason: &str) -> Result<(), GitError> {
        self.repo
            .reference(name, target.to_gix(), PreviousValue::Any, reason)
            .context(format!("update {name}"))?;
        Ok(())
    }

    /// Deletes `name`. Deleting a reference that doesn't exist is not an error.
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if the reference exists but can't be deleted.
    pub fn delete_ref(&self, name: &str) -> Result<(), GitError> {
        if let Some(reference) = self
            .repo
            .try_find_reference(name)
            .context(format!("read {name}"))?
        {
            reference.delete().context(format!("delete {name}"))?;
        }
        Ok(())
    }

    /// Every local branch (`refs/heads/*`) with the commit it points to, sorted by name.
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if the references can't be read.
    pub fn local_branches(&self) -> Result<Vec<(String, CommitId)>, GitError> {
        let platform = self.repo.references().context("list branches")?;
        let mut branches = Vec::new();
        for reference in platform.local_branches().context("list branches")? {
            let reference = reference.context("read a branch")?;
            let name = reference.name().shorten().to_str_lossy().into_owned();
            let id = reference
                .into_fully_peeled_id()
                .context(format!("resolve branch {name}"))?;
            branches.push((name, CommitId::from_gix(id)));
        }
        branches.sort();
        Ok(branches)
    }

    /// Rewrites git's index (the staging area) to match `tree`, so `git status` compares the
    /// working files against that tree. Paper has no staging area of its own.
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if the tree can't be read or the index can't be written.
    pub fn reset_index_to(&self, tree: &TreeId) -> Result<(), GitError> {
        let mut index = self
            .repo
            .index_from_tree(&tree.to_gix())
            .context("build the index")?;
        index
            .write(gix::index::write::Options::default())
            .context("write the index")?;
        Ok(())
    }

    /// The branch HEAD is attached to (e.g. `main`), or `None` when HEAD is detached.
    ///
    /// # Errors
    ///
    /// [`GitError::Gix`] if HEAD can't be read.
    pub fn head_branch(&self) -> Result<Option<String>, GitError> {
        let head = self.repo.head_name().context("read HEAD")?;
        Ok(head.and_then(|name| {
            name.as_bstr()
                .strip_prefix(b"refs/heads/")
                .map(|branch| branch.to_str_lossy().into_owned())
        }))
    }

    /// Attaches HEAD to `branch` (e.g. `main`). The branch doesn't need to exist yet.
    ///
    /// # Panics
    ///
    /// Never: `HEAD` is always a valid reference name.
    ///
    /// # Errors
    ///
    /// [`GitError::InvalidBranchName`] if git doesn't accept `branch`,
    /// [`GitError::Gix`] if HEAD can't be written.
    pub fn set_head_branch(&self, branch: &str, reason: &str) -> Result<(), GitError> {
        let target = format!("refs/heads/{branch}")
            .try_into()
            .map_err(|source| GitError::InvalidBranchName {
                name: branch.to_owned(),
                source,
            })?;
        let edit = RefEdit {
            change: Change::Update {
                log: LogChange {
                    mode: RefLog::AndReference,
                    force_create_reflog: false,
                    message: reason.into(),
                },
                expected: PreviousValue::Any,
                new: Target::Symbolic(target),
            },
            name: "HEAD".try_into().expect("HEAD is a valid reference name"),
            deref: false,
        };
        self.repo.edit_reference(edit).context("update HEAD")?;
        Ok(())
    }
}

fn to_gix_signature(signature: &Signature) -> gix::actor::Signature {
    gix::actor::Signature {
        name: signature.identity.name.as_str().into(),
        email: signature.identity.email.as_str().into(),
        time: gix::date::Time {
            seconds: signature.time.seconds,
            offset: signature.time.offset_seconds,
        },
    }
}

fn from_gix_signature(signature: &gix::actor::Signature) -> Signature {
    Signature {
        identity: Identity {
            name: signature.name.to_str_lossy().into_owned(),
            email: signature.email.to_str_lossy().into_owned(),
        },
        time: GitTime {
            seconds: signature.time.seconds,
            offset_seconds: signature.time.offset,
        },
    }
}

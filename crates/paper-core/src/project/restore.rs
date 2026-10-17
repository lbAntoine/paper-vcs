//! `paper restore`: put files back the way they were at a point (02-cli.md § paper restore).

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use paper_git::{CommitId, FileKind, GitError};

use super::Project;
use super::resolve::ResolveError;
use super::save::{SaveError, matches};
use crate::activity::OpKind;
use crate::fs_util::write_atomic;
use crate::point::Point;
use crate::snapshot::{Change, ChangeKind, SkipReason, Skipped};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreRequest {
    /// Must not be empty.
    pub paths: Vec<String>,
    /// Defaults to `last`.
    pub from: Option<Point>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restored {
    pub from: CommitId,
    /// Files rewritten or deleted, compared with the draft before the restore.
    pub changes: Vec<Change>,
    /// Matching files that were left alone: symlink entries in the `from` tree and
    /// things the snapshot skips (symlinks, oversize files, nested repositories).
    pub skipped: Vec<SkippedPath>,
}

/// A matching file that restore left alone, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedPath {
    pub path: String,
    pub reason: RestoreSkip,
}

impl SkippedPath {
    fn new(path: &str, reason: RestoreSkip) -> Self {
        Self {
            path: path.to_owned(),
            reason,
        }
    }
}

/// Why restore left a file alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreSkip {
    /// The saved version is a symbolic link, which paper doesn't restore.
    SavedSymlink,
    /// The file on disk is a symbolic link.
    Symlink,
    /// A folder above the file is a symbolic link.
    SymlinkedFolder,
    /// The file on disk is too large to track.
    TooLarge { bytes: u64 },
    /// The file is inside a separate git repository.
    NestedRepository,
    /// The file's name can't be read as text.
    NonUtf8Path,
    /// Something paper doesn't track (an ignored file, say) is in the way.
    Blocked,
    /// The draft has a file that differs from this one only in letter case, and the
    /// restore wasn't asked to replace it (name both spellings to do so).
    CaseAlias { other: String },
    /// The file was kept because the folder or file it stood in the way of couldn't be
    /// restored (`target`): a file/folder swap is all or nothing.
    KeptForSwap { target: String },
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("no files were named, so there is nothing to restore")]
    NoPaths,
    #[error("these paths are not in your draft or in that save: {}; no files were changed", paths.join(", "))]
    UnknownPaths { paths: Vec<String> },
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("could not {action} {path}; your previous draft is recorded")]
    Io {
        /// "restore" or "remove".
        action: &'static str,
        /// Relative to the project folder.
        path: String,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Puts the matching files back as they were at `from`, after recording the draft.
    ///
    /// Files the snapshot skips are never overwritten or deleted.
    ///
    /// # Errors
    ///
    /// [`RestoreError::NoPaths`], [`RestoreError::UnknownPaths`] (no working file is changed), and the
    /// wrapped lower-level errors.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn restore(&mut self, request: &RestoreRequest) -> Result<Restored, RestoreError> {
        if request.paths.is_empty() {
            return Err(RestoreError::NoPaths);
        }
        // I1: unsaved edits become a commit before anything is overwritten.
        let before = self.snapshot()?;
        let from_point = request.from.clone().unwrap_or(Point::Last { back: 0 });
        let from = self.resolve(&from_point)?.commit;
        let from_files = self.repo.tree_files(&self.repo.read_commit(&from)?.tree)?;
        let draft_tree = self.repo.read_commit(&self.view.current().draft)?.tree;
        let draft_files = self.repo.tree_files(&draft_tree)?;

        let unknown: Vec<String> = request
            .paths
            .iter()
            .filter(|path| {
                !from_files
                    .keys()
                    .chain(draft_files.keys())
                    .any(|file| matches(path, file))
            })
            .cloned()
            .collect();
        if !unknown.is_empty() {
            return Err(RestoreError::UnknownPaths { paths: unknown });
        }

        let targets: BTreeSet<&String> = from_files
            .keys()
            .chain(draft_files.keys())
            .filter(|file| request.paths.iter().any(|p| matches(p, file)))
            .collect();
        let mut skipped = Vec::new();
        let mut deletes: Vec<&String> = Vec::new();
        let mut candidates = Vec::new();
        for path in targets {
            let wanted = from_files.get(path);
            let kept = left_alone(&before.skipped, path)
                .or_else(|| self.on_disk_symlink(path))
                .or_else(|| {
                    (wanted.is_some_and(|f| f.kind == FileKind::Symlink))
                        .then_some(RestoreSkip::SavedSymlink)
                });
            if let Some(reason) = kept {
                skipped.push(SkippedPath::new(path, reason));
            } else if wanted.is_none() {
                deletes.push(path);
            } else if wanted != draft_files.get(path) {
                candidates.push(path);
            }
        }
        // Decide everything, and read every blob, before the first byte on disk changes.
        let plan = self.settle(candidates, deletes, |p| draft_files.contains_key(p));
        let deletes = plan.deletes;
        skipped.extend(
            plan.blocked
                .iter()
                .map(|(path, reason)| SkippedPath::new(path, reason.clone())),
        );
        skipped.sort_by(|a, b| a.path.cmp(&b.path));
        let mut writes = Vec::new();
        for path in plan.writes {
            let file = &from_files[path];
            writes.push((path, file.kind, self.repo.read_blob(&file.blob)?));
        }

        let mut changes: Vec<Change> = deletes
            .iter()
            .map(|path| Change {
                path: (*path).clone(),
                kind: ChangeKind::Deleted,
            })
            .chain(writes.iter().map(|(path, _, _)| Change {
                path: (*path).clone(),
                kind: if draft_files.contains_key(*path) {
                    ChangeKind::Modified
                } else {
                    ChangeKind::Added
                },
            }))
            .collect();
        changes.sort_by(|a, b| a.path.cmp(&b.path));

        // Deletions first, so a file turning into a directory (or back) finds room.
        for path in deletes {
            self.delete_working_file(path)?;
        }
        for (path, kind, bytes) in writes {
            let io_err = |source| RestoreError::Io {
                action: "restore",
                path: path.clone(),
                source,
            };
            let absolute = self.repo.workdir().join(path);
            if let Some(parent) = absolute.parent() {
                fs::create_dir_all(parent).map_err(io_err)?;
            }
            write_atomic(&absolute, &bytes).map_err(io_err)?;
            set_executable(&absolute, kind == FileKind::Executable).map_err(io_err)?;
        }

        // Record the new working files so the draft and the stat cache match them.
        self.snapshot_as(OpKind::Restore, "restore files")?;
        Ok(Restored {
            from,
            changes,
            skipped,
        })
    }

    /// Whether the path, or any directory above it, is a symlink on disk.
    fn on_disk_symlink(&self, path: &str) -> Option<RestoreSkip> {
        let root = self.repo.workdir();
        let mut prefix = String::new();
        path.split('/').find_map(|part| {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            let linked =
                fs::symlink_metadata(root.join(&prefix)).is_ok_and(|m| m.file_type().is_symlink());
            linked.then(|| {
                if prefix == path {
                    RestoreSkip::Symlink
                } else {
                    RestoreSkip::SymlinkedFolder
                }
            })
        })
    }

    /// Removes a file, then the directories it leaves empty.
    fn delete_working_file(&self, path: &str) -> Result<(), RestoreError> {
        let root = self.repo.workdir();
        let absolute = root.join(path);
        match fs::remove_file(&absolute) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(RestoreError::Io {
                    action: "remove",
                    path: path.to_owned(),
                    source,
                });
            }
        }
        let mut dir = absolute.parent();
        while let Some(current) = dir.filter(|d| *d != root && d.starts_with(root)) {
            // `remove_dir` only succeeds on an empty directory.
            if fs::remove_dir(current).is_err() {
                break;
            }
            dir = current.parent();
        }
        Ok(())
    }
}

/// Why the snapshot left `path` out (or something above it, like a nested repository).
fn left_alone(skipped: &[Skipped], path: &str) -> Option<RestoreSkip> {
    let found = skipped.iter().find(|s| {
        path == s.path
            || path
                .strip_prefix(s.path.as_str())
                .is_some_and(|r| r.starts_with('/'))
    })?;
    Some(match found.reason {
        SkipReason::TooLarge { bytes } => RestoreSkip::TooLarge { bytes },
        SkipReason::Symlink if found.path == path => RestoreSkip::Symlink,
        SkipReason::Symlink => RestoreSkip::SymlinkedFolder,
        SkipReason::NestedRepository => RestoreSkip::NestedRepository,
        SkipReason::NonUtf8Path => RestoreSkip::NonUtf8Path,
    })
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> io::Result<()> {
    Ok(())
}

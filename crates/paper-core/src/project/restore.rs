//! `paper restore`: put files back the way they were at a point (02-cli.md § paper restore).

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use paper_git::{CommitId, FileKind, GitError};

use super::Project;
use super::resolve::ResolveError;
use super::save::{SaveError, matches};
use crate::fs_util::write_atomic;
use crate::point::Point;
use crate::snapshot::{Change, ChangeKind, Skipped};

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
    pub skipped: Vec<String>,
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
            let kept = is_left_alone(&before.skipped, path) || self.on_disk_is_symlink(path);
            if wanted.is_some_and(|f| f.kind == FileKind::Symlink) || kept {
                skipped.push(path.clone());
            } else if wanted.is_none() {
                deletes.push(path);
            } else if wanted != draft_files.get(path) {
                candidates.push(path);
            }
        }
        let doomed: BTreeSet<&str> = deletes.iter().map(|p| p.as_str()).collect();

        // Decide everything, and read every blob, before the first byte on disk changes.
        let mut writes = Vec::new();
        for path in candidates {
            let file = &from_files[path];
            if self.blocked(path, draft_files.contains_key(path), &doomed) {
                skipped.push(path.clone());
                continue;
            }
            writes.push((path, file.kind, self.repo.read_blob(&file.blob)?));
        }
        skipped.sort();

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
        self.snapshot()?;
        Ok(Restored {
            from,
            changes,
            skipped,
        })
    }

    /// Whether writing `path` would clobber or escape through something paper doesn't track:
    /// a symlinked parent, an untracked (ignored or oversize) file or directory in the way,
    /// or a parent that is a file the restore isn't removing.
    fn blocked(&self, path: &str, in_draft: bool, doomed: &BTreeSet<&str>) -> bool {
        let root = self.repo.workdir();
        let mut prefix = String::new();
        for part in path.split('/').take(path.matches('/').count()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            match fs::symlink_metadata(root.join(&prefix)) {
                Ok(meta) if meta.file_type().is_symlink() => return true,
                Ok(meta) if meta.is_file() && !doomed.contains(prefix.as_str()) => return true,
                _ => {}
            }
        }
        if in_draft {
            return false;
        }
        match fs::symlink_metadata(root.join(path)) {
            Ok(meta) if meta.is_dir() => !only_holds(root, path, doomed),
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Whether the path, or any directory above it, is a symlink on disk.
    fn on_disk_is_symlink(&self, path: &str) -> bool {
        let root = self.repo.workdir();
        let mut prefix = String::new();
        path.split('/').any(|part| {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            fs::symlink_metadata(root.join(&prefix)).is_ok_and(|meta| meta.file_type().is_symlink())
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

/// Whether every file under the directory `dir` is about to be removed by this restore.
fn only_holds(root: &Path, dir: &str, doomed: &BTreeSet<&str>) -> bool {
    let Ok(entries) = fs::read_dir(root.join(dir)) else {
        return false;
    };
    entries.flatten().all(|entry| {
        let child = format!("{dir}/{}", entry.file_name().to_string_lossy());
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => only_holds(root, &child, doomed),
            Ok(kind) if kind.is_file() => doomed.contains(child.as_str()),
            _ => false,
        }
    })
}

/// Whether the snapshot left `path` out (or something above it, like a nested repository).
fn is_left_alone(skipped: &[Skipped], path: &str) -> bool {
    skipped.iter().any(|s| {
        path == s.path
            || path
                .strip_prefix(s.path.as_str())
                .is_some_and(|r| r.starts_with('/'))
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

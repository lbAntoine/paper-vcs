//! `paper restore`: put files back the way they were at a point (02-cli.md § paper restore).

use std::collections::BTreeSet;
use std::io;

use paper_git::{CommitId, GitError};

use super::Project;
use super::checkout::ApplyFailure;
use super::resolve::ResolveError;
use super::save::{SaveError, matches};
use crate::activity::{OpKind, describe};
use crate::point::Point;
use crate::snapshot::Change;

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
    pub(super) fn new(path: &str, reason: RestoreSkip) -> Self {
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
        crate::fault::crash_point("after-snapshot");
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
        let plan = self.plan_checkout(targets, &from_files, &draft_files, &before.skipped)?;
        self.apply_checkout(&plan, "restore").map_err(restore_io)?;
        crate::fault::crash_point("after-checkout");

        // Record the new working files so the draft and the stat cache match them.
        let description = describe::restored(&request.paths, &from_point.to_string());
        self.snapshot_as(OpKind::Restore, Some(&description))?;
        Ok(Restored {
            from,
            changes: plan.changes,
            skipped: plan.skipped,
        })
    }
}

fn restore_io(failure: ApplyFailure) -> RestoreError {
    RestoreError::Io {
        action: failure.action,
        path: failure.path,
        source: failure.source,
    }
}

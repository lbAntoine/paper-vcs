//! `paper restore`: put files back the way they were at a point (02-cli.md § paper restore).

use std::io;
use std::path::PathBuf;

use paper_git::{CommitId, GitError};

use super::Project;
use super::resolve::ResolveError;
use super::save::SaveError;
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
    /// Symlink entries in the `from` tree, which are never written.
    pub skipped: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("say which files to restore, for example `paper restore main.tex`")]
    NoPaths,
    #[error("these paths are not in your draft or in that save: {}; nothing was changed. Run `paper status` or `paper show <point>` to see the files", paths.join(", "))]
    UnknownPaths { paths: Vec<String> },
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("could not write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Puts the matching files back as they were at `from`, after recording the draft.
    ///
    /// # Errors
    ///
    /// [`RestoreError::NoPaths`], [`RestoreError::UnknownPaths`] (nothing is written), and the
    /// wrapped lower-level errors.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn restore(&mut self, _request: &RestoreRequest) -> Result<Restored, RestoreError> {
        todo!("PV-010")
    }
}

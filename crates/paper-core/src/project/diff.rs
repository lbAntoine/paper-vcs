//! `paper diff`: compare two points file by file (02-cli.md § paper diff).

use paper_diff::FileDiff;
use paper_git::GitError;

use super::Project;
use super::resolve::ResolveError;
use crate::point::Point;
use crate::snapshot::ChangeKind;

/// What to compare. Paths use the same rule as `save` (file or whole directory components).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffRequest {
    /// Defaults to `last`.
    pub from: Option<Point>,
    /// Defaults to `@`, the stored draft (the front-end snapshots first).
    pub to: Option<Point>,
    /// Restrict to these paths; empty means every file.
    pub paths: Vec<String>,
}

/// One changed file with its line diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiffEntry {
    pub path: String,
    pub kind: ChangeKind,
    pub diff: FileDiff,
}

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("these paths are in neither of the compared saves: {}; run `paper status` or `paper show <point>` to see the files", paths.join(", "))]
    UnknownPaths { paths: Vec<String> },
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl Project {
    /// Compares two points, file by file, sorted by path.
    ///
    /// # Errors
    ///
    /// [`DiffError::UnknownPaths`] for paths naming no file on either side,
    /// [`DiffError::Resolve`] for an unknown point.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn diff(&self, _request: &DiffRequest) -> Result<Vec<FileDiffEntry>, DiffError> {
        todo!("PV-010")
    }
}

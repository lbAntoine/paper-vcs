//! `paper history` and `paper show` (02-cli.md).

use paper_git::{ChangeId, CommitId, GitError, GitTime, Identity};

use super::Project;
use super::resolve::ResolveError;
use crate::name::LineName;
use crate::point::Point;
use crate::snapshot::Change;

/// What `history` should list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryRequest {
    /// Only saves that touch this file, or anything under this directory.
    pub path: Option<String>,
    /// List every line's saves instead of the current line's.
    pub all_lines: bool,
    /// At most this many entries.
    pub limit: Option<usize>,
}

/// One save, as listed by `history` and `show`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub commit: CommitId,
    /// `None` for commits made by plain git.
    pub change_id: Option<ChangeId>,
    /// The shortest unique change-ID prefix (at least 4 letters); the first 7 hash
    /// characters when there is no change ID.
    pub short_change_id: String,
    pub message: String,
    pub author: Identity,
    pub time: GitTime,
    /// The lines whose last save this is.
    pub tip_of: Vec<LineName>,
}

/// A save and what it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub entry: HistoryEntry,
    /// Compared with the first parent (the empty tree for the first save).
    pub changes: Vec<Change>,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error(transparent)]
    Git(#[from] GitError),
}

#[derive(Debug, thiserror::Error)]
pub enum ShowError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl Project {
    /// The saves on the current line (or on every line), newest first.
    ///
    /// # Errors
    ///
    /// [`HistoryError::Git`] if a commit can't be read.
    pub fn history(&self, _request: &HistoryRequest) -> Result<Vec<HistoryEntry>, HistoryError> {
        todo!("PV-002: list history")
    }

    /// One save (or the draft, for `@`) with its changes.
    ///
    /// # Errors
    ///
    /// [`ShowError::Resolve`] if the point names nothing, [`ShowError::Git`] on read failures.
    pub fn show(&self, _point: &Point) -> Result<Shown, ShowError> {
        todo!("PV-002: show a save")
    }
}

//! `paper status`: what is unsaved (02-cli.md § paper status).

use paper_git::GitError;

use super::Project;
use super::history::HistoryEntry;
use super::resolve::ResolveError;
use super::save::SaveError;
use crate::name::LineName;
use crate::snapshot::{ChangeKind, Skipped};

/// How `status` reads the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusOptions {
    /// Snapshot the working files first (I1). `false` is `--no-snapshot`.
    pub snapshot: bool,
}

impl Default for StatusOptions {
    fn default() -> Self {
        Self { snapshot: true }
    }
}

/// How big a change is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStat {
    /// A text file: lines added and removed.
    Lines { added: usize, removed: usize },
    /// A binary file: sizes in bytes, `None` for an absent side.
    Bytes { old: Option<u64>, new: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: String,
    pub kind: ChangeKind,
    pub stat: ChangeStat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub line: LineName,
    /// The line's tip, as `history` lists it.
    pub last: HistoryEntry,
    /// Sorted by path; empty when nothing is unsaved.
    pub changes: Vec<StatusEntry>,
    pub skipped: Vec<Skipped>,
    pub large: Vec<(String, u64)>,
}

#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl Project {
    /// What the draft holds relative to the line's last save.
    ///
    /// # Errors
    ///
    /// [`StatusError`] if the working files or the repository can't be read.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn status(&mut self, _options: &StatusOptions) -> Result<Status, StatusError> {
        todo!("PV-010")
    }
}

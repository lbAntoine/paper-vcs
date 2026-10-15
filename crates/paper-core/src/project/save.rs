//! `paper save`: turn the draft (or part of it) into a save (02-cli.md § paper save).

use std::io;
use std::path::PathBuf;

use paper_git::{ChangeId, CommitId, GitError};

use super::Project;
use crate::name::LineName;
use crate::snapshot::{Change, Skipped, SnapshotError};
use crate::view::ViewError;

/// Which of the draft's changes a save takes.
///
/// A path selects the file with exactly that path, or every file under it when it names a
/// directory (`sections` or `sections/`). Paths are repository-relative, with `/` separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSelection {
    /// The whole draft.
    All,
    /// Only the files matching these paths.
    Only(Vec<String>),
    /// Everything except the files matching these paths.
    AllBut(Vec<String>),
}

/// What to save. The message is data: `paper-core` never prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub message: String,
    pub paths: PathSelection,
}

/// The draft after a snapshot: what it holds relative to the line's last save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftState {
    /// Changes from the line's last save to the draft, sorted by path.
    pub changes: Vec<Change>,
    /// Things the snapshot deliberately left out.
    pub skipped: Vec<Skipped>,
    /// Large tracked files, with their size.
    pub large: Vec<(String, u64)>,
}

/// A save that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub save: CommitId,
    /// The change ID of the save: the one the draft had.
    pub change_id: ChangeId,
    pub line: LineName,
    /// What the save contains, relative to the previous save.
    pub saved: Vec<Change>,
    /// What stayed in the new draft.
    pub remaining: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The draft (or the selection) was empty; nothing was written.
    NothingToSave,
    Saved(Saved),
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("a save needs a message; describe what changed")]
    MissingMessage,
    #[error("none of these paths has unsaved changes: {}", paths.join(", "))]
    NoChangesAt { paths: Vec<String> },
    #[error("git moved the line '{line}' since paper last saw it")]
    OutOfSyncWithGit { line: LineName },
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("could not write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Invariant I1 as a reusable step: records the working files in the current line's
    /// draft. The draft commit is rewritten (same change ID, same parent) only if its tree
    /// changed.
    ///
    /// # Errors
    ///
    /// [`SaveError`] if the working files can't be read or the repository can't be written.
    pub fn snapshot(&mut self) -> Result<DraftState, SaveError> {
        todo!("PV-001: snapshot the working files into the draft")
    }

    /// Turns the draft (or the selected part of it) into a save on the current line and
    /// starts a fresh draft on top.
    ///
    /// # Errors
    ///
    /// [`SaveError::OutOfSyncWithGit`] if git moved the line, [`SaveError::MissingMessage`]
    /// for an empty message, [`SaveError::NoChangesAt`] for paths matching nothing, and the
    /// wrapped lower-level errors.
    pub fn save(&mut self, _request: &SaveRequest) -> Result<SaveOutcome, SaveError> {
        todo!("PV-001: write the save and the new draft")
    }
}

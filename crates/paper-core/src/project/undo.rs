//! `paper undo`, `paper redo` and `paper undo --to`: take back an activity entry, or bring it
//! back, including the working files (03-architecture.md § Undo / redo algorithm).

use std::io;

use paper_git::GitError;

use super::restore::SkippedPath;
use super::save::SaveError;
use super::{ActivityEntry, ExportError, Project};
use crate::activity::ActivityError;
use crate::snapshot::Change;

/// An entry that was undone or redone, and what the working files did about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undone {
    /// What was undone (`undo`), brought back (`redo`) or gone back to (`undo --to`).
    pub entry: ActivityEntry,
    /// Working files rewritten or deleted, compared with the files before the command.
    pub changed: Vec<Change>,
    /// Files paper does not record that the checkout left alone, with the reason.
    pub skipped: Vec<SkippedPath>,
}

/// What an undo-family command did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    /// The state changed; the working files follow it.
    Undone(Undone),
    /// The position is the first entry. Nothing was recorded.
    NothingToUndo,
    /// The redo stack is empty. Nothing was recorded.
    NothingToRedo,
}

#[derive(Debug, thiserror::Error)]
pub enum UndoError {
    /// The refusals of `save`: plain git moved a line ref or HEAD.
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error("no entry in the activity log starts with '{prefix}'")]
    UnknownEntry { prefix: String },
    #[error("'{prefix}' starts more than one entry in the activity log")]
    AmbiguousEntry {
        prefix: String,
        /// Every entry the prefix matches, with its short id and description.
        candidates: Vec<ActivityEntry>,
    },
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Activity(#[from] ActivityError),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error("could not {action} {path}")]
    Io {
        /// "write" or "remove".
        action: &'static str,
        /// Relative to the project folder.
        path: String,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Takes back the entry the project is at: the previous state becomes current, files
    /// included. Unsaved edits are recorded first, so they are taken back (and can be
    /// redone) like any entry.
    ///
    /// # Errors
    ///
    /// [`UndoError::Save`] (`OutOfSyncWithGit`, `HeadOnAnotherBranch`) when plain git moved a
    /// line or HEAD, before anything is written; the wrapped lower-level errors.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn undo(&mut self) -> Result<UndoOutcome, UndoError> {
        todo!("PV-023: undo")
    }

    /// Brings back the entry most recently undone.
    ///
    /// # Errors
    ///
    /// As [`Project::undo`].
    ///
    /// # Panics
    ///
    /// Never.
    pub fn redo(&mut self) -> Result<UndoOutcome, UndoError> {
        todo!("PV-023: redo")
    }

    /// Goes back to the state of the entry whose id starts with `prefix`, anywhere in the log,
    /// including entries that are only on the redo stack. Clears the redo stack.
    ///
    /// # Errors
    ///
    /// [`UndoError::UnknownEntry`], [`UndoError::AmbiguousEntry`] (nothing is changed), and
    /// everything [`Project::undo`] reports.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn undo_to(&mut self, _prefix: &str) -> Result<UndoOutcome, UndoError> {
        todo!("PV-023: undo --to")
    }
}

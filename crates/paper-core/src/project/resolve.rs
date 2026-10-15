//! Resolving a [`Point`] to a commit (01-concepts.md § Points).

use paper_git::{CommitId, GitError};

use super::Project;
use crate::name::LineName;
use crate::point::{DateSpec, Point};

/// What a point turned out to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointKind {
    /// `@`: the current line's draft.
    Draft,
    /// `last` or `last~N`.
    Last,
    /// A line name: its tip.
    Line(LineName),
    /// A change-ID prefix.
    ChangeId,
    /// A commit-hash prefix.
    CommitHash,
    /// `date:…`.
    Date,
}

/// A point resolved against a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub commit: CommitId,
    pub matched: PointKind,
}

/// One of several commits a prefix could mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub commit: CommitId,
    /// The short change ID, or the first 7 hash characters for commits made by plain git.
    pub short_id: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// `available` is the number of saves on the current line.
    #[error(
        "the line has only {available} saves, so 'last~{back}' goes too far back; try `paper history` to see them"
    )]
    NotEnoughHistory { back: u32, available: usize },
    #[error("no save was made on this line on or before {date:?}; try `paper history`")]
    NoSaveBefore { date: DateSpec },
    #[error("'{prefix}' could mean several saves; type more characters to pick one")]
    Ambiguous {
        prefix: String,
        candidates: Vec<Candidate>,
    },
    #[error("'{name}' is not a line, save or date; try `paper history` or `paper lines`")]
    UnknownPoint { name: String },
    #[error(transparent)]
    Git(#[from] GitError),
}

impl Project {
    /// Turns a point into a commit, by the precedence in 01-concepts.md § Points.
    ///
    /// # Errors
    ///
    /// See [`ResolveError`].
    ///
    /// # Panics
    ///
    /// Never in practice.
    pub fn resolve(&self, _point: &Point) -> Result<Resolved, ResolveError> {
        todo!("PV-002: resolve a point")
    }
}

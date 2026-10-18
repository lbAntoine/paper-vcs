//! `paper gc`: trim the activity log to a recent window and rebuild the keep-alive chain
//! (03-architecture.md § On-disk layout).

use std::io;
use std::path::PathBuf;

use paper_git::GitError;

use super::{ActivityEntry, Project};
use crate::activity::ActivityError;

/// How old an entry must be before `gc` removes it: a whole number of days, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcAge {
    days: u32,
}

impl GcAge {
    /// The default window, 90 days.
    pub const DEFAULT: Self = Self { days: 90 };

    /// Parses `--older-than`: `<n>d` (days) or `<n>w` (weeks), at least `1d`.
    ///
    /// # Errors
    ///
    /// [`GcError::InvalidAge`] for `0d`, a missing unit, an unknown unit or garbage.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn parse(_text: &str) -> Result<Self, GcError> {
        todo!("PV-027: parse d and w, reject 0 and garbage")
    }

    /// The window in days.
    #[must_use]
    pub fn days(self) -> u32 {
        self.days
    }
}

impl Default for GcAge {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What `gc` did, or with `dry_run` would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcOutcome {
    pub dry_run: bool,
    /// Entries on the head chain whose op files were (or would be) removed.
    pub removed_entries: usize,
    /// Entries left on the head chain, the horizon and the head included.
    pub kept_entries: usize,
    /// The oldest kept entry: where `activity` and `undo` now stop.
    pub horizon: ActivityEntry,
    /// Files removed (or that would be) besides those of the removed entries: `.tmp*` files,
    /// orphan ops and `view.json.v1`. Paths relative to `.git/paper/`, with `/` separators,
    /// sorted.
    pub swept: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GcError {
    /// `--older-than` is not `<n>d` or `<n>w` with n at least 1.
    #[error("'{given}' is not a duration paper understands")]
    InvalidAge { given: String },
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Activity(#[from] ActivityError),
    #[error("could not {action} {path}")]
    Io {
        /// "write" or "remove".
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Removes the activity entries older than `older_than` (keeping the head, the redo
    /// stacks and everything newer), writes the horizon, rebuilds `refs/paper/keep` and sweeps
    /// crash leftovers. With `dry_run`, only reports. Records no activity entry.
    ///
    /// # Errors
    ///
    /// [`GcError`] if the log can't be read or a file can't be written or removed. Running
    /// `gc` again finishes an interrupted run.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn gc(&mut self, _older_than: GcAge, _dry_run: bool) -> Result<GcOutcome, GcError> {
        todo!("PV-027: gc")
    }
}

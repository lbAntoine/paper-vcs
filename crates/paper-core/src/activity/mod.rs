//! The operation log: every state of the project as an immutable operation, and a head
//! pointer to the current one (03-architecture.md § On-disk layout).
//!
//! Layout under `.git/paper/activity/`: `ops/<id>.json` (written once, never changed) and
//! `head` (one op id, replaced atomically after the op file is on disk).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::view::View;

/// The id of an operation: 32 lowercase hexadecimal characters of randomness.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpId(String);

/// The text was not 32 lowercase hexadecimal characters.
#[derive(Debug, thiserror::Error)]
#[error("'{text}' is not an operation id (32 lowercase hexadecimal characters)")]
pub struct InvalidOpId {
    pub text: String,
}

impl OpId {
    /// A fresh random id.
    #[must_use]
    pub fn random() -> Self {
        todo!("PV-020: 32 lowercase hex characters from the change-id randomness source")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpId {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("PV-020")
    }
}

impl FromStr for OpId {
    type Err = InvalidOpId;

    fn from_str(_text: &str) -> Result<Self, Self::Err> {
        todo!("PV-020")
    }
}

/// What an operation did. Serialised in lowercase (`"init"`, `"save"`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpKind {
    Init,
    Migrate,
    Snapshot,
    Save,
    Restore,
}

/// One immutable operation: the full view after it, and the undo bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    pub id: OpId,
    /// `None` (JSON `null`) for the first op.
    pub parent: Option<OpId>,
    pub kind: OpKind,
    /// RFC 3339.
    pub time: String,
    pub view: View,
    /// The op whose state is current: the op itself for a normal op.
    pub position: OpId,
    /// The redo stack, as op ids. Empty for normal ops.
    pub redo: Vec<OpId>,
    pub description: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("could not read or write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// `head` or an op file exists but is not valid.
    #[error("{path} is damaged: {reason}")]
    Corrupt { path: PathBuf, reason: String },
    /// `head` or an op's `parent` names an op file that isn't there.
    #[error("the operation file {path} is missing")]
    MissingOp { path: PathBuf },
}

/// The operation log of one project, rooted at `<paper_dir>/activity/`.
#[derive(Debug, Clone)]
pub struct ActivityLog {
    // Read once the stubs are implemented (PV-020 GREEN); then drop the allow.
    #[allow(dead_code)]
    root: PathBuf,
}

impl ActivityLog {
    /// The log under `paper_dir`. Touches nothing on disk; directories are created by the
    /// first [`ActivityLog::append`].
    #[must_use]
    pub fn open(_paper_dir: &Path) -> Self {
        todo!("PV-020")
    }

    /// The id `head` holds; `Ok(None)` when there is no head yet.
    ///
    /// # Errors
    ///
    /// [`ActivityError::Corrupt`] if `head` isn't a valid id, [`ActivityError::Io`] otherwise.
    pub fn head(&self) -> Result<Option<OpId>, ActivityError> {
        todo!("PV-020")
    }

    /// Reads one op.
    ///
    /// # Errors
    ///
    /// [`ActivityError::MissingOp`] if the file is absent, [`ActivityError::Corrupt`] if it
    /// isn't a valid op, [`ActivityError::Io`] otherwise.
    pub fn read(&self, _id: &OpId) -> Result<Op, ActivityError> {
        todo!("PV-020")
    }

    /// Writes a new op whose parent is the current head (`None` if there is no head), then
    /// moves `head` to it. The op file is complete on disk before `head` changes.
    ///
    /// # Errors
    ///
    /// [`ActivityError::Io`] if a file can't be written, and whatever [`ActivityLog::head`]
    /// reports.
    pub fn append(
        &self,
        _kind: OpKind,
        _view: &View,
        _description: &str,
    ) -> Result<Op, ActivityError> {
        todo!("PV-020")
    }
}

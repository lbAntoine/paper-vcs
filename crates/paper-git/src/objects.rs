//! Paper's view of git objects: plain data, no gix types

use crate::{BlobId, ChangeId, CommitId, TreeId};

/// Who is acting: used for commit signatures and for reflog entries
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

/// A moment as git records it: seconds since the Unix epoch plus the author's UTC offset
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitTime {
    pub seconds: i64,
    pub offset_seconds: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub identity: Identity,
    pub time: GitTime,
}

/// A commit to be written
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommit {
    pub tree: TreeId,
    pub parents: Vec<CommitId>,
    pub author: Signature,
    pub committer: Signature,
    pub message: String,
    /// Stored as the `change-id` commit header
    pub change_id: Option<ChangeId>,
}

/// A commit read back from the repository
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: CommitId,
    pub tree: TreeId,
    pub parents: Vec<CommitId>,
    pub author: Signature,
    pub committer: Signature,
    pub message: String,
    /// `None` for commits made by plain git, which don't carry the header
    pub change_id: Option<ChangeId>,
}

/// One change to apply to a tree. Paths are relative to the repository root and use `/`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeEdit {
    Upsert {
        path: String,
        blob: BlobId,
        executable: bool,
    },
    Remove {
        path: String,
    },
}

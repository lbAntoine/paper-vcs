//! Git storage layer for paper-vcs, built on gitoxide.
//!
//! Nothing gix-specific leaks out of this crate: callers see paper's own id and commit types.

pub mod change_id;
pub mod config;
mod error;
pub mod id;
pub mod objects;
mod repo;

pub use change_id::ChangeId;
pub use config::configured_identity;
pub use error::GitError;
pub use id::{BlobId, CommitId, TreeId};
pub use objects::{Commit, FileKind, GitTime, Identity, NewCommit, Signature, TreeEdit, TreeFile};
pub use repo::GitRepo;

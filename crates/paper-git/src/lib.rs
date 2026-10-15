//! Git storage layer for paper-vcs, built on gitoxide.
//!
//! Nothing gix-specific leaks out of this crate: callers see paper's own id and commit types.

pub mod change_id;
mod error;
pub mod id;
pub mod objects;
mod repo;

pub use change_id::ChangeId;
pub use error::GitError;
pub use id::{BlobId, CommitId, TreeId};
pub use objects::{Commit, GitTime, Identity, NewCommit, Signature, TreeEdit};
pub use repo::GitRepo;

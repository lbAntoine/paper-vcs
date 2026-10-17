//! The operation log: every state of the project as an immutable operation, and a head
//! pointer to the current one (03-architecture.md § On-disk layout).
//!
//! Layout under `.git/paper/activity/`: `ops/<id>.json` (written once, never changed) and
//! `head` (one op id, replaced atomically after the op file is on disk).

mod id;
mod op;
mod store;

pub use id::{InvalidOpId, OpId};
pub use op::{Op, OpKind};
pub use store::{ActivityError, ActivityLog};

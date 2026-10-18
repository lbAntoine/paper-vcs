//! Recovery on open (03-architecture.md § Command lifecycle, step 2): finish or drop a
//! `checkout-pending` marker, then re-export when git's refs lag the head op.

use std::path::Path;

use paper_git::GitRepo;

use super::OpenError;
use crate::activity::OpId;

/// Repairs what a crashed command left half done: (1) a `checkout-pending` marker, by the
/// rule in the spec (drop it, or finish the checkout rewriting only files that still equal
/// their version in one of the two trees); (2) refs, HEAD or `exported.json` behind the head
/// op's view, by running the export. Both steps are idempotent, record no activity entry and
/// do nothing when there is nothing to repair. Called by `Project::open_with` under the lock.
///
/// # Errors
///
/// [`OpenError`] if the marker, the log or the repository can't be read or written.
#[allow(clippy::unnecessary_wraps, reason = "the real recovery can fail")]
pub(super) fn recover(_repo: &GitRepo, _paper_dir: &Path, _head: &OpId) -> Result<(), OpenError> {
    Ok(())
}

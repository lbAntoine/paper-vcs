//! Reading the user's identity from git's configuration.

use std::path::Path;

use crate::{GitError, Identity};

/// The `user.name` and `user.email` git would use for a commit made from `path`.
///
/// Looks at the repository's own config when `path` is inside one, then the global and
/// system config (honouring `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM` and `HOME`).
/// Returns `None` when either value is missing or empty.
///
/// # Errors
///
/// [`GitError`] if a config file exists but can't be read or parsed.
///
/// # Panics
///
/// Never.
pub fn configured_identity(_path: &Path) -> Result<Option<Identity>, GitError> {
    todo!("PV-003: read user.name and user.email from git config")
}

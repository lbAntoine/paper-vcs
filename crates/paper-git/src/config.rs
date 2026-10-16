//! Reading the user's identity from git's configuration.

use std::path::Path;

use crate::error::ResultExt;
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
pub fn configured_identity(path: &Path) -> Result<Option<Identity>, GitError> {
    let (name, email) = if let Ok(repo) = gix::discover(path) {
        let config = repo.config_snapshot();
        (
            value(config.string("user.name")),
            value(config.string("user.email")),
        )
    } else {
        let config = gix::config::File::from_globals().context("read the git configuration")?;
        (
            value(config.string("user.name")),
            value(config.string("user.email")),
        )
    };
    Ok(match (name, email) {
        (Some(name), Some(email)) if !name.trim().is_empty() && !email.trim().is_empty() => {
            Some(Identity { name, email })
        }
        _ => None,
    })
}

fn value<T: ToString>(raw: Option<T>) -> Option<String> {
    raw.map(|v| v.to_string())
}

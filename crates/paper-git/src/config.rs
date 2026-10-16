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
/// [`GitError`] if a config file exists but can't be read or parsed, or if `path` itself holds
/// a damaged repository. A stray or broken `.git` above `path` is ignored.
///
/// # Panics
///
/// Never.
pub fn configured_identity(path: &Path) -> Result<Option<Identity>, GitError> {
    // Start at the nearest folder that exists: `paper init new-folder` asks before it is made.
    let start = path.ancestors().find(|p| p.exists()).unwrap_or(path);
    let (name, email) = if let Some(repo) = open_repository(start, path)? {
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

/// The repository around `start`, or `None` when there is none to use.
///
/// A `.git` entry directly at the `target` itself that can't be opened is an error: the folder
/// claims to be a repository and is damaged. One anywhere above it (including at `start`, the
/// nearest existing folder, when the target is still to be created) that can't be opened (a
/// stray file or an empty folder) is not the target's business, so the caller falls back to
/// the global configuration.
fn open_repository(start: &Path, target: &Path) -> Result<Option<gix::Repository>, GitError> {
    match gix::discover(start) {
        Ok(repo) => Ok(Some(repo)),
        Err(err) if target.join(".git").exists() => Err(err).context("open the repository"),
        Err(_) => Ok(None),
    }
}

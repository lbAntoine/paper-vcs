//! The project lock: one paper command at a time (03-architecture.md § Command lifecycle).
//!
//! An exclusive OS lock on `.git/paper/lock`. The OS releases it when the holder exits or
//! crashes, so a stale lock file is harmless.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a command waits for another paper command before giving up.
pub const DEFAULT_LOCK_WAIT: Duration = Duration::from_secs(5);

/// Locking choices for [`super::Project::open_with`] and [`super::Project::init_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockOptions {
    /// How long to wait for the lock before reporting the project as busy.
    pub wait: Duration,
}

impl Default for LockOptions {
    /// Waits [`DEFAULT_LOCK_WAIT`] (5 s).
    fn default() -> Self {
        todo!("PV-019: default wait of 5 s")
    }
}

/// Why the lock could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// Another process held the lock for the whole wait.
    #[error("another paper command holds the lock")]
    Busy {
        /// The holder's process id, when the lock file names one.
        holder: Option<u32>,
    },
    #[error("could not lock {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The held lock; released when dropped.
#[derive(Debug)]
pub struct ProjectLock {
    _file: std::fs::File,
}

impl ProjectLock {
    /// Takes the exclusive lock on `<paper_dir>/lock`, creating the file if missing, polling
    /// until `wait` has passed. Writes this process's id into the file as a hint.
    ///
    /// # Errors
    ///
    /// [`LockError::Busy`] when the wait runs out, [`LockError::Io`] otherwise.
    pub fn acquire(_paper_dir: &Path, _wait: Duration) -> Result<Self, LockError> {
        todo!("PV-019: acquire the lock")
    }
}

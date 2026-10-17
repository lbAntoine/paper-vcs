//! The project lock: one paper command at a time (03-architecture.md § Command lifecycle).
//!
//! An exclusive OS lock on `.git/paper/lock`. The OS releases it when the holder exits or
//! crashes, so a stale lock file is harmless.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LOCK_FILE: &str = "lock";
const POLL: Duration = Duration::from_millis(50);

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
        Self {
            wait: DEFAULT_LOCK_WAIT,
        }
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
    /// The lock file could not be created, locked or written.
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
    _file: File,
}

impl ProjectLock {
    /// Takes the exclusive lock on `<paper_dir>/lock`, creating the file if missing, polling
    /// until `wait` has passed. Writes this process's id into the file as a hint.
    ///
    /// # Errors
    ///
    /// [`LockError::Busy`] when the wait runs out, [`LockError::Io`] otherwise.
    pub fn acquire(paper_dir: &Path, wait: Duration) -> Result<Self, LockError> {
        let path = paper_dir.join(LOCK_FILE);
        let io = |source| LockError::Io {
            path: path.clone(),
            source,
        };
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(io)?;
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {
                    if started.elapsed() >= wait {
                        return Err(LockError::Busy {
                            holder: read_holder(&path),
                        });
                    }
                    std::thread::sleep(POLL.min(wait));
                }
                Err(TryLockError::Error(source)) => return Err(io(source)),
            }
        }
        // The pid is a hint for the error message; the OS lock is the lock.
        file.set_len(0).map_err(io)?;
        write!(file, "{}", std::process::id()).map_err(io)?;
        file.flush().map_err(io)?;
        Ok(Self { _file: file })
    }
}

fn read_holder(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

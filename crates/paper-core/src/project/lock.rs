//! The project lock: one paper command at a time (03-architecture.md § Command lifecycle).
//!
//! An exclusive OS lock on `.git/paper/lock`. The OS releases it when the holder exits or
//! crashes, so a stale lock file is harmless.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LOCK_FILE: &str = "lock";
/// The holder's pid. Never locked, so every OS lets other processes read it (the lock file
/// itself is mandatory-locked on Windows).
const HOLDER_FILE: &str = "lock-holder";
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
    holder_path: PathBuf,
}

impl ProjectLock {
    /// Takes the exclusive lock on `<paper_dir>/lock`, creating the file if missing, polling
    /// until `wait` has passed. Writes this process's id to `<paper_dir>/lock-holder` as a hint
    /// for the busy message. The lock file itself is never read, written or deleted.
    ///
    /// # Errors
    ///
    /// [`LockError::Busy`] when the wait runs out, [`LockError::Io`] otherwise.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn acquire(paper_dir: &Path, wait: Duration) -> Result<Self, LockError> {
        let path = paper_dir.join(LOCK_FILE);
        let io = |source| LockError::Io {
            path: path.clone(),
            source,
        };
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(io)?;
        let holder_path = paper_dir.join(HOLDER_FILE);
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {
                    if started.elapsed() >= wait {
                        return Err(LockError::Busy {
                            holder: read_holder(&holder_path),
                        });
                    }
                    std::thread::sleep(POLL.min(wait));
                }
                Err(TryLockError::Error(source)) => return Err(io(source)),
            }
        }
        // The pid is a hint for the error message; the OS lock is the lock, so a failed write
        // is ignored and a stale value (after a crash) is replaced by the next holder.
        let _ = std::fs::write(&holder_path, std::process::id().to_string());
        Ok(Self {
            _file: file,
            holder_path,
        })
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        // Still locked here (`_file` drops after this), so the hint is ours to remove. The lock
        // file is never deleted.
        let _ = std::fs::remove_file(&self.holder_path);
    }
}

fn read_holder(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

impl ProjectLock {
    /// Takes the project lock for a change made without opening the project (project-level
    /// `config set`). Returns `None` when `workdir` has no `.git/paper/` yet, because then no
    /// paper command can be running there.
    ///
    /// # Errors
    ///
    /// [`LockError::Busy`] when the wait runs out, [`LockError::Io`] otherwise.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn acquire_for_workdir(workdir: &Path, wait: Duration) -> Result<Option<Self>, LockError> {
        let paper_dir = workdir.join(".git").join("paper");
        if paper_dir.is_dir() {
            Self::acquire(&paper_dir, wait).map(Some)
        } else {
            Ok(None)
        }
    }
}

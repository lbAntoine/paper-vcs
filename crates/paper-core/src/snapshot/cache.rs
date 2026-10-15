//! The stat cache: lets a snapshot skip reading files that haven't changed.
//!
//! Stored in `.git/paper/wc-state.json`. It's purely an optimisation: if it's missing or
//! damaged, the next snapshot reads every file once and rebuilds it.

use std::collections::BTreeMap;
use std::fs::{self, Metadata};
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use paper_git::BlobId;
use serde::{Deserialize, Serialize};

use crate::fs_util;

const FILE_NAME: &str = "wc-state.json";

/// Timestamps closer than this to the snapshot start aren't trusted: filesystems store
/// mtimes with limited precision, so a file edited right after it was cached could
/// otherwise keep the same size and mtime and be missed ("racy git").
const RACY_WINDOW_NANOS: u64 = 2_000_000_000;

/// What we remember about a file on disk, compared cheaply on the next snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStamp {
    pub size: u64,
    pub mtime_nanos: u64,
    /// Unix inode; `None` on platforms without one.
    pub inode: Option<u64>,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedFile {
    stamp: FileStamp,
    blob: BlobId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatCache {
    /// When the snapshot that produced this cache started.
    started_nanos: u64,
    files: BTreeMap<String, CachedFile>,
}

impl FileStamp {
    pub fn new(metadata: &Metadata, executable: bool) -> Self {
        Self {
            size: metadata.len(),
            mtime_nanos: metadata.modified().map_or(0, nanos_since_epoch),
            inode: inode(metadata),
            executable,
        }
    }
}

impl StatCache {
    /// An empty cache recording that a snapshot started at `started_nanos`.
    pub fn starting_at(started_nanos: u64) -> Self {
        Self {
            started_nanos,
            files: BTreeMap::new(),
        }
    }

    /// The blob recorded for `path`, if the file on disk still has exactly the same stamp
    /// and its mtime is safely older than the snapshot that recorded it.
    pub fn lookup(&self, path: &str, stamp: &FileStamp) -> Option<&BlobId> {
        let cached = self.files.get(path)?;
        let safely_old = stamp.mtime_nanos.saturating_add(RACY_WINDOW_NANOS) < self.started_nanos;
        (cached.stamp == *stamp && safely_old).then_some(&cached.blob)
    }

    /// This cache with `path` recorded (builder style: consumes and returns the cache).
    #[must_use]
    pub fn with_file(mut self, path: String, stamp: FileStamp, blob: BlobId) -> Self {
        self.files.insert(path, CachedFile { stamp, blob });
        self
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Reads the cache from `paper_dir`. A missing or damaged cache is simply empty.
    pub fn load(paper_dir: &Path) -> Self {
        fs::read(paper_dir.join(FILE_NAME))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// # Errors
    ///
    /// Any I/O error while writing the file.
    ///
    /// # Panics
    ///
    /// Never in practice: every field serialises to JSON.
    pub fn save(&self, paper_dir: &Path) -> io::Result<()> {
        let json = serde_json::to_vec(self).expect("a stat cache always serialises");
        fs_util::write_atomic(&paper_dir.join(FILE_NAME), &json)
    }
}

/// Nanoseconds since the Unix epoch; times before 1970 count as 0.
pub fn nanos_since_epoch(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "same signature as the non-unix version"
)]
fn inode(metadata: &Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.ino())
}

#[cfg(not(unix))]
fn inode(_metadata: &Metadata) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 1_000_000_000;

    fn stamp(mtime_nanos: u64) -> FileStamp {
        FileStamp {
            size: 4,
            mtime_nanos,
            inode: Some(7),
            executable: false,
        }
    }

    fn blob() -> BlobId {
        "a".repeat(40).parse().unwrap()
    }

    #[test]
    fn hit_when_stamp_matches_and_file_is_old() {
        let cache = StatCache::starting_at(100 * SECOND).with_file(
            "a.tex".into(),
            stamp(10 * SECOND),
            blob(),
        );
        assert_eq!(cache.lookup("a.tex", &stamp(10 * SECOND)), Some(&blob()));
    }

    #[test]
    fn miss_when_anything_in_the_stamp_differs() {
        let cache = StatCache::starting_at(100 * SECOND).with_file(
            "a.tex".into(),
            stamp(10 * SECOND),
            blob(),
        );
        let bigger = FileStamp {
            size: 5,
            ..stamp(10 * SECOND)
        };
        let chmodded = FileStamp {
            executable: true,
            ..stamp(10 * SECOND)
        };
        assert_eq!(cache.lookup("a.tex", &bigger), None);
        assert_eq!(cache.lookup("a.tex", &chmodded), None);
        assert_eq!(cache.lookup("a.tex", &stamp(11 * SECOND)), None);
        assert_eq!(cache.lookup("other.tex", &stamp(10 * SECOND)), None);
    }

    #[test]
    fn miss_when_file_was_modified_close_to_the_snapshot_start() {
        let cache = StatCache::starting_at(100 * SECOND).with_file(
            "a.tex".into(),
            stamp(99 * SECOND),
            blob(),
        );
        assert_eq!(cache.lookup("a.tex", &stamp(99 * SECOND)), None);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let cache = StatCache::starting_at(100 * SECOND).with_file(
            "a.tex".into(),
            stamp(10 * SECOND),
            blob(),
        );
        cache.save(dir.path()).unwrap();
        assert_eq!(StatCache::load(dir.path()), cache);
    }

    #[test]
    fn missing_or_damaged_cache_loads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(StatCache::load(dir.path()).is_empty());
        fs::write(dir.path().join(FILE_NAME), "{ nope").unwrap();
        assert!(StatCache::load(dir.path()).is_empty());
    }
}

//! Snapshot: record the working files as a git tree (invariant I1, 01-concepts.md).
//!
//! Every file that isn't ignored is tracked automatically (I7). Files already tracked stay
//! tracked even if a `.gitignore` rule matches them later, exactly like git.

mod cache;
mod walk;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use paper_git::{BlobId, FileKind, GitError, GitRepo, TreeEdit, TreeFile, TreeId};

pub use cache::{FileStamp, StatCache};

/// Limits applied to files during a snapshot (see 04-content.md § Large files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotOptions {
    /// Files at least this big are reported in [`Snapshot::large`].
    pub warn_file_bytes: u64,
    /// New files bigger than this are not tracked. Already-tracked files stay tracked.
    pub max_file_bytes: u64,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            warn_file_bytes: 10 * 1024 * 1024,
            max_file_bytes: 100 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

/// A file that differs between the base tree and the working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    TooLarge { bytes: u64 },
    Symlink,
    NestedRepository,
    NonUtf8Path,
}

/// Something in the working directory that the snapshot deliberately did not record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub path: String,
    pub reason: SkipReason,
}

/// How much work a snapshot did; useful to check the stat cache is doing its job.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SnapshotStats {
    /// Files whose content was read and hashed.
    pub hashed: usize,
    /// Files recognised as unchanged from the stat cache alone.
    pub cached: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub tree: TreeId,
    /// Sorted by path.
    pub changes: Vec<Change>,
    pub skipped: Vec<Skipped>,
    /// Tracked files at or above [`SnapshotOptions::warn_file_bytes`], with their size.
    pub large: Vec<(String, u64)>,
    pub stats: SnapshotStats,
    /// The cache to save for the next snapshot.
    pub cache: StatCache,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("could not list the project's files")]
    Walk(#[from] ignore::Error),
    #[error("could not read {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Relative paths of the files a snapshot of `root` would see (ignore rules applied), sorted.
///
/// # Errors
///
/// [`SnapshotError`] if the folder can't be listed.
pub(crate) fn visible_files(root: &std::path::Path) -> Result<Vec<String>, SnapshotError> {
    Ok(walk::walk(root)?
        .files
        .into_iter()
        .map(|f| f.path)
        .collect())
}

/// Records the working directory of `repo` as a tree, starting from `base` (the draft's tree).
///
/// `previous` is the cache from the last snapshot. Nothing is written to `.git/paper/`: the
/// caller decides whether to save [`Snapshot::cache`].
///
/// # Errors
///
/// [`SnapshotError`] if the working directory can't be listed, a file can't be read, or the
/// repository can't be written.
pub fn snapshot(
    repo: &GitRepo,
    base: &TreeId,
    previous: &StatCache,
    options: &SnapshotOptions,
) -> Result<Snapshot, SnapshotError> {
    let started = cache::nanos_since_epoch(SystemTime::now());
    let base_files = repo.tree_files(base)?;
    let walked = walk::walk(repo.workdir())?;

    let mut state = State {
        repo,
        previous,
        options,
        base_files: &base_files,
        cache: StatCache::starting_at(started),
        edits: Vec::new(),
        changes: Vec::new(),
        skipped: Vec::new(),
        large: Vec::new(),
        stats: SnapshotStats::default(),
    };

    let mut seen = BTreeSet::new();
    for file in &walked.files {
        seen.insert(file.path.as_str());
        let metadata = metadata(&file.absolute)?;
        state.record(&file.path, &file.absolute, &metadata)?;
    }

    // Tracked files the walk didn't produce: deleted, ignored-but-tracked, or skipped kinds.
    let mut listings = Listings::default();
    for (path, tracked) in &base_files {
        if seen.contains(path.as_str()) {
            continue;
        }
        let absolute = repo.workdir().join(path);
        // On a filesystem that ignores case, `Notes.tex` finds a file spelled `notes.tex`:
        // the draft records the spelling the walk sees, so that is not this file.
        let found = match fs::symlink_metadata(&absolute) {
            Ok(_) if !listings.spelled_exactly(repo.workdir(), path) => {
                Err(io::ErrorKind::NotFound.into())
            }
            other => other,
        };
        match found {
            // NotADirectory: a parent that was a directory is now a file.
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                state.remove(path);
            }
            Err(source) => {
                return Err(SnapshotError::Io {
                    path: absolute,
                    source,
                });
            }
            Ok(meta) if meta.is_file() => state.record(path, &absolute, &meta)?,
            Ok(meta) if meta.is_dir() => state.remove(path),
            // A tracked symlink is kept exactly as it is in the tree.
            Ok(_) if tracked.kind == FileKind::Symlink => {}
            Ok(_) => state.remove(path),
        }
    }

    state.skipped.extend(
        walked
            .skipped
            .into_iter()
            .filter(|skipped| !base_files.contains_key(&skipped.path)),
    );

    let tree = if state.edits.is_empty() {
        base.clone()
    } else {
        // Removals first: a file turning into a directory (or back) must not have its new
        // entries wiped by the removal of the old path.
        let (removals, upserts): (Vec<_>, Vec<_>) = state
            .edits
            .iter()
            .cloned()
            .partition(|edit| matches!(edit, TreeEdit::Remove { .. }));
        let ordered: Vec<TreeEdit> = removals.into_iter().chain(upserts).collect();
        repo.edit_tree(base, &ordered)?
    };
    let mut changes = state.changes;
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Snapshot {
        tree,
        changes,
        skipped: state.skipped,
        large: state.large,
        stats: state.stats,
        cache: state.cache,
    })
}

/// Accumulates the result while walking; private to one `snapshot` call.
struct State<'a> {
    repo: &'a GitRepo,
    previous: &'a StatCache,
    options: &'a SnapshotOptions,
    base_files: &'a BTreeMap<String, TreeFile>,
    cache: StatCache,
    edits: Vec<TreeEdit>,
    changes: Vec<Change>,
    skipped: Vec<Skipped>,
    large: Vec<(String, u64)>,
    stats: SnapshotStats,
}

impl State<'_> {
    fn record(
        &mut self,
        path: &str,
        absolute: &Path,
        metadata: &Metadata,
    ) -> Result<(), SnapshotError> {
        let tracked = self.base_files.get(path);
        let bytes = metadata.len();
        if tracked.is_none() && bytes > self.options.max_file_bytes {
            self.skipped.push(Skipped {
                path: path.to_owned(),
                reason: SkipReason::TooLarge { bytes },
            });
            return Ok(());
        }
        if bytes >= self.options.warn_file_bytes {
            self.large.push((path.to_owned(), bytes));
        }

        let executable = is_executable(metadata, tracked);
        let stamp = FileStamp::new(metadata, executable);
        let blob = self.blob_for(path, absolute, &stamp)?;
        self.cache =
            std::mem::take(&mut self.cache).with_file(path.to_owned(), stamp, blob.clone());

        let kind = if executable {
            FileKind::Executable
        } else {
            FileKind::Regular
        };
        let change = match tracked {
            None => Some(ChangeKind::Added),
            Some(file) if file.blob != blob || file.kind != kind => Some(ChangeKind::Modified),
            Some(_) => None,
        };
        if let Some(kind) = change {
            self.edits.push(TreeEdit::Upsert {
                path: path.to_owned(),
                blob,
                executable,
            });
            self.changes.push(Change {
                path: path.to_owned(),
                kind,
            });
        }
        Ok(())
    }

    fn blob_for(
        &mut self,
        path: &str,
        absolute: &Path,
        stamp: &FileStamp,
    ) -> Result<BlobId, SnapshotError> {
        if let Some(blob) = self.previous.lookup(path, stamp) {
            self.stats.cached += 1;
            return Ok(blob.clone());
        }
        self.stats.hashed += 1;
        let content = fs::read(absolute).map_err(|source| SnapshotError::Io {
            path: absolute.to_owned(),
            source,
        })?;
        Ok(self.repo.write_blob(&content)?)
    }

    fn remove(&mut self, path: &str) {
        self.edits.push(TreeEdit::Remove {
            path: path.to_owned(),
        });
        self.changes.push(Change {
            path: path.to_owned(),
            kind: ChangeKind::Deleted,
        });
    }
}

/// Folder listings, each read at most once per snapshot.
#[derive(Default)]
struct Listings {
    /// `None` when the folder can't be listed.
    folders: BTreeMap<PathBuf, Option<BTreeSet<OsString>>>,
}

impl Listings {
    /// Whether every part of `path` (relative to `root`) is listed with exactly that
    /// spelling. A folder that can't be listed counts as matching, as it did before
    /// spelling was checked.
    fn spelled_exactly(&mut self, root: &Path, path: &str) -> bool {
        let mut folder = root.to_owned();
        for part in path.split('/') {
            let names = self.folders.entry(folder.clone()).or_insert_with(|| {
                fs::read_dir(&folder)
                    .ok()
                    .map(|entries| entries.flatten().map(|e| e.file_name()).collect())
            });
            if names
                .as_ref()
                .is_some_and(|n| !n.contains(OsStr::new(part)))
            {
                return false;
            }
            folder.push(part);
        }
        true
    }
}

fn metadata(path: &Path) -> Result<Metadata, SnapshotError> {
    fs::symlink_metadata(path).map_err(|source| SnapshotError::Io {
        path: path.to_owned(),
        source,
    })
}

#[cfg(unix)]
fn is_executable(metadata: &Metadata, _tracked: Option<&TreeFile>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

/// Windows has no executable bit: keep whatever the tree already says.
#[cfg(not(unix))]
fn is_executable(_metadata: &Metadata, tracked: Option<&TreeFile>) -> bool {
    tracked.is_some_and(|file| file.kind == FileKind::Executable)
}

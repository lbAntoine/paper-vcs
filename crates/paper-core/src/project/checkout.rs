//! Rewriting working files to match a tree: the apply step shared by `restore` and by
//! `undo`, `redo` and `undo --to` (03-architecture.md § Command lifecycle, step 8).
//!
//! The rules are restore's: files paper doesn't record (symlinks, oversized files, nested
//! repositories, ignored or untracked files in the way) are never overwritten or deleted, and
//! everything is decided, and every blob read, before the first byte on disk changes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use paper_git::{FileKind, GitError, TreeFile, TreeId};
use serde::{Deserialize, Serialize};

use super::Project;
use super::restore::{RestoreSkip, SkippedPath};
use crate::activity::OpId;
use crate::fs_util::write_atomic;
use crate::snapshot::{Change, ChangeKind, SkipReason, Skipped};

const PENDING_FILE: &str = "checkout-pending";
const PENDING_SCHEMA: u32 = 1;

/// What a checkout will do to the working files, fully decided.
pub(super) struct CheckoutPlan {
    /// Files to remove, before any write.
    pub deletes: Vec<String>,
    /// Files to write after the deletions, with their bytes.
    pub writes: Vec<(String, FileKind, Vec<u8>)>,
    /// Files left alone, sorted by path.
    pub skipped: Vec<SkippedPath>,
    /// Files rewritten or deleted, sorted by path.
    pub changes: Vec<Change>,
}

/// A working-file change that failed.
pub(super) struct ApplyFailure {
    pub action: &'static str,
    pub path: String,
    pub source: io::Error,
}

/// `.git/paper/checkout-pending`: written before the first working file changes and removed
/// once the stat cache matches the new files. Finding one on open means a checkout was cut
/// short; finishing it is PV-025.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct PendingCheckout {
    schema: u32,
    /// The op that asked for the checkout.
    op: String,
    /// The draft tree the working files are being made to match.
    tree: String,
}

impl PendingCheckout {
    pub(super) fn new(op: &OpId, tree: &TreeId) -> Self {
        Self {
            schema: PENDING_SCHEMA,
            op: op.as_str().to_owned(),
            tree: tree.as_str().to_owned(),
        }
    }

    /// Writes the marker atomically.
    pub(super) fn write(&self, paper_dir: &Path) -> io::Result<()> {
        let mut json = serde_json::to_vec_pretty(self).expect("the marker always serialises");
        json.push(b'\n');
        write_atomic(&paper_dir.join(PENDING_FILE), &json)
    }

    /// Removes the marker; absent is fine.
    pub(super) fn clear(paper_dir: &Path) -> io::Result<()> {
        match fs::remove_file(paper_dir.join(PENDING_FILE)) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    }
}

impl Project {
    /// Decides how to turn the working files, which hold `current`, into `wanted`, for the
    /// paths in `targets`. `snapshot_skipped` is what the latest snapshot left out.
    pub(super) fn plan_checkout(
        &self,
        targets: BTreeSet<&String>,
        wanted: &BTreeMap<String, TreeFile>,
        current: &BTreeMap<String, TreeFile>,
        snapshot_skipped: &[Skipped],
    ) -> Result<CheckoutPlan, GitError> {
        let mut skipped = Vec::new();
        let mut deletes: Vec<&String> = Vec::new();
        let mut candidates = Vec::new();
        for path in targets {
            let want = wanted.get(path);
            let kept = left_alone(snapshot_skipped, path)
                .or_else(|| self.on_disk_symlink(path))
                .or_else(|| {
                    (want.is_some_and(|f| f.kind == FileKind::Symlink))
                        .then_some(RestoreSkip::SavedSymlink)
                });
            if let Some(reason) = kept {
                skipped.push(SkippedPath::new(path, reason));
            } else if want.is_none() {
                deletes.push(path);
            } else if want != current.get(path) {
                candidates.push(path);
            }
        }
        let plan = self.settle(candidates, deletes, |p| current.contains_key(p));
        skipped.extend(
            plan.blocked
                .iter()
                .map(|(path, reason)| SkippedPath::new(path, reason.clone())),
        );
        skipped.sort_by(|a, b| a.path.cmp(&b.path));
        let mut writes = Vec::new();
        for path in &plan.writes {
            let file = &wanted[*path];
            writes.push(((*path).clone(), file.kind, self.repo.read_blob(&file.blob)?));
        }
        let mut changes: Vec<Change> = plan
            .deletes
            .iter()
            .map(|path| Change {
                path: (*path).clone(),
                kind: ChangeKind::Deleted,
            })
            .chain(writes.iter().map(|(path, _, _)| Change {
                path: path.clone(),
                kind: if current.contains_key(path) {
                    ChangeKind::Modified
                } else {
                    ChangeKind::Added
                },
            }))
            .collect();
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(CheckoutPlan {
            deletes: plan.deletes.into_iter().cloned().collect(),
            writes,
            skipped,
            changes,
        })
    }

    /// Carries out a plan: deletions first (so a file turning into a directory finds room),
    /// then the writes. `write_action` names a failed write in the error ("restore", "write").
    pub(super) fn apply_checkout(
        &self,
        plan: &CheckoutPlan,
        write_action: &'static str,
    ) -> Result<(), ApplyFailure> {
        for path in &plan.deletes {
            self.delete_working_file(path)?;
        }
        for (path, kind, bytes) in &plan.writes {
            let fail = |source| ApplyFailure {
                action: write_action,
                path: path.clone(),
                source,
            };
            let absolute = self.repo.workdir().join(path);
            if let Some(parent) = absolute.parent() {
                fs::create_dir_all(parent).map_err(fail)?;
            }
            write_atomic(&absolute, bytes).map_err(fail)?;
            set_executable(&absolute, *kind == FileKind::Executable).map_err(fail)?;
        }
        Ok(())
    }

    /// Whether the path, or any directory above it, is a symlink on disk.
    fn on_disk_symlink(&self, path: &str) -> Option<RestoreSkip> {
        let root = self.repo.workdir();
        let mut prefix = String::new();
        path.split('/').find_map(|part| {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            let linked =
                fs::symlink_metadata(root.join(&prefix)).is_ok_and(|m| m.file_type().is_symlink());
            linked.then(|| {
                if prefix == path {
                    RestoreSkip::Symlink
                } else {
                    RestoreSkip::SymlinkedFolder
                }
            })
        })
    }

    /// Removes a file, then the directories it leaves empty.
    fn delete_working_file(&self, path: &str) -> Result<(), ApplyFailure> {
        let root = self.repo.workdir();
        let absolute = root.join(path);
        match fs::remove_file(&absolute) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(ApplyFailure {
                    action: "remove",
                    path: path.to_owned(),
                    source,
                });
            }
        }
        let mut dir = absolute.parent();
        while let Some(current) = dir.filter(|d| *d != root && d.starts_with(root)) {
            // `remove_dir` only succeeds on an empty directory.
            if fs::remove_dir(current).is_err() {
                break;
            }
            dir = current.parent();
        }
        Ok(())
    }
}

/// Why the snapshot left `path` out (or something above it, like a nested repository).
pub(super) fn left_alone(skipped: &[Skipped], path: &str) -> Option<RestoreSkip> {
    let found = skipped.iter().find(|s| {
        path == s.path
            || path
                .strip_prefix(s.path.as_str())
                .is_some_and(|r| r.starts_with('/'))
    })?;
    Some(match found.reason {
        SkipReason::TooLarge { bytes } => RestoreSkip::TooLarge { bytes },
        SkipReason::Symlink if found.path == path => RestoreSkip::Symlink,
        SkipReason::Symlink => RestoreSkip::SymlinkedFolder,
        SkipReason::NestedRepository => RestoreSkip::NestedRepository,
        SkipReason::NonUtf8Path => RestoreSkip::NonUtf8Path,
    })
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_round_trips_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let op = OpId::random();
        let tree: TreeId = "ab".repeat(20).parse().unwrap();
        let marker = PendingCheckout::new(&op, &tree);
        marker.write(dir.path()).unwrap();

        let bytes = fs::read(dir.path().join(PENDING_FILE)).unwrap();
        let back: PendingCheckout = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, marker);
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["schema"], 1);
        assert_eq!(json["op"], op.as_str());
        assert_eq!(json["tree"], tree.as_str());

        PendingCheckout::clear(dir.path()).unwrap();
        assert!(!dir.path().join(PENDING_FILE).exists());
        PendingCheckout::clear(dir.path()).unwrap();
    }
}

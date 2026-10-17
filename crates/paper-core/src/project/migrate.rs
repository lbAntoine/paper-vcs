//! Format 1 to format 2: `view.json` becomes the first op of the activity log
//! (03-architecture.md § On-disk layout).
//!
//! Steps, in order: a `migrate` op from `view.json`, then `head`, then `version` = 2, then
//! `view.json` renamed to `view.json.v1`. Each step is safe to repeat, so a crash is finished
//! by the next open. Nothing in git changes.

use std::fs;
use std::path::Path;

use super::{FORMAT_VERSION, OpenError, VERSION_FILE};
use crate::activity::{ActivityError, ActivityLog, OpKind};
use crate::fs_util::write_atomic;
use crate::view::View;

const OLD_VIEW: &str = "view.json";
const KEPT_VIEW: &str = "view.json.v1";

/// Finishes (or does) the migration of a format 1 project. The caller holds the lock.
pub(super) fn migrate_to_current(paper_dir: &Path) -> Result<(), OpenError> {
    let log = ActivityLog::open(paper_dir);
    if log.head()?.is_none() {
        let view = View::load(paper_dir)?.ok_or_else(|| {
            OpenError::Activity(ActivityError::MissingHead {
                path: paper_dir.join(OLD_VIEW),
            })
        })?;
        log.append(OpKind::Migrate, &view, "migrate from format 1")?;
    }
    let version_path = paper_dir.join(VERSION_FILE);
    write_atomic(&version_path, format!("{FORMAT_VERSION}\n").as_bytes()).map_err(|source| {
        OpenError::Io {
            path: version_path,
            source,
        }
    })?;
    retire_old_view(paper_dir)
}

/// Renames a leftover `view.json` to `view.json.v1` (a no-op when there is none).
pub(super) fn retire_old_view(paper_dir: &Path) -> Result<(), OpenError> {
    let old = paper_dir.join(OLD_VIEW);
    if old.exists() {
        fs::rename(&old, paper_dir.join(KEPT_VIEW))
            .map_err(|source| OpenError::Io { path: old, source })?;
    }
    Ok(())
}

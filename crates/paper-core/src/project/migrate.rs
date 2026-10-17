//! Format 1 to format 2: `view.json` becomes the first op of the activity log
//! (03-architecture.md § On-disk layout).
//!
//! Steps, in order: a `migrate` op from `view.json`, then `head`, then `version` = 2, then
//! `view.json` renamed to `view.json.v1`. Each step is safe to repeat, so a crash is finished
//! by the next open. The `migrate` op gets a keep-alive commit and an export, like any op;
//! the refs already match, and the index is left alone.

use std::fs;
use std::path::Path;

use paper_git::{GitRepo, Identity};

use super::export::{KEEP_REF, Mode, export, write_keep};
use super::{ExportError, FORMAT_VERSION, OpenError, VERSION_FILE};
use crate::activity::{ActivityError, ActivityLog, OpKind};
use crate::fs_util::write_atomic;
use crate::view::View;

const OLD_VIEW: &str = "view.json";
const KEPT_VIEW: &str = "view.json.v1";

/// Finishes (or does) the migration of a format 1 project. The caller holds the lock.
pub(super) fn migrate_to_current(
    repo: &GitRepo,
    identity: &Identity,
    paper_dir: &Path,
) -> Result<(), OpenError> {
    let log = ActivityLog::open(paper_dir);
    if log.head()?.is_none() {
        let view = View::load(paper_dir)?.ok_or_else(|| {
            OpenError::Activity(ActivityError::MissingHead {
                path: paper_dir.join(OLD_VIEW),
            })
        })?;
        // A keep ref that is already there is a previous attempt at this same migration
        // (a crash between the keep commit and the op), which named the same commits.
        let keep = match repo.find_ref(KEEP_REF).map_err(ExportError::from)? {
            Some(keep) => keep,
            None => write_keep(repo, identity, None, &view).map_err(ExportError::from)?,
        };
        log.append(OpKind::Migrate, &view, "upgrade to paper format 2")?;
        export(
            repo,
            paper_dir,
            &view,
            &keep,
            &Mode::Fresh { reset_index: false },
        )?;
    }
    log.make_durable()?;
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

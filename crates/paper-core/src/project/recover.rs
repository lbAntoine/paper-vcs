//! Recovery on open (03-architecture.md § Command lifecycle, step 2): finish or drop a
//! `checkout-pending` marker, then re-export when git's refs lag the head op.

use std::collections::BTreeSet;
use std::fs;
use std::io;

use paper_git::{FileKind, TreeFile, TreeId};

use super::checkout::PendingCheckout;
use super::export::{self, Exported, KEEP_REF};
use super::{OpenError, Project, draft_ref, line_ref};
use crate::activity::{ActivityLog, OpId};
use crate::snapshot::{StatCache, snapshot};

impl Project {
    /// Repairs what a crashed command left half done: (1) a `checkout-pending` marker, by the
    /// rule in the spec (drop it, or finish the checkout rewriting only files that still equal
    /// their version in one of the two trees); (2) refs, HEAD or `exported.json` behind the
    /// head op's view, by running the export. Both steps are idempotent, record no activity
    /// entry and do nothing when there is nothing to repair. Called by `Project::open_with`
    /// under the lock.
    ///
    /// # Errors
    ///
    /// [`OpenError`] if the marker, the log or the repository can't be read or written.
    pub(super) fn recover(&mut self, head: &OpId) -> Result<(), OpenError> {
        self.repair_marker(head)?;
        self.re_export_if_lagging(head)
    }

    fn repair_marker(&self, head: &OpId) -> Result<(), OpenError> {
        let marker_path = PendingCheckout::path(&self.paper_dir);
        if !marker_path.exists() {
            return Ok(());
        }
        let log = ActivityLog::open(&self.paper_dir);
        if let Some(marker) = PendingCheckout::read(&self.paper_dir)
            && let (Some(from), Some(tree)) = (marker.started_at(), marker.target_tree())
            && from != *head
        {
            let head_op = log.read(head)?;
            let head_tree = self.repo.read_commit(&head_op.view.current().draft)?.tree;
            if head_op.parent.as_ref() == Some(&from) && head_tree == tree {
                let from_view = log.read(&from)?.view;
                let from_tree = self.repo.read_commit(&from_view.current().draft)?.tree;
                self.finish_checkout(&from_tree, &tree)?;
            }
        }
        PendingCheckout::clear(&self.paper_dir).map_err(|source| OpenError::Io {
            path: marker_path,
            source,
        })
    }

    /// Rewrites the working files from `from_tree` to `tree`, touching a file only if its
    /// content equals its version in one of the two; then refreshes the stat cache.
    fn finish_checkout(&self, from_tree: &TreeId, tree: &TreeId) -> Result<(), OpenError> {
        let current = self.repo.tree_files(from_tree)?;
        let wanted = self.repo.tree_files(tree)?;
        let mut targets = BTreeSet::new();
        for path in current.keys().chain(wanted.keys()) {
            if self.untouched_since(path, current.get(path), wanted.get(path))? {
                targets.insert(path);
            }
        }
        let plan = self.plan_checkout(targets, &wanted, &current, &[])?;
        self.apply_checkout(&plan, "write")
            .map_err(|f| OpenError::Io {
                path: self.repo.workdir().join(f.path),
                source: f.source,
            })?;
        let previous = StatCache::load(&self.paper_dir);
        let snap = snapshot(&self.repo, tree, &previous, &self.options)?;
        snap.cache
            .save(&self.paper_dir)
            .map_err(|source| OpenError::Io {
                path: self.paper_dir.clone(),
                source,
            })
    }

    /// Whether the file at `path` still equals its version in one of the two trees (absent
    /// counts as a version when a tree lacks the file). Symlinks are passed on, so the plan
    /// reports them as skipped.
    fn untouched_since(
        &self,
        path: &str,
        before: Option<&TreeFile>,
        after: Option<&TreeFile>,
    ) -> Result<bool, OpenError> {
        let absolute = self.repo.workdir().join(path);
        let on_disk = match fs::symlink_metadata(&absolute) {
            Ok(meta) if meta.file_type().is_symlink() => return Ok(true),
            Ok(_) => match fs::read(&absolute) {
                Ok(bytes) => Some(bytes),
                // A folder where a file belongs: not ours to touch.
                Err(_) => return Ok(false),
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(_) => return Ok(false),
        };
        for version in [before, after] {
            let same = match (version, &on_disk) {
                (None, None) => true,
                (Some(file), Some(bytes)) if file.kind != FileKind::Symlink => {
                    self.repo.read_blob(&file.blob)? == *bytes
                }
                _ => false,
            };
            if same {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Runs the export when git's refs, HEAD or `exported.json` are behind the head op's view.
    fn re_export_if_lagging(&mut self, head: &OpId) -> Result<(), OpenError> {
        let exported = export::read_exported(&self.paper_dir);
        if !self.export_lags(exported.as_ref())? {
            return Ok(());
        }
        let log = ActivityLog::open(&self.paper_dir);
        let previous = match log.read(head)?.parent {
            Some(parent) => log.read(&parent)?.view,
            None => self.view.clone(),
        };
        let keep = match self.repo.find_ref(KEEP_REF)? {
            Some(keep) => keep,
            None => export::write_keep(&self.repo, &self.identity, Some(&previous), &self.view)?,
        };
        export::export(
            &self.repo,
            &self.paper_dir,
            &self.view,
            &keep,
            &export::Mode::Follow {
                previous: &previous,
            },
        )?;
        Ok(())
    }

    /// Whether the export would change anything paper owns. A ref that plain git moved
    /// (it is neither what paper last wrote nor what it should write) is not a lag.
    fn export_lags(&self, exported: Option<&Exported>) -> Result<bool, OpenError> {
        let Some(exported) = exported else {
            return Ok(true);
        };
        let behind = |last: Option<&str>, found: Option<&str>, want: &str| {
            last != Some(want) && (found == last || found == Some(want))
        };
        for (name, line) in self.view.lines() {
            let draft = draft_ref(name);
            let found = self.repo.find_ref(&draft)?;
            if exported.ref_value(&draft) != Some(line.draft.as_str())
                || found.as_ref() != Some(&line.draft)
            {
                return Ok(true);
            }
            let tip_ref = line_ref(name);
            let found = self.repo.find_ref(&tip_ref)?;
            if behind(
                exported.ref_value(&tip_ref),
                found.as_ref().map(paper_git::CommitId::as_str),
                line.tip.as_str(),
            ) {
                return Ok(true);
            }
        }
        let current = self.view.current_line().as_str();
        let found_head = self.repo.head_branch()?;
        Ok(behind(
            Some(exported.head()),
            found_head.as_deref(),
            current,
        ))
    }
}

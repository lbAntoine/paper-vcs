//! `paper status`: what is unsaved (02-cli.md § paper status).

use paper_git::GitError;

use super::Project;
use super::diff::file_diff;
use super::history::HistoryEntry;
use super::resolve::ResolveError;
use super::save::SaveError;
use crate::name::LineName;
use paper_diff::FileDiff;

use crate::snapshot::{ChangeKind, Skipped};
use crate::tree_diff::diff_trees;

/// How `status` reads the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusOptions {
    /// Snapshot the working files first (I1). `false` is `--no-snapshot`.
    pub snapshot: bool,
}

impl Default for StatusOptions {
    fn default() -> Self {
        Self { snapshot: true }
    }
}

/// How big a change is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStat {
    /// A text file: lines added and removed.
    Lines { added: usize, removed: usize },
    /// A binary file: sizes in bytes, `None` for an absent side.
    Bytes { old: Option<u64>, new: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: String,
    pub kind: ChangeKind,
    pub stat: ChangeStat,
}

/// How plain git left the repository out of step with paper (PV-029).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutsidePaper {
    /// Commits were added to the line, or it was moved, with plain git.
    LineMoved { line: LineName },
    /// Git's HEAD is on another branch than the line; `found` is `None` for a detached HEAD.
    OnAnotherBranch {
        line: LineName,
        found: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub line: LineName,
    /// The line's tip, as `history` lists it.
    pub last: HistoryEntry,
    /// Sorted by path; empty when nothing is unsaved.
    pub changes: Vec<StatusEntry>,
    pub skipped: Vec<Skipped>,
    pub large: Vec<(String, u64)>,
    /// Set when plain git moved the line or HEAD since paper last wrote them.
    pub outside_paper: Option<OutsidePaper>,
}

#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl Project {
    /// What the draft holds relative to the line's last save.
    ///
    /// # Errors
    ///
    /// [`StatusError`] if the working files or the repository can't be read.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn status(&mut self, options: &StatusOptions) -> Result<Status, StatusError> {
        let (skipped, large) = if options.snapshot {
            let state = self.snapshot()?;
            (state.skipped, state.large)
        } else {
            (Vec::new(), Vec::new())
        };
        let line = self.view.current();
        let tip_tree = self.repo.read_commit(&line.tip)?.tree;
        let draft_tree = self.repo.read_commit(&line.draft)?.tree;
        let old = self.repo.tree_files(&tip_tree)?;
        let new = self.repo.tree_files(&draft_tree)?;

        let mut changes = Vec::new();
        for change in diff_trees(&self.repo, &tip_tree, &draft_tree)? {
            let diff = file_diff(&self.repo, old.get(&change.path), new.get(&change.path))?;
            let stat = match diff {
                FileDiff::Text(text) => ChangeStat::Lines {
                    added: text.added,
                    removed: text.removed,
                },
                FileDiff::Identical => ChangeStat::Lines {
                    added: 0,
                    removed: 0,
                },
                FileDiff::Binary { old_size, new_size } => ChangeStat::Bytes {
                    old: old_size,
                    new: new_size,
                },
            };
            changes.push(StatusEntry {
                path: change.path,
                kind: change.kind,
                stat,
            });
        }

        let known = self.known()?;
        let tip = self.repo.read_commit(&line.tip)?;
        let last = self.entry(&tip, &known);
        Ok(Status {
            line: self.view.current_line().clone(),
            last,
            changes,
            skipped,
            large,
            // RED placeholder: GREEN computes this from `check_in_sync`.
            outside_paper: None,
        })
    }
}

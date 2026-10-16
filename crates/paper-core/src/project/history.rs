//! `paper history` and `paper show` (02-cli.md).

use paper_git::{ChangeId, Commit, CommitId, GitError, GitTime, Identity};

use std::collections::{BTreeMap, BTreeSet};

use super::Project;
use super::resolve::{Known, ResolveError};
use super::save::matches;
use crate::name::LineName;
use crate::point::Point;
use crate::snapshot::Change;
use crate::tree_diff::diff_trees;

/// What `history` should list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryRequest {
    /// Only saves that touch this file, or anything under this directory.
    pub path: Option<String>,
    /// List every line's saves instead of the current line's.
    pub all_lines: bool,
    /// At most this many entries.
    pub limit: Option<usize>,
}

/// One save, as listed by `history` and `show`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub commit: CommitId,
    /// `None` for commits made by plain git.
    pub change_id: Option<ChangeId>,
    /// The shortest unique change-ID prefix (at least 4 letters); the first 7 hash
    /// characters when there is no change ID.
    pub short_change_id: String,
    pub message: String,
    pub author: Identity,
    pub time: GitTime,
    /// The lines whose last save this is.
    pub tip_of: Vec<LineName>,
}

/// A save and what it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub entry: HistoryEntry,
    /// Compared with the first parent (the empty tree for the first save).
    pub changes: Vec<Change>,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error(transparent)]
    Git(#[from] GitError),
}

#[derive(Debug, thiserror::Error)]
pub enum ShowError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl Project {
    /// The saves on the current line (or on every line), newest first.
    ///
    /// # Errors
    ///
    /// [`HistoryError::Git`] if a commit can't be read.
    pub fn history(&self, request: &HistoryRequest) -> Result<Vec<HistoryEntry>, HistoryError> {
        let known = self.known()?;
        let saves: Vec<Commit> = if request.all_lines {
            // One walk: the saves are the commits reachable from the tips, in `known` order.
            let by_id: BTreeMap<&CommitId, &Commit> =
                known.commits.iter().map(|c| (&c.id, c)).collect();
            let mut reachable: BTreeSet<&CommitId> = BTreeSet::new();
            let mut pending: Vec<&CommitId> =
                self.view.lines().map(|(_, line)| &line.tip).collect();
            while let Some(id) = pending.pop() {
                if reachable.insert(id)
                    && let Some(commit) = by_id.get(id)
                {
                    pending.extend(&commit.parents);
                }
            }
            known
                .commits
                .iter()
                .filter(|c| reachable.contains(&c.id))
                .cloned()
                .collect()
        } else {
            self.current_line_chain()?
        };
        let limit = request.limit.unwrap_or(usize::MAX);
        let mut entries = Vec::new();
        for commit in &saves {
            if entries.len() >= limit {
                break;
            }
            if let Some(path) = &request.path
                && !self.touches(commit, path)?
            {
                continue;
            }
            entries.push(self.entry(commit, &known));
        }
        Ok(entries)
    }

    /// One save (or the draft, for `@`) with its changes.
    ///
    /// # Errors
    ///
    /// [`ShowError::Resolve`] if the point names nothing, [`ShowError::Git`] on read failures.
    pub fn show(&self, point: &Point) -> Result<Shown, ShowError> {
        let known = self.known()?;
        let resolved = self.resolve_with(point, &known)?;
        let commit = self.repo.read_commit(&resolved.commit)?;
        let changes = self.changes_of(&commit)?;
        Ok(Shown {
            entry: self.entry(&commit, &known),
            changes,
        })
    }

    pub(super) fn entry(&self, commit: &Commit, known: &Known) -> HistoryEntry {
        HistoryEntry {
            commit: commit.id.clone(),
            change_id: commit.change_id,
            short_change_id: known.short_id(commit),
            message: commit.message.clone(),
            author: commit.author.identity.clone(),
            time: commit.committer.time,
            tip_of: self
                .view
                .lines()
                .filter(|(_, line)| line.tip == commit.id)
                .map(|(name, _)| name.clone())
                .collect(),
        }
    }

    /// What `commit` changed compared with its first parent (the empty tree for a first save).
    fn changes_of(&self, commit: &Commit) -> Result<Vec<Change>, GitError> {
        let before = match commit.parents.first() {
            Some(parent) => self.repo.read_commit(parent)?.tree,
            None => self.repo.empty_tree()?,
        };
        diff_trees(&self.repo, &before, &commit.tree)
    }

    fn touches(&self, commit: &Commit, path: &str) -> Result<bool, GitError> {
        Ok(self
            .changes_of(commit)?
            .iter()
            .any(|change| matches(path, &change.path)))
    }
}

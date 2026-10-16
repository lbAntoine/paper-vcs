//! `paper diff`: compare two points file by file (02-cli.md § paper diff).

use std::collections::{BTreeMap, BTreeSet};

use paper_diff::{FileDiff, LineDiffOptions, diff_files};
use paper_git::{GitError, GitRepo, TreeFile};

use super::Project;
use super::history::ShowError;
use super::resolve::ResolveError;
use super::save::matches;
use crate::point::Point;
use crate::snapshot::ChangeKind;

/// What to compare. Paths use the same rule as `save` (file or whole directory components).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffRequest {
    /// Defaults to `last`.
    pub from: Option<Point>,
    /// Defaults to `@`, the stored draft (the front-end snapshots first).
    pub to: Option<Point>,
    /// Restrict to these paths; empty means every file.
    pub paths: Vec<String>,
}

/// One changed file with its line diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiffEntry {
    pub path: String,
    pub kind: ChangeKind,
    pub diff: FileDiff,
}

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("these paths are not in the draft or in either save being compared: {}", paths.join(", "))]
    UnknownPaths { paths: Vec<String> },
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl Project {
    /// Compares two points, file by file, sorted by path.
    ///
    /// # Errors
    ///
    /// [`DiffError::UnknownPaths`] for paths naming no file on either side,
    /// [`DiffError::Resolve`] for an unknown point.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn diff(&self, request: &DiffRequest) -> Result<Vec<FileDiffEntry>, DiffError> {
        let from = request.from.clone().unwrap_or(Point::Last { back: 0 });
        let to = request.to.clone().unwrap_or(Point::Draft);
        let old_tree = self.repo.read_commit(&self.resolve(&from)?.commit)?.tree;
        let new_tree = self.repo.read_commit(&self.resolve(&to)?.commit)?.tree;
        let old = self.repo.tree_files(&old_tree)?;
        let new = self.repo.tree_files(&new_tree)?;

        let unknown: Vec<String> = request
            .paths
            .iter()
            .filter(|path| !old.keys().chain(new.keys()).any(|file| matches(path, file)))
            .cloned()
            .collect();
        if !unknown.is_empty() {
            return Err(DiffError::UnknownPaths { paths: unknown });
        }

        Ok(self.entries(&old, &new, &request.paths)?)
    }

    /// What one save (or the draft, for `@`) changed compared with its first parent, file by
    /// file with line diffs; a first save is compared with nothing, so every file is added.
    ///
    /// # Errors
    ///
    /// [`ShowError::Resolve`] if the point names nothing, [`ShowError::Git`] on read failures.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn show_diff(&self, point: &Point) -> Result<Vec<FileDiffEntry>, ShowError> {
        let commit = self.repo.read_commit(&self.resolve(point)?.commit)?;
        let before = match commit.parents.first() {
            Some(parent) => self.repo.read_commit(parent)?.tree,
            None => self.repo.empty_tree()?,
        };
        let old = self.repo.tree_files(&before)?;
        let new = self.repo.tree_files(&commit.tree)?;
        Ok(self.entries(&old, &new, &[])?)
    }

    fn entries(
        &self,
        old: &BTreeMap<String, TreeFile>,
        new: &BTreeMap<String, TreeFile>,
        paths: &[String],
    ) -> Result<Vec<FileDiffEntry>, GitError> {
        let all: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
        let mut entries = Vec::new();
        for path in all {
            if !paths.is_empty() && !paths.iter().any(|p| matches(p, path)) {
                continue;
            }
            let (before, after) = (old.get(path), new.get(path));
            if before == after {
                continue;
            }
            let kind = match (before, after) {
                (None, _) => ChangeKind::Added,
                (_, None) => ChangeKind::Deleted,
                _ => ChangeKind::Modified,
            };
            entries.push(FileDiffEntry {
                path: path.clone(),
                kind,
                diff: file_diff(&self.repo, before, after)?,
            });
        }
        Ok(entries)
    }
}

/// The line diff of one file between two tree entries (`None` for an absent side).
pub(super) fn file_diff(
    repo: &GitRepo,
    before: Option<&TreeFile>,
    after: Option<&TreeFile>,
) -> Result<FileDiff, GitError> {
    let read = |file: Option<&TreeFile>| file.map(|f| repo.read_blob(&f.blob)).transpose();
    let (old, new) = (read(before)?, read(after)?);
    Ok(diff_files(
        old.as_deref(),
        new.as_deref(),
        &LineDiffOptions::default(),
    ))
}

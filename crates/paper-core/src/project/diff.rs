//! `paper diff`: compare two points file by file (02-cli.md § paper diff).

use std::collections::{BTreeMap, BTreeSet};

use paper_diff::{
    Content, FileDiff, LineDiffOptions, WordDiff, classify, diff_files, diff_words, tokeniser_for,
};
use paper_git::{GitError, GitRepo, TreeFile};

use super::Project;
use super::history::ShowError;
use super::resolve::ResolveError;
use super::save::matches;
use crate::point::Point;
use crate::snapshot::ChangeKind;

/// How text files are compared (`diff.default`, `--lines`, `--words`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DiffMode {
    /// A unified line diff: `FileDiff::Text` with hunks.
    Lines,
    /// Word by word, grouped by paragraph (04-content.md, Word diff). The default.
    #[default]
    Words,
}

/// What to compare. Paths use the same rule as `save` (file or whole directory components).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffRequest {
    /// How to compare text files. Binary files are the same in both modes.
    pub mode: DiffMode,
    /// Defaults to `last`.
    pub from: Option<Point>,
    /// Defaults to `@`, the stored draft (the front-end snapshots first).
    pub to: Option<Point>,
    /// Restrict to these paths; empty means every file.
    pub paths: Vec<String>,
}

/// The comparison of one changed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryDiff {
    /// A line diff in [`DiffMode::Lines`]; also every binary file, in either mode.
    Lines(FileDiff),
    /// A word diff of a text file in [`DiffMode::Words`]. An added or deleted file is
    /// compared with the empty text.
    Words(WordDiff),
}

/// One changed file with its line or word diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiffEntry {
    pub path: String,
    pub kind: ChangeKind,
    pub diff: EntryDiff,
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

        Ok(self.entries(&old, &new, &request.paths, request.mode)?)
    }

    /// What one save (or the draft, for `@`) changed compared with its first parent, file by
    /// file, in `mode`; a first save is compared with nothing, so every file is added.
    ///
    /// # Errors
    ///
    /// [`ShowError::Resolve`] if the point names nothing, [`ShowError::Git`] on read failures.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn show_diff(
        &self,
        point: &Point,
        mode: DiffMode,
    ) -> Result<Vec<FileDiffEntry>, ShowError> {
        let commit = self.repo.read_commit(&self.resolve(point)?.commit)?;
        let old = self.repo.tree_files(&self.parent_tree(&commit)?)?;
        let new = self.repo.tree_files(&commit.tree)?;
        Ok(self.entries(&old, &new, &[], mode)?)
    }

    fn entries(
        &self,
        old: &BTreeMap<String, TreeFile>,
        new: &BTreeMap<String, TreeFile>,
        paths: &[String],
        mode: DiffMode,
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
                diff: entry_diff(&self.repo, path, before, after, mode)?,
            });
        }
        Ok(entries)
    }
}

/// The line diff of one file between two tree entries (`None` for an absent side).
fn file_diff(
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

/// The comparison of one file in `mode`. Binary files are a line-diff summary in both modes;
/// an absent side is the empty text.
pub(super) fn entry_diff(
    repo: &GitRepo,
    path: &str,
    before: Option<&TreeFile>,
    after: Option<&TreeFile>,
    mode: DiffMode,
) -> Result<EntryDiff, GitError> {
    if mode == DiffMode::Lines {
        return Ok(EntryDiff::Lines(file_diff(repo, before, after)?));
    }
    let read = |file: Option<&TreeFile>| file.map(|f| repo.read_blob(&f.blob)).transpose();
    let (old, new) = (read(before)?, read(after)?);
    Ok(match (text_of(old.as_deref()), text_of(new.as_deref())) {
        (Some(old), Some(new)) => EntryDiff::Words(diff_words(old, new, tokeniser_for(path))),
        _ => EntryDiff::Lines(diff_files(
            old.as_deref(),
            new.as_deref(),
            &LineDiffOptions::default(),
        )),
    })
}

/// The text of a side (empty when absent), or `None` for binary content.
fn text_of(bytes: Option<&[u8]>) -> Option<&str> {
    match bytes.map(classify) {
        None => Some(""),
        Some(Content::Text(text)) => Some(text),
        Some(Content::Binary) => None,
    }
}

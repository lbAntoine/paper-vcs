//! `paper save`: turn the draft (or part of it) into a save (02-cli.md § paper save).

use std::io;
use std::path::PathBuf;

use paper_git::{ChangeId, CommitId, FileKind, GitError, NewCommit, TreeEdit};

use super::{Project, draft_ref, line_ref, signature_now, write_draft};
use crate::name::LineName;
use crate::snapshot::{Change, ChangeKind, Skipped, SnapshotError, StatCache, snapshot};
use crate::tree_diff::diff_trees;
use crate::view::{Line, ViewError};

/// Which of the draft's changes a save takes.
///
/// A path selects the file with exactly that path, or every file under it when it names a
/// directory (`sections` or `sections/`). Paths are repository-relative, with `/` separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSelection {
    /// The whole draft.
    All,
    /// Only the files matching these paths.
    Only(Vec<String>),
    /// Everything except the files matching these paths.
    AllBut(Vec<String>),
}

/// What to save. The message is data: `paper-core` never prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub message: String,
    pub paths: PathSelection,
}

/// The draft after a snapshot: what it holds relative to the line's last save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftState {
    /// Changes from the line's last save to the draft, sorted by path.
    pub changes: Vec<Change>,
    /// Things the snapshot deliberately left out.
    pub skipped: Vec<Skipped>,
    /// Large tracked files, with their size.
    pub large: Vec<(String, u64)>,
}

/// A save that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub save: CommitId,
    /// The change ID of the save: the one the draft had.
    pub change_id: ChangeId,
    pub line: LineName,
    /// What the save contains, relative to the previous save.
    pub saved: Vec<Change>,
    /// What stayed in the new draft.
    pub remaining: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The draft (or the selection) was empty; nothing was written.
    NothingToSave,
    Saved(Saved),
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("a save needs a message; describe what changed")]
    MissingMessage,
    #[error("none of these paths has unsaved changes: {}", paths.join(", "))]
    NoChangesAt { paths: Vec<String> },
    #[error("git moved the line '{line}' since paper last saw it")]
    OutOfSyncWithGit { line: LineName },
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("could not write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Invariant I1 as a reusable step: records the working files in the current line's
    /// draft. The draft commit is rewritten (same change ID, same parent) only if its tree
    /// changed.
    ///
    /// # Errors
    ///
    /// [`SaveError`] if the working files can't be read or the repository can't be written.
    pub fn snapshot(&mut self) -> Result<DraftState, SaveError> {
        let line_name = self.view.current_line().clone();
        let line = self.view.current().clone();
        let draft = self.repo.read_commit(&line.draft)?;
        let previous = StatCache::load(&self.paper_dir);
        let snap = snapshot(&self.repo, &draft.tree, &previous, &self.options)?;

        if snap.tree != draft.tree {
            let signature = signature_now(&self.identity);
            let rewritten = self.repo.write_commit(&NewCommit {
                tree: snap.tree.clone(),
                parents: draft.parents.clone(),
                author: draft.author.clone(),
                committer: signature,
                message: draft.message.clone(),
                change_id: draft.change_id,
            })?;
            self.repo
                .set_ref(&draft_ref(&line_name), &rewritten, "paper: snapshot")?;
            let view = self.view.with_line(
                line_name,
                Line {
                    tip: line.tip.clone(),
                    draft: rewritten,
                },
            );
            view.save(&self.paper_dir)?;
            self.view = view;
        }
        snap.cache
            .save(&self.paper_dir)
            .map_err(|source| SaveError::Io {
                path: self.paper_dir.clone(),
                source,
            })?;

        let tip_tree = self.repo.read_commit(&line.tip)?.tree;
        Ok(DraftState {
            changes: diff_trees(&self.repo, &tip_tree, &snap.tree)?,
            skipped: snap.skipped,
            large: snap.large,
        })
    }

    /// Turns the draft (or the selected part of it) into a save on the current line and
    /// starts a fresh draft on top.
    ///
    /// # Errors
    ///
    /// [`SaveError::OutOfSyncWithGit`] if git moved the line, [`SaveError::MissingMessage`]
    /// for an empty message, [`SaveError::NoChangesAt`] for paths matching nothing, and the
    /// wrapped lower-level errors.
    pub fn save(&mut self, request: &SaveRequest) -> Result<SaveOutcome, SaveError> {
        let line_name = self.view.current_line().clone();
        self.check_in_sync(&line_name)?;
        let message = request.message.trim();
        if message.is_empty() {
            return Err(SaveError::MissingMessage);
        }

        let state = self.snapshot()?;
        let (saved, remaining) = select(&request.paths, state.changes)?;
        if saved.is_empty() {
            return Ok(SaveOutcome::NothingToSave);
        }

        let line = self.view.current().clone();
        let draft = self.repo.read_commit(&line.draft)?;
        let save_tree = if remaining.is_empty() {
            draft.tree.clone()
        } else {
            let tip_tree = self.repo.read_commit(&line.tip)?.tree;
            let files = self.repo.tree_files(&draft.tree)?;
            let edits: Vec<TreeEdit> = saved
                .iter()
                .map(|change| match files.get(&change.path) {
                    Some(file) if change.kind != ChangeKind::Deleted => TreeEdit::Upsert {
                        path: change.path.clone(),
                        blob: file.blob.clone(),
                        executable: file.kind == FileKind::Executable,
                    },
                    _ => TreeEdit::Remove {
                        path: change.path.clone(),
                    },
                })
                .collect();
            self.repo.edit_tree(&tip_tree, &edits)?
        };

        let signature = signature_now(&self.identity);
        let change_id = draft.change_id.unwrap_or_else(ChangeId::random);
        let save = self.repo.write_commit(&NewCommit {
            tree: save_tree.clone(),
            parents: vec![line.tip.clone()],
            author: signature.clone(),
            committer: signature.clone(),
            message: message.to_owned(),
            change_id: Some(change_id),
        })?;
        let new_draft = write_draft(&self.repo, &draft.tree, &save, &signature)?;

        self.repo
            .set_ref(&line_ref(&line_name), &save, "paper: save")?;
        self.repo
            .set_ref(&draft_ref(&line_name), &new_draft, "paper: save")?;
        self.repo.reset_index_to(&save_tree)?;
        let view = self.view.with_line(
            line_name.clone(),
            Line {
                tip: save.clone(),
                draft: new_draft,
            },
        );
        view.save(&self.paper_dir)?;
        self.view = view;

        Ok(SaveOutcome::Saved(Saved {
            save,
            change_id,
            line: line_name,
            saved,
            remaining,
        }))
    }

    /// Refuses to go on when plain git moved the line or switched branches.
    fn check_in_sync(&self, line: &LineName) -> Result<(), SaveError> {
        let tip_matches =
            self.repo.find_ref(&line_ref(line))?.as_ref() == Some(&self.view.current().tip);
        let head_matches = self.repo.head_branch()?.as_deref() == Some(line.as_str());
        if tip_matches && head_matches {
            Ok(())
        } else {
            Err(SaveError::OutOfSyncWithGit { line: line.clone() })
        }
    }
}

/// Splits `changes` into (saved, remaining) according to `selection`.
fn select(
    selection: &PathSelection,
    changes: Vec<Change>,
) -> Result<(Vec<Change>, Vec<Change>), SaveError> {
    let (paths, keep_matches) = match selection {
        PathSelection::All => return Ok((changes, Vec::new())),
        PathSelection::Only(paths) => (paths, true),
        PathSelection::AllBut(paths) => (paths, false),
    };
    let unmatched: Vec<String> = paths
        .iter()
        .filter(|path| !changes.iter().any(|change| matches(path, &change.path)))
        .cloned()
        .collect();
    if !unmatched.is_empty() {
        return Err(SaveError::NoChangesAt { paths: unmatched });
    }
    Ok(changes
        .into_iter()
        .partition(|change| paths.iter().any(|path| matches(path, &change.path)) == keep_matches))
}

/// A selected path names the file itself or a directory containing it.
fn matches(selected: &str, file: &str) -> bool {
    let selected = selected.trim_end_matches('/');
    file == selected
        || file
            .strip_prefix(selected)
            .is_some_and(|rest| rest.starts_with('/'))
}

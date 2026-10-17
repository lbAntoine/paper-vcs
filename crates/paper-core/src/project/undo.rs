//! `paper undo`, `paper redo` and `paper undo --to`: take back an activity entry, or bring it
//! back, including the working files (03-architecture.md § Undo / redo algorithm).

use std::collections::BTreeSet;
use std::io;

use paper_git::GitError;

use super::checkout::PendingCheckout;
use super::restore::SkippedPath;
use super::save::SaveError;
use super::{ActivityEntry, ExportError, Project};
use crate::activity::{ActivityError, ActivityLog, Op, OpId, OpKind, short_ids_among};
use crate::snapshot::{Change, StatCache, snapshot};

/// An entry that was undone or redone, and what the working files did about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undone {
    /// What was undone (`undo`), brought back (`redo`) or gone back to (`undo --to`).
    pub entry: ActivityEntry,
    /// Working files rewritten or deleted, compared with the files before the command.
    pub changed: Vec<Change>,
    /// Files paper does not record that the checkout left alone, with the reason.
    pub skipped: Vec<SkippedPath>,
}

/// What an undo-family command did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    /// The state changed; the working files follow it.
    Undone(Undone),
    /// The position is the first entry. Nothing was recorded.
    NothingToUndo,
    /// The redo stack is empty. Nothing was recorded.
    NothingToRedo,
}

#[derive(Debug, thiserror::Error)]
pub enum UndoError {
    /// The refusals of `save`: plain git moved a line ref or HEAD.
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error("no entry in the activity log starts with '{prefix}'")]
    UnknownEntry { prefix: String },
    #[error("'{prefix}' starts more than one entry in the activity log")]
    AmbiguousEntry {
        prefix: String,
        /// Every entry the prefix matches, with its short id and description.
        candidates: Vec<ActivityEntry>,
    },
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Activity(#[from] ActivityError),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error("could not {action} {path}")]
    Io {
        /// "write" or "remove".
        action: &'static str,
        /// Relative to the project folder.
        path: String,
        #[source]
        source: io::Error,
    },
}

/// Where an undo-family command is going.
enum Aim {
    Undo,
    Redo,
    To(String),
}

/// A decided transition: the entry it moves to and how the new op is bookkept.
struct Step {
    /// The op whose view becomes current.
    target: Op,
    /// What the outcome reports as undone, redone or gone back to.
    entry: Op,
    kind: OpKind,
    description: String,
    redo: Vec<OpId>,
}

impl Project {
    /// Takes back the entry the project is at: the previous state becomes current, files
    /// included. Unsaved edits are recorded first, so they are taken back (and can be
    /// redone) like any entry.
    ///
    /// # Errors
    ///
    /// [`UndoError::Save`] (`OutOfSyncWithGit`, `HeadOnAnotherBranch`) when plain git moved a
    /// line or HEAD, before anything is written; the wrapped lower-level errors.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn undo(&mut self) -> Result<UndoOutcome, UndoError> {
        self.go(&Aim::Undo)
    }

    /// Brings back the entry most recently undone.
    ///
    /// # Errors
    ///
    /// As [`Project::undo`].
    ///
    /// # Panics
    ///
    /// Never.
    pub fn redo(&mut self) -> Result<UndoOutcome, UndoError> {
        self.go(&Aim::Redo)
    }

    /// Goes back to the state of the entry whose id starts with `prefix`, anywhere in the log,
    /// including entries that are only on the redo stack. Clears the redo stack.
    ///
    /// # Errors
    ///
    /// [`UndoError::UnknownEntry`], [`UndoError::AmbiguousEntry`] (nothing is changed), and
    /// everything [`Project::undo`] reports.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn undo_to(&mut self, prefix: &str) -> Result<UndoOutcome, UndoError> {
        self.go(&Aim::To(prefix.to_owned()))
    }

    /// Snapshot, then the lifecycle: keep-alive, op, export (inside `record_at`), checkout.
    fn go(&mut self, aim: &Aim) -> Result<UndoOutcome, UndoError> {
        // Refuse before anything is written, with save's errors.
        let line = self.view.current_line().clone();
        self.check_in_sync(&line)?;
        let log = ActivityLog::open(&self.paper_dir);
        let wanted = match aim {
            Aim::To(prefix) => Some(find_entry(&log, prefix)?),
            Aim::Undo | Aim::Redo => None,
        };
        // I1: edits made since the last command become their own entry first.
        let state = self.snapshot()?;
        let head_id = log.head()?.ok_or_else(|| ActivityError::MissingHead {
            path: log.root().join("head"),
        })?;
        let head = log.read(&head_id)?;
        let Some(step) = decide(&log, aim, &head, wanted)? else {
            return Ok(match aim {
                Aim::Redo => UndoOutcome::NothingToRedo,
                Aim::Undo | Aim::To(_) => UndoOutcome::NothingToUndo,
            });
        };

        let entry = entry_of(&log, &step.entry)?;
        let view = step.target.view.clone();
        let current = self.draft_files_of(&self.view)?;
        let tree = self.repo.read_commit(&view.current().draft)?.tree;
        let wanted_files = self.repo.tree_files(&tree)?;
        let targets: BTreeSet<&String> = current.keys().chain(wanted_files.keys()).collect();
        // Decided, and every blob read, before anything is written.
        let plan = self.plan_checkout(targets, &wanted_files, &current, &state.skipped)?;

        let placement = Some((step.target.id.clone(), step.redo));
        let op = self.record_at(step.kind, view, &step.description, placement)?;

        let marker_path = ".git/paper/checkout-pending";
        PendingCheckout::new(&op.id, &tree)
            .write(&self.paper_dir)
            .map_err(|source| UndoError::io("write", marker_path, source))?;
        self.apply_checkout(&plan, "write")
            .map_err(|f| UndoError::io(f.action, &f.path, f.source))?;
        self.refresh_stat_cache(&tree)?;
        PendingCheckout::clear(&self.paper_dir)
            .map_err(|source| UndoError::io("remove", marker_path, source))?;

        Ok(UndoOutcome::Undone(Undone {
            entry,
            changed: plan.changes,
            skipped: plan.skipped,
        }))
    }

    fn draft_files_of(
        &self,
        view: &crate::view::View,
    ) -> Result<std::collections::BTreeMap<String, paper_git::TreeFile>, GitError> {
        let tree = self.repo.read_commit(&view.current().draft)?.tree;
        self.repo.tree_files(&tree)
    }

    /// Re-reads the working files so the stat cache describes what the checkout wrote.
    fn refresh_stat_cache(&self, tree: &paper_git::TreeId) -> Result<(), UndoError> {
        let previous = StatCache::load(&self.paper_dir);
        let snap = snapshot(&self.repo, tree, &previous, &self.options).map_err(SaveError::from)?;
        snap.cache.save(&self.paper_dir).map_err(|source| {
            SaveError::Io {
                path: self.paper_dir.clone(),
                source,
            }
            .into()
        })
    }
}

impl UndoError {
    fn io(action: &'static str, path: &str, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.to_owned(),
            source,
        }
    }
}

/// The transitions of § Undo / redo algorithm; `None` when there is nothing to do.
fn decide(
    log: &ActivityLog,
    aim: &Aim,
    head: &Op,
    wanted: Option<Op>,
) -> Result<Option<Step>, UndoError> {
    Ok(match aim {
        Aim::Undo => {
            let position = log.read(&head.position)?;
            let Some(parent) = position.parent.clone() else {
                return Ok(None);
            };
            let mut redo = vec![position.id.clone()];
            redo.extend(head.redo.iter().cloned());
            Some(Step {
                target: log.read(&parent)?,
                description: format!("undo: {}", position.description),
                entry: position,
                kind: OpKind::Undo,
                redo,
            })
        }
        Aim::Redo => {
            let Some((first, rest)) = head.redo.split_first() else {
                return Ok(None);
            };
            let target = log.read(first)?;
            Some(Step {
                description: format!("redo: {}", target.description),
                entry: target.clone(),
                target,
                kind: OpKind::Redo,
                redo: rest.to_vec(),
            })
        }
        Aim::To(_) => match wanted {
            None => None,
            Some(target) => {
                let entry = entry_of(log, &target)?;
                Some(Step {
                    description: format!("go back to {}: {}", entry.short_id, target.description),
                    entry: target.clone(),
                    target,
                    kind: OpKind::Undo,
                    redo: Vec::new(),
                })
            }
        },
    })
}

/// The one op whose id starts with `prefix`, looking at every op file.
fn find_entry(log: &ActivityLog, prefix: &str) -> Result<Op, UndoError> {
    let names = log.op_file_names()?;
    let matching: Vec<OpId> = names
        .iter()
        .filter(|name| name.starts_with(prefix))
        .filter_map(|name| name.parse().ok())
        .collect();
    match matching.as_slice() {
        [] => Err(UndoError::UnknownEntry {
            prefix: prefix.to_owned(),
        }),
        [only] => Ok(log.read(only)?),
        many => {
            let shorts = short_ids_among(many, &names);
            let mut candidates = Vec::new();
            for (id, short_id) in many.iter().zip(shorts) {
                let op = log.read(id)?;
                candidates.push(ActivityEntry {
                    id: op.id,
                    short_id,
                    time: op.time,
                    kind: op.kind,
                    description: op.description,
                });
            }
            Err(UndoError::AmbiguousEntry {
                prefix: prefix.to_owned(),
                candidates,
            })
        }
    }
}

fn entry_of(log: &ActivityLog, op: &Op) -> Result<ActivityEntry, UndoError> {
    let short = short_ids_among(std::slice::from_ref(&op.id), &log.op_file_names()?);
    Ok(ActivityEntry {
        id: op.id.clone(),
        short_id: short.into_iter().next().unwrap_or_default(),
        time: op.time.clone(),
        kind: op.kind,
        description: op.description.clone(),
    })
}

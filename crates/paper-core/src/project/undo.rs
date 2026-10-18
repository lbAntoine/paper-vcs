//! `paper undo`, `paper redo` and `paper undo --to`: take back an activity entry, or bring it
//! back, including the working files (03-architecture.md § Undo / redo algorithm).

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use paper_git::{GitError, TreeFile};

use super::checkout::PendingCheckout;
use super::checkout_left::CheckoutLeft;
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
    /// Whether the working files now equal the new draft. `false` when files keep their own
    /// content (`skipped`). `paper undo` and `paper redo` go on from there; any other command
    /// that looks at the files records the difference as an edits entry (which clears the redo
    /// stack).
    pub files_match: bool,
}

/// What an undo-family command did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    /// The state changed; the working files follow it.
    Undone(Undone),
    /// The position is the first entry. Nothing was recorded, besides an edits entry for
    /// unsaved changes.
    NothingToUndo,
    /// The redo stack is empty. Nothing was recorded, besides an edits entry for unsaved
    /// changes.
    NothingToRedo,
    /// `undo --to` named the entry the project is already at (the entry is the target).
    /// Nothing was recorded, besides an edits entry for unsaved changes (after which the target
    /// is no longer current), and the redo stack is kept.
    AlreadyThere(ActivityEntry),
}

#[derive(Debug, thiserror::Error)]
pub enum UndoError {
    /// The refusals of `save`: plain git moved a line ref or HEAD.
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error("an entry id is needed after --to")]
    EmptyEntry,
    #[error("no entry in the activity log starts with '{prefix}'")]
    UnknownEntry { prefix: String },
    #[error("'{prefix}' starts more than one entry in the activity log")]
    AmbiguousEntry {
        prefix: String,
        /// Every entry the prefix matches, newest first, with its short id and description.
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
enum Aim<'a> {
    Undo,
    Redo,
    To(&'a str),
}

/// An [`Aim`] with its entry looked up.
enum Resolved {
    Undo,
    Redo,
    To(Op),
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
    /// including entries that are only on the redo stack. Clears the redo stack, except when the
    /// project is already there ([`UndoOutcome::AlreadyThere`]): then nothing changes.
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
        self.go(&Aim::To(prefix))
    }

    /// Snapshot, then the lifecycle: keep-alive, op, export (inside `record_at`), checkout.
    fn go(&mut self, aim: &Aim<'_>) -> Result<UndoOutcome, UndoError> {
        // Refuse before anything is written, with save's errors.
        let line = self.view.current_line().clone();
        self.check_in_sync(&line)?;
        let log = ActivityLog::open(&self.paper_dir);
        let resolved = match aim {
            Aim::Undo => Resolved::Undo,
            Aim::Redo => Resolved::Redo,
            Aim::To(prefix) => Resolved::To(find_entry(&log, prefix)?),
        };
        // I1: edits made since the last command become their own entry first.
        // Leftovers the previous undo-family checkout remembered are not edits.
        let before = log.head()?;
        let left = before
            .as_ref()
            .and_then(|head| CheckoutLeft::load_for(&self.paper_dir, head));
        let state = self.snapshot_ignoring(OpKind::Snapshot, None, left.as_ref())?;
        crate::fault::crash_point("after-snapshot");
        let head_id = log.head()?.ok_or_else(|| ActivityError::MissingHead {
            path: log.root().join("head"),
        })?;
        let head = log.read(&head_id)?;
        // The head's view is its position's view, so naming either is a no-op.
        if let Resolved::To(target) = &resolved
            && (head.position == target.id || head.id == target.id)
        {
            return Ok(UndoOutcome::AlreadyThere(entry_of(&log, target)?));
        }
        let step = match resolved {
            Resolved::Undo => step_undo(&log, &head)?,
            Resolved::Redo => step_redo(&log, &head)?,
            Resolved::To(target) => Some(step_to(&log, target)?),
        };
        let Some(step) = step else {
            return Ok(match aim {
                Aim::Redo => UndoOutcome::NothingToRedo,
                Aim::Undo | Aim::To(_) => UndoOutcome::NothingToUndo,
            });
        };

        let entry = entry_of(&log, &step.entry)?;
        let view = step.target.view.clone();
        // Plan against what is really on disk (the I1 snapshot's files), which differs from
        // the stored draft at remembered leftovers.
        let current = self.repo.tree_files(&state.working)?;
        let tree = self.repo.read_commit(&view.current().draft)?.tree;
        let wanted_files = self.repo.tree_files(&tree)?;
        // Taking back or bringing back an ignore or unignore changes what is tracked, never
        // the files: only `.gitignore` files are written or deleted.
        let tracking_only = !matches!(aim, Aim::To(_))
            && matches!(step.entry.kind, OpKind::Ignore | OpKind::Unignore);
        let targets: BTreeSet<&String> = current
            .keys()
            .chain(wanted_files.keys())
            .filter(|path| !tracking_only || *path == ".gitignore" || path.ends_with("/.gitignore"))
            .collect();
        // Decided, and every blob read, before anything is written.
        let plan = self.plan_checkout_to_draft(targets, &wanted_files, &current, &state.skipped)?;

        // The marker goes first, naming the op the project is at and the tree the files must
        // reach, so a crash anywhere below can be repaired (03-architecture.md § Command
        // lifecycle).
        let marker_path = PendingCheckout::path(&self.paper_dir);
        let marker_name = marker_path.display().to_string();
        PendingCheckout::new(&head.id, &tree)
            .write(&self.paper_dir)
            .map_err(|source| UndoError::io("write", &marker_name, source))?;
        crate::fault::crash_point("after-marker");
        let placement = Some((step.target.id.clone(), step.redo));
        self.record_at(step.kind, view, &step.description, placement)?;
        self.apply_checkout(&plan, "write")
            .map_err(|f| UndoError::io(f.action, &f.path, f.source))?;
        crate::fault::crash_point("after-checkout");
        let now = self.refresh_stat_cache(&tree)?;
        let files_match = tracking_only || now == tree;
        if tracking_only {
            // The files left alone are not leftovers: the next command records them as edits.
            let file = CheckoutLeft::path(&self.paper_dir).display().to_string();
            CheckoutLeft::remove(&self.paper_dir)
                .map_err(|source| UndoError::io("remove", &file, source))?;
        } else {
            self.remember_leftovers(&log, &tree, &now, &current)?;
        }
        PendingCheckout::clear(&self.paper_dir)
            .map_err(|source| UndoError::io("remove", &marker_name, source))?;

        Ok(UndoOutcome::Undone(Undone {
            entry,
            changed: plan.changes,
            skipped: plan.skipped,
            files_match,
        }))
    }

    /// Re-reads the working files so the stat cache describes what the checkout wrote.
    /// Returns the tree the working files make.
    fn refresh_stat_cache(&self, tree: &paper_git::TreeId) -> Result<paper_git::TreeId, UndoError> {
        let previous = StatCache::load(&self.paper_dir);
        let snap = snapshot(&self.repo, tree, &previous, &self.options).map_err(SaveError::from)?;
        snap.cache
            .save(&self.paper_dir)
            .map_err(|source| SaveError::Io {
                path: self.paper_dir.clone(),
                source,
            })?;
        Ok(snap.tree)
    }

    /// Writes `checkout-left.json` for the op just recorded when the working files differ
    /// from `tree`, or removes it when they match. A path is remembered only if it differs
    /// and holds what the I1 snapshot saw there (`before`): content that never entered
    /// history (an edit that landed during the command) is left for the next I1 to record.
    fn remember_leftovers(
        &self,
        log: &ActivityLog,
        tree: &paper_git::TreeId,
        now: &paper_git::TreeId,
        before: &BTreeMap<String, TreeFile>,
    ) -> Result<(), UndoError> {
        let file = CheckoutLeft::path(&self.paper_dir).display().to_string();
        if now == tree {
            CheckoutLeft::remove(&self.paper_dir)
                .map_err(|source| UndoError::io("remove", &file, source))?;
            return Ok(());
        }
        let recorded = log.head()?.ok_or_else(|| ActivityError::MissingHead {
            path: log.root().join("head"),
        })?;
        let wanted = self.repo.tree_files(tree)?;
        let seen = self.repo.tree_files(now)?;
        let differing = leftovers_to_remember(&wanted, &seen, before);
        if differing.is_empty() {
            CheckoutLeft::remove(&self.paper_dir)
                .map_err(|source| UndoError::io("remove", &file, source))?;
            return Ok(());
        }
        CheckoutLeft::new(&recorded, &differing, &seen)
            .write(&self.paper_dir)
            .map_err(|source| UndoError::io("write", &file, source))?;
        Ok(())
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

/// The paths to remember after a checkout: those where the working files (`seen`) differ
/// from the new draft (`wanted`) *and* still hold what the I1 snapshot saw (`before`). A
/// path whose content changed since I1 (an editor autosave during the command) never entered
/// history, so it is left for the next command's snapshot to record as an edit.
fn leftovers_to_remember(
    wanted: &BTreeMap<String, TreeFile>,
    seen: &BTreeMap<String, TreeFile>,
    before: &BTreeMap<String, TreeFile>,
) -> Vec<String> {
    wanted
        .keys()
        .chain(seen.keys())
        .filter(|path| wanted.get(*path) != seen.get(*path))
        .filter(|path| before.get(*path) == seen.get(*path))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// `undo`: back to the parent of the position; `None` at the first entry.
fn step_undo(log: &ActivityLog, head: &Op) -> Result<Option<Step>, UndoError> {
    let position = log.read(&head.position)?;
    let Some(parent) = position.parent.clone() else {
        return Ok(None);
    };
    let mut redo = vec![position.id.clone()];
    redo.extend(head.redo.iter().cloned());
    Ok(Some(Step {
        target: log.read(&parent)?,
        description: format!("undo: {}", position.description),
        entry: position,
        kind: OpKind::Undo,
        redo,
    }))
}

/// `redo`: forward to the first entry of the redo stack; `None` when it is empty.
fn step_redo(log: &ActivityLog, head: &Op) -> Result<Option<Step>, UndoError> {
    let Some((first, rest)) = head.redo.split_first() else {
        return Ok(None);
    };
    let target = log.read(first)?;
    Ok(Some(Step {
        description: format!("redo: {}", target.description),
        entry: target.clone(),
        target,
        kind: OpKind::Redo,
        redo: rest.to_vec(),
    }))
}

/// `undo --to`: the state of `target`, with an empty redo stack.
fn step_to(log: &ActivityLog, target: Op) -> Result<Step, UndoError> {
    let entry = entry_of(log, &target)?;
    Ok(Step {
        description: format!("go back to {}: {}", entry.short_id, target.description),
        entry: target.clone(),
        target,
        kind: OpKind::Undo,
        redo: Vec::new(),
    })
}

/// The one op whose id starts with `prefix`, looking at every op file.
fn find_entry(log: &ActivityLog, prefix: &str) -> Result<Op, UndoError> {
    if prefix.is_empty() {
        return Err(UndoError::EmptyEntry);
    }
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
            let mut candidates: Vec<ActivityEntry> = many
                .iter()
                .zip(shorts)
                .map(|(id, short_id)| match log.read(id) {
                    Ok(op) => ActivityEntry {
                        id: op.id,
                        short_id,
                        time: op.time,
                        kind: op.kind,
                        description: op.description,
                    },
                    // A leftover that can't be read is listed by its id alone.
                    Err(_) => ActivityEntry {
                        id: id.clone(),
                        short_id,
                        time: String::new(),
                        kind: OpKind::Snapshot,
                        description: "(unreadable operation file)".to_owned(),
                    },
                })
                .collect();
            candidates.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.id.cmp(&b.id)));
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

#[cfg(test)]
mod tests {
    use paper_git::{BlobId, FileKind};

    use super::*;

    fn file(content: &[u8]) -> TreeFile {
        let hash = content.iter().fold(17_u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u64::from(*b))
        });
        let hex = format!("{hash:016x}");
        TreeFile {
            kind: FileKind::Regular,
            blob: format!("{hex:0<40}").parse::<BlobId>().unwrap(),
        }
    }

    fn files(entries: &[(&str, &[u8])]) -> BTreeMap<String, TreeFile> {
        entries
            .iter()
            .map(|(path, content)| ((*path).to_owned(), file(content)))
            .collect()
    }

    #[test]
    fn a_path_that_still_holds_what_i1_saw_is_remembered() {
        let before = files(&[("a", b"mine"), ("b", b"same")]);
        let wanted = files(&[("a", b"draft"), ("b", b"same")]);
        let seen = before.clone();
        assert_eq!(leftovers_to_remember(&wanted, &seen, &before), ["a"]);
    }

    #[test]
    fn a_path_with_nothing_there_before_and_after_is_remembered() {
        let before = files(&[]);
        let wanted = files(&[("a", b"draft")]);
        let seen = files(&[]);
        assert_eq!(leftovers_to_remember(&wanted, &seen, &before), ["a"]);
    }

    #[test]
    fn an_edit_that_landed_after_the_i1_snapshot_is_not_remembered() {
        let before = files(&[("a", b"mine"), ("b", b"old")]);
        let wanted = files(&[("a", b"draft"), ("b", b"draft")]);
        // `b` was autosaved between the I1 snapshot and the post-checkout snapshot.
        let seen = files(&[("a", b"mine"), ("b", b"autosaved")]);
        assert_eq!(leftovers_to_remember(&wanted, &seen, &before), ["a"]);
    }

    #[test]
    fn a_file_created_after_the_i1_snapshot_is_not_remembered() {
        let before = files(&[]);
        let wanted = files(&[]);
        let seen = files(&[("new", b"typed")]);
        assert!(leftovers_to_remember(&wanted, &seen, &before).is_empty());
    }
}

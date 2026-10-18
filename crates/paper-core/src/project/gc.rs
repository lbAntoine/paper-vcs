//! `paper gc`: trim the activity log to a recent window and rebuild the keep-alive chain
//! (03-architecture.md § On-disk layout).

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use paper_git::{CommitId, GitError, GitRepo, Identity, NewCommit};

use super::export::KEEP_REF;
use super::{ActivityEntry, Project, signature_now};
use crate::activity::{ActivityError, ActivityLog, Op, OpId, short_ids_among};

/// Parents per keep-alive commit besides the link to the previous one.
const KEEP_GROUP: usize = 50;
const KEEP_MESSAGE: &str = "paper keep-alive";
const OLD_VIEW: &str = "view.json.v1";

/// How old an entry must be before `gc` removes it: a whole number of days, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcAge {
    days: u32,
}

impl GcAge {
    /// The default window, 90 days.
    pub const DEFAULT: Self = Self { days: 90 };

    /// Parses `--older-than`: `<n>d` (days) or `<n>w` (weeks), at least `1d`.
    ///
    /// # Errors
    ///
    /// [`GcError::InvalidAge`] for `0d`, a missing unit, an unknown unit or garbage.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn parse(text: &str) -> Result<Self, GcError> {
        let invalid = || GcError::InvalidAge {
            given: text.to_owned(),
        };
        let (number, per_unit) = if let Some(n) = text.strip_suffix('d') {
            (n, 1)
        } else if let Some(n) = text.strip_suffix('w') {
            (n, 7)
        } else {
            return Err(invalid());
        };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        let days = number
            .parse::<u32>()
            .ok()
            .and_then(|n| n.checked_mul(per_unit))
            .filter(|days| *days >= 1)
            .ok_or_else(invalid)?;
        Ok(Self { days })
    }

    /// The window in days.
    #[must_use]
    pub fn days(self) -> u32 {
        self.days
    }
}

impl Default for GcAge {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What `gc` did, or with `dry_run` would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcOutcome {
    pub dry_run: bool,
    /// Entries on the head chain whose op files were (or would be) removed.
    pub removed_entries: usize,
    /// Entries left on the head chain, the horizon and the head included.
    pub kept_entries: usize,
    /// The oldest kept entry: where `activity` and `undo` now stop.
    pub horizon: ActivityEntry,
    /// Files removed (or that would be) besides those of the removed entries: `.tmp*` files,
    /// orphan ops and `view.json.v1`. Paths relative to `.git/paper/`, with `/` separators,
    /// sorted.
    pub swept: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GcError {
    /// `--older-than` is not `<n>d` or `<n>w` with n at least 1.
    #[error("'{given}' is not a duration paper understands")]
    InvalidAge { given: String },
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Activity(#[from] ActivityError),
    #[error("could not {action} {path}")]
    Io {
        /// "list", "write" or "remove".
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Removes the activity entries older than `older_than` (keeping the head, the redo
    /// stacks and everything newer), writes the horizon, rebuilds `refs/paper/keep` and sweeps
    /// crash leftovers. With `dry_run`, only reports. Records no activity entry.
    ///
    /// When no entry is old enough, no horizon is written and the keep-alive chain is left
    /// alone; leftovers are still swept.
    ///
    /// # Errors
    ///
    /// [`GcError`] if the log can't be read or a file can't be written or removed. Running
    /// `gc` again finishes an interrupted run.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn gc(&mut self, older_than: GcAge, dry_run: bool) -> Result<GcOutcome, GcError> {
        let log = ActivityLog::open(&self.paper_dir);
        let plan = Plan::make(&log, &self.paper_dir, older_than)?;
        if !dry_run {
            // Horizon first: a crash before the new keep chain leaves the old chain covering
            // every op, and the ops below the horizon are just orphans the next run sweeps.
            if !plan.removed.is_empty() {
                log.write_horizon(&plan.horizon.id)?;
                rebuild_keep(&self.repo, &self.identity, &plan.kept)?;
            }
            for path in plan
                .removed
                .iter()
                .map(|id| log.op_path(id))
                .chain(plan.swept.iter().map(|rel| self.paper_dir.join(rel)))
            {
                remove(&path)?;
            }
            log.make_durable()?;
        }
        let gone: BTreeSet<String> = plan
            .removed
            .iter()
            .map(ToString::to_string)
            .chain(plan.orphans.iter().cloned())
            .collect();
        let remaining: Vec<String> = log
            .op_file_names()?
            .into_iter()
            .filter(|name| !gone.contains(name))
            .collect();
        let short_id = short_ids_among(std::slice::from_ref(&plan.horizon.id), &remaining)
            .into_iter()
            .next()
            .unwrap_or_default();
        Ok(GcOutcome {
            dry_run,
            removed_entries: plan.removed.len(),
            kept_entries: plan.kept.len(),
            horizon: ActivityEntry {
                id: plan.horizon.id.clone(),
                short_id,
                time: plan.horizon.time.clone(),
                kind: plan.horizon.kind,
                description: plan.horizon.description.clone(),
            },
            swept: plan.swept,
        })
    }
}

/// What a run would do.
struct Plan {
    /// Ops from the head to the horizon, newest first.
    kept: Vec<Op>,
    horizon: Op,
    /// Chain entries below the horizon.
    removed: Vec<OpId>,
    /// Op files that are on no chain, by file stem.
    orphans: Vec<String>,
    /// Everything swept besides the removed entries' files, relative to `.git/paper/`.
    swept: Vec<String>,
}

impl Plan {
    fn make(log: &ActivityLog, paper_dir: &Path, age: GcAge) -> Result<Self, GcError> {
        let chain = walk_chain(log)?;
        let last = horizon_index(&chain, age);
        let removed: Vec<OpId> = chain[last + 1..].iter().map(|op| op.id.clone()).collect();
        let on_chain: BTreeSet<String> = chain.iter().map(|op| op.id.to_string()).collect();
        // An op the head names (a redo id or the position) is never an orphan, even when a
        // damaged log leaves it off the chain.
        let named: BTreeSet<String> = chain
            .first()
            .into_iter()
            .flat_map(|head| head.redo.iter().chain(std::iter::once(&head.position)))
            .map(ToString::to_string)
            .collect();
        let orphans: Vec<String> = log
            .op_file_names()?
            .into_iter()
            .filter(|name| !on_chain.contains(name) && !named.contains(name))
            .collect();

        let mut swept: Vec<String> = orphans
            .iter()
            .map(|name| format!("activity/ops/{name}.json"))
            .collect();
        swept.extend(tmp_files(&log.ops_dir(), "activity/ops")?);
        swept.extend(tmp_files(log.root(), "activity")?);
        if paper_dir.join(OLD_VIEW).is_file() {
            swept.push(OLD_VIEW.to_owned());
        }
        swept.sort();

        let mut kept = chain;
        kept.truncate(last + 1);
        let horizon = kept[last].clone();
        Ok(Self {
            kept,
            horizon,
            removed,
            orphans,
            swept,
        })
    }
}

/// The ops from the head back along `parent` (the horizon, if any, ends the chain).
fn walk_chain(log: &ActivityLog) -> Result<Vec<Op>, GcError> {
    let mut next = log.head()?.ok_or_else(|| ActivityError::MissingHead {
        path: log.root().join("head"),
    })?;
    let mut chain = Vec::new();
    loop {
        let op = log.read(&next)?;
        let parent = op.parent.clone();
        chain.push(op);
        match parent {
            Some(parent) => next = parent,
            None => return Ok(chain),
        }
    }
}

/// The index of the oldest op to keep: the head, everything newer than the cutoff, the head's
/// `position` op (where the next undo starts) and the redo stacks of what is kept (the chain
/// between them stays whole).
fn horizon_index(chain: &[Op], age: GcAge) -> usize {
    let cutoff = jiff::Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(24 * i64::from(age.days)))
        .unwrap_or(jiff::Timestamp::MIN);
    // An unreadable time counts as new: when in doubt, keep.
    let is_new = |op: &Op| {
        op.time
            .parse::<jiff::Timestamp>()
            .map_or(true, |time| time >= cutoff)
    };
    let mut last = chain.iter().rposition(is_new).unwrap_or(0);
    // The next undo starts from the head's `position` op, so that op stays.
    if let Some(position) = chain
        .first()
        .and_then(|head| chain.iter().position(|op| op.id == head.position))
    {
        last = last.max(position);
    }
    loop {
        let reach = chain[..=last]
            .iter()
            .flat_map(|op| &op.redo)
            .filter_map(|id| chain.iter().position(|op| op.id == *id))
            .max()
            .unwrap_or(0);
        if reach <= last {
            return last;
        }
        last = reach;
    }
}

/// Writes a new `refs/paper/keep`: commits with at most [`KEEP_GROUP`] parents each (plus the
/// previous commit of the new chain) naming every commit the kept ops' views hold.
fn rebuild_keep(repo: &GitRepo, identity: &Identity, kept: &[Op]) -> Result<(), GitError> {
    let commits: BTreeSet<CommitId> = kept
        .iter()
        .flat_map(|op| op.view.lines())
        .flat_map(|(_, line)| [line.tip.clone(), line.draft.clone()])
        .collect();
    let commits: Vec<CommitId> = commits.into_iter().collect();
    let tree = repo.empty_tree()?;
    let signature = signature_now(identity);
    let mut previous: Option<CommitId> = None;
    for group in commits.chunks(KEEP_GROUP) {
        let mut parents: Vec<CommitId> = previous.into_iter().collect();
        parents.extend(group.iter().cloned());
        previous = Some(repo.write_commit(&NewCommit {
            tree: tree.clone(),
            parents,
            author: signature.clone(),
            committer: signature.clone(),
            message: KEEP_MESSAGE.to_owned(),
            change_id: None,
        })?);
    }
    if let Some(tip) = previous {
        repo.set_ref(KEEP_REF, &tip, "paper: gc")?;
    }
    Ok(())
}

/// The `.tmp*` files in `dir`, as `<rel>/<name>`.
fn tmp_files(dir: &Path, rel: &str) -> Result<Vec<String>, GcError> {
    let io_err = |source| GcError::Io {
        action: "list",
        path: dir.to_owned(),
        source,
    };
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(io_err(err)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_err)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".tmp") && entry.file_type().map_err(io_err)?.is_file() {
            found.push(format!("{rel}/{name}"));
        }
    }
    Ok(found)
}

fn remove(path: &Path) -> Result<(), GcError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GcError::Io {
            action: "remove",
            path: path.to_owned(),
            source,
        }),
    }
}

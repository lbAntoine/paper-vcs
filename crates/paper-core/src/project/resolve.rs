//! Resolving a [`Point`] to a commit (01-concepts.md § Points).

use jiff::tz::TimeZone;
use paper_git::{ChangeId, Commit, CommitId, GitError};

use super::Project;
use crate::name::LineName;
use crate::point::{DateSpec, Point};

/// What a point turned out to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointKind {
    /// `@`: the current line's draft.
    Draft,
    /// `last` or `last~N`.
    Last,
    /// A line name: its tip.
    Line(LineName),
    /// A change-ID prefix.
    ChangeId,
    /// A commit-hash prefix.
    CommitHash,
    /// `date:…`.
    Date,
}

/// A point resolved against a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub commit: CommitId,
    pub matched: PointKind,
}

/// One of several commits a prefix could mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub commit: CommitId,
    /// The short change ID, or the first 7 hash characters for commits made by plain git.
    pub short_id: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// `available` is the number of saves on the current line.
    #[error(
        "the line has only {available} saves, so 'last~{back}' goes too far back; try `paper history` to see them"
    )]
    NotEnoughHistory { back: u32, available: usize },
    #[error("no save was made on this line on or before {date:?}; try `paper history`")]
    NoSaveBefore { date: DateSpec },
    #[error("'{prefix}' could mean several saves; type more characters to pick one")]
    Ambiguous {
        prefix: String,
        candidates: Vec<Candidate>,
    },
    #[error("'{name}' is not a line, save or date; try `paper history` or `paper lines`")]
    UnknownPoint { name: String },
    #[error(transparent)]
    Git(#[from] GitError),
}

/// The shortest hash prefix that counts as a hash.
const MIN_HASH_PREFIX: usize = 4;
/// Hash characters shown for commits without a change ID.
const SHORT_HASH_LEN: usize = 7;

/// Every commit reachable from any line's tip or draft.
pub(super) struct Known {
    pub(super) commits: Vec<Commit>,
    change_ids: Vec<ChangeId>,
}

impl Known {
    /// The shortest unique change-ID prefix, or the first 7 hash characters.
    pub(super) fn short_id(&self, commit: &Commit) -> String {
        match &commit.change_id {
            Some(id) => {
                let len = id.unique_prefix_len(&self.change_ids);
                id.to_string().chars().take(len).collect()
            }
            None => commit.id.as_str().chars().take(SHORT_HASH_LEN).collect(),
        }
    }
}

fn is_change_id_prefix(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| ('k'..='z').contains(&c))
}

fn is_hash_prefix(text: &str) -> bool {
    text.len() >= MIN_HASH_PREFIX && text.chars().all(|c| c.is_ascii_hexdigit())
}

impl Project {
    pub(super) fn known(&self) -> Result<Known, GitError> {
        let roots: Vec<CommitId> = self
            .view
            .lines()
            .flat_map(|(_, line)| [line.tip.clone(), line.draft.clone()])
            .collect();
        let commits = self.repo.ancestors(&roots)?;
        let change_ids = commits.iter().filter_map(|c| c.change_id).collect();
        Ok(Known {
            commits,
            change_ids,
        })
    }

    /// The saves on the current line, newest first, following first parents from the tip.
    pub(super) fn current_line_chain(&self) -> Result<Vec<Commit>, GitError> {
        let mut chain = Vec::new();
        let mut next = Some(self.view.current().tip.clone());
        while let Some(id) = next {
            let commit = self.repo.read_commit(&id)?;
            next = commit.parents.first().cloned();
            chain.push(commit);
        }
        Ok(chain)
    }

    /// Turns a point into a commit, by the precedence in 01-concepts.md § Points.
    ///
    /// # Errors
    ///
    /// See [`ResolveError`].
    ///
    /// # Panics
    ///
    /// Never in practice.
    pub fn resolve(&self, point: &Point) -> Result<Resolved, ResolveError> {
        match point {
            Point::Draft => Ok(Resolved {
                commit: self.view.current().draft.clone(),
                matched: PointKind::Draft,
            }),
            Point::Last { back } => self.resolve_last(*back),
            Point::Date(date) => self.resolve_date(*date),
            Point::Name(name) => self.resolve_name(name),
        }
    }

    fn resolve_last(&self, back: u32) -> Result<Resolved, ResolveError> {
        let chain = self.current_line_chain()?;
        let index = usize::try_from(back).unwrap_or(usize::MAX);
        match chain.get(index) {
            Some(commit) => Ok(Resolved {
                commit: commit.id.clone(),
                matched: PointKind::Last,
            }),
            None => Err(ResolveError::NotEnoughHistory {
                back,
                available: chain.len(),
            }),
        }
    }

    fn resolve_date(&self, date: DateSpec) -> Result<Resolved, ResolveError> {
        let cutoff = date
            .inclusive_end()
            .to_zoned(TimeZone::system())
            .expect("a civil time within the supported range exists in the local zone")
            .timestamp()
            .as_second();
        let found = self
            .current_line_chain()?
            .into_iter()
            .find(|commit| commit.committer.time.seconds <= cutoff);
        match found {
            Some(commit) => Ok(Resolved {
                commit: commit.id,
                matched: PointKind::Date,
            }),
            None => Err(ResolveError::NoSaveBefore { date }),
        }
    }

    fn resolve_name(&self, name: &str) -> Result<Resolved, ResolveError> {
        if let Some((line, found)) = self.view.lines().find(|(line, _)| line.as_str() == name) {
            return Ok(Resolved {
                commit: found.tip.clone(),
                matched: PointKind::Line(line.clone()),
            });
        }
        let known = self.known()?;
        let lower = name.to_ascii_lowercase();
        if is_change_id_prefix(name) {
            // `ancestors` is newest first, so the first commit per change ID is the newest.
            let mut matches: Vec<&Commit> = Vec::new();
            for commit in &known.commits {
                let Some(id) = commit.change_id else { continue };
                let same = |other: &&Commit| other.change_id == Some(id);
                if id.to_string().starts_with(name) && !matches.iter().any(same) {
                    matches.push(commit);
                }
            }
            return Self::pick(name, &matches, &known, PointKind::ChangeId);
        }
        if is_hash_prefix(name) {
            let matches: Vec<&Commit> = known
                .commits
                .iter()
                .filter(|commit| commit.id.as_str().starts_with(&lower))
                .collect();
            return Self::pick(name, &matches, &known, PointKind::CommitHash);
        }
        Err(ResolveError::UnknownPoint {
            name: name.to_owned(),
        })
    }

    fn pick(
        name: &str,
        matches: &[&Commit],
        known: &Known,
        kind: PointKind,
    ) -> Result<Resolved, ResolveError> {
        match matches {
            [] => Err(ResolveError::UnknownPoint {
                name: name.to_owned(),
            }),
            [only] => Ok(Resolved {
                commit: only.id.clone(),
                matched: kind,
            }),
            several => Err(ResolveError::Ambiguous {
                prefix: name.to_owned(),
                candidates: several
                    .iter()
                    .map(|commit| Candidate {
                        commit: commit.id.clone(),
                        short_id: known.short_id(commit),
                        message: commit.message.clone(),
                    })
                    .collect(),
            }),
        }
    }
}

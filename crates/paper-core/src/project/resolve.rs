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
        "the line '{line}' has only {available} {}, so 'last~{back}' goes too far back; try `paper history` to see them",
        if *available == 1 { "save" } else { "saves" }
    )]
    NotEnoughHistory {
        line: LineName,
        back: u32,
        available: usize,
    },
    #[error("no save was made on this line on or before {date}; try `paper history`")]
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
#[derive(Default)]
pub(super) struct Known {
    pub(super) commits: Vec<Commit>,
    change_ids: Vec<ChangeId>,
    /// Every commit hash, sorted, for shortest-unique-prefix lookups.
    hashes: Vec<String>,
}

impl Known {
    /// The shortest unique change-ID prefix, or the shortest unique hash prefix (at least
    /// 7 characters, as git shows) for commits without a change ID.
    pub(super) fn short_id(&self, commit: &Commit) -> String {
        if let Some(id) = &commit.change_id {
            let len = id.unique_prefix_len(&self.change_ids);
            id.to_string().chars().take(len).collect()
        } else {
            let hash = commit.id.as_str();
            let len = unique_hash_len(&self.hashes, hash);
            hash.chars().take(len).collect()
        }
    }
}

/// Length of the shortest prefix of `hash` that no other hash in `sorted` shares (min 7).
fn unique_hash_len(sorted: &[String], hash: &str) -> usize {
    let common = |other: &String| {
        hash.chars()
            .zip(other.chars())
            .take_while(|(a, b)| a == b)
            .count()
    };
    let at = sorted.partition_point(|other| other.as_str() < hash);
    let before = at.checked_sub(1).and_then(|i| sorted.get(i));
    let after = sorted
        .get(at..)
        .and_then(|rest| rest.iter().find(|other| other.as_str() != hash));
    [before, after]
        .into_iter()
        .flatten()
        .map(|other| common(other) + 1)
        .fold(SHORT_HASH_LEN, usize::max)
        .min(hash.len())
}

/// The end of `date` in the local time zone, as Unix seconds. `None` when it lies before
/// the earliest time paper can represent; a date past the latest means "no limit".
fn cutoff_seconds(date: DateSpec) -> Option<i64> {
    let end = date.inclusive_end();
    // In a clock change that repeats an hour, the end of the date is the later occurrence.
    match TimeZone::system().to_ambiguous_zoned(end).later() {
        Ok(zoned) => Some(zoned.timestamp().as_second()),
        Err(_) if end.year() > 0 => Some(i64::MAX),
        Err(_) => None,
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
        let mut hashes: Vec<String> = commits.iter().map(|c| c.id.as_str().to_owned()).collect();
        hashes.sort();
        Ok(Known {
            commits,
            change_ids,
            hashes,
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
    pub fn resolve(&self, point: &Point) -> Result<Resolved, ResolveError> {
        // Only names need the whole history.
        let known = match point {
            Point::Name(_) => self.known()?,
            _ => Known::default(),
        };
        self.resolve_with(point, &known)
    }

    /// Like [`Project::resolve`], reusing a history walk the caller already made.
    pub(super) fn resolve_with(
        &self,
        point: &Point,
        known: &Known,
    ) -> Result<Resolved, ResolveError> {
        match point {
            Point::Draft => Ok(Resolved {
                commit: self.view.current().draft.clone(),
                matched: PointKind::Draft,
            }),
            Point::Last { back } => self.resolve_last(*back),
            Point::Date(date) => self.resolve_date(*date),
            Point::Name(name) => self.resolve_name(name, known),
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
                line: self.view.current_line().clone(),
                back,
                available: chain.len(),
            }),
        }
    }

    fn resolve_date(&self, date: DateSpec) -> Result<Resolved, ResolveError> {
        let cutoff = cutoff_seconds(date);
        let chain = self.current_line_chain()?;
        let found = cutoff.and_then(|cutoff| {
            chain
                .into_iter()
                .find(|commit| commit.committer.time.seconds <= cutoff)
        });
        match found {
            Some(commit) => Ok(Resolved {
                commit: commit.id,
                matched: PointKind::Date,
            }),
            None => Err(ResolveError::NoSaveBefore { date }),
        }
    }

    fn resolve_name(&self, name: &str, known: &Known) -> Result<Resolved, ResolveError> {
        if let Some((line, found)) = self.view.lines().find(|(line, _)| line.as_str() == name) {
            return Ok(Resolved {
                commit: found.tip.clone(),
                matched: PointKind::Line(line.clone()),
            });
        }
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
            return Self::pick(name, &matches, known, PointKind::ChangeId);
        }
        if is_hash_prefix(name) {
            let matches: Vec<&Commit> = known
                .commits
                .iter()
                .filter(|commit| commit.id.as_str().starts_with(&lower))
                .collect();
            return Self::pick(name, &matches, known, PointKind::CommitHash);
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

#[cfg(test)]
mod tests {
    use super::unique_hash_len;

    fn hashes(list: &[&str]) -> Vec<String> {
        let mut all: Vec<String> = list.iter().map(|h| (*h).to_owned()).collect();
        all.sort();
        all
    }

    #[test]
    fn hash_prefixes_are_seven_long_unless_a_neighbour_shares_them() {
        let all = hashes(&["abcdef0123", "abcdef9999", "ffffffffff"]);
        assert_eq!(unique_hash_len(&all, "ffffffffff"), 7);
        // Share "abcdef" (6): need 7 to tell apart.
        assert_eq!(unique_hash_len(&all, "abcdef0123"), 7);
        let close = hashes(&["abcdefa0123", "abcdefa9999"]);
        assert_eq!(unique_hash_len(&close, "abcdefa0123"), 8);
        assert_eq!(unique_hash_len(&close, "abcdefa9999"), 8);
    }
}

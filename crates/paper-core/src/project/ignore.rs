//! `paper ignore`: stop tracking files and folders, even saved ones, by writing rules in
//! paper's own `.gitignore` block (02-cli.md § paper ignore).

use std::io;
use std::path::PathBuf;

use paper_git::GitError;

use super::{ActivityEntry, ExportError, Project, SaveError};
use crate::profile::gitignore::GitignoreError;

/// What to ignore, and from where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreRequest {
    /// The arguments as typed: paths (relative to `from`) and patterns such as `*.aux`.
    pub args: Vec<String>,
    /// The folder paper runs in, relative to the project folder with `/` separators; empty at
    /// the top.
    pub from: String,
    /// Go ahead when more than 20 saved files would stop being tracked.
    pub yes: bool,
}

/// What `paper ignore <args>` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreOutcome {
    /// Rules written to paper's block, in the order added.
    pub added: Vec<String>,
    /// Rules that were already in paper's block.
    pub already: Vec<String>,
    /// The rules of `added` and `already` that came from a pattern, not a path: they apply to
    /// the whole project wherever paper runs.
    pub patterns: Vec<String>,
    /// Saved files that left the draft (still on disk, byte-identical), sorted.
    pub untracked: Vec<String>,
    /// The activity entry recorded; `None` when there was nothing to do.
    pub entry: Option<ActivityEntry>,
}

/// Where an ignore rule comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreSource {
    /// Paper's own block.
    Paper,
    /// The profile block.
    Profile,
    /// Other lines of the root `.gitignore`, and nested `.gitignore` files.
    Yours,
    /// `.git/info/exclude`.
    Local,
    /// Git's `core.excludesFile`, or the XDG default `git/ignore`.
    Global,
}

impl IgnoreSource {
    /// `paper`, `profile`, `yours`, `local` or `global`.
    #[must_use]
    pub fn name(self) -> &'static str {
        todo!("PV-018")
    }
}

/// One ignore rule that applies, with where it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreRule {
    /// The rule as written, without the line ending.
    pub rule: String,
    pub source: IgnoreSource,
    /// Relative to the project folder for `.gitignore` files (`docs/.gitignore`) and
    /// `.git/info/exclude`; the path of git's global ignore file for `global`.
    pub file: String,
    /// 1-based line in `file`.
    pub line: usize,
}

/// Why `paper ignore` did nothing.
#[derive(Debug, thiserror::Error)]
pub enum IgnoreError {
    #[error("an empty name can't be ignored")]
    Empty,
    #[error("{arg} starts with '!', which would stop ignoring something")]
    Unignore { arg: String },
    #[error("{path} can't be ignored: paper needs it")]
    Protected { path: String },
    #[error("{arg} is inside .git, which paper never tracks")]
    InsideGit { arg: String },
    #[error("{arg} is outside this project")]
    Outside { arg: String },
    #[error("{count} saved files would stop being tracked")]
    TooMany {
        count: usize,
        /// The first five, sorted.
        first: Vec<String>,
    },
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Gitignore(#[from] GitignoreError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error("could not read {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Writes a rule for each argument in paper's `.gitignore` block, and takes the saved
    /// files that now match out of the draft (they stay on disk). Records one `ignore` entry.
    ///
    /// # Errors
    ///
    /// [`IgnoreError`]; nothing is written when it is returned.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn ignore(&mut self, _request: &IgnoreRequest) -> Result<IgnoreOutcome, IgnoreError> {
        todo!("PV-018")
    }

    /// Every ignore rule that applies, grouped by source (paper, profile, yours, local,
    /// global), each with its file and line. Comments and blank lines are skipped.
    #[must_use]
    pub fn ignore_rules(&self) -> Vec<IgnoreRule> {
        todo!("PV-018")
    }
}

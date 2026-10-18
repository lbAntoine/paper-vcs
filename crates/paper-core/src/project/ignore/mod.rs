//! `paper ignore`: stop tracking files and folders, even saved ones, by writing rules in
//! paper's own `.gitignore` block (02-cli.md § paper ignore).

mod args;
mod layers;
mod listing;
mod plan;
mod store;
mod unignore;
mod why;

pub use unignore::{UnignoreError, UnignoreOutcome, UnignoreRequest};
pub use why::{IgnoreMatch, IgnoreWhy};

use std::io;
use std::path::PathBuf;

use paper_git::GitError;

use super::{ActivityEntry, ExportError, SaveError};
use crate::profile::gitignore::GitignoreError;

const GITIGNORE: &str = ".gitignore";

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
    /// Write the rules to paper's block in `.git/info/exclude` (not shared, outside history)
    /// instead of the root `.gitignore`.
    pub local: bool,
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
    /// Whether the rules went to `.git/info/exclude` (`--local`).
    pub local: bool,
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
        match self {
            Self::Paper => "paper",
            Self::Profile => "profile",
            Self::Yours => "yours",
            Self::Local => "local",
            Self::Global => "global",
        }
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

/// Every ignore rule that applies, plus the sources paper could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreListing {
    /// Grouped paper, profile, yours, local, global.
    pub rules: Vec<IgnoreRule>,
    /// Files that exist but could not be read (not UTF-8 text, no permission): their rules are
    /// not in `rules`. The source and the file, as in [`IgnoreRule`].
    pub unreadable: Vec<(IgnoreSource, String)>,
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
    #[error("the whole project can't be ignored")]
    WholeProject,
    #[error("{count} saved files would stop being tracked")]
    TooMany {
        count: usize,
        /// The first five, sorted.
        first: Vec<String>,
        /// The arguments as typed, so the error can suggest the same command with `--yes`.
        args: Vec<String>,
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

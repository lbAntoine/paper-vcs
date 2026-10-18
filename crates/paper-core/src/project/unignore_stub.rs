//! RED stubs for PV-032 (`paper unignore`, `--local`, `--why`). GREEN moves these types and
//! functions into the `project/ignore/` module folder (`unignore.rs`, `why.rs`) and deletes
//! this file. Every body is `todo!()`.

use super::{ActivityEntry, IgnoreError, IgnoreSource, Project, Undone};
use crate::error::PaperError;

/// What to unignore, and from where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnignoreRequest {
    /// The arguments as typed: paths (relative to `from`) and patterns such as `*.aux`.
    pub args: Vec<String>,
    /// The folder paper runs in, relative to the project folder with `/` separators; empty at
    /// the top.
    pub from: String,
    /// Work on paper's block in `.git/info/exclude` instead of the root `.gitignore`.
    pub local: bool,
}

/// What `paper unignore <args>` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnignoreOutcome {
    /// Rules taken out of paper's block, in the order given.
    pub removed: Vec<String>,
    /// Negations added to paper's block (`!/notes/keep.log`), in the order given.
    pub negated: Vec<String>,
    /// Files that entered the draft because they are no longer ignored, sorted.
    pub tracked: Vec<String>,
    /// The activity entry recorded; `None` when nothing was ignored.
    pub entry: Option<ActivityEntry>,
    /// Whether the rules live in `.git/info/exclude` (`--local`).
    pub local: bool,
}

/// One rule that decides whether a path is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreMatch {
    /// The rule as written, for example `/data/raw/` or `!/notes/keep.log`.
    pub rule: String,
    pub source: IgnoreSource,
    /// As in [`super::IgnoreRule`].
    pub file: String,
    /// 1-based line in `file`.
    pub line: usize,
    /// The rule is a negation: it brings the path back.
    pub negated: bool,
}

/// The answer of `paper ignore --why <path>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreWhy {
    /// The path relative to the project folder, with `/` separators.
    pub path: String,
    /// Whether git would ignore the path.
    pub ignored: bool,
    /// Whether the current draft holds the path (a saved file stays tracked under a later rule).
    pub tracked: bool,
    /// The rule that decides, as `git check-ignore -v` reports it; `None` when no rule matches.
    pub rule: Option<IgnoreMatch>,
}

/// Why `paper unignore` did nothing.
#[derive(Debug, thiserror::Error)]
pub enum UnignoreError {
    /// The refusals shared with `paper ignore` (empty name, `.git`, outside, protected files,
    /// a leading `!`, the whole project) and I/O failures.
    #[error(transparent)]
    Refused(#[from] IgnoreError),
    /// After the change the path would still be ignored.
    #[error("{path} would still be ignored")]
    StillIgnored {
        path: String,
        /// The rule that keeps it ignored.
        rule: IgnoreMatch,
        /// The command was run with `--local`.
        local: bool,
    },
}

impl From<UnignoreError> for PaperError {
    fn from(_error: UnignoreError) -> Self {
        todo!("PV-032: three-part errors for unignore")
    }
}

impl Project {
    /// Removes paper's rule for each argument, or adds a negation to paper's block when
    /// another rule still ignores it, and tracks the files again. Records one `unignore`
    /// entry.
    ///
    /// # Errors
    ///
    /// [`UnignoreError`]; nothing is written when it is returned.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn unignore(
        &mut self,
        _request: &UnignoreRequest,
    ) -> Result<UnignoreOutcome, UnignoreError> {
        todo!("PV-032: unignore")
    }

    /// Which rule, file and line decide whether `path` (typed in `from`) is ignored. Reads
    /// only; records nothing.
    ///
    /// # Errors
    ///
    /// [`IgnoreError`] for an argument that is empty, inside `.git` or outside the project.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn why_ignored(&self, _from: &str, _path: &str) -> Result<IgnoreWhy, IgnoreError> {
        todo!("PV-032: why_ignored")
    }
}

impl Undone {
    /// Whether the undone, redone or reached entry was written with `--local`: its rule lives
    /// in `.git/info/exclude`, outside history, and undo left that file as it was.
    ///
    /// # Panics
    ///
    /// Never.
    #[must_use]
    pub fn local_rules(&self) -> bool {
        todo!("PV-032: derive from the entry's kind and description")
    }
}

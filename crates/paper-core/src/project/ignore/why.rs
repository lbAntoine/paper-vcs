//! `paper ignore --why`: which rule decides whether a path is ignored.

use super::{IgnoreError, IgnoreSource};
use crate::project::Project;

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

impl Project {
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

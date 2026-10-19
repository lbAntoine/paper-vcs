//! `paper ignore --why`: which rule decides whether a path is ignored.

use std::fs;

use super::args::{on_disk_spelling, resolve};
use super::layers::{Layers, Overrides};
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

impl IgnoreMatch {
    /// Where the rule is written, for a sentence: `paper's block, .gitignore line 12`,
    /// `the profile block, .gitignore line 3` or `.gitignore line 7`.
    #[must_use]
    pub fn location(&self) -> String {
        let place = format!("{} line {}", self.file, self.line);
        match self.source {
            IgnoreSource::Paper => format!("paper's block, {place}"),
            IgnoreSource::Profile => format!("the profile block, {place}"),
            IgnoreSource::Yours | IgnoreSource::Local | IgnoreSource::Global => place,
        }
    }
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
    /// only; records nothing. The rules are read as they are on disk now, like
    /// `git check-ignore -v`, and a folder given as itself is judged as a folder.
    ///
    /// # Errors
    ///
    /// [`IgnoreError`] for an argument that is empty, inside `.git` or outside the project.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn why_ignored(&self, from: &str, path: &str) -> Result<IgnoreWhy, IgnoreError> {
        if path.is_empty() {
            return Err(IgnoreError::Empty);
        }
        let workdir = self.repo.workdir();
        let parts = resolve(path, from, workdir)?;
        match parts.first() {
            None => return Err(IgnoreError::WholeProject),
            Some(first) if first.eq_ignore_ascii_case(".git") => {
                return Err(IgnoreError::InsideGit {
                    arg: path.to_owned(),
                });
            }
            Some(_) => {}
        }
        // The spelling the folders list, as for `ignore`: rules written for it must match.
        let rel = on_disk_spelling(workdir, &parts).join("/");

        let draft = self.repo.read_commit(&self.view.current().draft)?;
        let files = self.repo.tree_files(&draft.tree)?;
        let prefix = format!("{rel}/");
        let in_draft_as_folder = files
            .range(prefix.clone()..)
            .next()
            .is_some_and(|(file, _)| file.starts_with(&prefix));
        let is_dir = match fs::symlink_metadata(workdir.join(&rel)) {
            Ok(meta) => meta.is_dir(),
            Err(_) => in_draft_as_folder,
        };
        let rule = Layers::from_disk(&self.repo, &Overrides::default()).explain(&rel, is_dir);
        Ok(IgnoreWhy {
            ignored: rule.as_ref().is_some_and(|found| !found.negated),
            tracked: files.contains_key(&rel) || in_draft_as_folder,
            path: rel,
            rule,
        })
    }
}

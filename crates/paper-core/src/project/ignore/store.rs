//! The two files that can hold paper's rule block: the shared root `.gitignore`, and
//! `.git/info/exclude` for `--local`.

use std::fs;

use paper_git::GitRepo;

use super::GITIGNORE;
use super::layers::Overrides;
use crate::fs_util::write_atomic;
use crate::profile::gitignore::{self, GitignoreError};

/// Where paper's block lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Store {
    /// The root `.gitignore`, shared with co-authors and part of every save.
    Gitignore,
    /// `.git/info/exclude`: private to this copy, outside history.
    Exclude,
}

impl Store {
    pub(super) fn of(local: bool) -> Self {
        if local {
            Self::Exclude
        } else {
            Self::Gitignore
        }
    }

    /// The file's text; `None` when it doesn't exist.
    pub(super) fn read(self, repo: &GitRepo) -> Result<Option<String>, GitignoreError> {
        match self {
            Self::Gitignore => gitignore::read(repo.workdir()),
            Self::Exclude => gitignore::read_path(&repo.git_dir().join("info").join("exclude")),
        }
    }

    /// Replaces the file atomically, creating `.git/info` if it is missing.
    pub(super) fn write(self, repo: &GitRepo, text: &str) -> Result<(), GitignoreError> {
        let path = match self {
            Self::Gitignore => repo.workdir().join(GITIGNORE),
            Self::Exclude => repo.git_dir().join("info").join("exclude"),
        };
        let result = match path.parent() {
            Some(dir) if self == Self::Exclude => fs::create_dir_all(dir),
            _ => Ok(()),
        }
        .and_then(|()| write_atomic(&path, text.as_bytes()));
        result.map_err(|source| GitignoreError::Write { path, source })
    }

    /// The layers with `text` in place of this file.
    pub(super) fn overriding(self, text: &str) -> Overrides<'_> {
        match self {
            Self::Gitignore => Overrides {
                root: Some(text),
                exclude: None,
            },
            Self::Exclude => Overrides {
                root: None,
                exclude: Some(text),
            },
        }
    }
}

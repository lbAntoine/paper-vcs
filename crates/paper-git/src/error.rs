use std::path::PathBuf;

use crate::CommitId;
use crate::change_id::ParseChangeIdError;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("{path} is a bare git repository, which paper doesn't support")]
    Bare { path: PathBuf },
    #[error("commit {id} has a damaged change-id header")]
    BadChangeId {
        id: CommitId,
        #[source]
        source: ParseChangeIdError,
    },
    #[error("git doesn't accept '{name}' as a branch name")]
    InvalidBranchName {
        name: String,
        #[source]
        source: gix::validate::reference::name::Error,
    },
    #[error("commit {id} has a message that isn't valid UTF-8")]
    NonUtf8Message { id: CommitId },
    #[error("git: could not {action}")]
    Gix {
        action: String,
        #[source]
        source: gix::Error,
    },
}

/// Attaches a human-readable action to gix errors: `.context("read commit")?`
pub(crate) trait ResultExt<T> {
    fn context(self, action: impl Into<String>) -> Result<T, GitError>;
}

impl<T, E: Into<gix::Error>> ResultExt<T> for Result<T, E> {
    fn context(self, action: impl Into<String>) -> Result<T, GitError> {
        self.map_err(|err| GitError::Gix {
            action: action.into(),
            source: err.into(),
        })
    }
}

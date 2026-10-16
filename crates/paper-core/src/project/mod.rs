//! A paper project: a git repository plus paper's own state in `.git/paper/`.

mod diff;
mod history;
mod init;
mod init_profile;
mod resolve;
mod restore;
mod restore_plan;
mod save;
mod status;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use paper_git::{
    ChangeId, CommitId, GitError, GitRepo, GitTime, Identity, NewCommit, Signature, TreeId,
};

use crate::name::LineName;
use crate::snapshot::SnapshotOptions;
use crate::view::{View, ViewError};

pub use diff::{DiffError, DiffRequest, FileDiffEntry};
pub use history::{HistoryEntry, HistoryError, HistoryRequest, ShowError, Shown};
pub use init::{InitError, Initialized, ProfileSource, has_damaged_git};
pub use resolve::{Candidate, PointKind, ResolveError, Resolved};
pub use restore::{RestoreError, RestoreRequest, RestoreSkip, Restored, SkippedPath};
pub use save::{DraftState, PathSelection, SaveError, SaveOutcome, SaveRequest, Saved};
pub use status::{ChangeStat, Status, StatusEntry, StatusError, StatusOptions};

/// Version of the on-disk format in `.git/paper/`. Migrations are keyed on it.
pub const FORMAT_VERSION: u32 = 1;

const PAPER_DIR: &str = "paper";
const VERSION_FILE: &str = "version";

/// An open paper project.
#[derive(Debug)]
pub struct Project {
    repo: GitRepo,
    paper_dir: PathBuf,
    view: View,
    identity: Identity,
    options: SnapshotOptions,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("{path} is a git repository but not a paper project yet")]
    NotAPaperProject { path: PathBuf },
    #[error(
        "this project uses paper format '{found}', but this version of paper reads format {FORMAT_VERSION}"
    )]
    UnsupportedFormat { found: String },
    #[error("paper's state in {path} is incomplete")]
    Incomplete { path: PathBuf },
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("could not read {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Opens the paper project whose working directory is exactly `path`.
    ///
    /// # Errors
    ///
    /// [`OpenError::NotAPaperProject`] for a plain git repository, and the other
    /// [`OpenError`] variants if paper's state can't be read.
    pub fn open(path: &Path, identity: Identity) -> Result<Self, OpenError> {
        let repo = GitRepo::open(path, &identity)?;
        let paper_dir = paper_dir(&repo);
        let version_path = paper_dir.join(VERSION_FILE);
        let found = match fs::read_to_string(&version_path) {
            Ok(content) => content.trim().to_owned(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(OpenError::NotAPaperProject {
                    path: path.to_owned(),
                });
            }
            Err(source) => {
                return Err(OpenError::Io {
                    path: version_path,
                    source,
                });
            }
        };
        if found != FORMAT_VERSION.to_string() {
            return Err(OpenError::UnsupportedFormat { found });
        }
        let view = View::load(&paper_dir)?.ok_or_else(|| OpenError::Incomplete {
            path: paper_dir.clone(),
        })?;
        Ok(Self {
            repo,
            paper_dir,
            view,
            identity,
            options: SnapshotOptions::default(),
        })
    }

    /// A copy of this project that applies `options` (large-file thresholds) in later snapshots.
    #[must_use]
    pub fn with_options(self, options: SnapshotOptions) -> Self {
        Self { options, ..self }
    }

    pub fn repo(&self) -> &GitRepo {
        &self.repo
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    /// `.git/paper/`
    pub fn paper_dir(&self) -> &Path {
        &self.paper_dir
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }
}

fn paper_dir(repo: &GitRepo) -> PathBuf {
    repo.git_dir().join(PAPER_DIR)
}

/// `refs/heads/<line>`: the line's saves, visible to plain git.
pub(crate) fn line_ref(line: &LineName) -> String {
    format!("refs/heads/{line}")
}

/// `refs/paper/draft/<line>`: the line's draft, never pushed (I5).
pub(crate) fn draft_ref(line: &LineName) -> String {
    format!("refs/paper/draft/{line}")
}

/// A signature for `identity` at the current time, in the local timezone.
pub(crate) fn signature_now(identity: &Identity) -> Signature {
    let now = jiff::Zoned::now();
    Signature {
        identity: identity.clone(),
        time: GitTime {
            seconds: now.timestamp().as_second(),
            offset_seconds: now.offset().seconds(),
        },
    }
}

/// Writes an empty draft on top of `parent`, holding `tree`.
pub(crate) fn write_draft(
    repo: &GitRepo,
    tree: &TreeId,
    parent: &CommitId,
    signature: &Signature,
) -> Result<CommitId, GitError> {
    repo.write_commit(&NewCommit {
        tree: tree.clone(),
        parents: vec![parent.clone()],
        author: signature.clone(),
        committer: signature.clone(),
        message: String::new(),
        change_id: Some(ChangeId::random()),
    })
}

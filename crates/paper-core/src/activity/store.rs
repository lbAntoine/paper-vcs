//! The files of the log: `ops/<id>.json` and `head`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{Op, OpId, OpKind};
use crate::fs_util;
use crate::view::View;

const ACTIVITY_DIR: &str = "activity";
const OPS_DIR: &str = "ops";
const HEAD_FILE: &str = "head";

#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("could not read or write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// `head` or an op file exists but is not valid.
    #[error("{path} is damaged: {reason}")]
    Corrupt { path: PathBuf, reason: String },
    /// A project that has been set up has no `head` (or, mid-migration, no `view.json`).
    #[error("{path} is missing, so paper cannot tell which state the project is in")]
    MissingHead { path: PathBuf },
    /// `head` or an op's `parent` names an op file that isn't there.
    #[error("the operation file {path} is missing")]
    MissingOp { path: PathBuf },
}

/// The operation log of one project, rooted at `<paper_dir>/activity/`.
#[derive(Debug, Clone)]
pub struct ActivityLog {
    root: PathBuf,
}

impl ActivityLog {
    /// The log under `paper_dir`. Touches nothing on disk; directories are created by the
    /// first [`ActivityLog::append`].
    #[must_use]
    pub fn open(paper_dir: &Path) -> Self {
        Self {
            root: paper_dir.join(ACTIVITY_DIR),
        }
    }

    /// The folder holding the log: `.git/paper/activity/`.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn head_path(&self) -> PathBuf {
        self.root.join(HEAD_FILE)
    }

    fn ops_dir(&self) -> PathBuf {
        self.root.join(OPS_DIR)
    }

    fn op_path(&self, id: &OpId) -> PathBuf {
        self.ops_dir().join(format!("{id}.json"))
    }

    /// The id `head` holds; `Ok(None)` when there is no head yet.
    ///
    /// # Errors
    ///
    /// [`ActivityError::Corrupt`] if `head` isn't a valid id, [`ActivityError::Io`] otherwise.
    pub fn head(&self) -> Result<Option<OpId>, ActivityError> {
        let path = self.head_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ActivityError::Io { path, source }),
        };
        text.trim()
            .parse()
            .map(Some)
            .map_err(|error: super::InvalidOpId| ActivityError::Corrupt {
                path,
                reason: error.to_string(),
            })
    }

    /// Reads one op.
    ///
    /// # Errors
    ///
    /// [`ActivityError::MissingOp`] if the file is absent, [`ActivityError::Corrupt`] if it
    /// isn't a valid op, [`ActivityError::Io`] otherwise.
    pub fn read(&self, id: &OpId) -> Result<Op, ActivityError> {
        let path = self.op_path(id);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(ActivityError::MissingOp { path });
            }
            Err(source) => return Err(ActivityError::Io { path, source }),
        };
        let op: Op = serde_json::from_slice(&bytes).map_err(|error| ActivityError::Corrupt {
            path: path.clone(),
            reason: error.to_string(),
        })?;
        if op.id != *id {
            return Err(ActivityError::Corrupt {
                path,
                reason: format!("it holds operation {}, not {id}", op.id),
            });
        }
        Ok(op)
    }

    /// Writes a new op whose parent is the current head (`None` if there is no head), then
    /// moves `head` to it. The op file is complete on disk before `head` changes, and the
    /// directories are flushed so a power cut leaves the old head or a complete new op.
    ///
    /// The caller must hold the project lock: two unlocked appends could both read the same
    /// head and fork the chain.
    ///
    /// # Errors
    ///
    /// [`ActivityError::Io`] if a file can't be written, and whatever [`ActivityLog::head`]
    /// reports.
    ///
    /// # Panics
    ///
    /// Never in practice: every field of an op serialises to JSON.
    pub fn append(
        &self,
        kind: OpKind,
        view: &View,
        description: &str,
    ) -> Result<Op, ActivityError> {
        let parent = self.head()?;
        let id = OpId::random();
        let op = Op {
            id: id.clone(),
            parent,
            kind,
            time: jiff::Timestamp::now().to_string(),
            view: view.clone(),
            position: id,
            redo: Vec::new(),
            description: description.to_owned(),
        };
        let mut json = serde_json::to_vec_pretty(&op).expect("an op always serialises");
        json.push(b'\n');

        let ops_dir = self.ops_dir();
        let io_err = |path: &Path| {
            let path = path.to_owned();
            move |source| ActivityError::Io { path, source }
        };
        fs::create_dir_all(&ops_dir).map_err(io_err(&ops_dir))?;
        let op_path = self.op_path(&op.id);
        fs_util::write_new(&op_path, &json).map_err(io_err(&op_path))?;
        fs_util::sync_dir(&ops_dir).map_err(io_err(&ops_dir))?;

        let head_path = self.head_path();
        fs_util::write_atomic(&head_path, format!("{}\n", op.id).as_bytes())
            .map_err(io_err(&head_path))?;
        // `head`'s entry, then the `activity/` and `ops/` entries themselves (new on the
        // first append): all durable before the caller writes `version`.
        fs_util::sync_dir(&self.root).map_err(io_err(&self.root))?;
        if let Some(paper_dir) = self.root.parent() {
            fs_util::sync_dir(paper_dir).map_err(io_err(paper_dir))?;
        }
        Ok(op)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::LineName;
    use crate::view::Line;

    fn view() -> View {
        let commit = |c: char| c.to_string().repeat(40).parse().unwrap();
        View::new(
            LineName::main(),
            Line {
                tip: commit('a'),
                draft: commit('b'),
            },
        )
    }

    #[test]
    fn an_op_written_and_read_back_equals_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let log = ActivityLog::open(dir.path());
        let written = log.append(OpKind::Init, &view(), "start").unwrap();
        let read = log.read(&written.id).unwrap();
        assert_eq!(read, written);
        assert_eq!(read.view, view());
    }

    #[test]
    fn no_temporary_files_are_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let log = ActivityLog::open(dir.path());
        log.append(OpKind::Init, &view(), "start").unwrap();
        log.append(OpKind::Save, &view(), "save").unwrap();
        assert_eq!(fs::read_dir(log.ops_dir()).unwrap().count(), 2);
        assert_eq!(fs::read_dir(log.root()).unwrap().count(), 2);
    }

    #[test]
    fn an_op_file_holding_another_id_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let log = ActivityLog::open(dir.path());
        let op = log.append(OpKind::Init, &view(), "start").unwrap();
        let other = OpId::random();
        fs::copy(log.op_path(&op.id), log.op_path(&other)).unwrap();
        assert!(matches!(
            log.read(&other),
            Err(ActivityError::Corrupt { .. })
        ));
    }

    #[test]
    fn a_garbled_head_is_corrupt_and_garbled_json_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let log = ActivityLog::open(dir.path());
        let op = log.append(OpKind::Init, &view(), "start").unwrap();
        fs::write(log.head_path(), "nope\n").unwrap();
        assert!(matches!(log.head(), Err(ActivityError::Corrupt { .. })));
        fs::write(log.op_path(&op.id), "{ not json").unwrap();
        assert!(matches!(
            log.read(&op.id),
            Err(ActivityError::Corrupt { .. })
        ));
    }
}

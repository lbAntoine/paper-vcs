//! `.git/paper/checkout-left.json`: what the last undo-family checkout had to leave alone
//! (03-architecture.md § Undo / redo algorithm, "Skipped paths").
//!
//! After `undo`, `redo` or `undo --to` leaves a file as it is (a symlink or an ignored file
//! in the way), the working files differ from the new draft at those paths. The next
//! undo-family command would record that difference as an edits entry and clear the redo
//! stack. This file remembers the leftovers, so that command can tell them from a real edit.
//! It is values only: it never prints, and a file that is stale or unreadable is simply
//! "nothing remembered".

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use paper_git::{FileKind, TreeFile};
use serde::{Deserialize, Serialize};

use crate::activity::OpId;
use crate::fs_util::write_atomic;

const LEFT_FILE: &str = "checkout-left.json";
const LEFT_SCHEMA: u32 = 1;

/// What a snapshot sees at a remembered path: a blob id and a git file mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Seen {
    id: String,
    mode: String,
}

impl Seen {
    fn of(file: &TreeFile) -> Self {
        let mode = match file.kind {
            FileKind::Regular => "100644",
            FileKind::Executable => "100755",
            FileKind::Symlink => "120000",
        };
        Self {
            id: file.blob.as_str().to_owned(),
            mode: mode.to_owned(),
        }
    }
}

/// The paths the checkout that recorded op `op` left alone, each with what a snapshot saw
/// there afterwards (`None`: nothing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CheckoutLeft {
    schema: u32,
    op: String,
    paths: BTreeMap<String, Option<Seen>>,
}

impl CheckoutLeft {
    /// Remembers `paths`, looking each up in `seen` (the files of the post-checkout snapshot).
    pub(super) fn new<'a>(
        op: &OpId,
        paths: impl IntoIterator<Item = &'a String>,
        seen: &BTreeMap<String, TreeFile>,
    ) -> Self {
        Self {
            schema: LEFT_SCHEMA,
            op: op.as_str().to_owned(),
            paths: paths
                .into_iter()
                .map(|path| (path.clone(), seen.get(path).map(Seen::of)))
                .collect(),
        }
    }

    pub(super) fn path(paper_dir: &Path) -> PathBuf {
        paper_dir.join(LEFT_FILE)
    }

    /// Writes the file atomically.
    pub(super) fn write(&self, paper_dir: &Path) -> io::Result<()> {
        let mut json = serde_json::to_vec_pretty(self).expect("the leftovers always serialise");
        json.push(b'\n');
        write_atomic(&Self::path(paper_dir), &json)
    }

    /// Removes the file; absent is fine.
    pub(super) fn remove(paper_dir: &Path) -> io::Result<()> {
        match fs::remove_file(Self::path(paper_dir)) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    }

    /// The remembered leftovers, only if the file is readable, of this schema, and names
    /// `head`. Anything else (absent, bad JSON, another op) is "nothing remembered", never an
    /// error.
    pub(super) fn load_for(paper_dir: &Path, head: &OpId) -> Option<Self> {
        let bytes = fs::read(Self::path(paper_dir)).ok()?;
        let left: Self = serde_json::from_slice(&bytes).ok()?;
        (left.schema == LEFT_SCHEMA && left.op == head.as_str()).then_some(left)
    }

    /// Whether the difference between the stored draft (`draft`) and the working files
    /// (`now`) is nothing but the remembered leftovers, each exactly as remembered.
    pub(super) fn explains(
        &self,
        draft: &BTreeMap<String, TreeFile>,
        now: &BTreeMap<String, TreeFile>,
    ) -> bool {
        let remembered_as_is = self
            .paths
            .iter()
            .all(|(path, seen)| now.get(path).map(Seen::of) == *seen);
        let differing_only_there = draft
            .keys()
            .chain(now.keys())
            .filter(|path| draft.get(*path) != now.get(*path))
            .all(|path| self.paths.contains_key(path));
        remembered_as_is && differing_only_there
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_remembers_nothing_and_removing_it_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let head: OpId = "0".repeat(32).parse().unwrap();
        assert!(CheckoutLeft::load_for(dir.path(), &head).is_none());
        CheckoutLeft::remove(dir.path()).unwrap();
    }

    #[test]
    fn garbage_and_foreign_files_remember_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let head: OpId = "0".repeat(32).parse().unwrap();
        let other: OpId = "1".repeat(32).parse().unwrap();
        fs::write(CheckoutLeft::path(dir.path()), b"not json").unwrap();
        assert!(CheckoutLeft::load_for(dir.path(), &head).is_none());
        CheckoutLeft::new(&other, [], &BTreeMap::new())
            .write(dir.path())
            .unwrap();
        assert!(CheckoutLeft::load_for(dir.path(), &head).is_none());
        assert!(CheckoutLeft::load_for(dir.path(), &other).is_some());
    }
}

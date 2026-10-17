//! The export step: after every recorded operation, git's refs, HEAD and index follow the
//! view, and `exported.json` records what paper wrote (03-architecture.md § Command lifecycle).
//!
//! Also the keep-alive chain: one commit per operation on `refs/paper/keep`, so `git gc` never
//! removes a commit any operation named.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use paper_git::{CommitId, GitError, GitRepo, Identity, NewCommit};
use serde::{Deserialize, Serialize};

use super::{draft_ref, line_ref, signature_now};
use crate::activity::ActivityError;
use crate::fs_util::write_atomic;
use crate::view::View;

const EXPORTED_FILE: &str = "exported.json";
const EXPORTED_SCHEMA: u32 = 1;
/// The keep-alive chain's ref.
pub(super) const KEEP_REF: &str = "refs/paper/keep";
const KEEP_MESSAGE: &str = "paper keep-alive";

/// Why the export step or the keep-alive commit failed.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Activity(#[from] ActivityError),
    #[error("could not write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// What paper last wrote into git: `.git/paper/exported.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Exported {
    schema: u32,
    /// The current line, which HEAD was attached to.
    head: String,
    /// Ref name to commit id.
    refs: BTreeMap<String, String>,
}

impl Exported {
    /// What paper last wrote for `ref_name`, if it wrote it.
    pub(super) fn ref_value(&self, ref_name: &str) -> Option<&str> {
        self.refs.get(ref_name).map(String::as_str)
    }
}

/// Reads `exported.json`. A missing or unreadable file is `None`: the next export rewrites it.
pub(super) fn read_exported(paper_dir: &Path) -> Option<Exported> {
    let bytes = std::fs::read(paper_dir.join(EXPORTED_FILE)).ok()?;
    let exported: Exported = serde_json::from_slice(&bytes).ok()?;
    (exported.schema == EXPORTED_SCHEMA).then_some(exported)
}

/// How an export relates to what came before.
pub(super) enum Mode<'a> {
    /// `init` only: the project is being set up, so write everything and check nothing. Any
    /// other caller must use [`Mode::Follow`], or it would overwrite what plain git did.
    Fresh {
        /// Whether to rewrite git's index from the current tip.
        reset_index: bool,
    },
    /// After an operation on top of `previous`: leave alone whatever plain git moved since
    /// paper last wrote, and rewrite the index only if the current tip changed.
    Follow { previous: &'a View },
}

/// Writes the keep-alive commit for an operation that turns `previous` into `view`, and moves
/// `refs/paper/keep` to it. Its parents are the previous keep commit and every commit of
/// `view` that `previous` did not have (all of them when there is no `previous`).
pub(super) fn write_keep(
    repo: &GitRepo,
    identity: &Identity,
    previous: Option<&View>,
    view: &View,
) -> Result<CommitId, GitError> {
    let old = previous.map(commits_of).unwrap_or_default();
    let mut parents: Vec<CommitId> = repo.find_ref(KEEP_REF)?.into_iter().collect();
    parents.extend(commits_of(view).into_iter().filter(|id| !old.contains(id)));
    let signature = signature_now(identity);
    let keep = repo.write_commit(&NewCommit {
        tree: repo.empty_tree()?,
        parents,
        author: signature.clone(),
        committer: signature,
        message: KEEP_MESSAGE.to_owned(),
        change_id: None,
    })?;
    repo.set_ref(KEEP_REF, &keep, "paper: keep")?;
    Ok(keep)
}

fn commits_of(view: &View) -> BTreeSet<CommitId> {
    view.lines()
        .flat_map(|(_, line)| [line.tip.clone(), line.draft.clone()])
        .collect()
}

/// Makes git's refs, HEAD and (when due) index follow `view`, then writes `exported.json`.
pub(super) fn export(
    repo: &GitRepo,
    paper_dir: &Path,
    view: &View,
    keep: &CommitId,
    mode: &Mode<'_>,
) -> Result<(), ExportError> {
    let exported = match mode {
        Mode::Fresh { .. } => None,
        Mode::Follow { .. } => read_exported(paper_dir),
    };
    let current = view.current_line();
    let mut refs = BTreeMap::new();
    let mut tip_moved = false;
    let mut line_left_alone = false;
    for (name, line) in view.lines() {
        let draft = draft_ref(name);
        if repo.find_ref(&draft)?.as_ref() != Some(&line.draft) {
            repo.set_ref(&draft, &line.draft, "paper: export")?;
        }
        refs.insert(draft, line.draft.as_str().to_owned());

        let name_ref = line_ref(name);
        let last = last_written(mode, exported.as_ref(), &name_ref, || match mode {
            Mode::Follow { previous } => previous.line(name).map(|l| l.tip.as_str().to_owned()),
            Mode::Fresh { .. } => None,
        });
        let found = repo.find_ref(&name_ref)?;
        let plain_git_moved = matches!(mode, Mode::Follow { .. })
            && found
                .as_ref()
                .is_some_and(|found| found != &line.tip && Some(found.as_str()) != last.as_deref());
        if plain_git_moved {
            if let Some(last) = last {
                refs.insert(name_ref, last);
            }
            line_left_alone |= name == current;
            continue;
        }
        if found.as_ref() != Some(&line.tip) {
            repo.set_ref(&name_ref, &line.tip, "paper: export")?;
        }
        if name == current {
            tip_moved = last.as_deref() != Some(line.tip.as_str());
        }
        refs.insert(name_ref, line.tip.as_str().to_owned());
    }
    refs.insert(KEEP_REF.to_owned(), keep.as_str().to_owned());

    let last_head = match mode {
        Mode::Follow { previous } => Some(
            exported
                .as_ref()
                .map_or_else(|| previous.current_line().to_string(), |e| e.head.clone()),
        ),
        Mode::Fresh { .. } => None,
    };
    let found_head = repo.head_branch()?;
    let head_left_alone = last_head.is_some()
        && found_head.as_deref() != Some(current.as_str())
        && found_head != last_head;
    let head = if head_left_alone {
        last_head.clone().unwrap_or_default()
    } else {
        if found_head.as_deref() != Some(current.as_str()) {
            repo.set_head_branch(current.as_str(), "paper: export")?;
        }
        current.to_string()
    };

    let index_due = match mode {
        Mode::Fresh { reset_index } => *reset_index,
        Mode::Follow { .. } => {
            !head_left_alone
                && !line_left_alone
                && (tip_moved || found_head.as_deref() != Some(current.as_str()))
        }
    };
    if index_due {
        repo.reset_index_to(&repo.read_commit(&view.current().tip)?.tree)?;
    }

    let record = Exported {
        schema: EXPORTED_SCHEMA,
        head,
        refs,
    };
    let mut json = serde_json::to_vec_pretty(&record).expect("exported.json always serialises");
    json.push(b'\n');
    let path = paper_dir.join(EXPORTED_FILE);
    write_atomic(&path, &json).map_err(|source| ExportError::Io { path, source })
}

/// What paper last wrote for `ref_name`: `exported.json` if it has the ref, else `fallback`.
fn last_written(
    mode: &Mode<'_>,
    exported: Option<&Exported>,
    ref_name: &str,
    fallback: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if matches!(mode, Mode::Fresh { .. }) {
        return None;
    }
    exported
        .and_then(|e| e.ref_value(ref_name).map(str::to_owned))
        .or_else(fallback)
}

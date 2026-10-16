//! `paper save`: turn the draft (or part of it) into a save (02-cli.md § paper save).

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::ops::Bound;
use std::path::PathBuf;

use paper_git::{ChangeId, CommitId, FileKind, GitError, NewCommit, TreeEdit};

use super::{Project, draft_ref, line_ref, signature_now, write_draft};
use crate::name::LineName;
use crate::snapshot::{Change, ChangeKind, Skipped, SnapshotError, StatCache, snapshot};
use crate::tree_diff::diff_trees;
use crate::view::{Line, ViewError};

/// Which of the draft's changes a save takes.
///
/// A path selects the file with exactly that path, or every file under it when it names a
/// directory (`sections` or `sections/`). Paths are repository-relative, with `/` separators.
/// The empty path is the project folder itself, so it selects every file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSelection {
    /// The whole draft.
    All,
    /// Only the files matching these paths.
    Only(Vec<String>),
    /// Everything except the files matching these paths.
    AllBut(Vec<String>),
}

/// What to save. The message is data: `paper-core` never prompts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub message: String,
    pub paths: PathSelection,
}

/// The draft after a snapshot: what it holds relative to the line's last save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftState {
    /// Changes from the line's last save to the draft, sorted by path.
    pub changes: Vec<Change>,
    /// Things the snapshot deliberately left out.
    pub skipped: Vec<Skipped>,
    /// Large tracked files, with their size.
    pub large: Vec<(String, u64)>,
}

/// A save that was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub save: CommitId,
    /// The change ID of the save: the one the draft had.
    pub change_id: ChangeId,
    pub line: LineName,
    /// What the save contains, relative to the previous save.
    pub saved: Vec<Change>,
    /// What stayed in the new draft.
    pub remaining: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The draft (or the selection) was empty; nothing was written.
    NothingToSave,
    Saved(Saved),
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("a save needs a message; describe what changed")]
    MissingMessage,
    #[error("none of these paths has unsaved changes: {}", paths.join(", "))]
    NoChangesAt { paths: Vec<String> },
    #[error(
        "the line '{line}' was changed with plain git since paper last saw it; paper can't save on top of that yet (a later version will). Nothing was saved or changed. Run `git log {line}` to see what changed"
    )]
    OutOfSyncWithGit { line: LineName },
    #[error(
        "git is on {} instead of the line '{line}'; switch back with `git switch {line}`",
        found.as_ref().map_or("no branch (detached HEAD)".to_owned(), |b| format!("branch '{b}'"))
    )]
    HeadOnAnotherBranch {
        line: LineName,
        /// The branch HEAD points at; `None` when HEAD is detached.
        found: Option<String>,
    },
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("could not write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Invariant I1 as a reusable step: records the working files in the current line's
    /// draft. The draft commit is rewritten (same change ID, same parent) only if its tree
    /// changed.
    ///
    /// # Errors
    ///
    /// [`SaveError`] if the working files can't be read or the repository can't be written.
    pub fn snapshot(&mut self) -> Result<DraftState, SaveError> {
        let line_name = self.view.current_line().clone();
        let line = self.view.current().clone();
        let draft = self.repo.read_commit(&line.draft)?;
        let previous = StatCache::load(&self.paper_dir);
        let snap = snapshot(&self.repo, &draft.tree, &previous, &self.options)?;

        if snap.tree != draft.tree {
            let signature = signature_now(&self.identity);
            let rewritten = self.repo.write_commit(&NewCommit {
                tree: snap.tree.clone(),
                parents: draft.parents.clone(),
                author: draft.author.clone(),
                committer: signature,
                message: draft.message.clone(),
                change_id: draft.change_id,
            })?;
            self.repo
                .set_ref(&draft_ref(&line_name), &rewritten, "paper: snapshot")?;
            let view = self.view.with_line(
                line_name,
                Line {
                    tip: line.tip.clone(),
                    draft: rewritten,
                },
            );
            view.save(&self.paper_dir)?;
            self.view = view;
        }
        snap.cache
            .save(&self.paper_dir)
            .map_err(|source| SaveError::Io {
                path: self.paper_dir.clone(),
                source,
            })?;

        let tip_tree = self.repo.read_commit(&line.tip)?.tree;
        Ok(DraftState {
            changes: diff_trees(&self.repo, &tip_tree, &snap.tree)?,
            skipped: snap.skipped,
            large: snap.large,
        })
    }

    /// Turns the draft (or the selected part of it) into a save on the current line and
    /// starts a fresh draft on top.
    ///
    /// # Errors
    ///
    /// [`SaveError::OutOfSyncWithGit`] if git moved the line, [`SaveError::MissingMessage`]
    /// for an empty message, [`SaveError::NoChangesAt`] for paths matching nothing, and the
    /// wrapped lower-level errors.
    pub fn save(&mut self, request: &SaveRequest) -> Result<SaveOutcome, SaveError> {
        let line_name = self.view.current_line().clone();
        self.check_in_sync(&line_name)?;
        let message = request.message.trim();
        if message.is_empty() {
            return Err(SaveError::MissingMessage);
        }

        let state = self.snapshot()?;
        let (selected, _) = select(&request.paths, state.changes.clone())?;
        if selected.is_empty() {
            return Ok(SaveOutcome::NothingToSave);
        }
        // A selection that already covers every change (always so for `All`) needs no closure.
        let selected = if selected.len() == state.changes.len() {
            selected
        } else {
            close_over_conflicts(selected, &state.changes)
        };

        let line = self.view.current().clone();
        let draft = self.repo.read_commit(&line.draft)?;
        let tip_tree = self.repo.read_commit(&line.tip)?.tree;
        let save_tree = if selected.len() == state.changes.len() {
            draft.tree.clone()
        } else {
            let files = self.repo.tree_files(&draft.tree)?;
            // Removals first, so a file turning into a directory (or back) edits cleanly.
            let (removed, kept): (Vec<_>, Vec<_>) = selected.iter().partition(|change| {
                change.kind == ChangeKind::Deleted || !files.contains_key(&change.path)
            });
            let edits: Vec<TreeEdit> = removed
                .iter()
                .map(|change| TreeEdit::Remove {
                    path: change.path.clone(),
                })
                .chain(kept.iter().map(|change| {
                    let file = &files[&change.path];
                    // Snapshots never record symlinks, so the executable flag is enough.
                    debug_assert_ne!(file.kind, FileKind::Symlink);
                    TreeEdit::Upsert {
                        path: change.path.clone(),
                        blob: file.blob.clone(),
                        executable: file.kind == FileKind::Executable,
                    }
                }))
                .collect();
            self.repo.edit_tree(&tip_tree, &edits)?
        };
        // Derived from the trees, so the outcome is true by construction.
        let saved = diff_trees(&self.repo, &tip_tree, &save_tree)?;
        let remaining = diff_trees(&self.repo, &save_tree, &draft.tree)?;

        let signature = signature_now(&self.identity);
        // Drafts written by paper always carry a change ID; a missing one means the commit
        // came from elsewhere, so the save gets a fresh ID.
        let change_id = draft.change_id.unwrap_or_else(ChangeId::random);
        let save = self.repo.write_commit(&NewCommit {
            tree: save_tree.clone(),
            parents: vec![line.tip.clone()],
            author: signature.clone(),
            committer: signature.clone(),
            message: message.to_owned(),
            change_id: Some(change_id),
        })?;
        let new_draft = write_draft(&self.repo, &draft.tree, &save, &signature)?;

        // The line ref is the commit point: until it moves, plain git sees no save.
        self.repo
            .set_ref(&draft_ref(&line_name), &new_draft, "paper: save")?;
        self.repo.reset_index_to(&save_tree)?;
        self.repo
            .set_ref(&line_ref(&line_name), &save, "paper: save")?;
        // The window between the line ref and the view is deliberate until the v0.2
        // activity log makes the writes recoverable.
        let view = self.view.with_line(
            line_name.clone(),
            Line {
                tip: save.clone(),
                draft: new_draft,
            },
        );
        view.save(&self.paper_dir)?;
        self.view = view;

        Ok(SaveOutcome::Saved(Saved {
            save,
            change_id,
            line: line_name,
            saved,
            remaining,
        }))
    }

    /// Refuses to go on when plain git moved the line or switched branches.
    fn check_in_sync(&self, line: &LineName) -> Result<(), SaveError> {
        let head = self.repo.head_branch()?;
        if head.as_deref() != Some(line.as_str()) {
            return Err(SaveError::HeadOnAnotherBranch {
                line: line.clone(),
                found: head,
            });
        }
        if self.repo.find_ref(&line_ref(line))?.as_ref() != Some(&self.view.current().tip) {
            return Err(SaveError::OutOfSyncWithGit { line: line.clone() });
        }
        Ok(())
    }
}

/// Splits `changes` into (saved, remaining) according to `selection`.
fn select(
    selection: &PathSelection,
    changes: Vec<Change>,
) -> Result<(Vec<Change>, Vec<Change>), SaveError> {
    let (paths, keep_matches) = match selection {
        PathSelection::All => return Ok((changes, Vec::new())),
        PathSelection::Only(paths) => (paths, true),
        PathSelection::AllBut(paths) => (paths, false),
    };
    // Complexity: O((P + C) log C) for P selected paths and C changes (times the path depth,
    // which is small). Each selected path is looked up in the sorted change paths by binary
    // search, and each change is tested against the set of selected paths through its own
    // ancestors, so no path is ever compared with every change. Results match `matches`.
    let mut sorted: Vec<&str> = changes.iter().map(|change| change.path.as_str()).collect();
    sorted.sort_unstable();
    let unmatched: Vec<String> = paths
        .iter()
        .filter(|path| !any_change_under(&sorted, path))
        .cloned()
        .collect();
    if !unmatched.is_empty() {
        return Err(SaveError::NoChangesAt { paths: unmatched });
    }
    let chosen: BTreeSet<&str> = paths
        .iter()
        .map(|path| path.trim_end_matches('/'))
        .collect();
    Ok(changes
        .into_iter()
        .partition(|change| is_selected(&chosen, &change.path) == keep_matches))
}

/// Whether some file in `sorted` (ascending) is selected by `path`, as `matches` defines it.
///
/// The files under `p` are exactly the keys in `[p + "/", p + "0")`: `'0'` is the byte after
/// `'/'`, so the range stops before neighbours such as `p-x`, `p.x` and `p0`, which sort
/// between `p` and `p/`.
fn any_change_under(sorted: &[&str], path: &str) -> bool {
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return !sorted.is_empty();
    }
    let lower = format!("{path}/");
    let upper = format!("{path}0");
    let exact = sorted.binary_search(&path).is_ok();
    let start = sorted.partition_point(|file| *file < lower.as_str());
    exact || sorted.get(start).is_some_and(|file| *file < upper.as_str())
}

/// Whether `file` is selected by any of the (slash-trimmed) paths in `chosen`: the empty path,
/// the file itself, or one of its ancestor directories.
fn is_selected(chosen: &BTreeSet<&str>, file: &str) -> bool {
    chosen.contains("")
        || chosen.contains(file)
        || file
            .match_indices('/')
            .any(|(at, _)| chosen.contains(&file[..at]))
}

/// A selected path names the file itself or a directory containing it. The empty path is the
/// project folder, which contains every file.
pub(super) fn matches(selected: &str, file: &str) -> bool {
    let selected = selected.trim_end_matches('/');
    selected.is_empty()
        || file == selected
        || file
            .strip_prefix(selected)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Pulls in the deletions a valid tree needs once `selected` is saved. Only a selected
/// addition or modification forces anything: a file `a/b/c` forces the deletion of its
/// ancestors `a` and `a/b`, and a file `p` forces the deletion of every file under `p/`.
/// Forced changes are deletions, which force nothing, so one pass is enough.
///
/// Lookups use a sorted map (ancestors by exact key, descendants by key range), so the cost
/// is O(n log n) rather than a scan of every pair.
fn close_over_conflicts<'a>(selected: Vec<Change>, all: &'a [Change]) -> Vec<Change> {
    let by_path: BTreeMap<&str, &Change> = all.iter().map(|c| (c.path.as_str(), c)).collect();
    let taken: BTreeSet<&str> = selected.iter().map(|c| c.path.as_str()).collect();
    let mut forced: BTreeMap<&str, &Change> = BTreeMap::new();
    let mut force = |change: &'a Change| {
        if change.kind == ChangeKind::Deleted && !taken.contains(change.path.as_str()) {
            forced.insert(change.path.as_str(), change);
        }
    };
    for change in selected.iter().filter(|c| c.kind != ChangeKind::Deleted) {
        let path = change.path.as_str();
        for (slash, _) in path.match_indices('/') {
            if let Some(above) = by_path.get(&path[..slash]) {
                force(above);
            }
        }
        // "p/" up to "p0": '0' is the character right after '/'.
        let (low, high) = (format!("{path}/"), format!("{path}0"));
        for (_, below) in by_path.range::<str, _>((
            Bound::Included(low.as_str()),
            Bound::Excluded(high.as_str()),
        )) {
            force(below);
        }
    }
    let mut selected = selected;
    selected.extend(forced.into_values().cloned());
    selected
}

#[cfg(test)]
mod tests {
    use super::{PathSelection, SaveError, matches, select};
    use crate::snapshot::{Change, ChangeKind};

    #[test]
    fn a_path_matches_the_file_itself_or_whole_components_below_it() {
        assert!(matches("a/b.tex", "a/b.tex"));
        assert!(matches("a", "a/b.tex"));
        assert!(matches("a/", "a/b.tex"));
        assert!(!matches("a/b", "a/b.tex"));
        assert!(!matches("sec", "sections/intro.tex"));
    }

    #[test]
    fn the_empty_path_is_the_project_folder_and_matches_every_file() {
        assert!(matches("", "a.tex"));
        assert!(matches("", "a/b/c.tex"));
    }

    // ---- differential tests: `select` against a deliberately naive reference

    /// A tiny seeded generator (xorshift64*), so failures reproduce without a dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, bound: usize) -> usize {
            usize::try_from(self.next() % bound as u64).unwrap()
        }

        fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
            &items[self.below(items.len())]
        }
    }

    /// Names that are neighbours in sort order, so nesting exercises `/` against ` -.`.
    const COMPONENTS: [&str; 8] = ["a", "a b", "a-b", "a.b", "b", "ab", "a0", "a/b"];

    fn kind(rng: &mut Rng) -> ChangeKind {
        *rng.pick(&[ChangeKind::Added, ChangeKind::Modified, ChangeKind::Deleted])
    }

    fn random_path(rng: &mut Rng) -> String {
        let depth = 1 + rng.below(3);
        let parts: Vec<&str> = (0..depth).map(|_| *rng.pick(&COMPONENTS)).collect();
        parts.join("/")
    }

    /// Changes sorted by path with no repeats, as `snapshot` produces them.
    fn random_changes(rng: &mut Rng, count: usize) -> Vec<Change> {
        let mut paths: Vec<String> = (0..count).map(|_| random_path(rng)).collect();
        paths.sort();
        paths.dedup();
        paths
            .into_iter()
            .map(|path| Change {
                path,
                kind: kind(rng),
            })
            .collect()
    }

    /// Paths that name changes, their ancestors, near misses, the empty path and strangers.
    fn random_selection(rng: &mut Rng, changes: &[Change], count: usize) -> Vec<String> {
        (0..count)
            .map(|_| {
                let mut path = match rng.below(6) {
                    0 if !changes.is_empty() => rng.pick(changes).path.clone(),
                    1 if !changes.is_empty() => {
                        let full = &rng.pick(changes).path;
                        let cuts: Vec<usize> = full.match_indices('/').map(|(i, _)| i).collect();
                        if cuts.is_empty() {
                            full.clone()
                        } else {
                            full[..*rng.pick(&cuts)].to_owned()
                        }
                    }
                    2 => String::new(),
                    3 => {
                        let mut near = random_path(rng);
                        near.pop();
                        near
                    }
                    _ => random_path(rng),
                };
                if rng.below(4) == 0 {
                    path.push('/');
                }
                path
            })
            .collect()
    }

    /// The obvious reading of the rules: every path against every change.
    fn reference(
        selection: &PathSelection,
        changes: &[Change],
    ) -> Result<(Vec<Change>, Vec<Change>), Vec<String>> {
        let (paths, keep_matches) = match selection {
            PathSelection::All => return Ok((changes.to_vec(), Vec::new())),
            PathSelection::Only(paths) => (paths, true),
            PathSelection::AllBut(paths) => (paths, false),
        };
        let covers = |path: &str, file: &str| {
            let path = path.trim_end_matches('/');
            path.is_empty() || file == path || file.starts_with(&format!("{path}/"))
        };
        let unmatched: Vec<String> = paths
            .iter()
            .filter(|path| changes.iter().all(|change| !covers(path, &change.path)))
            .cloned()
            .collect();
        if !unmatched.is_empty() {
            return Err(unmatched);
        }
        let mut chosen = Vec::new();
        let mut rest = Vec::new();
        for change in changes {
            let hit = paths.iter().any(|path| covers(path, &change.path));
            if hit == keep_matches {
                chosen.push(change.clone());
            } else {
                rest.push(change.clone());
            }
        }
        Ok((chosen, rest))
    }

    fn actual(
        selection: &PathSelection,
        changes: &[Change],
    ) -> Result<(Vec<Change>, Vec<Change>), Vec<String>> {
        select(selection, changes.to_vec()).map_err(|err| match err {
            SaveError::NoChangesAt { paths } => paths,
            other => panic!("unexpected error {other:?}"),
        })
    }

    #[test]
    fn select_agrees_with_the_reference_on_generated_inputs() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let (mut selected_ok, mut refused) = (0, 0);
        for round in 0..4000 {
            let count = rng.below(14);
            let changes = random_changes(&mut rng, count);
            let wanted = rng.below(6);
            // Mostly selections that match, so both outcomes are well covered.
            let mut paths = random_selection(&mut rng, &changes, wanted);
            if rng.below(3) != 0 {
                paths.retain(|p| changes.iter().any(|c| reference_hit(p, &c.path)));
            }
            for selection in [
                PathSelection::All,
                PathSelection::Only(paths.clone()),
                PathSelection::AllBut(paths),
            ] {
                let expected = reference(&selection, &changes);
                match &expected {
                    Ok(_) => selected_ok += 1,
                    Err(_) => refused += 1,
                }
                assert_eq!(
                    actual(&selection, &changes),
                    expected,
                    "round {round}: {selection:?} over {changes:?}"
                );
            }
        }
        assert!(
            selected_ok > 3000 && refused > 500,
            "{selected_ok}/{refused}"
        );
    }

    fn reference_hit(path: &str, file: &str) -> bool {
        let path = path.trim_end_matches('/');
        path.is_empty() || file == path || file.starts_with(&format!("{path}/"))
    }

    #[test]
    fn select_handles_the_sort_neighbours_of_a_directory() {
        let changes: Vec<Change> = ["a b", "a-b", "a.b", "a/b", "a/b/c", "a0", "ab"]
            .iter()
            .map(|path| Change {
                path: (*path).to_owned(),
                kind: ChangeKind::Modified,
            })
            .collect();
        for path in ["a", "a/", "a b", "a/b", "a/b/", "ab", "", "a-", "a/b/c"] {
            for selection in [
                PathSelection::Only(vec![path.to_owned()]),
                PathSelection::AllBut(vec![path.to_owned()]),
            ] {
                assert_eq!(
                    actual(&selection, &changes),
                    reference(&selection, &changes),
                    "{selection:?}"
                );
            }
        }
    }

    #[test]
    fn select_agrees_with_the_reference_on_a_large_input() {
        let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
        let changes: Vec<Change> = (0..20_000)
            .map(|i| Change {
                path: format!("d{:03}/s{}/f{i:05}.tex", i % 200, i % 7),
                kind: kind(&mut rng),
            })
            .collect::<Vec<_>>();
        let mut changes = changes;
        changes.sort_by(|x, y| x.path.cmp(&y.path));

        // 2 000 paths that all match: files, whole folders, with and without a trailing slash.
        let paths: Vec<String> = (0..2_000)
            .map(|_| {
                let full = &rng.pick(&changes).path;
                let mut path = match rng.below(3) {
                    0 => full.clone(),
                    1 => full
                        .rsplit_once('/')
                        .map_or(String::new(), |p| p.0.to_owned()),
                    _ => full.split('/').next().unwrap().to_owned(),
                };
                if rng.below(4) == 0 {
                    path.push('/');
                }
                path
            })
            .collect();

        for selection in [
            PathSelection::Only(paths.clone()),
            PathSelection::AllBut(paths.clone()),
        ] {
            let expected = reference(&selection, &changes);
            assert!(expected.is_ok());
            assert_eq!(actual(&selection, &changes), expected);
        }

        let mut with_stranger = paths;
        with_stranger.insert(1000, "nowhere/x".to_owned());
        let selection = PathSelection::Only(with_stranger);
        assert_eq!(
            actual(&selection, &changes),
            Err(vec!["nowhere/x".to_owned()])
        );
        assert_eq!(
            reference(&selection, &changes),
            Err(vec!["nowhere/x".to_owned()])
        );
    }
}

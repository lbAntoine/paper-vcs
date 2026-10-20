//! What `paper restore` may write and delete, decided before anything on disk changes.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::Project;
use super::restore::RestoreSkip;

/// The outcome of [`Project::settle`].
pub(super) struct Plan<'a> {
    /// Files to remove, before any write.
    pub deletes: Vec<&'a String>,
    /// Files to write after the deletions.
    pub writes: Vec<&'a String>,
    /// Paths left alone, with why: the writes that can't happen, and the deletions that
    /// only existed to make room for them.
    pub blocked: Vec<(String, RestoreSkip)>,
}

impl Project {
    /// Drops every write that something untracked blocks, and with it the deletions that
    /// were only there to make room, until the plan is stable (all-or-nothing swaps).
    pub(super) fn settle<'a>(
        &self,
        candidates: Vec<&'a String>,
        mut deletes: Vec<&'a String>,
        in_draft: impl Fn(&str) -> bool,
    ) -> Plan<'a> {
        let mut writes = candidates;
        let mut blocked = Vec::new();
        // Dropping a deletion can turn an obstacle that was about to be removed into one
        // that stays, which blocks other writes, so repeat until nothing changes.
        loop {
            let doomed: BTreeSet<&str> = deletes.iter().map(|p| p.as_str()).collect();
            let Some((at, reason)) = writes
                .iter()
                .enumerate()
                .find_map(|(at, path)| Some((at, self.obstacle(path, &in_draft, &doomed)?)))
            else {
                break;
            };
            let path = writes.remove(at);
            // A write that lands on a differently spelled entry also frees that entry.
            let mut names = vec![path.clone()];
            names.extend(actual_spelling(self.repo.workdir(), path));
            let root = self.repo.workdir();
            // The target and each of its parent paths: removing the same file under another
            // spelling would take the blocked write's file or folder with it.
            let prefixes: Vec<&str> = path
                .match_indices('/')
                .map(|(at, _)| &path[..at])
                .chain([path.as_str()])
                .collect();
            deletes.retain(|gone| {
                let room_for_it = names.iter().any(|name| makes_room(gone, name))
                    || prefixes.iter().any(|prefix| same_file(root, gone, prefix));
                if room_for_it {
                    let kept = RestoreSkip::KeptForSwap {
                        target: path.clone(),
                    };
                    blocked.push(((*gone).clone(), kept));
                }
                !room_for_it
            });
            blocked.push((path.clone(), reason));
        }
        Plan {
            deletes,
            writes,
            blocked,
        }
    }

    /// What stops `path` from being written, if anything: a symlinked parent, an untracked
    /// (ignored or oversize) file or directory in the way, or a parent that is a file the
    /// restore isn't removing.
    ///
    /// An entry that is the same name in another case (`notes.tex` where `Notes.tex` goes)
    /// is in the way only if the restore isn't removing it; when it is a draft file, the
    /// reason names it. Entries are compared by listing folders, so this never needs to
    /// know whether the filesystem ignores case.
    fn obstacle(
        &self,
        path: &str,
        in_draft: &impl Fn(&str) -> bool,
        doomed: &BTreeSet<&str>,
    ) -> Option<RestoreSkip> {
        let root = self.repo.workdir();
        let mut prefix = String::new();
        for part in path.split('/').take(path.matches('/').count()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            let Ok(meta) = fs::symlink_metadata(root.join(&prefix)) else {
                continue;
            };
            let real = actual_spelling(root, &prefix).unwrap_or_else(|| prefix.clone());
            if meta.file_type().is_symlink()
                || (meta.is_file()
                    && !doomed.contains(real.as_str())
                    && !any_same_file(root, &prefix, doomed))
                // A folder spelled differently stays unless this restore empties it.
                || (meta.is_dir() && real != prefix && !only_holds(root, &real, doomed))
            {
                return Some(RestoreSkip::Blocked);
            }
        }
        if in_draft(path) {
            return None;
        }
        let meta = fs::symlink_metadata(root.join(path)).ok()?;
        let real = actual_spelling(root, path).unwrap_or_else(|| path.to_owned());
        if meta.is_dir() {
            return (!only_holds(root, &real, doomed)).then_some(RestoreSkip::Blocked);
        }
        if doomed.contains(real.as_str()) || any_same_file(root, path, doomed) {
            return None;
        }
        Some(if real != path && in_draft(&real) {
            RestoreSkip::CaseAlias { other: real }
        } else {
            RestoreSkip::Blocked
        })
    }
}

/// Whether `a` and `b` (relative to `root`) are one file that the filesystem shows under two
/// spellings: the same device and inode (without following symlinks), but not both listed
/// in their folders with exactly that name. A folded name (Unicode normalisation, case) is
/// listed once, under one spelling; a real hard link is listed under both, and stays two
/// paths. This sees through any folding rule without knowing it.
///
/// Only on Unix: elsewhere std has no stable file id, so this is `false` and the name
/// comparison decides.
pub(super) fn same_file(root: &Path, a: &str, b: &str) -> bool {
    same_inode(root, a, b) && !(listed_exactly(root, a) && listed_exactly(root, b))
}

#[cfg(unix)]
fn same_inode(root: &Path, a: &str, b: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (
        fs::symlink_metadata(root.join(a)),
        fs::symlink_metadata(root.join(b)),
    ) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_inode(_root: &Path, _a: &str, _b: &str) -> bool {
    false
}

/// Whether every part of `path` is listed on disk with exactly that spelling.
fn listed_exactly(root: &Path, path: &str) -> bool {
    let mut dir = root.to_owned();
    for part in path.split('/') {
        let listed = fs::read_dir(&dir)
            .is_ok_and(|entries| entries.flatten().any(|e| e.file_name() == part));
        if !listed {
            return false;
        }
        dir.push(part);
    }
    true
}

/// Whether `path` is the same file as any of the `doomed` paths.
fn any_same_file(root: &Path, path: &str, doomed: &BTreeSet<&str>) -> bool {
    doomed.iter().any(|gone| same_file(root, gone, path))
}

/// Whether removing `gone` only serves to let `target` be written: it is under the target,
/// or it is a file at one of the target's parent paths.
fn makes_room(gone: &str, target: &str) -> bool {
    let under = |outer: &str, inner: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
    };
    gone == target || under(target, gone) || under(gone, target)
}

/// How `path` is really spelled on disk, found by listing each folder: an exact name wins,
/// otherwise the entry with the same name ignoring case.
///
/// "Ignoring case" is `str::to_lowercase`, which lowercases fully rather than folding
/// simply: `ß` and Turkish `İ` don't match the way a filesystem's own rules would. So an
/// entry that resolves to the same file (device and inode) as `path` also counts, which
/// catches Unicode normalisation and any other folding the filesystem applies.
fn actual_spelling(root: &Path, path: &str) -> Option<String> {
    let mut found = String::new();
    for part in path.split('/') {
        let mut alias = None;
        let mut exact = false;
        for entry in fs::read_dir(root.join(&found)).ok()?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == part {
                exact = true;
                break;
            }
            if alias.is_none()
                && (name.to_lowercase() == part.to_lowercase()
                    || same_inode(root, &join(&found, &name), &join(&found, part)))
            {
                alias = Some(name);
            }
        }
        let name = if exact { part.to_owned() } else { alias? };
        if !found.is_empty() {
            found.push('/');
        }
        found.push_str(&name);
    }
    Some(found)
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// Whether every file under the directory `dir` is about to be removed by this restore.
fn only_holds(root: &Path, dir: &str, doomed: &BTreeSet<&str>) -> bool {
    let Ok(entries) = fs::read_dir(root.join(dir)) else {
        return false;
    };
    entries.flatten().all(|entry| {
        let child = format!("{dir}/{}", entry.file_name().to_string_lossy());
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => only_holds(root, &child, doomed),
            Ok(kind) if kind.is_file() => {
                doomed.contains(child.as_str()) || any_same_file(root, &child, doomed)
            }
            _ => false,
        }
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    const NFC: &str = "\u{e9}t\u{e9}.tex";
    const NFD: &str = "e\u{301}te\u{301}.tex";

    /// A temp dir with `a` and `b` hard-linked (one inode, two listed names: a real hard
    /// link) and an unrelated `other`.
    fn linked() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("a"), "x").expect("write a");
        fs::hard_link(dir.path().join("a"), dir.path().join("b")).expect("link");
        fs::write(dir.path().join("other"), "y").expect("write other");
        dir
    }

    /// A temp dir holding the NFC-spelled file, or `None` (with a note) when the
    /// filesystem does not fold the NFC and NFD spellings to one file.
    fn folded() -> Option<tempfile::TempDir> {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join(NFC), "x").expect("write");
        if fs::symlink_metadata(dir.path().join(NFD)).is_err() {
            eprintln!("skipped: this filesystem does not fold Unicode forms");
            return None;
        }
        Some(dir)
    }

    #[test]
    fn a_real_hard_link_is_two_files_not_one_folded_name() {
        let dir = linked();
        assert!(same_inode(dir.path(), "a", "b"));
        // Both names are listed, so deleting one leaves the other: not the same file.
        assert!(!same_file(dir.path(), "a", "b"));
        assert!(!same_file(dir.path(), "a", "other"));
        assert!(!any_same_file(dir.path(), "b", &["a"].into()));
    }

    #[test]
    fn a_hard_linked_untracked_file_still_blocks_a_write() {
        let dir = linked();
        let project = project_over(&dir);
        let (a, b) = ("a".to_owned(), "b".to_owned());
        let plan = project.settle(vec![&b], vec![&a], |p| p == "a");
        assert_eq!(plan.blocked[0].1, RestoreSkip::Blocked);
        assert!(plan.writes.is_empty());
        assert_invariant(dir.path(), &plan);
    }

    #[test]
    fn a_folded_name_is_the_same_file() {
        let Some(dir) = folded() else { return };
        assert!(same_file(dir.path(), NFC, NFD));
        assert_eq!(actual_spelling(dir.path(), NFD).as_deref(), Some(NFC));
    }

    /// A project over the linked folder, for running the real planner.
    fn project_over(dir: &tempfile::TempDir) -> Project {
        let identity = paper_git::Identity {
            name: "Mina Park".to_owned(),
            email: "mina@uni.example".to_owned(),
        };
        Project::init_with_profile(
            dir.path(),
            identity,
            &crate::snapshot::SnapshotOptions::default(),
            crate::profile::Profile::builtin("generic"),
        )
        .expect("init")
        .0
    }

    /// The data-safety invariant: no planned deletion removes a file that a blocked or
    /// written path refers to on disk, unless it is a swap (deleted, then written).
    fn assert_invariant(root: &Path, plan: &Plan<'_>) {
        for gone in &plan.deletes {
            for (path, _) in plan
                .blocked
                .iter()
                .filter(|(_, why)| !matches!(why, RestoreSkip::KeptForSwap { .. }))
            {
                assert!(
                    !same_file(root, gone, path),
                    "{gone} is deleted but {path}, which is the same file, is blocked"
                );
            }
        }
    }

    #[test]
    fn a_write_onto_the_same_file_as_a_deletion_is_a_swap() {
        let Some(dir) = folded() else { return };
        let project = project_over(&dir);
        let (old, new) = (NFD.to_owned(), NFC.to_owned());
        let plan = project.settle(vec![&new], vec![&old], |p| p == NFD);
        assert_eq!(plan.deletes, vec![&old]);
        assert_eq!(plan.writes, vec![&new]);
        assert!(plan.blocked.is_empty());
        assert_invariant(dir.path(), &plan);
    }

    #[test]
    fn a_file_deleted_under_another_spelling_makes_room_for_its_folder() {
        let Some(dir) = folded() else { return };
        let project = project_over(&dir);
        // Disk: a file `été.tex`. Plan: delete it as NFD, write `été.tex/x` (a folder).
        let (old, new) = (NFD.to_owned(), format!("{NFC}/x"));
        let plan = project.settle(vec![&new], vec![&old], |p| p == NFD);
        assert_eq!(plan.deletes, vec![&old]);
        assert_eq!(plan.writes, vec![&new]);
        assert_invariant(dir.path(), &plan);
    }

    #[test]
    fn an_unrelated_untracked_file_blocks_its_write_and_spares_the_deletion_of_another() {
        let dir = linked();
        let project = project_over(&dir);
        let (a, other) = ("a".to_owned(), "other".to_owned());
        let plan = project.settle(vec![&other], vec![&a], |p| p == "a");
        assert_eq!(plan.deletes, vec![&a]);
        assert!(plan.writes.is_empty());
        assert_eq!(
            plan.blocked,
            vec![("other".to_owned(), RestoreSkip::Blocked)]
        );
        assert_invariant(dir.path(), &plan);
    }
}

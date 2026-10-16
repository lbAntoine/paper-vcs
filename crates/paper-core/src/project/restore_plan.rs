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
            deletes.retain(|gone| {
                let room_for_it = names.iter().any(|name| makes_room(gone, name));
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
                || (meta.is_file() && !doomed.contains(real.as_str()))
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
        if doomed.contains(real.as_str()) {
            return None;
        }
        Some(if real != path && in_draft(&real) {
            RestoreSkip::CaseAlias { other: real }
        } else {
            RestoreSkip::Blocked
        })
    }
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
/// simply: `ß` and Turkish `İ` don't match the way a filesystem's own rules would. A
/// spelling this misses is treated as a different file, so restore errs towards leaving
/// things alone (a `Blocked` skip) rather than overwriting.
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
            if alias.is_none() && name.to_lowercase() == part.to_lowercase() {
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

/// Whether every file under the directory `dir` is about to be removed by this restore.
fn only_holds(root: &Path, dir: &str, doomed: &BTreeSet<&str>) -> bool {
    let Ok(entries) = fs::read_dir(root.join(dir)) else {
        return false;
    };
    entries.flatten().all(|entry| {
        let child = format!("{dir}/{}", entry.file_name().to_string_lossy());
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => only_holds(root, &child, doomed),
            Ok(kind) if kind.is_file() => doomed.contains(child.as_str()),
            _ => false,
        }
    })
}

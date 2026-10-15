//! Walks the working directory the way git would: `.gitignore`, `.git/info/exclude` and the
//! global excludes file are respected, and `.git` itself is never entered.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ignore::WalkBuilder;

use super::{SkipReason, Skipped, SnapshotError};

/// A regular file found in the working directory.
#[derive(Debug)]
pub(super) struct WorkFile {
    /// Relative to the repository root, `/`-separated.
    pub path: String,
    pub absolute: PathBuf,
}

#[derive(Debug, Default)]
pub(super) struct Walk {
    pub files: Vec<WorkFile>,
    pub skipped: Vec<Skipped>,
}

pub(super) fn walk(root: &Path) -> Result<Walk, SnapshotError> {
    // `filter_entry` must be `'static`, so nested repositories are collected through a shared list.
    let nested = Arc::new(Mutex::new(Vec::new()));
    let nested_in_filter = Arc::clone(&nested);
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_global(true)
        .git_ignore(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .sort_by_file_name(Ord::cmp)
        .filter_entry(move |entry| {
            if entry.file_name() == ".git" {
                return false;
            }
            let is_nested_repo = entry.depth() > 0
                && entry.file_type().is_some_and(|t| t.is_dir())
                && entry.path().join(".git").exists();
            if is_nested_repo {
                nested_in_filter
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(entry.path().to_owned());
            }
            !is_nested_repo
        })
        .build();

    let mut result = Walk::default();
    for entry in walker {
        let entry = entry?;
        let Some(file_type) = entry.file_type() else {
            continue; // stdin; never happens for a directory walk
        };
        if entry.depth() == 0 || file_type.is_dir() {
            continue;
        }
        let Some(path) = relative_path(root, entry.path()) else {
            result.skipped.push(Skipped {
                path: entry.path().to_string_lossy().into_owned(),
                reason: SkipReason::NonUtf8Path,
            });
            continue;
        };
        if file_type.is_symlink() {
            result.skipped.push(Skipped {
                path,
                reason: SkipReason::Symlink,
            });
        } else if file_type.is_file() {
            result.files.push(WorkFile {
                path,
                absolute: entry.into_path(),
            });
        }
    }

    let nested = std::mem::take(&mut *nested.lock().unwrap_or_else(PoisonError::into_inner));
    result.skipped.extend(nested.iter().filter_map(|dir| {
        relative_path(root, dir).map(|path| Skipped {
            path,
            reason: SkipReason::NestedRepository,
        })
    }));
    Ok(result)
}

/// `root/sections/intro.tex` → `sections/intro.tex` with `/` separators on every platform.
/// `None` if the path isn't valid UTF-8.
pub(super) fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let parts: Option<Vec<&str>> = relative
        .components()
        .map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect();
    Some(parts?.join("/"))
}

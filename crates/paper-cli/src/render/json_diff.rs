//! `--json` output for `status`, `diff` and `restore`; `show` reuses [`files`].
//! Shapes are documented in `docs/specs/02-cli.md` (JSON output).

use paper_core::project::{ChangeStat, FileDiffEntry, RestoreSkip, Restored, SkippedPath, Status};
use paper_diff::{FileDiff, LineKind};
use serde::Serialize;

use super::json::{
    LargeFile, SCHEMA, SaveInfo, SkippedFile, kind_name, large, line, save_info, skipped,
};

#[derive(Serialize)]
pub struct DiffLine {
    kind: &'static str,
    text: String,
    missing_newline: bool,
}

#[derive(Serialize)]
pub struct Hunk {
    old_start: usize,
    old_len: usize,
    new_start: usize,
    new_len: usize,
    lines: Vec<DiffLine>,
}

/// One changed file. `added`/`removed` are `null` for binary files, the byte sizes are `null`
/// for text files (and for an absent side).
#[derive(Serialize)]
pub struct DiffFile {
    path: String,
    kind: &'static str,
    binary: bool,
    added: Option<usize>,
    removed: Option<usize>,
    old_bytes: Option<u64>,
    new_bytes: Option<u64>,
    hunks: Vec<Hunk>,
}

#[derive(Serialize)]
struct StatusChange {
    path: String,
    kind: &'static str,
    binary: bool,
    added: Option<usize>,
    removed: Option<usize>,
    old_bytes: Option<u64>,
    new_bytes: Option<u64>,
}

#[derive(Serialize)]
struct StatusOut {
    schema: u32,
    line: String,
    last: SaveInfo,
    changes: Vec<StatusChange>,
    skipped: Vec<SkippedFile>,
    large: Vec<LargeFile>,
}

#[derive(Serialize)]
struct DiffOut {
    schema: u32,
    files: Vec<DiffFile>,
}

#[derive(Serialize)]
struct RestoreChange {
    path: String,
    kind: &'static str,
}

#[derive(Serialize)]
struct RestoreOut {
    schema: u32,
    from: String,
    changes: Vec<RestoreChange>,
    skipped: Vec<SkippedFile>,
}

fn kind_of(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Context => "context",
        LineKind::Added => "added",
        LineKind::Removed => "removed",
    }
}

pub fn files(entries: &[FileDiffEntry]) -> Vec<DiffFile> {
    entries.iter().map(file).collect()
}

fn file(entry: &FileDiffEntry) -> DiffFile {
    let (binary, added, removed, old_bytes, new_bytes, hunks) = match &entry.diff {
        FileDiff::Text(text) => {
            let hunks = text
                .hunks
                .iter()
                .map(|hunk| Hunk {
                    old_start: hunk.old_start,
                    old_len: hunk.old_len,
                    new_start: hunk.new_start,
                    new_len: hunk.new_len,
                    lines: hunk
                        .lines
                        .iter()
                        .map(|l| DiffLine {
                            kind: kind_of(l.kind),
                            text: l.text.clone(),
                            missing_newline: l.missing_newline,
                        })
                        .collect(),
                })
                .collect();
            (
                false,
                Some(text.added),
                Some(text.removed),
                None,
                None,
                hunks,
            )
        }
        FileDiff::Identical => (false, Some(0), Some(0), None, None, Vec::new()),
        FileDiff::Binary { old_size, new_size } => {
            (true, None, None, *old_size, *new_size, Vec::new())
        }
    };
    DiffFile {
        path: entry.path.clone(),
        kind: kind_name(entry.kind),
        binary,
        added,
        removed,
        old_bytes,
        new_bytes,
        hunks,
    }
}

pub fn diff(entries: &[FileDiffEntry]) -> String {
    line(&DiffOut {
        schema: SCHEMA,
        files: files(entries),
    })
}

pub fn status(status: &Status) -> String {
    let changes = status
        .changes
        .iter()
        .map(|change| {
            let (binary, added, removed, old_bytes, new_bytes) = match change.stat {
                ChangeStat::Lines { added, removed } => {
                    (false, Some(added), Some(removed), None, None)
                }
                ChangeStat::Bytes { old, new } => (true, None, None, old, new),
            };
            StatusChange {
                path: change.path.clone(),
                kind: kind_name(change.kind),
                binary,
                added,
                removed,
                old_bytes,
                new_bytes,
            }
        })
        .collect();
    line(&StatusOut {
        schema: SCHEMA,
        line: status.line.to_string(),
        last: save_info(&status.last),
        changes,
        skipped: status.skipped.iter().map(skipped).collect(),
        large: large(&status.large),
    })
}

pub fn restore(restored: &Restored) -> String {
    line(&RestoreOut {
        schema: SCHEMA,
        from: restored.from.to_string(),
        changes: restored
            .changes
            .iter()
            .map(|change| RestoreChange {
                path: change.path.clone(),
                kind: kind_name(change.kind),
            })
            .collect(),
        skipped: restore_skipped(&restored.skipped),
    })
}

/// Paths restore (or undo) left alone, in the shape of `restore --json`'s `skipped`.
pub(super) fn restore_skipped(skipped: &[SkippedPath]) -> Vec<SkippedFile> {
    skipped
        .iter()
        .map(|item| {
            let (reason, bytes) = match &item.reason {
                RestoreSkip::SavedSymlink => ("saved_symlink", None),
                RestoreSkip::Symlink => ("symlink", None),
                RestoreSkip::SymlinkedFolder => ("symlinked_folder", None),
                RestoreSkip::TooLarge { bytes } => ("too_large", Some(*bytes)),
                RestoreSkip::NestedRepository => ("nested_repository", None),
                RestoreSkip::NonUtf8Path => ("non_utf8_path", None),
                RestoreSkip::Blocked => ("blocked", None),
                RestoreSkip::CaseAlias { .. } => ("case_alias", None),
                RestoreSkip::KeptForSwap { .. } => ("kept_for_swap", None),
            };
            let entry = SkippedFile::new(item.path.clone(), reason, bytes);
            match &item.reason {
                RestoreSkip::CaseAlias { other } => entry.with_other(other.clone()),
                RestoreSkip::KeptForSwap { target } => entry.with_target(target.clone()),
                _ => entry,
            }
        })
        .collect()
}

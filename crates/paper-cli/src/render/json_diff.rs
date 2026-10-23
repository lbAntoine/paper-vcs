//! `--json` output for `status`, `diff` and `restore`; `show` reuses [`files`].
//! Shapes are documented in `docs/specs/02-cli.md` (JSON output).

use paper_core::project::{
    ChangeStat, EntryDiff, FileDiffEntry, OutsidePaper, RestoreSkip, Restored, SkippedPath, Status,
};
use paper_diff::{FileDiff, LineKind, Segment, WordDiff};
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
    /// `null` in word mode.
    hunks: Option<Vec<Hunk>>,
    /// `"words"` or `"lines"`; `null` for binary files.
    unit: Option<&'static str>,
    layout_only: bool,
    /// `null` in line mode.
    paragraphs: Option<Vec<ParagraphOut>>,
}

#[derive(Serialize)]
pub struct ParagraphOut {
    number: usize,
    segments: Vec<SegmentOut>,
}

/// `text` is `null` only for a `cut`.
#[derive(Serialize)]
pub struct SegmentOut {
    kind: &'static str,
    text: Option<String>,
}

fn paragraphs(diff: &WordDiff) -> Vec<ParagraphOut> {
    diff.paragraphs
        .iter()
        .map(|paragraph| ParagraphOut {
            number: paragraph.number,
            segments: paragraph
                .segments
                .iter()
                .map(|segment| match segment {
                    Segment::Same(text) => SegmentOut {
                        kind: "same",
                        text: Some(text.clone()),
                    },
                    Segment::Removed(text) => SegmentOut {
                        kind: "removed",
                        text: Some(text.clone()),
                    },
                    Segment::Added(text) => SegmentOut {
                        kind: "added",
                        text: Some(text.clone()),
                    },
                    Segment::Cut => SegmentOut {
                        kind: "cut",
                        text: None,
                    },
                })
                .collect(),
        })
        .collect()
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
    unit: Option<&'static str>,
}

#[derive(Serialize)]
struct StatusOut {
    schema: u32,
    line: String,
    last: SaveInfo,
    changes: Vec<StatusChange>,
    skipped: Vec<SkippedFile>,
    large: Vec<LargeFile>,
    outside_paper: Option<OutsidePaperOut>,
}

/// Where the line stands when plain git moved it. Key order is `kind, line, found`.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum OutsidePaperOut {
    LineMoved { line: String },
    OnAnotherBranch { line: String, found: Option<String> },
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
    let diff = match &entry.diff {
        EntryDiff::Lines(diff) => diff,
        EntryDiff::Words(words) => {
            return DiffFile {
                path: entry.path.clone(),
                kind: kind_name(entry.kind),
                binary: false,
                added: Some(words.added),
                removed: Some(words.removed),
                old_bytes: None,
                new_bytes: None,
                hunks: None,
                unit: Some("words"),
                layout_only: words.layout_only,
                paragraphs: Some(paragraphs(words)),
            };
        }
    };
    let (binary, added, removed, old_bytes, new_bytes, hunks) = match diff {
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
        unit: (!binary).then_some("lines"),
        hunks: Some(hunks),
        layout_only: false,
        paragraphs: None,
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
            let (binary, added, removed, old_bytes, new_bytes, unit) = match change.stat {
                ChangeStat::Lines { added, removed } => {
                    (false, Some(added), Some(removed), None, None, Some("lines"))
                }
                ChangeStat::Words { added, removed, .. } => {
                    (false, Some(added), Some(removed), None, None, Some("words"))
                }
                ChangeStat::Bytes { old, new } => (true, None, None, old, new, None),
            };
            StatusChange {
                path: change.path.clone(),
                kind: kind_name(change.kind),
                binary,
                added,
                removed,
                old_bytes,
                new_bytes,
                unit,
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
        outside_paper: status.outside_paper.as_ref().map(|outside| match outside {
            OutsidePaper::LineMoved { line } => OutsidePaperOut::LineMoved {
                line: line.to_string(),
            },
            OutsidePaper::OnAnotherBranch { line, found } => OutsidePaperOut::OnAnotherBranch {
                line: line.to_string(),
                found: found.clone(),
            },
        }),
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

//! `--json` output: CLI-owned DTOs, so `paper-core` carries no serde derives. Every object has
//! `"schema": 1`; the shapes are part of the CLI's contract.

use paper_core::project::{HistoryEntry, Initialized, SaveOutcome, Saved, Shown};
use paper_core::snapshot::{Change, ChangeKind, SkipReason, Skipped};
use serde::Serialize;

const SCHEMA: u32 = 1;

#[derive(Serialize)]
struct FileChange {
    path: String,
    kind: &'static str,
}

#[derive(Serialize)]
struct Author {
    name: String,
    email: String,
}

#[derive(Serialize)]
struct SaveInfo {
    id: String,
    change_id: Option<String>,
    commit: String,
    message: String,
    author: Author,
    /// RFC 3339, in the author's own UTC offset.
    time: String,
    tip_of: Vec<String>,
}

#[derive(Serialize)]
struct SkippedFile {
    path: String,
    reason: &'static str,
    bytes: Option<u64>,
}

#[derive(Serialize)]
struct LargeFile {
    path: String,
    bytes: u64,
}

#[derive(Serialize)]
struct InitOut {
    schema: u32,
    adopted: bool,
    tracked_files: usize,
    skipped: Vec<SkippedFile>,
    large: Vec<LargeFile>,
    skipped_branches: Vec<String>,
}

#[derive(Serialize)]
struct SaveOut {
    schema: u32,
    saved: bool,
    id: Option<String>,
    change_id: Option<String>,
    commit: Option<String>,
    line: Option<String>,
    message: Option<String>,
    saved_files: Vec<FileChange>,
    remaining: Vec<FileChange>,
}

#[derive(Serialize)]
struct HistoryOut {
    schema: u32,
    saves: Vec<SaveInfo>,
}

#[derive(Serialize)]
struct ShowOut {
    schema: u32,
    save: SaveInfo,
    changes: Vec<FileChange>,
}

fn changes(changes: &[Change]) -> Vec<FileChange> {
    changes
        .iter()
        .map(|change| FileChange {
            path: change.path.clone(),
            kind: kind_name(change.kind),
        })
        .collect()
}

pub fn kind_name(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "added",
        ChangeKind::Modified => "modified",
        ChangeKind::Deleted => "deleted",
    }
}

fn skipped(skipped: &Skipped) -> SkippedFile {
    let (reason, bytes) = match skipped.reason {
        SkipReason::TooLarge { bytes } => ("too_large", Some(bytes)),
        SkipReason::Symlink => ("symlink", None),
        SkipReason::NestedRepository => ("nested_repository", None),
        SkipReason::NonUtf8Path => ("non_utf8_path", None),
    };
    SkippedFile {
        path: skipped.path.clone(),
        reason,
        bytes,
    }
}

fn save_info(entry: &HistoryEntry) -> SaveInfo {
    SaveInfo {
        id: entry.short_change_id.clone(),
        change_id: entry.change_id.map(|id| id.to_string()),
        commit: entry.commit.to_string(),
        message: entry.message.clone(),
        author: Author {
            name: entry.author.name.clone(),
            email: entry.author.email.clone(),
        },
        time: rfc3339(entry.time.seconds, entry.time.offset_seconds),
        tip_of: entry.tip_of.iter().map(ToString::to_string).collect(),
    }
}

/// `2026-10-16T09:30:00+02:00`; the raw seconds if the time is out of range.
fn rfc3339(seconds: i64, offset_seconds: i32) -> String {
    let zoned = jiff::Timestamp::from_second(seconds).and_then(|ts| {
        let offset = jiff::tz::Offset::from_seconds(offset_seconds)?;
        Ok(ts.to_zoned(jiff::tz::TimeZone::fixed(offset)))
    });
    zoned.map_or_else(
        |_| seconds.to_string(),
        |z| z.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(),
    )
}

fn line<T: Serialize>(value: &T) -> String {
    // The DTOs hold only strings, numbers and bools, which always serialise.
    format!("{}\n", serde_json::to_string(value).unwrap_or_default())
}

pub fn init(initialized: &Initialized) -> String {
    line(&InitOut {
        schema: SCHEMA,
        adopted: initialized.adopted,
        tracked_files: initialized.tracked_files,
        skipped: initialized.skipped.iter().map(skipped).collect(),
        large: initialized
            .large
            .iter()
            .map(|(path, bytes)| LargeFile {
                path: path.clone(),
                bytes: *bytes,
            })
            .collect(),
        skipped_branches: initialized.skipped_branches.clone(),
    })
}

/// `short_id` is the saved change's shortest unique ID; `message` is what the researcher typed.
pub fn save(outcome: &SaveOutcome, short_id: &str, message: &str) -> String {
    match outcome {
        SaveOutcome::NothingToSave => line(&SaveOut {
            schema: SCHEMA,
            saved: false,
            id: None,
            change_id: None,
            commit: None,
            line: None,
            message: None,
            saved_files: Vec::new(),
            remaining: Vec::new(),
        }),
        SaveOutcome::Saved(Saved {
            save,
            change_id,
            line: line_name,
            saved,
            remaining,
        }) => line(&SaveOut {
            schema: SCHEMA,
            saved: true,
            id: Some(short_id.to_owned()),
            change_id: Some(change_id.to_string()),
            commit: Some(save.to_string()),
            line: Some(line_name.to_string()),
            message: Some(message.to_owned()),
            saved_files: changes(saved),
            remaining: changes(remaining),
        }),
    }
}

pub fn history(entries: &[HistoryEntry]) -> String {
    line(&HistoryOut {
        schema: SCHEMA,
        saves: entries.iter().map(save_info).collect(),
    })
}

pub fn show(shown: &Shown) -> String {
    line(&ShowOut {
        schema: SCHEMA,
        save: save_info(&shown.entry),
        changes: changes(&shown.changes),
    })
}

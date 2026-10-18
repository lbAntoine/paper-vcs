//! `--json` output: CLI-owned DTOs, so `paper-core` carries no serde derives. Every object has
//! `"schema": 1`; the shapes are part of the CLI's contract.

use paper_core::activity::OpKind;
use paper_core::project::{
    ActivityEntry, FileDiffEntry, HistoryEntry, Initialized, ProfileSource, SaveOutcome, Saved,
    Shown,
};
use paper_core::snapshot::{Change, ChangeKind, SkipReason, Skipped};
use serde::Serialize;

pub(super) const SCHEMA: u32 = 1;

#[derive(Serialize)]
pub(super) struct FileChange {
    path: String,
    kind: &'static str,
}

#[derive(Serialize)]
struct Author {
    name: String,
    email: String,
}

#[derive(Serialize)]
pub(super) struct SaveInfo {
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
pub(super) struct SkippedFile {
    path: String,
    reason: &'static str,
    bytes: Option<u64>,
    /// `case_alias` (restore): the draft file that differs only in letter case.
    #[serde(skip_serializing_if = "Option::is_none")]
    other: Option<String>,
    /// `kept_for_swap` (restore): the path that couldn't be restored.
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
}

#[derive(Serialize)]
pub(super) struct LargeFile {
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
    profile: String,
    detected: bool,
    written: Vec<String>,
    tips: Vec<String>,
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
    files: Vec<super::json_diff::DiffFile>,
}

pub(super) fn changes(changes: &[Change]) -> Vec<FileChange> {
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

pub(super) fn skipped(skipped: &Skipped) -> SkippedFile {
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
        other: None,
        target: None,
    }
}

pub(super) fn save_info(entry: &HistoryEntry) -> SaveInfo {
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

impl SkippedFile {
    pub(super) fn new(path: String, reason: &'static str, bytes: Option<u64>) -> Self {
        Self {
            path,
            reason,
            bytes,
            other: None,
            target: None,
        }
    }

    pub(super) fn with_other(mut self, other: String) -> Self {
        self.other = Some(other);
        self
    }

    pub(super) fn with_target(mut self, target: String) -> Self {
        self.target = Some(target);
        self
    }
}

pub(super) fn large(large: &[(String, u64)]) -> Vec<LargeFile> {
    large
        .iter()
        .map(|(path, bytes)| LargeFile {
            path: path.clone(),
            bytes: *bytes,
        })
        .collect()
}

pub(super) fn line<T: Serialize>(value: &T) -> String {
    // The DTOs hold only strings, numbers and bools, which always serialise.
    format!("{}\n", serde_json::to_string(value).unwrap_or_default())
}

pub fn init(initialized: &Initialized) -> String {
    line(&InitOut {
        schema: SCHEMA,
        adopted: initialized.adopted,
        tracked_files: initialized.tracked_files,
        skipped: initialized.skipped.iter().map(skipped).collect(),
        large: large(&initialized.large),
        skipped_branches: initialized.skipped_branches.clone(),
        profile: initialized.profile.clone(),
        detected: initialized.profile_source == ProfileSource::Detected,
        written: initialized.written.clone(),
        tips: initialized.tips.clone(),
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

pub fn show(shown: &Shown, diffs: &[FileDiffEntry]) -> String {
    line(&ShowOut {
        schema: SCHEMA,
        save: save_info(&shown.entry),
        changes: changes(&shown.changes),
        files: super::json_diff::files(diffs),
    })
}

#[derive(Serialize)]
struct ActivityInfo {
    id: String,
    short_id: String,
    /// RFC 3339.
    time: String,
    kind: &'static str,
    description: String,
}

#[derive(Serialize)]
struct ActivityOut {
    schema: u32,
    entries: Vec<ActivityInfo>,
}

pub fn activity(entries: &[ActivityEntry]) -> String {
    line(&ActivityOut {
        schema: SCHEMA,
        entries: entries
            .iter()
            .map(|e| ActivityInfo {
                id: e.id.to_string(),
                short_id: e.short_id.clone(),
                time: e.time.clone(),
                kind: op_kind_name(e.kind),
                description: e.description.clone(),
            })
            .collect(),
    })
}

pub(super) fn op_kind_name(kind: OpKind) -> &'static str {
    match kind {
        OpKind::Init => "init",
        OpKind::Migrate => "migrate",
        OpKind::Snapshot => "snapshot",
        OpKind::Save => "save",
        OpKind::Restore => "restore",
        OpKind::Undo => "undo",
        OpKind::Redo => "redo",
        OpKind::Ignore => "ignore",
    }
}

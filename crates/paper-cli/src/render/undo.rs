//! Output for `paper undo`, `paper redo` and `paper undo --to`.
//! The JSON shape is documented in `docs/specs/02-cli.md` (`paper undo` / `redo` / `activity`).

use std::fmt::Write as _;

use paper_core::project::{UndoOutcome, Undone};
use serde::Serialize;

use super::human::{change_lines, files};
use super::json::{FileChange, SCHEMA, SkippedFile, changes, line, op_kind_name};
use super::json_diff::restore_skipped;
use super::status::restore_skipped_lines;

/// Which command ran; it picks the verb and the JSON `outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Undo,
    Redo,
    GoBack,
}

/// The text for stdout.
pub fn human(verb: Verb, outcome: &UndoOutcome) -> String {
    match outcome {
        UndoOutcome::NothingToUndo => "Nothing to undo.\n".to_owned(),
        UndoOutcome::NothingToRedo => "Nothing to redo.\n".to_owned(),
        UndoOutcome::Undone(undone) => undone_text(verb, undone),
    }
}

fn undone_text(verb: Verb, undone: &Undone) -> String {
    let entry = &undone.entry;
    let mut out = match verb {
        Verb::Undo => format!("Undid: {}\n", entry.description),
        Verb::Redo => format!("Redid: {}\n", entry.description),
        Verb::GoBack => format!("Went back to {}: {}\n", entry.short_id, entry.description),
    };
    if !undone.changed.is_empty() {
        let _ = writeln!(out, "\nChanged {}:", files(undone.changed.len()));
        out.push_str(&change_lines(&undone.changed));
    }
    if !undone.skipped.is_empty() {
        out.push_str("\nSkipped (left as they are):\n");
        out.push_str(&restore_skipped_lines(&undone.skipped));
    }
    if !undone.files_match {
        out.push('\n');
        out.push_str(&warning(undone.skipped.len()));
        out.push('\n');
    }
    out
}

fn warning(skipped: usize) -> String {
    if skipped == 1 {
        "1 file was left as it is (see above); your next paper command will record it as an \
         edit, and that clears what you can redo."
            .to_owned()
    } else {
        format!(
            "{skipped} files were left as they are (see above); your next paper command will \
             record them as edits, and that clears what you can redo."
        )
    }
}

#[derive(Serialize)]
struct EntryInfo {
    id: String,
    short_id: String,
    /// RFC 3339.
    time: String,
    kind: &'static str,
    description: String,
}

#[derive(Serialize)]
struct UndoOut {
    schema: u32,
    outcome: &'static str,
    entry: Option<EntryInfo>,
    changed: Vec<FileChange>,
    skipped: Vec<SkippedFile>,
    files_match: Option<bool>,
}

/// One line of JSON with every key present.
pub fn json(verb: Verb, outcome: &UndoOutcome) -> String {
    let out = match outcome {
        UndoOutcome::NothingToUndo => nothing("nothing_to_undo"),
        UndoOutcome::NothingToRedo => nothing("nothing_to_redo"),
        UndoOutcome::Undone(undone) => UndoOut {
            schema: SCHEMA,
            outcome: match verb {
                Verb::Undo => "undone",
                Verb::Redo => "redone",
                Verb::GoBack => "went_back",
            },
            entry: Some(EntryInfo {
                id: undone.entry.id.to_string(),
                short_id: undone.entry.short_id.clone(),
                time: undone.entry.time.clone(),
                kind: op_kind_name(undone.entry.kind),
                description: undone.entry.description.clone(),
            }),
            changed: changes(&undone.changed),
            skipped: restore_skipped(&undone.skipped),
            files_match: Some(undone.files_match),
        },
    };
    line(&out)
}

fn nothing(outcome: &'static str) -> UndoOut {
    UndoOut {
        schema: SCHEMA,
        outcome,
        entry: None,
        changed: Vec::new(),
        skipped: Vec::new(),
        files_match: None,
    }
}

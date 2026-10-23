//! Human output for `paper status` and `paper restore`.

use std::fmt::Write as _;

use paper_core::project::{ChangeStat, OutsidePaper, RestoreSkip, Restored, SkippedPath, Status};
use paper_core::snapshot::{ChangeKind, SkipReason};

use super::diff::{binary_sizes, counts};
use super::human::{change_lines, files, large_lines, size, skipped_lines};
use super::style::Style;
use super::time::relative_time;

fn status_kind(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "new",
        ChangeKind::Modified => "modified",
        ChangeKind::Deleted => "deleted",
    }
}

pub fn status(status: &Status, now: i64, style: Style) -> String {
    let last = &status.last;
    let mut out = format!(
        "On {} \u{b7} last save {} \"{}\" ({})\n\n",
        status.line,
        style.id(&last.short_change_id),
        last.message,
        relative_time(last.time.seconds, now)
    );
    if let Some(outside) = &status.outside_paper {
        out.push_str(&outside_paper(outside));
        out.push('\n');
    }
    if status.changes.is_empty() {
        out.push_str("Nothing unsaved.\n");
    } else {
        out.push_str("Draft (unsaved):\n");
        let width = status
            .changes
            .iter()
            .map(|c| c.path.chars().count())
            .max()
            .unwrap_or(0);
        for change in &status.changes {
            let detail = match change.stat {
                ChangeStat::Lines { added, removed } => format!("{} lines", counts(added, removed)),
                ChangeStat::Bytes { old, new } => binary_sizes(old, new),
            };
            let _ = writeln!(
                out,
                "  {:<8}  {:<width$}  {detail}",
                status_kind(change.kind),
                change.path
            );
        }
    }
    if !status.skipped.is_empty() {
        out.push_str("\nSkipped (paper leaves these out of the draft):\n");
        out.push_str(&skipped_lines(&status.skipped));
        if status
            .skipped
            .iter()
            .any(|item| matches!(item.reason, SkipReason::TooLarge { .. }))
        {
            out.push_str("  Too large to track? See `paper help large-files`.\n");
        }
    }
    if !status.large.is_empty() {
        out.push_str("\nLarge files (tracked, but they make saves slow):\n");
        out.push_str(&large_lines(&status.large));
    }
    out
}

/// The block that says plain git moved things, worded like save's refusal.
fn outside_paper(outside: &OutsidePaper) -> String {
    let (text, command, explanation) = match outside {
        OutsidePaper::LineMoved { line } => (
            format!(
                "The line '{line}' was changed outside paper: commits were added to it, or it \
                 was moved, with plain git. Paper can't save on top of that yet."
            ),
            format!("git log {line}"),
            "see what changed",
        ),
        OutsidePaper::OnAnotherBranch { line, found } => {
            let place = found
                .as_ref()
                .map_or("no branch (detached HEAD)".to_owned(), |b| {
                    format!("the branch '{b}'")
                });
            (
                format!("Git is on {place}, not on the line '{line}'."),
                format!("git switch {line}"),
                "go back to the line",
            )
        }
    };
    format!("{text}\n  next: {command}  # {explanation}\n")
}

pub fn restore(restored: &Restored) -> String {
    let mut out = String::new();
    if restored.changes.is_empty() && restored.skipped.is_empty() {
        return "Nothing to restore: those files already match.\n".to_owned();
    }
    if restored.changes.is_empty() {
        out.push_str("Nothing was restored.\n");
    } else {
        let _ = writeln!(out, "Restored {}:", files(restored.changes.len()));
        out.push_str(&change_lines(&restored.changes));
    }
    if !restored.skipped.is_empty() {
        out.push_str("\nSkipped (left as they are):\n");
        out.push_str(&restore_skipped_lines(&restored.skipped));
    }
    out
}

/// One line per path restore (or undo) left alone, with the reason in parentheses.
pub(super) fn restore_skipped_lines(skipped: &[SkippedPath]) -> String {
    let mut out = String::new();
    for item in skipped {
        let why = match &item.reason {
            RestoreSkip::SavedSymlink => "the saved version is a symbolic link".to_owned(),
            RestoreSkip::Symlink => "a symbolic link".to_owned(),
            RestoreSkip::SymlinkedFolder => "inside a symbolic link to a folder".to_owned(),
            RestoreSkip::TooLarge { bytes } => {
                format!("too large to track, {}", size(*bytes))
            }
            RestoreSkip::NestedRepository => "inside a separate git repository".to_owned(),
            RestoreSkip::NonUtf8Path => "its name can't be read as text".to_owned(),
            RestoreSkip::Blocked => "a file paper doesn't track is in the way".to_owned(),
            RestoreSkip::CaseAlias { other } => format!(
                "differs only in letter case from {other}; run `paper restore {} {other}`",
                item.path
            ),
            RestoreSkip::KeptForSwap { target } => {
                format!("kept because {target} can't be restored")
            }
        };
        let _ = writeln!(out, "  {}  ({why})", item.path);
    }
    out
}

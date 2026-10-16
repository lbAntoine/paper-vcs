//! Human-readable output for stdout. Every function returns the full text (lines end in `\n`);
//! `-q` is applied by the caller, which prints nothing on success.

use std::fmt::Write as _;

use paper_core::profile::Profile;
use paper_core::project::FileDiffEntry;
use paper_core::project::{HistoryEntry, Initialized, ProfileSource, Saved, Shown};
use paper_core::snapshot::{Change, ChangeKind, SkipReason, Skipped};

use super::Verbosity;
use super::diff;
use super::style::Style;
use super::time::relative_time;

/// Width of the time column in `history`: wide enough for `59 minutes ago`.
const TIME_WIDTH: usize = 14;

pub(super) fn kind_label(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "added",
        ChangeKind::Modified => "modified",
        ChangeKind::Deleted => "deleted",
    }
}

pub(super) fn change_lines(changes: &[Change]) -> String {
    let mut out = String::new();
    for change in changes {
        let _ = writeln!(out, "  {:<8}  {}", kind_label(change.kind), change.path);
    }
    out
}

/// `318 KB`, `2.1 GB`: sizes in powers of 1024, one decimal above megabytes.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    #[allow(clippy::cast_precision_loss)] // sizes are shown to one decimal, not exactly
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub(super) fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

/// One line per skipped file, with the reason in parentheses.
pub(super) fn skipped_lines(skipped: &[Skipped]) -> String {
    let mut out = String::new();
    for item in skipped {
        let why = match item.reason {
            SkipReason::TooLarge { bytes } => format!("too large to track, {}", size(bytes)),
            SkipReason::Symlink => "a symbolic link".to_owned(),
            SkipReason::NestedRepository => "a separate git repository".to_owned(),
            SkipReason::NonUtf8Path => "its name can't be read as text".to_owned(),
        };
        let _ = writeln!(out, "  {}  ({why})", item.path);
    }
    out
}

/// One line per large file, with its size.
pub(super) fn large_lines(large: &[(String, u64)]) -> String {
    let mut out = String::new();
    for (path, bytes) in large {
        let _ = writeln!(out, "  {path}  ({})", size(*bytes));
    }
    out
}

pub fn init(initialized: &Initialized) -> String {
    let mut out = if initialized.adopted {
        "Adopted this git repository as a paper project. Nothing in it was rewritten.\n".to_owned()
    } else {
        "Created a paper project, with a first save: \"Start project\".\n".to_owned()
    };
    let _ = writeln!(out, "Tracking {}.", files(initialized.tracked_files));
    let _ = writeln!(out, "Profile: {}", profile_origin(initialized));
    if !initialized.written.is_empty() {
        let _ = write!(out, "Wrote {}", initialized.written.join(" and "));
        out.push_str(
            if initialized.adopted && !initialized.written_in_first_save {
                ". They show up as unsaved changes in your draft.\n"
            } else {
                ".\n"
            },
        );
    }
    for tip in &initialized.tips {
        let _ = writeln!(out, "Tip: {tip}");
    }
    if !initialized.skipped.is_empty() {
        out.push_str("\nLeft out:\n");
        out.push_str(&skipped_lines(&initialized.skipped));
    }
    if !initialized.large.is_empty() {
        out.push_str("\nLarge files (tracked, but they make saves slow):\n");
        out.push_str(&large_lines(&initialized.large));
    }
    if !initialized.skipped_branches.is_empty() {
        let _ = writeln!(
            out,
            "\nBranches left alone (their names can't be used as lines): {}",
            initialized.skipped_branches.join(", ")
        );
    }
    out
}

/// `latex (detected from *.tex)`, `markdown (chosen)` or `latex (from paper.toml)`.
fn profile_origin(initialized: &Initialized) -> String {
    let name = &initialized.profile;
    match initialized.profile_source {
        ProfileSource::PaperToml => return format!("{name} (from paper.toml)"),
        ProfileSource::Chosen => return format!("{name} (chosen)"),
        ProfileSource::Detected => {}
    }
    match Profile::builtin(name).map(|profile| profile.detect.join(", ")) {
        Some(patterns) if !patterns.is_empty() => format!("{name} (detected from {patterns})"),
        _ => format!("{name} (nothing more specific found)"),
    }
}

pub fn nothing_to_save() -> String {
    "Nothing to save.\n".to_owned()
}

pub fn save(
    saved: &Saved,
    short_id: &str,
    message: &str,
    verbosity: Verbosity,
    style: Style,
) -> String {
    let mut out = format!("Saved {}: {message}", style.id(short_id));
    if verbosity.verbose {
        let _ = write!(
            out,
            " {}",
            style.dim(&format!("(commit {})", short_hash(&saved.save)))
        );
    }
    out.push('\n');
    out.push_str(&change_lines(&saved.saved));
    if !saved.remaining.is_empty() {
        out.push_str("\nLeft in the draft (not saved):\n");
        out.push_str(&change_lines(&saved.remaining));
    }
    out
}

fn short_hash(commit: &paper_core::CommitId) -> String {
    commit.as_str().chars().take(7).collect()
}

pub fn history(
    entries: &[HistoryEntry],
    now: i64,
    verbosity: Verbosity,
    style: Style,
    scope: &str,
) -> String {
    if entries.is_empty() {
        return format!("No saves {scope}.\n");
    }
    let id_width = entries
        .iter()
        .map(|e| e.short_change_id.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for entry in entries {
        let id = format!("{:<id_width$}", entry.short_change_id);
        let time = format!("{:<TIME_WIDTH$}", relative_time(entry.time.seconds, now));
        let _ = write!(
            out,
            "{}  {}  {}",
            style.id(&id),
            style.dim(&time),
            entry.message
        );
        if !entry.tip_of.is_empty() {
            let lines: Vec<String> = entry.tip_of.iter().map(ToString::to_string).collect();
            let _ = write!(out, "  ({})", lines.join(", "));
        }
        if verbosity.verbose {
            let _ = write!(
                out,
                "  {}",
                style.dim(&format!("(commit {})", short_hash(&entry.commit)))
            );
        }
        out.push('\n');
    }
    out
}

/// A save: metadata, the changed files, then their diffs (or their line counts for `--stat`).
pub fn show(
    shown: &Shown,
    diffs: &[FileDiffEntry],
    stat: bool,
    now: i64,
    verbosity: Verbosity,
    style: Style,
) -> String {
    let entry = &shown.entry;
    let is_draft = entry.message.is_empty();
    let title = if is_draft {
        "(unsaved draft)"
    } else {
        entry.message.as_str()
    };
    let mut out = format!("{}  {title}\n", style.id(&entry.short_change_id));
    let _ = writeln!(
        out,
        "Author:  {} <{}>",
        entry.author.name, entry.author.email
    );
    let _ = writeln!(out, "Date:    {}", relative_time(entry.time.seconds, now));
    if !entry.tip_of.is_empty() {
        let lines: Vec<String> = entry.tip_of.iter().map(ToString::to_string).collect();
        let _ = writeln!(out, "Line:    latest save of {}", lines.join(", "));
    }
    if verbosity.verbose {
        let _ = writeln!(out, "Commit:  {}", short_hash(&entry.commit));
    }
    out.push('\n');
    if shown.changes.is_empty() {
        out.push_str("No changed files.\n");
    } else if stat {
        out.push_str(&diff::stat(diffs));
    } else {
        let _ = writeln!(out, "Changed {}:", files(shown.changes.len()));
        out.push_str(&change_lines(&shown.changes));
        if !diffs.is_empty() {
            out.push('\n');
            out.push_str(&diff::unified(diffs, style));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::size;

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(size(10), "10 B");
        assert_eq!(size(318 * 1024), "318 KB");
        assert_eq!(size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(size(2_254_857_831), "2.1 GB");
    }
}

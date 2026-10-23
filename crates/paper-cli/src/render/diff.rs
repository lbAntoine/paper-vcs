//! Human output for diffs: unified text, binary one-liners and `--stat`.

use std::fmt::Write as _;

use paper_core::project::{EntryDiff, FileDiffEntry};
use paper_core::snapshot::ChangeKind;
use paper_diff::{FileDiff, to_unified};

use super::human::size;
use super::style::Style;
use super::words;

/// The old and new sizes of a binary file: `2 KB → 4 KB`, or `(2 KB)` when one side is absent.
pub fn binary_sizes(old: Option<u64>, new: Option<u64>) -> String {
    match (old, new) {
        (Some(old), Some(new)) => format!("{} \u{2192} {}", size(old), size(new)),
        (Some(only), None) | (None, Some(only)) => format!("({})", size(only)),
        (None, None) => String::new(),
    }
}

fn binary_kind(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Added => "new",
        ChangeKind::Modified => "changed",
        ChangeKind::Deleted => "deleted",
    }
}

/// `+N −M`, with a true minus sign.
pub fn counts(added: usize, removed: usize) -> String {
    format!("+{added} \u{2212}{removed}")
}

/// The unified diff of every file, in order; a binary file is one summary line.
pub fn unified(entries: &[FileDiffEntry], style: Style) -> String {
    if entries
        .iter()
        .any(|e| matches!(e.diff, EntryDiff::Words(_)))
    {
        return word_blocks(entries, style, words::terminal_width());
    }
    let mut out = String::new();
    for entry in entries {
        let EntryDiff::Lines(diff) = &entry.diff else {
            continue;
        };
        match diff {
            FileDiff::Text(text) => {
                let (old, new) = labels(entry);
                out.push_str(&paint_unified(&to_unified(text, &old, &new), style));
            }
            FileDiff::Binary { old_size, new_size } => {
                let _ = writeln!(
                    out,
                    "{}  {}  {}",
                    entry.path,
                    binary_kind(entry.kind),
                    binary_sizes(*old_size, *new_size)
                );
            }
            FileDiff::Identical => {}
        }
    }
    out
}

/// Word diffs, one block per file with a blank line between blocks. A binary file is its
/// one-line summary, and two of those in a row stay together.
fn word_blocks(entries: &[FileDiffEntry], style: Style, width: usize) -> String {
    let mut out = String::new();
    let mut previous_binary = None;
    for entry in entries {
        let (block, binary) = match &entry.diff {
            EntryDiff::Words(diff) => (words::file(&entry.path, diff, style, width), false),
            EntryDiff::Lines(FileDiff::Binary { old_size, new_size }) => (
                format!(
                    "{}  {}  {}\n",
                    entry.path,
                    binary_kind(entry.kind),
                    binary_sizes(*old_size, *new_size)
                ),
                true,
            ),
            EntryDiff::Lines(_) => continue,
        };
        if previous_binary.is_some_and(|prev| !(prev && binary)) {
            out.push('\n');
        }
        out.push_str(&block);
        previous_binary = Some(binary);
    }
    out
}

/// `a/<path>` and `b/<path>`, or `/dev/null` for the side that does not exist.
fn labels(entry: &FileDiffEntry) -> (String, String) {
    let a = if entry.kind == ChangeKind::Added {
        "/dev/null".to_owned()
    } else {
        format!("a/{}", entry.path)
    };
    let b = if entry.kind == ChangeKind::Deleted {
        "/dev/null".to_owned()
    } else {
        format!("b/{}", entry.path)
    };
    (a, b)
}

/// Colours removed lines red and added lines green. The first two lines are the file labels;
/// after them every line starts with `@`, ` `, `+`, `-` or `\`, so the first character says
/// what the line is.
fn paint_unified(text: &str, style: Style) -> String {
    let mut out = String::new();
    for (index, line) in text.lines().enumerate() {
        let painted = match (index, line.chars().next()) {
            (0 | 1, _) => line.to_owned(),
            (_, Some('-')) => style.removed(line),
            (_, Some('+')) => style.added(line),
            _ => line.to_owned(),
        };
        out.push_str(&painted);
        out.push('\n');
    }
    out
}

/// One line per file with its counts or sizes, then a total.
pub fn stat(entries: &[FileDiffEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let width = entries
        .iter()
        .map(|e| e.path.chars().count())
        .max()
        .unwrap_or(0);
    let in_words = entries
        .iter()
        .any(|e| matches!(e.diff, EntryDiff::Words(_)));
    let (mut added, mut removed) = (0, 0);
    let mut out = String::new();
    for entry in entries {
        let detail = match &entry.diff {
            EntryDiff::Words(diff) => {
                added += diff.added;
                removed += diff.removed;
                if diff.layout_only {
                    "spacing and line breaks only".to_owned()
                } else {
                    words::counts(diff.added, diff.removed)
                }
            }
            EntryDiff::Lines(FileDiff::Text(text)) => {
                added += text.added;
                removed += text.removed;
                counts(text.added, text.removed)
            }
            EntryDiff::Lines(FileDiff::Identical) => counts(0, 0),
            EntryDiff::Lines(FileDiff::Binary { old_size, new_size }) => format!(
                "{}  {}",
                binary_kind(entry.kind),
                binary_sizes(*old_size, *new_size)
            ),
        };
        let _ = writeln!(out, "{:<width$}  {detail}", entry.path);
    }
    let total = if in_words {
        words::counts(added, removed)
    } else {
        counts(added, removed)
    };
    let _ = writeln!(
        out,
        "{} changed, {total}",
        super::human::files(entries.len()),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_show_both_sides_or_the_only_one() {
        assert_eq!(binary_sizes(Some(2048), Some(4096)), "2 KB \u{2192} 4 KB");
        assert_eq!(binary_sizes(None, Some(2048)), "(2 KB)");
        assert_eq!(binary_sizes(Some(2048), None), "(2 KB)");
    }

    #[test]
    fn only_content_lines_are_coloured_not_the_file_labels() {
        let style = Style::new(crate::cli::ColorChoice::Always, false);
        let text = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n--old\n++new\n";
        let painted = paint_unified(text, style);
        assert!(painted.starts_with("--- a/x\n+++ b/x\n@@"), "{painted}");
        assert!(painted.contains("\x1b[31m--old"), "{painted}");
        assert!(painted.contains("\x1b[32m++new"), "{painted}");
    }
}

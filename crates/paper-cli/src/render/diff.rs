//! Human output for diffs: unified text, binary one-liners and `--stat`.

use std::fmt::Write as _;

use paper_core::project::FileDiffEntry;
use paper_core::snapshot::ChangeKind;
use paper_diff::{FileDiff, to_unified};

use super::human::size;
use super::style::Style;

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
    let mut out = String::new();
    for entry in entries {
        match &entry.diff {
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
    let (mut added, mut removed) = (0, 0);
    let mut out = String::new();
    for entry in entries {
        let detail = match &entry.diff {
            FileDiff::Text(text) => {
                added += text.added;
                removed += text.removed;
                counts(text.added, text.removed)
            }
            FileDiff::Identical => counts(0, 0),
            FileDiff::Binary { old_size, new_size } => format!(
                "{}  {}",
                binary_kind(entry.kind),
                binary_sizes(*old_size, *new_size)
            ),
        };
        let _ = writeln!(out, "{:<width$}  {detail}", entry.path);
    }
    let _ = writeln!(
        out,
        "{} changed, {}",
        super::human::files(entries.len()),
        counts(added, removed)
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

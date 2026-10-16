//! Tokenisers and diff algorithms for prose, tables and office documents.
//!
//! Pure functions over byte slices: no git, no file access.

use similar::{Algorithm, ChangeTag};

/// How many leading bytes are scanned for a NUL byte when classifying content.
pub const BINARY_SNIFF_LEN: usize = 8 * 1024;

/// What kind of content a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Content<'a> {
    /// Valid UTF-8 with no NUL byte in the first 8 KiB.
    Text(&'a str),
    /// Anything else.
    Binary,
}

/// Decide whether `bytes` are text or binary (04-content.md, File classes).
#[must_use]
pub fn classify(bytes: &[u8]) -> Content<'_> {
    let head = &bytes[..bytes.len().min(BINARY_SNIFF_LEN)];
    if head.contains(&0) {
        return Content::Binary;
    }
    std::str::from_utf8(bytes).map_or(Content::Binary, Content::Text)
}

/// Options for [`diff_files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineDiffOptions {
    /// Lines of context around each change. Defaults to 3, like git.
    pub context: usize,
}

impl Default for LineDiffOptions {
    fn default() -> Self {
        Self { context: 3 }
    }
}

/// Whether a diff line is shared, added or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Present on both sides.
    Context,
    /// Only on the new side.
    Added,
    /// Only on the old side.
    Removed,
}

/// One line inside a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Context, added or removed.
    pub kind: LineKind,
    /// The line without its trailing newline.
    pub text: String,
    /// True when the line is the last of its file and has no trailing newline.
    pub missing_newline: bool,
}

/// A group of nearby changes with its context. Line numbers are 1-based;
/// an empty side has start 0 and length 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// First old line covered (0 when the old side is empty).
    pub old_start: usize,
    /// Number of old lines covered.
    pub old_len: usize,
    /// First new line covered (0 when the new side is empty).
    pub new_start: usize,
    /// Number of new lines covered.
    pub new_len: usize,
    /// The lines of the hunk, in order.
    pub lines: Vec<DiffLine>,
}

/// A line diff of two text versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDiff {
    /// The hunks, in file order.
    pub hunks: Vec<Hunk>,
    /// Total added lines.
    pub added: usize,
    /// Total removed lines.
    pub removed: usize,
}

/// The comparison of one file between two versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDiff {
    /// Both sides are equal (or both absent).
    Identical,
    /// At least one side is binary. Sizes are `None` for an absent side.
    Binary {
        /// Size of the old side in bytes.
        old_size: Option<u64>,
        /// Size of the new side in bytes.
        new_size: Option<u64>,
    },
    /// Both sides are text and differ. An absent side against an empty file
    /// counts as a difference but has no hunks.
    Text(TextDiff),
}

fn text_of(bytes: Option<&[u8]>) -> Option<&str> {
    match bytes.map(classify) {
        None => Some(""),
        Some(Content::Text(t)) => Some(t),
        Some(Content::Binary) => None,
    }
}

/// Compare two versions of a file. `None` means the file does not exist on
/// that side (added or deleted). `None` against `Some(b"")` is a
/// [`FileDiff::Text`] with no hunks; only equal inputs are `Identical`.
#[must_use]
pub fn diff_files(old: Option<&[u8]>, new: Option<&[u8]>, options: &LineDiffOptions) -> FileDiff {
    if old == new {
        return FileDiff::Identical;
    }
    let (Some(old_text), Some(new_text)) = (text_of(old), text_of(new)) else {
        return FileDiff::Binary {
            old_size: old.map(|b| b.len() as u64),
            new_size: new.map(|b| b.len() as u64),
        };
    };
    FileDiff::Text(line_diff(old_text, new_text, options.context))
}

fn line_diff(old: &str, new: &str, context: usize) -> TextDiff {
    // Split on `\n` only: a lone `\r` is not a line end (it is for `diff_lines`).
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    let diff = similar::TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .diff_slices(&old_lines, &new_lines);
    let mut added = 0;
    let mut removed = 0;
    let mut hunks = Vec::new();
    for group in diff.grouped_ops(context) {
        let mut lines = Vec::new();
        let (mut old_len, mut new_len) = (0, 0);
        let first = group
            .first()
            .map(|op| (op.old_range().start, op.new_range().start));
        for op in &group {
            for change in diff.iter_changes(op) {
                let kind = match change.tag() {
                    ChangeTag::Equal => {
                        old_len += 1;
                        new_len += 1;
                        LineKind::Context
                    }
                    ChangeTag::Delete => {
                        old_len += 1;
                        removed += 1;
                        LineKind::Removed
                    }
                    ChangeTag::Insert => {
                        new_len += 1;
                        added += 1;
                        LineKind::Added
                    }
                };
                let value: &str = change.value();
                lines.push(DiffLine {
                    kind,
                    text: value.strip_suffix('\n').unwrap_or(value).to_owned(),
                    missing_newline: !value.ends_with('\n'),
                });
            }
        }
        let (old_idx, new_idx) = first.unwrap_or((0, 0));
        hunks.push(Hunk {
            old_start: if old_len == 0 { old_idx } else { old_idx + 1 },
            old_len,
            new_start: if new_len == 0 { new_idx } else { new_idx + 1 },
            new_len,
            lines,
        });
    }
    TextDiff {
        hunks,
        added,
        removed,
    }
}

/// Render a text diff in git's unified format. Line text is written as is:
/// control characters (including `\r`) are not escaped.
#[must_use]
pub fn to_unified(diff: &TextDiff, old_label: &str, new_label: &str) -> String {
    use std::fmt::Write as _;

    fn range(start: usize, len: usize) -> String {
        if len == 1 {
            start.to_string()
        } else {
            format!("{start},{len}")
        }
    }

    let mut out = format!("--- {old_label}\n+++ {new_label}\n");
    for h in &diff.hunks {
        let _ = writeln!(
            out,
            "@@ -{} +{} @@",
            range(h.old_start, h.old_len),
            range(h.new_start, h.new_len),
        );
        for line in &h.lines {
            out.push(match line.kind {
                LineKind::Context => ' ',
                LineKind::Added => '+',
                LineKind::Removed => '-',
            });
            out.push_str(&line.text);
            out.push('\n');
            if line.missing_newline {
                out.push_str("\\ No newline at end of file\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::LineKind::{Added as A, Context as C, Removed as R};
    use super::*;

    fn opts() -> LineDiffOptions {
        LineDiffOptions::default()
    }

    fn numbered(n: usize) -> String {
        let mut s = String::new();
        for i in 1..=n {
            s.push_str("line ");
            s.push_str(&i.to_string());
            s.push('\n');
        }
        s
    }

    fn text_diff(old: Option<&str>, new: Option<&str>) -> TextDiff {
        match diff_files(old.map(str::as_bytes), new.map(str::as_bytes), &opts()) {
            FileDiff::Text(t) => t,
            other => panic!("expected text diff, got {other:?}"),
        }
    }

    #[test]
    fn default_context_is_three() {
        assert_eq!(LineDiffOptions::default().context, 3);
    }

    #[test]
    fn utf8_text_is_text() {
        assert_eq!(classify("héllo\n".as_bytes()), Content::Text("héllo\n"));
        assert_eq!(classify(b""), Content::Text(""));
    }

    #[test]
    fn a_nul_in_the_first_8_kib_is_binary() {
        assert_eq!(classify(b"ab\0cd"), Content::Binary);
        let mut bytes = vec![b'a'; BINARY_SNIFF_LEN - 1];
        bytes.push(0);
        assert_eq!(classify(&bytes), Content::Binary);
    }

    #[test]
    fn invalid_utf8_is_binary() {
        assert_eq!(classify(&[0xff, 0xfe, b'a']), Content::Binary);
    }

    #[test]
    fn a_nul_after_8_kib_in_valid_utf8_is_still_text() {
        let mut bytes = vec![b'a'; BINARY_SNIFF_LEN];
        bytes.push(0);
        assert!(matches!(classify(&bytes), Content::Text(_)));
    }

    #[test]
    fn identical_inputs_are_identical() {
        assert_eq!(diff_files(None, None, &opts()), FileDiff::Identical);
        assert_eq!(
            diff_files(Some(b""), Some(b""), &opts()),
            FileDiff::Identical
        );
        assert_eq!(
            diff_files(Some(b"a\nb\n"), Some(b"a\nb\n"), &opts()),
            FileDiff::Identical
        );
    }

    #[test]
    fn an_added_file_is_one_hunk_with_an_empty_old_side() {
        let d = text_diff(None, Some("a\nb\nc\n"));
        assert_eq!((d.added, d.removed), (3, 0));
        assert_eq!(d.hunks.len(), 1);
        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (0, 0, 1, 3)
        );
        assert!(h.lines.iter().all(|l| l.kind == LineKind::Added));
        assert_eq!(h.lines[0].text, "a");
    }

    #[test]
    fn a_deleted_file_is_one_hunk_with_an_empty_new_side() {
        let d = text_diff(Some("a\nb\n"), None);
        assert_eq!((d.added, d.removed), (0, 2));
        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (1, 2, 0, 0)
        );
        assert!(h.lines.iter().all(|l| l.kind == LineKind::Removed));
    }

    #[test]
    fn a_middle_edit_has_three_lines_of_context_each_side() {
        let old = numbered(20);
        let new = old.replace("line 10\n", "changed\n");
        let d = text_diff(Some(&old), Some(&new));
        assert_eq!(d.hunks.len(), 1);
        let h = &d.hunks[0];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (7, 7, 7, 7)
        );
        let kinds: Vec<_> = h.lines.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, vec![C, C, C, R, A, C, C, C]);
        assert_eq!((d.added, d.removed), (1, 1));
    }

    #[test]
    fn context_option_widens_the_hunk() {
        let old = numbered(20);
        let new = old.replace("line 10\n", "changed\n");
        let o = LineDiffOptions { context: 1 };
        let FileDiff::Text(d) = diff_files(Some(old.as_bytes()), Some(new.as_bytes()), &o) else {
            panic!("expected text");
        };
        assert_eq!(d.hunks[0].old_len, 3);
    }

    #[test]
    fn edits_further_apart_than_twice_the_context_make_two_hunks() {
        let old = numbered(30);
        let new = old
            .replace("line 5\n", "five\n")
            .replace("line 20\n", "twenty\n");
        let d = text_diff(Some(&old), Some(&new));
        assert_eq!(d.hunks.len(), 2);
        assert_eq!((d.added, d.removed), (2, 2));
    }

    #[test]
    fn edits_closer_than_twice_the_context_merge_into_one_hunk() {
        let old = numbered(30);
        // lines 10 and 15 have 4 unchanged lines between them (<= 2 * 3).
        let new = old
            .replace("line 10\n", "ten\n")
            .replace("line 15\n", "fifteen\n");
        let d = text_diff(Some(&old), Some(&new));
        assert_eq!(d.hunks.len(), 1);
        assert_eq!((d.added, d.removed), (2, 2));
    }

    #[test]
    fn a_missing_final_newline_on_the_new_side_is_reported() {
        let d = text_diff(Some("a\nb\n"), Some("a\nb"));
        let h = &d.hunks[0];
        let removed = h
            .lines
            .iter()
            .find(|l| l.kind == LineKind::Removed)
            .unwrap();
        let added = h.lines.iter().find(|l| l.kind == LineKind::Added).unwrap();
        assert!(!removed.missing_newline);
        assert!(added.missing_newline);
        assert!(to_unified(&d, "a/f", "b/f").contains("\\ No newline at end of file\n"));
    }

    #[test]
    fn a_missing_final_newline_on_the_old_side_is_reported() {
        let d = text_diff(Some("a\nb"), Some("a\nb\n"));
        let h = &d.hunks[0];
        let removed = h
            .lines
            .iter()
            .find(|l| l.kind == LineKind::Removed)
            .unwrap();
        assert!(removed.missing_newline);
    }

    #[test]
    fn either_side_binary_gives_binary_with_both_sizes() {
        assert_eq!(
            diff_files(Some(b"a\0b"), Some(b"text\n"), &opts()),
            FileDiff::Binary {
                old_size: Some(3),
                new_size: Some(5)
            }
        );
        assert_eq!(
            diff_files(None, Some(&[0xff, 0xfe]), &opts()),
            FileDiff::Binary {
                old_size: None,
                new_size: Some(2)
            }
        );
        assert_eq!(
            diff_files(Some(&[0, 1, 2, 3]), None, &opts()),
            FileDiff::Binary {
                old_size: Some(4),
                new_size: None
            }
        );
    }

    #[test]
    fn unified_output_has_labels_and_hunk_header() {
        let d = text_diff(Some("a\nb\nc\n"), Some("a\nB\nc\n"));
        assert_eq!(
            to_unified(&d, "a/f.txt", "b/f.txt"),
            "--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n"
        );
    }

    #[test]
    fn an_added_file_renders_with_dev_null_as_old_label() {
        let d = text_diff(None, Some("a\n"));
        assert_eq!(
            to_unified(&d, "/dev/null", "b/f.txt"),
            "--- /dev/null\n+++ b/f.txt\n@@ -0,0 +1 @@\n+a\n"
        );
    }

    #[test]
    fn a_lone_cr_is_not_a_line_break() {
        let d = text_diff(Some("a\rb\n"), Some("a\rc\n"));
        let h = &d.hunks[0];
        assert_eq!((h.old_len, h.new_len), (1, 1));
        assert_eq!(h.lines[0].text, "a\rb");
        assert!(!h.lines[0].missing_newline);
    }

    #[test]
    fn absent_against_empty_is_a_text_diff_without_hunks() {
        for (old, new) in [(None, Some(&b""[..])), (Some(&b""[..]), None)] {
            let FileDiff::Text(d) = diff_files(old, new, &opts()) else {
                panic!("expected text");
            };
            assert!(d.hunks.is_empty());
            assert_eq!((d.added, d.removed), (0, 0));
        }
    }

    #[test]
    fn invalid_utf8_after_8_kib_is_binary() {
        let mut bytes = vec![b'a'; BINARY_SNIFF_LEN];
        bytes.push(0xff);
        assert_eq!(classify(&bytes), Content::Binary);
    }
}

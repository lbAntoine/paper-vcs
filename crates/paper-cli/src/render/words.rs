//! Human output for word diffs: a header with counts, then each changed paragraph wrapped to
//! the terminal width, in colour or in the plain `[-…-]{+…+}` markup (04-content.md, Word diff).

use std::fmt::Write as _;

use paper_diff::{Paragraph, Segment, Tokeniser, WordDiff, tokeniser_for};
use unicode_width::UnicodeWidthStr;

use super::style::Style;

/// The width used when stdout is not a terminal.
pub const DEFAULT_WIDTH: usize = 80;

/// The narrowest width honoured.
const MIN_WIDTH: usize = 20;

/// The text width: `COLUMNS` when it is a number, else the size of the terminal on stdout,
/// else [`DEFAULT_WIDTH`]. Only stdout counts, so `paper diff > file` and `| less` wrap at 80.
pub fn terminal_width() -> usize {
    let columns = std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|width| *width > 0);
    columns
        .or_else(|| {
            terminal_size::terminal_size_of(std::io::stdout())
                .map(|(width, _)| usize::from(width.0))
        })
        .map_or(DEFAULT_WIDTH, |width| width.max(MIN_WIDTH))
}

/// Text is indented by this many columns under its `¶` line.
const INDENT: usize = 2;

/// `1 word` or `N words`.
pub fn words(count: usize) -> String {
    if count == 1 {
        "1 word".to_owned()
    } else {
        format!("{count} words")
    }
}

/// `+N words −M words`, with a true minus sign.
pub fn counts(added: usize, removed: usize) -> String {
    format!("+{} \u{2212}{}", words(added), words(removed))
}

/// The header and changed paragraphs of one file.
pub fn file(path: &str, diff: &WordDiff, style: Style, width: usize) -> String {
    if diff.layout_only {
        return format!("{path}  spacing and line breaks only\n");
    }
    let latex = tokeniser_for(path) == Tokeniser::Latex;
    let mut out = format!("{path}  {}\n", counts(diff.added, diff.removed));
    let mut last = None;
    for paragraph in &diff.paragraphs {
        // Paragraphs that share a number (a deleted one next to its neighbour) share a header.
        if last != Some(paragraph.number) {
            let _ = writeln!(out, "  \u{b6} {}", paragraph.number);
        }
        last = Some(paragraph.number);
        out.push_str(&paragraph_text(paragraph, latex, style, width));
    }
    out
}

fn paragraph_text(paragraph: &Paragraph, latex: bool, style: Style, width: usize) -> String {
    let mut out = String::new();
    let items = items(&paragraph.segments, latex);
    let available = width.saturating_sub(INDENT).max(1);
    for line in wrap(&items, available, style) {
        let _ = writeln!(out, "{:INDENT$}{line}", "");
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Same,
    Removed,
    Added,
    Cut,
}

impl Kind {
    fn paint(self, style: Style, text: &str) -> String {
        match self {
            Kind::Same => text.to_owned(),
            Kind::Removed => style.removed_words(text),
            Kind::Added => style.added(text),
            Kind::Cut => style.dim(text),
        }
    }
}

/// A run of text that is never split, with the marker delimiters it carries in plain output.
struct Piece {
    kind: Kind,
    text: String,
    open: bool,
    close: bool,
}

impl Piece {
    fn plain(&self) -> String {
        let (open, close) = match self.kind {
            Kind::Removed => ("[-", "-]"),
            Kind::Added => ("{+", "+}"),
            Kind::Same | Kind::Cut => ("", ""),
        };
        format!(
            "{}{}{}",
            if self.open { open } else { "" },
            self.text,
            if self.close { close } else { "" }
        )
    }

    fn render(&self, style: Style) -> String {
        if style.enabled() {
            self.kind.paint(style, &self.text)
        } else {
            self.plain()
        }
    }

    fn width(&self, style: Style) -> usize {
        if style.enabled() {
            self.text.width()
        } else {
            self.plain().width()
        }
    }
}

enum Item {
    /// Pieces that touch each other: no line break inside.
    Word(Vec<Piece>),
    /// White space: a place to break, or one space.
    Space,
    /// A place to break that is nothing when the line goes on (around a paragraph marker and
    /// between CJK characters).
    Joint,
    /// A line break that stays (after a LaTeX comment, which runs to the end of its line).
    HardBreak,
}

/// Cuts the segments into words and break opportunities.
struct Builder {
    out: Vec<Item>,
    word: Vec<Piece>,
    latex: bool,
    in_comment: bool,
}

fn items(segments: &[Segment], latex: bool) -> Vec<Item> {
    let mut b = Builder {
        out: Vec::new(),
        word: Vec::new(),
        latex,
        in_comment: false,
    };
    for segment in segments {
        match segment {
            Segment::Cut => {
                b.flush();
                b.out.push(Item::Word(vec![Piece {
                    kind: Kind::Cut,
                    text: " \u{2026} ".to_owned(),
                    open: false,
                    close: false,
                }]));
            }
            Segment::Same(text) => b.split(Kind::Same, text),
            Segment::Removed(_) | Segment::Added(_) if segment.is_break() => {
                b.flush();
                b.out.push(Item::Joint);
                let kind = if matches!(segment, Segment::Added(_)) {
                    Kind::Added
                } else {
                    Kind::Removed
                };
                b.out.push(Item::Word(vec![Piece {
                    kind,
                    text: "\u{b6}".to_owned(),
                    open: true,
                    close: true,
                }]));
                b.out.push(Item::Joint);
            }
            Segment::Removed(text) => b.split(Kind::Removed, text),
            Segment::Added(text) => b.split(Kind::Added, text),
        }
    }
    b.flush();
    b.out
}

impl Builder {
    fn flush(&mut self) {
        if !self.word.is_empty() {
            self.out.push(Item::Word(std::mem::take(&mut self.word)));
        }
    }

    fn push_space(&mut self) {
        if matches!(self.out.last(), Some(Item::Word(_) | Item::Joint)) {
            self.out.push(Item::Space);
        }
    }

    /// Splits one segment's text at white space. White space inside a marker pair is a place
    /// to break; white space at its edge stays next to the delimiter so the delimiter never
    /// starts or ends a line. A line break that ends a LaTeX comment stays a line break.
    fn split(&mut self, kind: Kind, text: &str) {
        let runs = runs(text);
        let marked = kind != Kind::Same;
        let last = runs.len().saturating_sub(1);
        for (index, (is_space, run)) in runs.into_iter().enumerate() {
            let (open, close) = (marked && index == 0, marked && index == last);
            if is_space && !open && !close {
                self.flush();
                if self.in_comment && run.contains('\n') {
                    self.in_comment = false;
                    self.out.push(Item::HardBreak);
                } else {
                    self.push_space();
                }
            } else if is_space {
                self.word.push(Piece {
                    kind,
                    text: " ".to_owned(),
                    open,
                    close,
                });
            } else {
                if self.latex && !self.in_comment {
                    self.in_comment = starts_comment(&run);
                }
                let parts = cjk_parts(&run);
                let count = parts.len();
                for (k, part) in parts.into_iter().enumerate() {
                    if k > 0 {
                        self.flush();
                        self.out.push(Item::Joint);
                    }
                    self.word.push(Piece {
                        kind,
                        text: part,
                        open: open && k == 0,
                        close: close && k + 1 == count,
                    });
                }
            }
        }
    }
}

/// True when the text holds a `%` that is not escaped (`\%`).
fn starts_comment(text: &str) -> bool {
    let mut backslashes = 0;
    for c in text.chars() {
        if c == '%' && backslashes % 2 == 0 {
            return true;
        }
        backslashes = if c == '\\' { backslashes + 1 } else { 0 };
    }
    false
}

/// True for the scripts whose lines may break between any two characters.
fn is_cjk(c: char) -> bool {
    matches!(u32::from(c),
        0x1100..=0x11FF | 0x2E80..=0xA4CF | 0xA960..=0xA97F | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF | 0xFF66..=0xFF9F | 0x20000..=0x3FFFF)
}

/// Cuts a run of text between two CJK characters.
fn cjk_parts(run: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut previous: Option<char> = None;
    for c in run.chars() {
        match (previous, parts.last_mut()) {
            (Some(p), Some(part)) if !(is_cjk(p) && is_cjk(c)) => part.push(c),
            _ => parts.push(c.to_string()),
        }
        previous = Some(c);
    }
    parts
}

/// Alternating runs of white space and other text.
fn runs(text: &str) -> Vec<(bool, String)> {
    let mut runs: Vec<(bool, String)> = Vec::new();
    for c in text.chars() {
        let space = c.is_whitespace();
        match runs.last_mut() {
            Some((is_space, run)) if *is_space == space => run.push(c),
            _ => runs.push((space, c.to_string())),
        }
    }
    runs
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Nothing,
    Joint,
    Space,
}

/// The line being filled.
#[derive(Default)]
struct Line {
    parts: Vec<String>,
    used: usize,
    /// The kind of the last piece, to paint the space between two pieces of one run.
    last: Option<Kind>,
    /// Index of the part that is a cut, and the space before it, while it is the last part.
    cut: Option<(usize, &'static str)>,
}

impl Line {
    /// The text of the line. A cut at the end loses its trailing space.
    fn finish(mut self, style: Style) -> String {
        if let Some((index, lead)) = self.cut
            && index + 1 == self.parts.len()
        {
            self.parts[index] = style.dim(&format!("{lead}\u{2026}"));
        }
        self.parts.concat()
    }
}

/// Greedy line filling. A word wider than the line gets a line of its own.
fn wrap(items: &[Item], available: usize, style: Style) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = Line::default();
    let mut pending = Pending::Nothing;
    let mut after_cut = false;
    for (index, item) in items.iter().enumerate() {
        match item {
            Item::Space => {
                if line.used > 0 && !after_cut {
                    pending = Pending::Space;
                }
            }
            Item::Joint => {
                if pending == Pending::Nothing {
                    pending = Pending::Joint;
                }
            }
            Item::HardBreak => {
                if line.used > 0 {
                    lines.push(std::mem::take(&mut line).finish(style));
                }
                pending = Pending::Nothing;
                after_cut = false;
            }
            Item::Word(pieces) => {
                let first = pieces.first().map(|p| p.kind);
                let is_cut = first == Some(Kind::Cut);
                let mut width: usize = pieces.iter().map(|p| p.width(style)).sum();
                let separator = usize::from(pending == Pending::Space && !is_cut && !after_cut);
                if line.used > 0 && line.used + separator + width > available {
                    lines.push(std::mem::take(&mut line).finish(style));
                }
                if is_cut {
                    // " … " carries its own spaces; trim the edges of a line.
                    let lead = if line.used == 0 { "" } else { " " };
                    let trail = if index + 1 == items.len() { "" } else { " " };
                    let bare = format!("{lead}\u{2026}{trail}");
                    width = bare.width();
                    line.cut = Some((line.parts.len(), if lead.is_empty() { "" } else { " " }));
                    line.parts.push(style.dim(&bare));
                } else {
                    if line.used > 0 && separator == 1 {
                        // Inside one removed or added run the space is part of the run.
                        let same_run = style.enabled()
                            && line.last == first
                            && matches!(first, Some(Kind::Removed | Kind::Added));
                        line.parts.push(match (same_run, first) {
                            (true, Some(kind)) => kind.paint(style, " "),
                            _ => " ".to_owned(),
                        });
                        line.used += 1;
                    }
                    line.parts.extend(pieces.iter().map(|p| p.render(style)));
                }
                line.used += width;
                line.last = pieces.last().map(|p| p.kind);
                pending = Pending::Nothing;
                after_cut = is_cut;
            }
        }
    }
    if line.used > 0 {
        lines.push(line.finish(style));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ColorChoice;
    use paper_diff::{Tokeniser, diff_words, to_word_markup};

    fn plain() -> Style {
        Style::new(ColorChoice::Never, false)
    }

    #[test]
    fn counts_use_the_singular_and_a_true_minus() {
        assert_eq!(counts(1, 0), "+1 word \u{2212}0 words");
        assert_eq!(counts(2, 1), "+2 words \u{2212}1 word");
    }

    #[test]
    fn unwrapped_text_matches_the_plain_markup() {
        let diff = diff_words(
            "Intro text here v1 and more.",
            "Intro text here v2 and more.",
            Tokeniser::Generic,
        );
        let markup = to_word_markup(&diff);
        let rendered = file("a.txt", &diff, plain(), 80);
        let body: Vec<&str> = markup.lines().collect();
        assert_eq!(body[0], "\u{b6} 1");
        assert!(rendered.contains(&format!("  {}\n", body[1])), "{rendered}");
    }

    #[test]
    fn markers_never_start_or_end_a_line() {
        let diff = diff_words(
            "alpha beta gamma delta epsilon zeta",
            "alpha beta GAMMA DELTA epsilon zeta",
            Tokeniser::Generic,
        );
        let rendered = file("a.txt", &diff, plain(), 14);
        for line in rendered.lines().skip(2) {
            assert!(!line.trim_end().ends_with("[-"), "{rendered}");
            assert!(!line.trim_start().starts_with("-]"), "{rendered}");
        }
    }

    #[test]
    fn paragraphs_with_one_number_share_a_header() {
        let diff = WordDiff {
            paragraphs: vec![
                Paragraph {
                    number: 3,
                    segments: vec![Segment::Removed("gone".to_owned())],
                },
                Paragraph {
                    number: 3,
                    segments: vec![Segment::Added("new".to_owned())],
                },
            ],
            added: 1,
            removed: 1,
            layout_only: false,
        };
        let rendered = file("a.txt", &diff, plain(), 80);
        assert_eq!(rendered.matches('\u{b6}').count(), 1, "{rendered}");
        assert!(rendered.contains("[-gone-]") && rendered.contains("{+new+}"));
    }
}

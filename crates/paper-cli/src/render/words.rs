//! Human output for word diffs: a header with counts, then each changed paragraph wrapped to
//! the terminal width, in colour or in the plain `[-…-]{+…+}` markup (04-content.md, Word diff).

use std::fmt::Write as _;

use paper_diff::{Paragraph, Segment, WordDiff};

use super::style::Style;

/// The width used when stdout is not a terminal.
pub const DEFAULT_WIDTH: usize = 80;

/// The width of the terminal on stdout, or [`DEFAULT_WIDTH`] when it is not one.
pub fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(width, _)| usize::from(width.0))
        .filter(|width| *width >= 20)
        .unwrap_or(DEFAULT_WIDTH)
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
    let mut out = format!("{path}  {}\n", counts(diff.added, diff.removed));
    for paragraph in &diff.paragraphs {
        out.push_str(&paragraph_text(paragraph, style, width));
    }
    out
}

fn paragraph_text(paragraph: &Paragraph, style: Style, width: usize) -> String {
    let mut out = format!("  \u{b6} {}\n", paragraph.number);
    let items = items(&paragraph.segments);
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
        if !style.enabled() {
            return self.plain();
        }
        match self.kind {
            Kind::Same => self.text.clone(),
            Kind::Removed => style.removed_words(&self.text),
            Kind::Added => style.added(&self.text),
            Kind::Cut => style.dim(&self.text),
        }
    }

    fn width(&self, style: Style) -> usize {
        if style.enabled() {
            self.text.chars().count()
        } else {
            self.plain().chars().count()
        }
    }
}

enum Item {
    /// Pieces that touch each other: no line break inside.
    Word(Vec<Piece>),
    /// White space: a place to break, or one space.
    Space,
    /// A place to break that is nothing when the line goes on (around a paragraph marker).
    Joint,
}

/// Cuts the segments into words and break opportunities.
fn items(segments: &[Segment]) -> Vec<Item> {
    let mut out = Vec::new();
    let mut word: Vec<Piece> = Vec::new();
    for segment in segments {
        match segment {
            Segment::Cut => {
                flush(&mut out, &mut word);
                out.push(Item::Word(vec![Piece {
                    kind: Kind::Cut,
                    text: " \u{2026} ".to_owned(),
                    open: false,
                    close: false,
                }]));
            }
            Segment::Same(text) => split(Kind::Same, text, &mut out, &mut word),
            Segment::Removed(_) | Segment::Added(_) if segment.is_break() => {
                flush(&mut out, &mut word);
                out.push(Item::Joint);
                let kind = if matches!(segment, Segment::Added(_)) {
                    Kind::Added
                } else {
                    Kind::Removed
                };
                out.push(Item::Word(vec![Piece {
                    kind,
                    text: "\u{b6}".to_owned(),
                    open: true,
                    close: true,
                }]));
                out.push(Item::Joint);
            }
            Segment::Removed(text) => split(Kind::Removed, text, &mut out, &mut word),
            Segment::Added(text) => split(Kind::Added, text, &mut out, &mut word),
        }
    }
    flush(&mut out, &mut word);
    out
}

fn flush(out: &mut Vec<Item>, word: &mut Vec<Piece>) {
    if !word.is_empty() {
        out.push(Item::Word(std::mem::take(word)));
    }
}

fn push_space(out: &mut Vec<Item>) {
    if matches!(out.last(), Some(Item::Word(_) | Item::Joint)) {
        out.push(Item::Space);
    }
}

/// Splits one segment's text at white space. White space inside a marker pair is a place to
/// break; white space at its edge stays next to the delimiter so the delimiter never starts or
/// ends a line.
fn split(kind: Kind, text: &str, out: &mut Vec<Item>, word: &mut Vec<Piece>) {
    let runs = runs(text);
    let marked = kind != Kind::Same;
    let last = runs.len().saturating_sub(1);
    for (index, (is_space, run)) in runs.into_iter().enumerate() {
        let (open, close) = (marked && index == 0, marked && index == last);
        if is_space && !open && !close {
            flush(out, word);
            push_space(out);
        } else {
            word.push(Piece {
                kind,
                text: if is_space { " ".to_owned() } else { run },
                open,
                close,
            });
        }
    }
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

/// Greedy line filling. A word wider than the line gets a line of its own.
fn wrap(items: &[Item], available: usize, style: Style) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    let mut pending = Pending::Nothing;
    let mut after_cut = false;
    for (index, item) in items.iter().enumerate() {
        match item {
            Item::Space => {
                if used > 0 && !after_cut {
                    pending = Pending::Space;
                }
            }
            Item::Joint => {
                if pending == Pending::Nothing {
                    pending = Pending::Joint;
                }
            }
            Item::Word(pieces) => {
                let is_cut = pieces.first().is_some_and(|p| p.kind == Kind::Cut);
                let mut text: Vec<String> = pieces.iter().map(|p| p.render(style)).collect();
                let mut width: usize = pieces.iter().map(|p| p.width(style)).sum();
                let separator = usize::from(pending == Pending::Space && !is_cut && !after_cut);
                if used > 0 && used + separator + width > available {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                if is_cut {
                    // " … " carries its own spaces; trim the edges of a line.
                    let lead = if used == 0 { "" } else { " " };
                    let trail = if index + 1 == items.len() { "" } else { " " };
                    let bare = format!("{lead}\u{2026}{trail}");
                    width = bare.chars().count();
                    text = vec![style.dim(&bare)];
                } else if used > 0 && separator == 1 {
                    line.push(' ');
                    used += 1;
                }
                for part in &text {
                    line.push_str(part);
                }
                used += width;
                pending = Pending::Nothing;
                after_cut = is_cut;
            }
        }
    }
    if used > 0 {
        lines.push(line);
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
}

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
    /// Whether the old and the new text are inside a `%` comment at this point.
    comment: [bool; 2],
}

fn items(segments: &[Segment], latex: bool) -> Vec<Item> {
    let mut b = Builder {
        out: Vec::new(),
        word: Vec::new(),
        latex,
        comment: [false; 2],
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
                // The cut may hide the end of a comment: the text after it starts a line.
                b.end_comment_line([true, true]);
            }
            Segment::Same(text) => b.split(Kind::Same, text),
            Segment::Removed(_) | Segment::Added(_) if segment.is_break() => {
                b.flush();
                b.out.push(Item::Joint);
                let (kind, sides) = if matches!(segment, Segment::Added(_)) {
                    (Kind::Added, [false, true])
                } else {
                    (Kind::Removed, [true, false])
                };
                b.out.push(Item::Word(vec![Piece {
                    kind,
                    text: "\u{b6}".to_owned(),
                    open: true,
                    close: true,
                }]));
                b.out.push(Item::Joint);
                // A paragraph break ends a comment.
                b.end_comment_line(sides);
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

    /// If a comment is open on one of the `sides` (old, new), it ends here: the text that
    /// follows starts a new line.
    fn end_comment_line(&mut self, sides: [bool; 2]) {
        if self.close_comments(sides) {
            self.flush();
            self.out.push(Item::HardBreak);
        }
    }

    /// Ends the comments open on the `sides`; true if there was one.
    fn close_comments(&mut self, sides: [bool; 2]) -> bool {
        let mut was_open = false;
        for (open, side) in self.comment.iter_mut().zip(sides) {
            was_open |= side && *open;
            *open &= !side;
        }
        was_open
    }

    /// The texts (old, new) a segment of this kind belongs to.
    fn sides(kind: Kind) -> [bool; 2] {
        [kind != Kind::Added, kind != Kind::Removed]
    }

    fn push_space(&mut self) {
        if matches!(self.out.last(), Some(Item::Word(_) | Item::Joint)) {
            self.out.push(Item::Space);
        }
    }

    /// Splits one segment's text into words and break opportunities.
    ///
    /// White space inside a marker pair is a place to break; white space at the edge of a
    /// marker pair stays next to its delimiter so the delimiter never starts or ends a line.
    /// In a LaTeX file the first line break after a `%` comment, wherever it falls, stays a
    /// line break: a marker open at that point closes before it and opens again after it.
    fn split(&mut self, kind: Kind, text: &str) {
        let elements = self.elements(kind, text);
        let marked = kind != Kind::Same;
        let mut start = 0;
        for at in 0..=elements.len() {
            if at < elements.len() && elements[at] != El::Break {
                continue;
            }
            self.span(kind, marked, &elements[start..at]);
            if at < elements.len() {
                self.flush();
                self.out.push(Item::HardBreak);
            }
            start = at + 1;
        }
    }

    /// The text as words, spaces, break points and comment-ending line breaks.
    fn elements(&mut self, kind: Kind, text: &str) -> Vec<El> {
        let mut out = Vec::new();
        for (is_space, run) in runs(text) {
            if is_space {
                if run.contains('\n') && self.close_comments(Self::sides(kind)) {
                    out.push(El::Break);
                } else {
                    out.push(El::Space);
                }
                continue;
            }
            if self.latex && starts_comment(&run) {
                for (open, side) in self.comment.iter_mut().zip(Self::sides(kind)) {
                    *open |= side;
                }
            }
            for (k, part) in cjk_parts(&run).into_iter().enumerate() {
                if k > 0 {
                    out.push(El::Joint);
                }
                out.push(El::Text(part));
            }
        }
        out
    }

    /// One stretch of a segment between line breaks, with its own marker pair.
    fn span(&mut self, kind: Kind, marked: bool, elements: &[El]) {
        let last = elements.len().saturating_sub(1);
        for (k, element) in elements.iter().enumerate() {
            let (open, close) = (marked && k == 0, marked && k == last);
            match element {
                El::Text(part) => {
                    // An edit may break away from the text next to it, Hangul included.
                    let before = self.word.last().and_then(|p| p.text.chars().next_back());
                    let after = part.chars().next();
                    if k == 0
                        && before
                            .zip(after)
                            .is_some_and(|(b, a)| is_cjk(b) && is_cjk(a))
                    {
                        self.flush();
                        self.out.push(Item::Joint);
                    }
                    self.word.push(Piece {
                        kind,
                        text: part.clone(),
                        open,
                        close,
                    });
                }
                El::Joint => {
                    self.flush();
                    self.out.push(Item::Joint);
                }
                El::Space if open || close => {
                    if open {
                        // The line may break before the opening delimiter.
                        self.flush();
                        self.out.push(Item::Joint);
                    }
                    self.word.push(Piece {
                        kind,
                        text: " ".to_owned(),
                        open,
                        close,
                    });
                    if close {
                        // The line may break after the closing delimiter.
                        self.flush();
                        self.out.push(Item::Joint);
                    }
                }
                El::Space => {
                    self.flush();
                    self.push_space();
                }
                El::Break => {}
            }
        }
    }
}

#[derive(PartialEq, Eq)]
enum El {
    Text(String),
    Space,
    Joint,
    Break,
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

/// True for the scripts whose lines may break between any two characters at the edge of an
/// edit: Han, kana and Hangul.
fn is_cjk(c: char) -> bool {
    matches!(u32::from(c),
        0x1100..=0x11FF | 0x2E80..=0xA4CF | 0xA960..=0xA97F | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF | 0xFF66..=0xFF9F | 0x20000..=0x3FFFF)
}

/// Hangul is written with spaces between words, so it is not cut inside a run.
fn is_hangul(c: char) -> bool {
    matches!(u32::from(c), 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF)
}

/// True when a line may break between two characters of one run: both Han or kana.
fn breaks_between(a: char, b: char) -> bool {
    [a, b].iter().all(|c| is_cjk(*c) && !is_hangul(*c))
}

/// Cuts a run of text between two Han or kana characters.
fn cjk_parts(run: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut previous: Option<char> = None;
    for c in run.chars() {
        match (previous, parts.last_mut()) {
            (Some(p), Some(part)) if !breaks_between(p, c) => {
                part.push(c);
            }
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

    // ------------------------------------------------ comments (property test)

    use paper_diff::{WordDiffOptions, diff_words_with};
    use proptest::prelude::*;

    /// A source line with unique words `l<id>w<n>`.
    #[derive(Clone, Debug)]
    struct Src {
        id: usize,
        kind: u8,
        words: usize,
    }

    impl Src {
        fn text(&self) -> String {
            let w: Vec<String> = (0..self.words)
                .map(|n| format!("l{}w{n}", self.id))
                .collect();
            match self.kind {
                0 => w.join(" "),
                1 => format!("% {}", w.join(" ")),
                2 => match w.split_first() {
                    Some((first, rest)) => format!("{first} % {}", rest.join(" ")),
                    None => String::new(),
                },
                3 => w.join(" 50\\% "),
                // An escaped backslash, then a real comment.
                5 => match w.split_first() {
                    Some((first, rest)) => format!("{first} \\\\% {}", rest.join(" ")),
                    None => String::new(),
                },
                // A comment glued to the word before it.
                6 => match w.split_first() {
                    Some((first, rest)) => format!("{first}%{}", rest.join(" ")),
                    None => String::new(),
                },
                _ => String::new(),
            }
        }
    }

    fn document(lines: &[Src], eol: &str, final_newline: bool) -> String {
        let mut text: String = lines.iter().map(|l| l.text() + eol).collect();
        if !final_newline {
            text.truncate(text.trim_end_matches(['\r', '\n']).len());
        }
        text
    }

    /// The logical line ids of the `l<id>w<n>` tokens in `text`, in order.
    fn ids(text: &str) -> Vec<usize> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter_map(|t| {
                let (a, b) = t.strip_prefix('l')?.split_once('w')?;
                b.parse::<usize>().ok()?;
                a.parse().ok()
            })
            .collect()
    }

    /// Splits a rendered line into its old-side and new-side text.
    fn sides_of(line: &str) -> (String, String) {
        let (mut old, mut new) = (String::new(), String::new());
        let mut rest = line;
        let mut mode = 'S';
        while !rest.is_empty() {
            if let Some(r) = rest.strip_prefix("[-") {
                mode = 'R';
                rest = r;
            } else if let Some(r) = rest.strip_prefix("{+") {
                mode = 'A';
                rest = r;
            } else if mode == 'R' && rest.starts_with("-]") || mode == 'A' && rest.starts_with("+}")
            {
                mode = 'S';
                rest = &rest[2..];
            } else {
                let c = rest.chars().next().unwrap();
                if mode != 'A' {
                    old.push(c);
                }
                if mode != 'R' {
                    new.push(c);
                }
                rest = &rest[c.len_utf8()..];
            }
        }
        (old, new)
    }

    /// No word after a `%` comes from another line than the comment itself.
    fn comment_is_contained(text: &str) -> Result<(), String> {
        let mut parts: Vec<&str> = Vec::new();
        let mut start = 0;
        let mut backslashes = 0;
        for (at, c) in text.char_indices() {
            if c == '%' && backslashes % 2 == 0 {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            backslashes = if c == '\\' { backslashes + 1 } else { 0 };
        }
        parts.push(&text[start..]);
        for part in parts.iter().skip(1) {
            let found = ids(part);
            if found.windows(2).any(|w| w[0] != w[1]) {
                return Err(format!("text of two lines after one comment: {part:?}"));
            }
        }
        Ok(())
    }

    fn markers_balance(line: &str) -> bool {
        let mut open: Option<&str> = None;
        let mut rest = line;
        while !rest.is_empty() {
            match (open, rest.get(..2).unwrap_or("")) {
                (None, "[-" | "{+") => open = Some(&rest[..2]),
                (Some("[-"), "-]") | (Some("{+"), "+}") => open = None,
                (Some(_), "[-" | "{+") | (None, "-]" | "+}") => return false,
                _ => {
                    let c = rest.chars().next().unwrap();
                    rest = &rest[c.len_utf8()..];
                    continue;
                }
            }
            rest = &rest[2..];
        }
        open.is_none()
    }

    /// Renders the diff of two documents in full, one line per source line, at a huge width.
    /// `None` when the only difference is layout.
    fn render_pair(
        old: &[Src],
        new: &[Src],
        eol: &str,
        final_newline: bool,
    ) -> (String, String, Option<String>) {
        let (before, after) = (
            document(old, eol, final_newline),
            document(new, eol, final_newline),
        );
        let diff = diff_words_with(
            &before,
            &after,
            Tokeniser::Latex,
            &WordDiffOptions::keep_everything(),
        );
        let rendered = (!diff.layout_only).then(|| file("e.tex", &diff, plain(), 10_000));
        (before, after, rendered)
    }

    fn check_pair(old: &[Src], new: &[Src], eol: &str, final_newline: bool) -> Result<(), String> {
        let (before, after, rendered) = render_pair(old, new, eol, final_newline);
        let Some(rendered) = rendered else {
            return Ok(());
        };
        for line in rendered.lines() {
            if !markers_balance(line) {
                return Err(format!("unbalanced markers in {line:?}\n{rendered}"));
            }
            // Unchanged text is spelled as in the new text, so only the new side is judged.
            let (_, new_side) = sides_of(line);
            comment_is_contained(&new_side).map_err(|e| format!("{e} in {line:?}\n{rendered}"))?;
        }
        let shown = ids_with_words(&rendered);
        for word in ids_with_words(&(before + &after)) {
            if !shown.contains(&word) {
                return Err(format!("{word} is missing from\n{rendered}"));
            }
        }
        Ok(())
    }

    /// The old side judged inside each single `[-…-]` span (see PV-049).
    fn check_removed_spans(
        old: &[Src],
        new: &[Src],
        eol: &str,
        final_newline: bool,
    ) -> Result<(), String> {
        let (_, _, rendered) = render_pair(old, new, eol, final_newline);
        for line in rendered.iter().flat_map(|r| r.lines()) {
            for span in line.split("[-").skip(1) {
                let span = span.split("-]").next().unwrap_or(span);
                comment_is_contained(span).map_err(|e| format!("{e} in {line:?}"))?;
            }
        }
        Ok(())
    }

    fn ids_with_words(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.starts_with('l') && t.contains('w') && ids(t).len() == 1)
            .map(str::to_owned)
            .collect()
    }

    type Scenario = (Vec<(u8, usize)>, Vec<(u8, u8, usize)>, bool, bool);

    fn scenario() -> impl Strategy<Value = Scenario> {
        (
            prop::collection::vec((0u8..7, 1usize..4), 1..8),
            // per line: its edit (0 to 3 keep, 4 remove, 5 and 6 replace by a new line), then the
            // kind and size of a line inserted after it (kind 7: none)
            prop::collection::vec((0u8..7, 0u8..8, 1usize..4), 1..8),
            any::<bool>(),
            any::<bool>(),
        )
    }

    /// The old and the new lines of a scenario.
    fn build(base: &[(u8, usize)], edits: &[(u8, u8, usize)]) -> (Vec<Src>, Vec<Src>) {
        let mut next = 0;
        let mut fresh = |kind: u8, words: usize| {
            next += 1;
            Src {
                id: next,
                kind,
                words,
            }
        };
        let old: Vec<Src> = base.iter().map(|(k, w)| fresh(*k, *w)).collect();
        let mut new = Vec::new();
        for (at, line) in old.iter().enumerate() {
            let (edit, insert_kind, insert_words) = edits[at % edits.len()];
            match edit {
                0..=3 => new.push(line.clone()),
                4 => {}
                _ => new.push(fresh(line.kind, line.words)),
            }
            if insert_kind < 7 {
                new.push(fresh(insert_kind, insert_words));
            }
        }
        (old, new)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        #[test]
        fn a_comment_never_swallows_text_from_another_line(
            (base, edits, crlf, final_newline) in scenario()
        ) {
            let (old, new) = build(&base, &edits);
            let eol = if crlf { "\r\n" } else { "\n" };
            let checked = check_pair(&old, &new, eol, final_newline);
            prop_assert!(checked.is_ok(), "{}", checked.unwrap_err());
        }

        #[test]
        #[ignore = "PV-049 (#93): removed text carries the new text's spacing"]
        fn removed_text_never_runs_a_comment_into_another_line(
            (base, edits, crlf, final_newline) in scenario()
        ) {
            let (old, new) = build(&base, &edits);
            let eol = if crlf { "\r\n" } else { "\n" };
            let checked = check_removed_spans(&old, &new, eol, final_newline);
            prop_assert!(checked.is_ok(), "{}", checked.unwrap_err());
        }
    }

    #[test]
    fn the_reviewers_comment_repros_render_on_separate_lines() {
        let render = |old: &str, new: &str| {
            let diff = diff_words(old, new, Tokeniser::Latex);
            file("e.tex", &diff, plain(), 80)
        };
        assert!(
            render(
                "Intro.\n% n\n\nP2.\n",
                "Intro.\n% n\nAdded sentence.\n\nP2.\n"
            )
            .ends_with("  Intro. % n\n  {+Added sentence.+}\n")
        );
        assert!(render("A % n\nB.\n\nP2.\n", "A % n\n\nP2.\n").ends_with("  A % n\n  [-B.-]\n"));
        assert!(render("A % n\n", "A % n\nB.\n").ends_with("  A % n\n  {+B.+}\n"));
    }

    #[test]
    fn the_reviewers_comment_repros_hold() {
        let src = |id, kind, words| Src { id, kind, words };
        // A comment line, then an added line.
        check_pair(
            &[src(1, 0, 1), src(2, 1, 1), src(3, 4, 1), src(4, 0, 1)],
            &[
                src(1, 0, 1),
                src(2, 1, 1),
                src(5, 0, 2),
                src(3, 4, 1),
                src(4, 0, 1),
            ],
            "\n",
            true,
        )
        .unwrap();
        // A removed line after a trailing comment, then the end of the paragraph.
        check_pair(&[src(1, 2, 2), src(2, 0, 1)], &[src(1, 2, 2)], "\n", true).unwrap();
        // An added line at the end of the file.
        check_pair(&[src(1, 2, 2)], &[src(1, 2, 2), src(3, 0, 1)], "\n", true).unwrap();
    }
}

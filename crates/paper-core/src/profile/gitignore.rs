//! The marked blocks in `.gitignore`.
//!
//! Paper only ever writes between two marker lines; every other byte of the file is the
//! user's and is never touched (04-content.md § Profiles, 02-cli.md § paper ignore). There are
//! two kinds of block, told apart by their marker text, at most one of each:
//!
//! ```text
//! # >>> paper-vcs profile: latex >>>
//! *.aux
//! # <<< paper-vcs profile: latex <<<
//! # >>> paper-vcs: ignored >>>
//! /data/raw/
//! # <<< paper-vcs: ignored <<<
//! ```
//!
//! The profile block is paper's suggestion for a kind of project; the second block holds the
//! rules `paper ignore` added. When both exist, the profile block comes first.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::Profile;
use crate::fs_util::write_atomic;

/// Why a profile block couldn't be applied. Nothing is written when this happens.
#[derive(Debug, thiserror::Error)]
pub enum GitignoreError {
    /// A start marker has no end marker after it.
    #[error("the paper-vcs block starting at line {line} of .gitignore is never closed")]
    UnterminatedBlock {
        /// 1-based line of the start marker.
        line: usize,
    },
    /// More than one paper-vcs block.
    #[error("there is more than one paper-vcs block in .gitignore (second one at line {line})")]
    DuplicateBlock {
        /// 1-based line of the second start marker.
        line: usize,
    },
    /// The file is not valid UTF-8, so paper can't edit it safely.
    #[error("{path} is not valid UTF-8 text")]
    NotUtf8 {
        /// The `.gitignore` involved.
        path: PathBuf,
    },
    /// The file exists but could not be read (permissions, a folder in its place, a disk error).
    #[error("could not read {path}")]
    Read {
        /// The `.gitignore` involved.
        path: PathBuf,
        /// What went wrong.
        #[source]
        source: io::Error,
    },
    /// The new text could not be written.
    #[error("could not write {path}")]
    Write {
        /// The `.gitignore` involved.
        path: PathBuf,
        /// What went wrong.
        #[source]
        source: io::Error,
    },
}

/// Applies `profile`'s block to the text of a `.gitignore` (`None` when the file doesn't exist).
///
/// - No file: the result is the block alone.
/// - A profile block (any profile name): replaced in place.
/// - No profile block, but paper's own block: the profile block goes right before it.
/// - Neither: the block is appended after a blank line, every existing byte kept.
///
/// Paper's own block is never touched. Lines end in CRLF if the first line of the file does,
/// else LF. Returns `Ok(None)` when the file would not change.
///
/// # Errors
///
/// [`GitignoreError::UnterminatedBlock`] or [`GitignoreError::DuplicateBlock`].
///
/// # Panics
///
/// Never.
pub fn apply(existing: Option<&str>, profile: &Profile) -> Result<Option<String>, GitignoreError> {
    let Some(text) = existing else {
        return Ok(Some(render_block(profile, "\n", true)));
    };
    let eol = eol_of(text);
    let result = if let Some(span) = find_block(text, Kind::Profile)? {
        let terminated = text[span.start..span.end].ends_with('\n');
        format!(
            "{}{}{}",
            &text[..span.start],
            render_block(profile, eol, terminated),
            &text[span.end..]
        )
    } else if let Some(paper) = find_block(text, Kind::Paper)? {
        format!(
            "{}{}{}",
            &text[..paper.start],
            render_block(profile, eol, true),
            &text[paper.start..]
        )
    } else {
        let mut out = text.to_owned();
        push_blank_line(&mut out, eol);
        out.push_str(&render_block(profile, eol, true));
        out
    };
    Ok((result != text).then_some(result))
}

/// The rules in paper's own block (`# >>> paper-vcs: ignored >>>`) of a `.gitignore` text, in
/// order. Comments and blank lines are skipped; no block gives no rules.
///
/// # Errors
///
/// [`GitignoreError::UnterminatedBlock`] or [`GitignoreError::DuplicateBlock`].
///
/// # Panics
///
/// Never.
pub fn paper_rules(existing: Option<&str>) -> Result<Vec<String>, GitignoreError> {
    let Some(text) = existing else {
        return Ok(Vec::new());
    };
    Ok(find_block(text, Kind::Paper)?
        .map(|span| rules_in(&text[span.start..span.end]))
        .unwrap_or_default())
}

/// Adds `rules` to paper's own block. A new block goes at the end of the file after a blank
/// line, so it comes after the profile block and after the user's lines (later lines win in
/// gitignore). Rules already in the block are not repeated. Returns
/// `Ok(None)` when the text would not change; an empty block is never written.
///
/// # Errors
///
/// [`GitignoreError::UnterminatedBlock`] or [`GitignoreError::DuplicateBlock`].
///
/// # Panics
///
/// Never.
pub fn add_rules(
    existing: Option<&str>,
    rules: &[String],
) -> Result<Option<String>, GitignoreError> {
    let text = existing.unwrap_or_default();
    let eol = eol_of(text);
    let span = find_block(text, Kind::Paper)?;
    let present = span
        .as_ref()
        .map(|span| rules_in(&text[span.start..span.end]))
        .unwrap_or_default();
    let mut fresh: Vec<&str> = Vec::new();
    for rule in rules {
        if !present.contains(rule) && !fresh.contains(&rule.as_str()) {
            fresh.push(rule);
        }
    }
    if fresh.is_empty() {
        return Ok(None);
    }
    let lines = fresh.iter().fold(String::new(), |mut out, rule| {
        out.push_str(rule);
        out.push_str(eol);
        out
    });
    if let Some(span) = span {
        // New rules go right before the end marker line, after the existing ones.
        let block = &text[span.start..span.end];
        let end_line = block
            .trim_end_matches('\n')
            .rfind('\n')
            .map_or(0, |at| at + 1);
        let at = span.start + end_line;
        return Ok(Some(format!("{}{lines}{}", &text[..at], &text[at..])));
    }
    let block = format!("{PAPER_START}{eol}{lines}{PAPER_END}{eol}");
    let mut out = text.to_owned();
    push_blank_line(&mut out, eol);
    out.push_str(&block);
    Ok(Some(out))
}

/// Which block a rule line sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// The profile block.
    Profile,
    /// Paper's own block.
    Paper,
}

/// One rule line of a `.gitignore`: not a comment, not blank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleLine {
    /// The block it sits in; `None` for the user's own lines.
    pub block: Option<BlockKind>,
    /// 1-based line number.
    pub line: usize,
    /// The line without its line ending.
    pub rule: String,
}

/// Every rule line of `text` with the block it sits in. Lenient: a damaged block (no end marker)
/// runs to the end of the file, so listing never fails.
#[must_use]
pub fn rule_lines(text: &str) -> Vec<RuleLine> {
    let mut block = None;
    let mut out = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        if Kind::Profile.is_start(trimmed) {
            block = Some(BlockKind::Profile);
        } else if Kind::Paper.is_start(trimmed) {
            block = Some(BlockKind::Paper);
        } else if Kind::Profile.is_end(trimmed) || Kind::Paper.is_end(trimmed) {
            block = None;
        } else if is_rule(line) {
            out.push(RuleLine {
                block,
                line: index + 1,
                rule: line.to_owned(),
            });
        }
    }
    out
}

const PAPER_START: &str = "# >>> paper-vcs: ignored >>>";
const PAPER_END: &str = "# <<< paper-vcs: ignored <<<";

/// The two kinds of block.
#[derive(Clone, Copy)]
enum Kind {
    Profile,
    Paper,
}

impl Kind {
    fn is_start(self, trimmed: &str) -> bool {
        match self {
            Self::Profile => {
                trimmed.starts_with("# >>> paper-vcs profile: ") && trimmed.ends_with(" >>>")
            }
            Self::Paper => trimmed == PAPER_START,
        }
    }

    fn is_end(self, trimmed: &str) -> bool {
        match self {
            Self::Profile => {
                trimmed.starts_with("# <<< paper-vcs profile: ") && trimmed.ends_with(" <<<")
            }
            Self::Paper => trimmed == PAPER_END,
        }
    }
}

/// `\r\n` if the first line of `text` ends that way, else `\n`.
fn eol_of(text: &str) -> &'static str {
    if text
        .split_inclusive('\n')
        .next()
        .is_some_and(|l| l.ends_with("\r\n"))
    {
        "\r\n"
    } else {
        "\n"
    }
}

/// Ends `out` with a line ending and a blank line, unless it is empty.
fn push_blank_line(out: &mut String, eol: &str) {
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push_str(eol);
        }
        out.push_str(eol);
    }
}

fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !trimmed.starts_with('#')
}

/// The rules between the marker lines of a block's text.
fn rules_in(block: &str) -> Vec<String> {
    let lines: Vec<&str> = block.split_inclusive('\n').collect();
    let inner = lines.get(1..lines.len().saturating_sub(1)).unwrap_or(&[]);
    inner
        .iter()
        .map(|line| line.trim_end_matches(['\r', '\n']))
        .filter(|line| is_rule(line))
        .map(str::to_owned)
        .collect()
}

/// Byte range of the existing block, from its start marker to the end of its end-marker line.
struct Span {
    start: usize,
    end: usize,
}

fn find_block(text: &str, kind: Kind) -> Result<Option<Span>, GitignoreError> {
    let mut found: Option<Span> = None;
    let mut open: Option<(usize, usize)> = None; // (byte offset, line number)
    let mut offset = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let number = index + 1;
        let trimmed = line.trim();
        if kind.is_start(trimmed) {
            if let Some((_, first)) = open {
                return Err(GitignoreError::UnterminatedBlock { line: first });
            }
            if found.is_some() {
                return Err(GitignoreError::DuplicateBlock { line: number });
            }
            open = Some((offset, number));
        } else if kind.is_end(trimmed)
            && let Some((start, _)) = open.take()
        {
            found = Some(Span {
                start,
                end: offset + line.len(),
            });
        }
        offset += line.len();
    }
    match open {
        Some((_, line)) => Err(GitignoreError::UnterminatedBlock { line }),
        None => Ok(found),
    }
}

fn render_block(profile: &Profile, eol: &str, terminated: bool) -> String {
    let mut out = format!("# >>> paper-vcs profile: {} >>>{eol}", profile.name);
    for line in &profile.ignore {
        out.push_str(line);
        out.push_str(eol);
    }
    let _ = write!(out, "# <<< paper-vcs profile: {} <<<", profile.name);
    if terminated {
        out.push_str(eol);
    }
    out
}

/// Reads the `.gitignore` in `workdir`: `None` when there is none.
///
/// # Errors
///
/// [`GitignoreError::NotUtf8`] or [`GitignoreError::Read`].
///
/// # Panics
///
/// Never.
pub fn read(workdir: &Path) -> Result<Option<String>, GitignoreError> {
    let path = workdir.join(".gitignore");
    match fs::read(&path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| GitignoreError::NotUtf8 { path }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(GitignoreError::Read { path, source }),
    }
}

/// Applies `profile`'s block to the `.gitignore` in `workdir`, creating it if needed.
/// Returns `true` when the file was written, `false` when it already had exactly this block.
///
/// # Errors
///
/// Everything [`apply`] and [`read`] report, and [`GitignoreError::Write`].
///
/// # Panics
///
/// Never.
pub fn apply_to_file(workdir: &Path, profile: &Profile) -> Result<bool, GitignoreError> {
    let path = workdir.join(".gitignore");
    match apply(read(workdir)?.as_deref(), profile)? {
        Some(text) => {
            write_atomic(&path, text.as_bytes())
                .map_err(|source| GitignoreError::Write { path, source })?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, ignore: &[&str]) -> Profile {
        Profile {
            name: name.to_owned(),
            detect: Vec::new(),
            ignore: ignore.iter().map(|line| (*line).to_owned()).collect(),
            tips: Vec::new(),
        }
    }

    fn latex() -> Profile {
        profile("latex", &["*.aux", "*.log", "build/"])
    }

    fn markdown() -> Profile {
        profile("markdown", &["_site/", ".cache/"])
    }

    fn block(name: &str, lines: &[&str], eol: &str) -> String {
        let mut out = format!("# >>> paper-vcs profile: {name} >>>{eol}");
        for line in lines {
            out.push_str(line);
            out.push_str(eol);
        }
        out.push_str("# <<< paper-vcs profile: ");
        out.push_str(name);
        out.push_str(" <<<");
        out.push_str(eol);
        out
    }

    fn latex_block(eol: &str) -> String {
        block("latex", &["*.aux", "*.log", "build/"], eol)
    }

    fn markdown_block(eol: &str) -> String {
        block("markdown", &["_site/", ".cache/"], eol)
    }

    #[test]
    fn a_missing_file_gets_just_the_block() {
        let result = apply(None, &latex()).unwrap();
        assert_eq!(result, Some(latex_block("\n")));
    }

    #[test]
    fn a_file_without_a_block_gets_it_appended_after_a_blank_line() {
        let existing = "notes.txt\n*.bak\n";
        let result = apply(Some(existing), &latex()).unwrap().unwrap();
        assert_eq!(result, format!("{existing}\n{}", latex_block("\n")));
    }

    #[test]
    fn appending_keeps_every_existing_byte_even_without_a_final_newline() {
        let existing = "notes.txt\n\n\n  keep me  ";
        let result = apply(Some(existing), &latex()).unwrap().unwrap();
        assert!(result.starts_with(existing), "{result:?}");
        assert!(result.ends_with(&latex_block("\n")), "{result:?}");
        assert!(
            result[existing.len()..].starts_with("\n\n# >>>"),
            "the block follows a blank line: {result:?}"
        );
    }

    #[test]
    fn a_block_of_another_profile_is_replaced_in_place() {
        let existing = format!("before\n{}after\n", markdown_block("\n"));
        let result = apply(Some(&existing), &latex()).unwrap().unwrap();
        assert_eq!(result, format!("before\n{}after\n", latex_block("\n")));
    }

    #[test]
    fn a_block_edited_by_hand_is_restored_to_the_profile() {
        let edited = block("latex", &["*.aux", "my-own-line"], "\n");
        let existing = format!("before\n{edited}after\n");
        let result = apply(Some(&existing), &latex()).unwrap().unwrap();
        assert_eq!(result, format!("before\n{}after\n", latex_block("\n")));
    }

    #[test]
    fn applying_the_same_profile_again_changes_nothing() {
        let once = apply(Some("mine\n"), &latex()).unwrap().unwrap();
        assert_eq!(apply(Some(&once), &latex()).unwrap(), None);
        let created = apply(None, &markdown()).unwrap().unwrap();
        assert_eq!(apply(Some(&created), &markdown()).unwrap(), None);
    }

    #[test]
    fn a_crlf_file_gets_a_crlf_block_and_blank_line() {
        let existing = "notes.txt\r\n*.bak\r\n";
        let result = apply(Some(existing), &latex()).unwrap().unwrap();
        assert_eq!(result, format!("{existing}\r\n{}", latex_block("\r\n")));
        assert_eq!(apply(Some(&result), &latex()).unwrap(), None);
    }

    #[test]
    fn replacing_in_a_crlf_file_keeps_crlf() {
        let existing = format!("before\r\n{}after\r\n", markdown_block("\r\n"));
        let result = apply(Some(&existing), &latex()).unwrap().unwrap();
        assert_eq!(
            result,
            format!("before\r\n{}after\r\n", latex_block("\r\n"))
        );
    }

    #[test]
    fn only_the_first_line_decides_the_line_ending() {
        let existing = "a\nb\r\n";
        let result = apply(Some(existing), &latex()).unwrap().unwrap();
        assert_eq!(result, format!("{existing}\n{}", latex_block("\n")));
    }

    #[test]
    fn a_start_marker_without_an_end_marker_is_an_error() {
        let existing = "mine\n# >>> paper-vcs profile: latex >>>\n*.aux\n";
        let error = apply(Some(existing), &markdown()).unwrap_err();
        assert!(
            matches!(error, GitignoreError::UnterminatedBlock { line: 2 }),
            "{error:?}"
        );
    }

    #[test]
    fn two_blocks_are_an_error() {
        let existing = format!("{}\n{}", latex_block("\n"), markdown_block("\n"));
        let error = apply(Some(&existing), &latex()).unwrap_err();
        assert!(
            matches!(error, GitignoreError::DuplicateBlock { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn the_error_messages_say_what_is_wrong() {
        let unterminated = apply(Some("# >>> paper-vcs profile: latex >>>\n"), &latex())
            .unwrap_err()
            .to_string();
        assert!(unterminated.contains("never closed"), "{unterminated}");
    }

    #[test]
    fn apply_to_file_creates_replaces_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".gitignore");

        assert!(apply_to_file(dir.path(), &latex()).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), latex_block("\n"));

        assert!(!apply_to_file(dir.path(), &latex()).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), latex_block("\n"));

        assert!(apply_to_file(dir.path(), &markdown()).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            markdown_block("\n")
        );
    }

    #[test]
    fn apply_to_file_writes_nothing_on_a_malformed_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".gitignore");
        let broken = "# >>> paper-vcs profile: latex >>>\n*.aux\n";
        std::fs::write(&path, broken).unwrap();

        let error = apply_to_file(dir.path(), &markdown()).unwrap_err();

        assert!(matches!(error, GitignoreError::UnterminatedBlock { .. }));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    }
}

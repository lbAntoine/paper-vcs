//! The marked profile block in `.gitignore`.
//!
//! Profiles only ever add a block between two marker lines; every other byte of the file is the
//! user's and is never touched (04-content.md § Profiles):
//!
//! ```text
//! # >>> paper-vcs profile: latex >>>
//! *.aux
//! # <<< paper-vcs profile: latex <<<
//! ```

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
    /// The file could not be read or written.
    #[error("could not update {path}")]
    Io {
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
/// - No block in the file: the block is appended after a blank line, every existing byte kept.
/// - A paper-vcs block (any profile name): replaced in place.
///
/// Lines end in CRLF if the first line of the file does, else LF. Returns `Ok(None)` when the
/// file would not change.
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
    let eol = if text
        .split_inclusive('\n')
        .next()
        .is_some_and(|l| l.ends_with("\r\n"))
    {
        "\r\n"
    } else {
        "\n"
    };
    let result = if let Some(span) = find_block(text)? {
        let terminated = text[span.start..span.end].ends_with('\n');
        format!(
            "{}{}{}",
            &text[..span.start],
            render_block(profile, eol, terminated),
            &text[span.end..]
        )
    } else {
        let mut out = text.to_owned();
        if !text.is_empty() {
            if !text.ends_with('\n') {
                out.push_str(eol);
            }
            out.push_str(eol);
        }
        out.push_str(&render_block(profile, eol, true));
        out
    };
    Ok((result != text).then_some(result))
}

/// Byte range of the existing block, from its start marker to the end of its end-marker line.
struct Span {
    start: usize,
    end: usize,
}

fn find_block(text: &str) -> Result<Option<Span>, GitignoreError> {
    let mut found: Option<Span> = None;
    let mut open: Option<(usize, usize)> = None; // (byte offset, line number)
    let mut offset = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let number = index + 1;
        let trimmed = line.trim();
        let is_marker = |open_tag: &str, close_tag: &str| {
            trimmed.starts_with(open_tag) && trimmed.ends_with(close_tag)
        };
        if is_marker("# >>> paper-vcs profile: ", " >>>") {
            if let Some((_, first)) = open {
                return Err(GitignoreError::UnterminatedBlock { line: first });
            }
            if found.is_some() {
                return Err(GitignoreError::DuplicateBlock { line: number });
            }
            open = Some((offset, number));
        } else if is_marker("# <<< paper-vcs profile: ", " <<<")
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

/// Applies `profile`'s block to the `.gitignore` in `workdir`, creating it if needed.
/// Returns `true` when the file was written, `false` when it already had exactly this block.
///
/// # Errors
///
/// Everything [`apply`] reports, and [`GitignoreError::Io`].
///
/// # Panics
///
/// Never.
pub fn apply_to_file(workdir: &Path, profile: &Profile) -> Result<bool, GitignoreError> {
    let path = workdir.join(".gitignore");
    let io_error = |source| GitignoreError::Io {
        path: path.clone(),
        source,
    };
    let existing = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(io_error(err)),
    };
    match apply(existing.as_deref(), profile)? {
        Some(text) => {
            write_atomic(&path, text.as_bytes()).map_err(io_error)?;
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

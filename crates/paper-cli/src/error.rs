//! Rendering of errors (three parts) and the mapping to exit codes.

use std::fmt::Write as _;

use paper_core::{ErrorKind, PaperError};
use serde_json::json;

use crate::render::style::Style;

/// The process exit code for an error of this kind: 1, 2 or 3.
pub fn exit_code(kind: ErrorKind) -> u8 {
    match kind {
        ErrorKind::User => 1,
        ErrorKind::NeedsAttention => 2,
        ErrorKind::Project => 3,
    }
}

/// The three-part human form, for stderr:
///
/// ```text
/// error: what happened
///   why:  the reason
///   next: command      # what it does
///         command      # what it does
/// ```
pub fn human(error: &PaperError, style: Style) -> String {
    let mut out = format!(
        "{} {}\n  why:  {}\n",
        style.error_label("error:"),
        error.message,
        error.why.replace('\n', "\n        ")
    );
    let width = error
        .next
        .iter()
        .map(|s| s.command.chars().count())
        .max()
        .unwrap_or(0);
    for (index, suggestion) in error.next.iter().enumerate() {
        let lead = if index == 0 { "  next: " } else { "        " };
        let padding = " ".repeat(width - suggestion.command.chars().count());
        let _ = writeln!(
            out,
            "{lead}{}{padding}  # {}",
            suggestion.command, suggestion.explanation
        );
    }
    out
}

/// `{"schema":1,"error":{kind,message,why,next}}`, for stdout under `--json`.
pub fn json(error: &PaperError) -> String {
    let kind = match error.kind {
        ErrorKind::User => "user",
        ErrorKind::NeedsAttention => "needs_attention",
        ErrorKind::Project => "project",
    };
    let next: Vec<_> = error
        .next
        .iter()
        .map(|s| json!({"command": s.command, "explanation": s.explanation}))
        .collect();
    let value = json!({
        "schema": 1,
        "error": {"kind": kind, "message": error.message, "why": error.why, "next": next},
    });
    format!("{value}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kinds_map_to_the_documented_exit_codes() {
        assert_eq!(exit_code(ErrorKind::User), 1);
        assert_eq!(exit_code(ErrorKind::NeedsAttention), 2);
        assert_eq!(exit_code(ErrorKind::Project), 3);
    }
}

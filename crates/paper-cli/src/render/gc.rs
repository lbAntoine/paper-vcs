//! Output for `paper gc`. The JSON shape is documented in `docs/specs/02-cli.md` (`paper gc`).

use std::fmt::Write as _;

use paper_core::project::GcOutcome;
use serde::Serialize;

use super::json::{SCHEMA, line};

/// What `paper activity` and `Nothing to undo.` add once `paper gc` has trimmed the log.
pub const HORIZON_NOTE: &str =
    "Older history was cleaned up by `paper gc`, so there is nothing further back to go to.";

/// The date (`YYYY-MM-DD`) of the horizon entry, from its RFC 3339 time.
fn horizon_date(outcome: &GcOutcome) -> &str {
    outcome
        .horizon
        .time
        .get(..10)
        .unwrap_or(&outcome.horizon.time)
}

fn entries(count: usize) -> String {
    if count == 1 {
        "1 entry".to_owned()
    } else {
        format!("{count} entries")
    }
}

/// The text for stdout.
pub fn human(outcome: &GcOutcome) -> String {
    let date = horizon_date(outcome);
    let mut out = String::new();
    if outcome.dry_run {
        out.push_str("Dry run: nothing was changed.\n");
    }
    let (verb, kept) = if outcome.dry_run {
        ("Would remove", "would keep")
    } else {
        ("Removed", "kept")
    };
    let _ = writeln!(
        out,
        "{verb} {} from the activity list; {kept} {}, back to {date}.",
        entries(outcome.removed_entries),
        entries(outcome.kept_entries),
    );
    if !outcome.swept.is_empty() {
        let _ = writeln!(
            out,
            "{} {} leftover {}:",
            if outcome.dry_run {
                "Would sweep"
            } else {
                "Swept"
            },
            outcome.swept.len(),
            if outcome.swept.len() == 1 {
                "file"
            } else {
                "files"
            },
        );
        for path in &outcome.swept {
            let _ = writeln!(out, "  {path}");
        }
    }
    out.push_str(
        "Paper does not run `git gc` for you: run it to let git reclaim the space of old saves.\n",
    );
    out
}

#[derive(Serialize)]
struct GcOut<'a> {
    schema: u32,
    dry_run: bool,
    removed_entries: usize,
    kept_entries: usize,
    /// The date of the oldest kept entry.
    horizon: &'a str,
    swept: &'a [String],
}

/// The JSON for stdout.
pub fn json(outcome: &GcOutcome) -> String {
    line(&GcOut {
        schema: SCHEMA,
        dry_run: outcome.dry_run,
        removed_entries: outcome.removed_entries,
        kept_entries: outcome.kept_entries,
        horizon: horizon_date(outcome),
        swept: &outcome.swept,
    })
}

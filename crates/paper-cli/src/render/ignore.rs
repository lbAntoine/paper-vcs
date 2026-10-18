//! Output for `paper ignore`. The JSON shapes are documented in `docs/specs/02-cli.md`
//! (`paper ignore`).

use std::fmt::Write as _;

use paper_core::project::{IgnoreListing, IgnoreOutcome, IgnoreSource};
use serde::Serialize;

use super::human::files;
use super::json::{SCHEMA, line, op_kind_name};

/// How many untracked files the human text names before saying `and N more`.
const SHOWN_FILES: usize = 20;

/// What `paper ignore` prints when it had nothing to do.
pub const NOTHING_TO_DO: &str = "Nothing to do: already ignored.\n";

/// The text for stdout after `paper ignore <args>`.
pub fn human(outcome: &IgnoreOutcome) -> String {
    if outcome.entry.is_none() {
        return NOTHING_TO_DO.to_owned();
    }
    let mut out = String::new();
    if !outcome.added.is_empty() {
        out.push_str("Now ignoring:\n");
        for rule in &outcome.added {
            out.push_str(&rule_line(rule, outcome));
        }
    }
    if !outcome.already.is_empty() {
        out.push_str("Already ignored:\n");
        for rule in &outcome.already {
            out.push_str(&rule_line(rule, outcome));
        }
    }
    if !outcome.untracked.is_empty() {
        let count = outcome.untracked.len();
        if !out.is_empty() {
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "Stopped tracking {}; they are still on your disk:",
            files(count)
        );
        for path in outcome.untracked.iter().take(SHOWN_FILES) {
            let _ = writeln!(out, "  {path}");
        }
        if count > SHOWN_FILES {
            let _ = writeln!(out, "  and {} more", count - SHOWN_FILES);
        }
        out.push_str("Your next save records their removal; earlier saves keep their versions.\n");
    }
    out.push_str("`paper undo` takes this back.\n");
    out
}

fn rule_line(rule: &str, outcome: &IgnoreOutcome) -> String {
    if outcome.patterns.iter().any(|pattern| pattern == rule) {
        format!("  {rule}  (a pattern: applies to the whole project)\n")
    } else {
        format!("  {rule}\n")
    }
}

#[derive(Serialize)]
struct EntryOut {
    id: String,
    short_id: String,
    /// RFC 3339.
    time: String,
    kind: &'static str,
    description: String,
}

#[derive(Serialize)]
struct AddOut<'a> {
    schema: u32,
    added: &'a [String],
    already: &'a [String],
    untracked: &'a [String],
    entry: Option<EntryOut>,
}

/// The JSON for stdout after `paper ignore <args>`.
pub fn json(outcome: &IgnoreOutcome) -> String {
    line(&AddOut {
        schema: SCHEMA,
        added: &outcome.added,
        already: &outcome.already,
        untracked: &outcome.untracked,
        entry: outcome.entry.as_ref().map(|entry| EntryOut {
            id: entry.id.to_string(),
            short_id: entry.short_id.clone(),
            time: entry.time.clone(),
            kind: op_kind_name(entry.kind),
            description: entry.description.clone(),
        }),
    })
}

/// The text for stdout after `paper ignore` with no arguments.
pub fn listing_human(listing: &IgnoreListing) -> String {
    let mut out = String::new();
    if listing.rules.is_empty() && listing.unreadable.is_empty() {
        return "No ignore rules.\n".to_owned();
    }
    let width = listing
        .rules
        .iter()
        .map(|rule| rule.rule.chars().count())
        .max()
        .unwrap_or(0);
    let mut group: Option<IgnoreSource> = None;
    for rule in &listing.rules {
        if group != Some(rule.source) {
            if group.is_some() {
                out.push('\n');
            }
            let _ = writeln!(out, "{}:", rule.source.name());
            group = Some(rule.source);
        }
        let _ = writeln!(
            out,
            "  {:<width$}  {}, line {}",
            rule.rule, rule.file, rule.line
        );
    }
    if listing.rules.is_empty() {
        out.push_str("No ignore rules could be read.\n");
    }
    for (source, file) in &listing.unreadable {
        let _ = writeln!(
            out,
            "\nNote: {file} ({}) could not be read, so its rules are not listed.",
            source.name()
        );
    }
    out
}

#[derive(Serialize)]
struct RuleOut<'a> {
    rule: &'a str,
    source: &'static str,
    file: &'a str,
    line: usize,
}

#[derive(Serialize)]
struct ListOut<'a> {
    schema: u32,
    rules: Vec<RuleOut<'a>>,
}

/// The JSON for stdout after `paper ignore` with no arguments. Unreadable sources have no
/// place in the fixed shape; the human text names them.
pub fn listing_json(listing: &IgnoreListing) -> String {
    line(&ListOut {
        schema: SCHEMA,
        rules: listing
            .rules
            .iter()
            .map(|rule| RuleOut {
                rule: &rule.rule,
                source: rule.source.name(),
                file: &rule.file,
                line: rule.line,
            })
            .collect(),
    })
}

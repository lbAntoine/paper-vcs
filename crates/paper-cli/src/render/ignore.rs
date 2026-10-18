//! Output for `paper ignore`. The JSON shapes are documented in `docs/specs/02-cli.md`
//! (`paper ignore`).

use std::fmt::Write as _;

use paper_core::project::{IgnoreListing, IgnoreOutcome, IgnoreSource, IgnoreWhy, UnignoreOutcome};
use serde::Serialize;

use super::human::files;
use super::json::{SCHEMA, line, op_kind_name};

/// How many untracked files the human text names before saying `and N more`.
const SHOWN_FILES: usize = 20;

/// What `paper ignore` prints when it had nothing to do.
pub const NOTHING_TO_DO: &str = "Nothing to do: already ignored.\n";

/// What `paper unignore` prints when nothing it was given is ignored.
pub const NOTHING_TO_UNIGNORE: &str = "Nothing to do: not ignored.\n";

/// The closing line: what `paper undo` does, and for `--local` that it leaves the rule alone.
fn undo_line(local: bool) -> String {
    if local {
        format!(
            "{LOCAL_NOTE}`paper undo` changes tracking only; the rule in .git/info/exclude stays.\n"
        )
    } else {
        "`paper undo` takes this back.\n".to_owned()
    }
}

/// Said after a command that wrote to `.git/info/exclude`.
const LOCAL_NOTE: &str =
    "The rules are in .git/info/exclude: they are yours alone, and not part of your saves.\n";

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
    out.push_str(&undo_line(outcome.local));
    out
}

/// The text for stdout after `paper unignore <args>`.
pub fn unignore_human(outcome: &UnignoreOutcome) -> String {
    if outcome.entry.is_none() {
        return NOTHING_TO_UNIGNORE.to_owned();
    }
    let mut out = String::new();
    if !outcome.removed.is_empty() {
        out.push_str("No longer ignoring:\n");
        for rule in &outcome.removed {
            let _ = writeln!(out, "  {rule}");
        }
    }
    if !outcome.negated.is_empty() {
        out.push_str("Exceptions added to the ignore rules:\n");
        for rule in &outcome.negated {
            let _ = writeln!(out, "  {rule}");
        }
    }
    if !outcome.tracked.is_empty() {
        let count = outcome.tracked.len();
        let _ = writeln!(out, "\nNow tracking {}:", files(count));
        for path in outcome.tracked.iter().take(SHOWN_FILES) {
            let _ = writeln!(out, "  {path}");
        }
        if count > SHOWN_FILES {
            let _ = writeln!(out, "  and {} more", count - SHOWN_FILES);
        }
        out.push_str("Your next save records them.\n");
    }
    out.push_str(&undo_line(outcome.local));
    out
}

/// The text for stdout after `paper ignore --why <path>`.
pub fn why_human(why: &IgnoreWhy) -> String {
    let path = &why.path;
    let Some(rule) = &why.rule else {
        return format!("{path} is not ignored.\n");
    };
    if !why.ignored {
        return format!(
            "{path} is not ignored: `{}` ({}) brings it back.\n",
            rule.rule,
            rule.location()
        );
    }
    let mut out = format!(
        "{path} is ignored by `{}` ({}).\n",
        rule.rule,
        rule.location()
    );
    if why.tracked {
        let _ = writeln!(
            out,
            "It is still tracked, because it was saved before the rule; paper ignore {path} \
             stops tracking it."
        );
    }
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
    local: bool,
}

fn entry_out(entry: &paper_core::project::ActivityEntry) -> EntryOut {
    EntryOut {
        id: entry.id.to_string(),
        short_id: entry.short_id.clone(),
        time: entry.time.clone(),
        kind: op_kind_name(entry.kind),
        description: entry.description.clone(),
    }
}

#[derive(Serialize)]
struct UnignoreOut<'a> {
    schema: u32,
    removed: &'a [String],
    negated: &'a [String],
    tracked: &'a [String],
    entry: Option<EntryOut>,
    local: bool,
}

/// The JSON for stdout after `paper unignore <args>`.
pub fn unignore_json(outcome: &UnignoreOutcome) -> String {
    line(&UnignoreOut {
        schema: SCHEMA,
        removed: &outcome.removed,
        negated: &outcome.negated,
        tracked: &outcome.tracked,
        entry: outcome.entry.as_ref().map(entry_out),
        local: outcome.local,
    })
}

#[derive(Serialize)]
struct WhyRuleOut<'a> {
    rule: &'a str,
    source: &'static str,
    file: &'a str,
    line: usize,
    negated: bool,
}

#[derive(Serialize)]
struct WhyOut<'a> {
    schema: u32,
    path: &'a str,
    ignored: bool,
    tracked: bool,
    rule: Option<WhyRuleOut<'a>>,
}

/// The JSON for stdout after `paper ignore --why <path>`.
pub fn why_json(why: &IgnoreWhy) -> String {
    line(&WhyOut {
        schema: SCHEMA,
        path: &why.path,
        ignored: why.ignored,
        tracked: why.tracked,
        rule: why.rule.as_ref().map(|rule| WhyRuleOut {
            rule: &rule.rule,
            source: rule.source.name(),
            file: &rule.file,
            line: rule.line,
            negated: rule.negated,
        }),
    })
}

/// The JSON for stdout after `paper ignore <args>`.
pub fn json(outcome: &IgnoreOutcome) -> String {
    line(&AddOut {
        schema: SCHEMA,
        added: &outcome.added,
        already: &outcome.already,
        untracked: &outcome.untracked,
        entry: outcome.entry.as_ref().map(entry_out),
        local: outcome.local,
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

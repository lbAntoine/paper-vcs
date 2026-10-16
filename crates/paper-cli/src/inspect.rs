//! The commands that look at or put back files: `status`, `diff`, `restore` and `show`.

use paper_core::point::Point;
use paper_core::project::{
    DiffRequest, FileDiffEntry, Project, ResolveError, RestoreRequest, StatusOptions,
};
use paper_core::{ErrorKind, PaperError, Suggestion};

use crate::cli::Cli;
use crate::commands::{finish, now, open, open_and_snapshot};
use crate::paths::Paths;
use crate::render::style::Style;
use crate::render::{
    Verbosity, diff as human_diff, human, json, json_diff, status as human_status,
};

type Output = Result<String, PaperError>;

/// `paper status`: the header, the unsaved changes and what was left out.
pub fn status(cli: &Cli, style: Style) -> Output {
    let (mut project, _) = open(cli)?;
    let status = project.status(&StatusOptions {
        snapshot: !cli.no_snapshot,
    })?;
    Ok(finish(
        cli,
        || human_status::status(&status, now(), style),
        || json_diff::status(&status),
    ))
}

/// `paper diff`: sort the arguments into points and paths, then print the unified diffs (or
/// `--stat`).
pub fn diff(
    cli: &Cli,
    args: &[String],
    forced_paths: &[String],
    words: bool,
    stat: bool,
    since: Option<&str>,
    style: Style,
) -> Output {
    if words {
        return Err(PaperError {
            kind: ErrorKind::User,
            message: "paper diff --words is not yet available".to_owned(),
            why: "Word-by-word comparison arrives in v0.3; for now the comparison is line by line."
                .to_owned(),
            next: vec![Suggestion {
                command: "paper diff".to_owned(),
                explanation: "compare line by line".to_owned(),
            }],
        });
    }
    let (project, locator) = open_and_snapshot(cli)?;
    let (points, mut paths) = split_args(&project, args)?;
    paths.extend(forced_paths.iter().cloned());
    let (from, to) = match (since, points.as_slice()) {
        (Some(_), [_, ..]) => return Err(since_with_points()),
        (Some(since), []) => (Some(since.parse::<Point>()?), None),
        (None, []) => (None, None),
        (None, [from]) => (Some(from.clone()), None),
        (None, [from, to, ..]) => (Some(from.clone()), Some(to.clone())),
    };
    let request = DiffRequest {
        from,
        to,
        paths: project_paths(&locator, &paths)?,
    };
    let entries = project.diff(&request)?;
    Ok(finish(
        cli,
        || diff_text(&entries, stat, style),
        || json_diff::diff(&entries),
    ))
}

fn diff_text(entries: &[FileDiffEntry], stat: bool, style: Style) -> String {
    if stat {
        human_diff::stat(entries)
    } else {
        human_diff::unified(entries, style)
    }
}

fn since_with_points() -> PaperError {
    PaperError {
        kind: ErrorKind::User,
        message: "--since can't be combined with points".to_owned(),
        why: "--since <point> already means \"from that point to the draft\".".to_owned(),
        next: vec![
            Suggestion {
                command: "paper diff --since <point>".to_owned(),
                explanation: "compare a point with the draft".to_owned(),
            },
            Suggestion {
                command: "paper diff <from> <to>".to_owned(),
                explanation: "compare two points".to_owned(),
            },
        ],
    }
}

/// Up to two leading arguments that resolve as points; the first one that doesn't, and
/// everything after it, are paths.
fn split_args(project: &Project, args: &[String]) -> Result<(Vec<Point>, Vec<String>), PaperError> {
    let mut points = Vec::new();
    for arg in args.iter().take(2) {
        match arg.parse::<Point>() {
            Ok(point) => match project.resolve(&point) {
                Ok(_) => points.push(point),
                Err(ResolveError::UnknownPoint { .. }) => break,
                Err(other) => return Err(other.into()),
            },
            Err(err @ paper_core::point::ParsePointError::ForeignSyntax { .. }) => {
                return Err(err.into());
            }
            Err(_) => break,
        }
    }
    Ok((points.clone(), args[points.len()..].to_vec()))
}

/// Paths from where paper runs to project paths; `.` at the top means no restriction.
fn project_paths(locator: &Paths, given: &[String]) -> Result<Vec<String>, PaperError> {
    let resolved = given
        .iter()
        .map(|p| locator.project_path(p))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if resolved.iter().any(String::is_empty) {
        Vec::new()
    } else {
        resolved
    })
}

/// `paper restore`: put the paths back as they were at `from` (default `last`).
pub fn restore(cli: &Cli, paths: &[String], from: Option<&str>) -> Output {
    let from = from.map(str::parse::<Point>).transpose()?;
    let (mut project, locator) = open(cli)?;
    let restored = project.restore(&RestoreRequest {
        paths: restore_paths(&locator, paths)?,
        from,
    })?;
    Ok(finish(
        cli,
        || human_status::restore(&restored),
        || json_diff::restore(&restored),
    ))
}

/// Like [`project_paths`], but `.` stays: it names the whole folder, which the core takes as `""`.
fn restore_paths(locator: &Paths, given: &[String]) -> Result<Vec<String>, PaperError> {
    given.iter().map(|p| locator.project_path(p)).collect()
}

/// `paper show`: metadata, then the diffs (or `--stat`).
pub fn show(cli: &Cli, point: &str, stat: bool, verbosity: Verbosity, style: Style) -> Output {
    let point: Point = point.parse()?;
    let (project, _) = open_and_snapshot(cli)?;
    let shown = project.show(&point)?;
    let diffs = project.show_diff(&point)?;
    Ok(finish(
        cli,
        || human::show(&shown, &diffs, stat, now(), verbosity, style),
        || json::show(&shown, &diffs),
    ))
}

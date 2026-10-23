//! The commands that look at or put back files: `status`, `diff`, `restore` and `show`.

use std::fmt::Write as _;

use paper_core::point::{ParsePointError, Point};
use paper_core::project::{
    DiffError, DiffMode, DiffRequest, FileDiffEntry, Project, ResolveError, RestoreRequest,
    StatusOptions,
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
    // A word that sat where a point could be, and is neither a point nor a file. A word with
    // `/` or `.` is treated as a file; a line name with `/` or `.` also gets the file wording.
    let ambiguous_word = (points.len() < 2 && !paths.is_empty() && !paths[0].contains(['/', '.']))
        .then(|| {
            locator
                .project_path(&paths[0])
                .map(|resolved| (paths[0].clone(), resolved))
        })
        .and_then(Result::ok);
    paths.extend(forced_paths.iter().cloned());
    let (from, to) = match (since, points.as_slice()) {
        (Some(_), [_, ..]) => return Err(since_with_points()),
        (Some(since), []) => (Some(since.parse::<Point>()?), None),
        (None, []) => (None, None),
        (None, [from]) => (Some(from.clone()), None),
        (None, [from, to, ..]) => (Some(from.clone()), Some(to.clone())),
    };
    let request = DiffRequest {
        mode: DiffMode::Lines,
        from,
        to,
        paths: project_paths(&locator, &paths)?,
    };
    let entries = project
        .diff(&request)
        .map_err(|error| mistyped_point(error, ambiguous_word.as_ref()))?;
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
///
/// An argument that is clearly a point but mistyped (`last~x`, `date:tomorrow`, `HEAD~1`) is an
/// error, not a path.
fn split_args(project: &Project, args: &[String]) -> Result<(Vec<Point>, Vec<String>), PaperError> {
    let mut points = Vec::new();
    for arg in args.iter().take(2) {
        match arg.parse::<Point>() {
            Ok(point) => match project.resolve(&point) {
                Ok(_) => points.push(point),
                Err(ResolveError::UnknownPoint { .. }) => break,
                Err(other) => return Err(other.into()),
            },
            Err(err) if looks_like_a_point(arg, &err) => return Err(err.into()),
            Err(_) => break,
        }
    }
    let paths = args[points.len()..].to_vec();
    Ok((points, paths))
}

fn looks_like_a_point(arg: &str, error: &ParsePointError) -> bool {
    matches!(error, ParsePointError::ForeignSyntax { .. })
        || arg.starts_with("last~")
        || arg.starts_with("date:")
}

/// Turns "no file matches `lsat`" into "`lsat` is not a save, line, date or file" when the
/// word sat where a point could have been.
fn mistyped_point(error: DiffError, word: Option<&(String, String)>) -> PaperError {
    match (&error, word) {
        (DiffError::UnknownPaths { paths }, Some((word, resolved))) if paths.contains(resolved) => {
            let others: Vec<&String> = paths.iter().filter(|p| *p != resolved).collect();
            let mut why = "Paper looked for it as a save, a line, a date and then as a file in \
                           the draft and the saves being compared."
                .to_owned();
            if !others.is_empty() {
                let listed: Vec<&str> = others.iter().map(|p| p.as_str()).collect();
                let _ = write!(why, " No file matches {} either.", listed.join(", "));
            }
            PaperError {
                kind: ErrorKind::User,
                message: format!("'{word}' is not a save, line, date or file"),
                why,
                next: vec![
                    Suggestion {
                        command: "paper history".to_owned(),
                        explanation: "list the saves and their IDs".to_owned(),
                    },
                    Suggestion {
                        command: "paper status".to_owned(),
                        explanation: "list the files in the draft".to_owned(),
                    },
                ],
            }
        }
        _ => error.into(),
    }
}

/// Paths from where paper runs to project paths. `.` at the top is the empty path, which
/// selects every file but is still checked like any other.
fn project_paths(locator: &Paths, given: &[String]) -> Result<Vec<String>, PaperError> {
    given.iter().map(|p| locator.project_path(p)).collect()
}

/// `paper restore`: put the paths back as they were at `from` (default `last`).
pub fn restore(cli: &Cli, paths: &[String], from: Option<&str>) -> Output {
    let from = from.map(str::parse::<Point>).transpose()?;
    let (mut project, locator) = open(cli)?;
    let restored = project.restore(&RestoreRequest {
        paths: project_paths(&locator, paths)?,
        from,
    })?;
    Ok(finish(
        cli,
        || human_status::restore(&restored),
        || json_diff::restore(&restored),
    ))
}

/// `paper show`: metadata, then the diffs (or `--stat`).
pub fn show(cli: &Cli, point: &str, stat: bool, verbosity: Verbosity, style: Style) -> Output {
    let point: Point = point.parse()?;
    let (project, _) = open_and_snapshot(cli)?;
    let shown = project.show(&point)?;
    let diffs = project.show_diff(&point, DiffMode::Lines)?;
    Ok(finish(
        cli,
        || human::show(&shown, &diffs, stat, now(), verbosity, style),
        || json::show(&shown, &diffs),
    ))
}

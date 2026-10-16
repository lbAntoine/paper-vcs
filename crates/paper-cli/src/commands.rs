//! The four commands: open the project, call `paper-core`, and render the result as text.
//! Each returns what to print on stdout, or a [`PaperError`] for the error renderer.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use paper_core::point::Point;
use paper_core::project::{HistoryRequest, PathSelection, Project, SaveOutcome, SaveRequest};
use paper_core::snapshot::SnapshotOptions;
use paper_core::{ErrorKind, PaperError, Suggestion};
use paper_git::{GitRepo, Identity, configured_identity};

use crate::cli::{Cli, Command};
use crate::render::style::Style;
use crate::render::{Verbosity, human, json};

type Output = Result<String, PaperError>;

/// Runs the command in `cli` and returns the text for stdout.
pub fn execute(cli: &Cli, style: Style) -> Output {
    let verbosity = Verbosity {
        verbose: cli.verbose,
    };
    match &cli.command {
        Command::Init { path, .. } => init(cli, path.as_deref()),
        Command::Save {
            message,
            paths,
            all_but,
        } => save(cli, message.as_deref(), paths, all_but, verbosity, style),
        Command::History {
            path,
            all,
            limit,
            graph,
        } => history(cli, path.as_deref(), *all, *limit, *graph, verbosity, style),
        Command::Show { point } => show(cli, point, verbosity, style),
    }
}

/// What a successful command prints: nothing under `-q` (unless `--json`).
fn finish(
    cli: &Cli,
    human_text: impl FnOnce() -> String,
    json_text: impl FnOnce() -> String,
) -> String {
    if cli.json {
        json_text()
    } else if cli.quiet {
        String::new()
    } else {
        human_text()
    }
}

fn init(cli: &Cli, path: Option<&Path>) -> Output {
    let base = start_dir(cli)?;
    let target = path.map_or_else(|| base.clone(), |p| base.join(p));
    let identity = identity(&target)?;
    let (_project, initialized) = Project::init(&target, identity, &SnapshotOptions::default())?;
    Ok(finish(
        cli,
        || human::init(&initialized),
        || json::init(&initialized),
    ))
}

fn save(
    cli: &Cli,
    message: Option<&str>,
    paths: &[String],
    all_but: &[String],
    verbosity: Verbosity,
    style: Style,
) -> Output {
    let mut project = open(cli)?;
    let message = match message {
        Some(message) => message.to_owned(),
        None => prompt_for_message(cli),
    };
    let selection = if !paths.is_empty() {
        PathSelection::Only(paths.to_vec())
    } else if !all_but.is_empty() {
        PathSelection::AllBut(all_but.to_vec())
    } else {
        PathSelection::All
    };
    let outcome = project.save(&SaveRequest {
        message: message.clone(),
        paths: selection,
    })?;
    let short_id = match &outcome {
        SaveOutcome::Saved(_) => latest_short_id(&project)?,
        SaveOutcome::NothingToSave => String::new(),
    };
    Ok(finish(
        cli,
        || match &outcome {
            SaveOutcome::Saved(saved) => human::save(saved, &short_id, &message, verbosity, style),
            SaveOutcome::NothingToSave => human::nothing_to_save(),
        },
        || json::save(&outcome, &short_id, &message),
    ))
}

/// The shortest unique ID of the newest save on the current line.
fn latest_short_id(project: &Project) -> Result<String, PaperError> {
    let request = HistoryRequest {
        limit: Some(1),
        ..HistoryRequest::default()
    };
    let entries = project.history(&request)?;
    Ok(entries
        .first()
        .map(|entry| entry.short_change_id.clone())
        .unwrap_or_default())
}

/// Asks for a message on a terminal; returns an empty string otherwise, which `save` rejects
/// with the "a save needs a message" error.
fn prompt_for_message(cli: &Cli) -> String {
    if cli.json || !io::stdin().is_terminal() {
        return String::new();
    }
    eprint!("Describe this save: ");
    let _ = io::stderr().flush();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return String::new();
    }
    line.trim().to_owned()
}

fn history(
    cli: &Cli,
    path: Option<&str>,
    all: bool,
    limit: Option<usize>,
    graph: bool,
    verbosity: Verbosity,
    style: Style,
) -> Output {
    if graph {
        return Err(PaperError {
            kind: ErrorKind::User,
            message: "paper history --graph is not yet available".to_owned(),
            why: "Drawing the lines as a graph is planned for a later version.".to_owned(),
            next: vec![Suggestion {
                command: "paper history".to_owned(),
                explanation: "list the saves without the graph".to_owned(),
            }],
        });
    }
    let project = open_and_snapshot(cli)?;
    let entries = project.history(&HistoryRequest {
        path: path.map(str::to_owned),
        all_lines: all,
        limit,
    })?;
    let scope = path.map_or_else(|| "yet".to_owned(), |p| format!("touch {p}"));
    Ok(finish(
        cli,
        || human::history(&entries, now(), verbosity, style, &scope),
        || json::history(&entries),
    ))
}

fn show(cli: &Cli, point: &str, verbosity: Verbosity, style: Style) -> Output {
    let point: Point = point.parse()?;
    let project = open_and_snapshot(cli)?;
    let shown = project.show(&point)?;
    Ok(finish(
        cli,
        || human::show(&shown, now(), verbosity, style),
        || json::show(&shown),
    ))
}

fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// The folder to work from: `-C <path>` or the current directory, as an absolute path.
fn start_dir(cli: &Cli) -> Result<PathBuf, PaperError> {
    let dir = match &cli.directory {
        Some(dir) => std::path::absolute(dir),
        None => std::env::current_dir(),
    }
    .map_err(|err| failure("Paper could not find the folder to work in", &err))?;
    if dir.is_dir() {
        Ok(dir)
    } else {
        Err(PaperError {
            kind: ErrorKind::User,
            message: format!("{} is not a folder", dir.display()),
            why: "-C needs an existing folder to run in.".to_owned(),
            next: vec![Suggestion {
                command: "paper -C <folder> history".to_owned(),
                explanation: "name a folder that exists".to_owned(),
            }],
        })
    }
}

fn failure(message: &str, err: &dyn std::error::Error) -> PaperError {
    PaperError {
        kind: ErrorKind::Project,
        message: message.to_owned(),
        why: err.to_string(),
        next: Vec::new(),
    }
}

/// `user.name` and `user.email` from git config, or the "who are you" error.
fn identity(path: &Path) -> Result<Identity, PaperError> {
    configured_identity(path)
        .map_err(|err| PaperError::from(paper_core::project::OpenError::Git(err)))?
        .ok_or_else(PaperError::no_identity)
}

/// Finds the project that contains the working folder (walking up) and opens it.
fn open(cli: &Cli) -> Result<Project, PaperError> {
    let start = start_dir(cli)?;
    let Some(workdir) = GitRepo::enclosing_workdir(&start) else {
        return Err(PaperError::not_a_project(&start, false));
    };
    let identity = identity(&workdir)?;
    Ok(Project::open(&workdir, identity)?)
}

/// Opens the project and, unless `--no-snapshot`, records the working files first (I1).
fn open_and_snapshot(cli: &Cli) -> Result<Project, PaperError> {
    let mut project = open(cli)?;
    if !cli.no_snapshot {
        project.snapshot()?;
    }
    Ok(project)
}

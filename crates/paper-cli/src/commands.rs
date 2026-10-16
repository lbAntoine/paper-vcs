//! The commands: open the project, call `paper-core`, and render the result as text.
//! Each returns what to print on stdout, or a [`PaperError`] for the error renderer.

use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use paper_core::config::Settings;
use paper_core::profile::Profile;
use paper_core::project::{
    HistoryRequest, InitError, PathSelection, Project, SaveOutcome, SaveRequest,
};
use paper_core::{ErrorKind, PaperError, Suggestion};
use paper_git::{GitRepo, Identity};

use crate::cli::{Cli, Command};
use crate::config_cmd;
use crate::inspect;
use crate::paths::Paths;
use crate::prompt;
use crate::render::style::Style;
use crate::render::{Verbosity, human, json};
use crate::settings;

type Output = Result<String, PaperError>;

/// Runs the command in `cli` and returns the text for stdout.
pub fn execute(cli: &Cli, style: Style) -> Output {
    let verbosity = Verbosity {
        verbose: cli.verbose,
    };
    match &cli.command {
        Command::Init { path, profile } => init(cli, path.as_deref(), profile.as_deref()),
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
        Command::Show { point, stat } => inspect::show(cli, point, *stat, verbosity, style),
        Command::Status => inspect::status(cli, style),
        Command::Diff {
            args,
            paths,
            words,
            stat,
            since,
            ..
        } => inspect::diff(cli, args, paths, *words, *stat, since.as_deref(), style),
        Command::Config { action } => config_cmd::run(cli, action),
        Command::Restore { paths, from } => inspect::restore(cli, paths, from.as_deref()),
    }
}

/// What a successful command prints: nothing under `-q` (unless `--json`).
pub(crate) fn finish(
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

fn init(cli: &Cli, path: Option<&Path>, profile: Option<&str>) -> Output {
    let base = start_dir(cli)?;
    let target = path.map_or_else(|| base.clone(), |p| base.join(p));
    // A `.git` here that git can't open is reported before git settings are read from it.
    if target.join(".git").exists() && GitRepo::enclosing_workdir(&target).is_none() {
        return Err(InitError::DamagedGit { path: target }.into());
    }
    let settings = settings::resolve(Some(&target), &target)?;
    let identity = identity(cli, &settings)?;
    // clap only lets the six built-in names through.
    let choice = profile.and_then(Profile::builtin);
    let (_project, initialized) =
        Project::init_with_profile(&target, identity, &settings.snapshot_options(), choice)?;
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
    let (mut project, locator) = open(cli)?;
    let message = if let Some(message) = message {
        message.to_owned()
    } else {
        let stdin = io::stdin();
        let is_tty = !cli.json && stdin.is_terminal();
        prompt::ask_for_message(is_tty, &mut stdin.lock(), &mut io::stderr())?
    };
    let selection = selection(&locator, paths, all_but)?;
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

/// Which files a save takes, with paths made relative to the project folder.
fn selection(
    locator: &Paths,
    paths: &[String],
    all_but: &[String],
) -> Result<PathSelection, PaperError> {
    let resolve = |given: &[String]| -> Result<Vec<String>, PaperError> {
        given.iter().map(|p| locator.project_path(p)).collect()
    };
    if !paths.is_empty() {
        let resolved = resolve(paths)?;
        // `.` names the whole project folder (or, from a subfolder, everything below it).
        let whole_project = resolved.iter().any(String::is_empty);
        Ok(if whole_project {
            PathSelection::All
        } else {
            PathSelection::Only(resolved)
        })
    } else if !all_but.is_empty() {
        let resolved = resolve(all_but)?;
        if resolved.iter().any(String::is_empty) {
            return Err(PaperError {
                kind: ErrorKind::User,
                message: "--all-but can't leave out the whole project".to_owned(),
                why: "Everything would be excluded, so there would be nothing to save.".to_owned(),
                next: vec![Suggestion {
                    command: "paper save --all-but <file>".to_owned(),
                    explanation: "name the files to leave in the draft".to_owned(),
                }],
            });
        }
        Ok(PathSelection::AllBut(resolved))
    } else {
        Ok(PathSelection::All)
    }
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
    let (project, locator) = open_and_snapshot(cli)?;
    let path = match path {
        Some(given) => Some(locator.project_path(given)?).filter(|p| !p.is_empty()),
        None => None,
    };
    let entries = project.history(&HistoryRequest {
        path: path.clone(),
        all_lines: all,
        limit,
    })?;
    let scope = path
        .as_ref()
        .map_or_else(|| "yet".to_owned(), |p| format!("touch {p}"));
    Ok(finish(
        cli,
        || human::history(&entries, now(), verbosity, style, &scope),
        || json::history(&entries),
    ))
}

pub(crate) fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// The folder to work from: `-C <path>` or the current directory, as an absolute path.
pub(crate) fn start_dir(cli: &Cli) -> Result<PathBuf, PaperError> {
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
    PaperError::project_failure(message, err)
}

/// The author for saves: from the settings, else asked for once on a terminal and stored in the
/// user config, else the "who are you" error.
fn identity(cli: &Cli, settings: &Settings) -> Result<Identity, PaperError> {
    if let Some(identity) = settings.identity() {
        return Ok(identity);
    }
    let stdin = io::stdin();
    let is_tty = !cli.json && stdin.is_terminal();
    if let Some(path) = settings::user_config_file()
        && let Some(identity) =
            prompt::ask_for_identity(is_tty, &mut stdin.lock(), &mut io::stderr(), &path)?
    {
        return Ok(identity);
    }
    Err(PaperError::no_identity())
}

/// Finds the project that contains the working folder (walking up) and opens it.
pub(crate) fn open(cli: &Cli) -> Result<(Project, Paths), PaperError> {
    let start = start_dir(cli)?;
    let Some(workdir) = GitRepo::enclosing_workdir(&start) else {
        return Err(PaperError::not_a_project(&start, false));
    };
    let settings = settings::resolve(Some(&workdir), &workdir)?;
    let identity = identity(cli, &settings)?;
    let paths = Paths::new(&start, &workdir);
    let project = Project::open(&workdir, identity)?.with_options(settings.snapshot_options());
    Ok((project, paths))
}

/// Opens the project and, unless `--no-snapshot`, records the working files first (I1).
pub(crate) fn open_and_snapshot(cli: &Cli) -> Result<(Project, Paths), PaperError> {
    let (mut project, paths) = open(cli)?;
    if !cli.no_snapshot {
        project.snapshot()?;
    }
    Ok((project, paths))
}

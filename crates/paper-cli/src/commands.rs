//! The commands: open the project, call `paper-core`, and render the result as text.
//! Each returns what to print on stdout, or a [`PaperError`] for the error renderer.

use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use paper_core::activity::ActivityLog;
use paper_core::config::Settings;
use paper_core::profile::Profile;
use paper_core::project::{
    GcAge, HistoryRequest, IgnoreRequest, InitError, LockOptions, PathSelection, Project,
    SaveOutcome, SaveRequest, UndoError, UndoOutcome, UnignoreRequest, has_damaged_git,
};
use paper_core::{ErrorKind, PaperError, Suggestion};
use paper_git::{GitRepo, Identity};

use crate::cli::{Cli, Command};
use crate::config_cmd;
use crate::inspect;
use crate::paths::Paths;
use crate::prompt;
use crate::render::gc;
use crate::render::ignore as ignore_render;
use crate::render::style::Style;
use crate::render::undo::{self, Verb};
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
        Command::Activity { limit } => activity(cli, *limit, style),
        Command::Undo { to } => undo(cli, to.as_deref()),
        Command::Gc {
            older_than,
            dry_run,
        } => gc(cli, older_than, *dry_run),
        Command::Redo => undo_family(cli, Verb::Redo, Project::redo),
        Command::Ignore {
            paths,
            yes,
            local,
            why,
        } => ignore(cli, paths, *yes, *local, why.as_deref()),
        Command::Unignore { paths, local } => unignore(cli, paths, *local),
        Command::Help { topic } => crate::help::run(cli, topic),
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

/// `paper ignore [<path|pattern>...]`: add rules, or with no arguments list the rules.
fn ignore(cli: &Cli, args: &[String], yes: bool, local: bool, why: Option<&str>) -> Output {
    let (mut project, locator) = open(cli)?;
    if let Some(path) = why {
        let from = locator.project_path("")?;
        let path = from_here(&locator, &from, path)?;
        let answer = project.why_ignored(&from, &path)?;
        return Ok(finish(
            cli,
            || ignore_render::why_human(&answer),
            || ignore_render::why_json(&answer),
        ));
    }
    if args.is_empty() {
        let listing = project.ignore_listing();
        return Ok(finish(
            cli,
            || ignore_render::listing_human(&listing),
            || ignore_render::listing_json(&listing),
        ));
    }
    let from = locator.project_path("")?;
    let args = args
        .iter()
        .map(|arg| from_here(&locator, &from, arg))
        .collect::<Result<Vec<_>, _>>()?;
    let outcome = project.ignore(&IgnoreRequest {
        args,
        from,
        yes,
        local,
    })?;
    Ok(finish(
        cli,
        || ignore_render::human(&outcome),
        || ignore_render::json(&outcome),
    ))
}

/// `paper unignore <path|pattern>...`: stop ignoring, and track the files again.
fn unignore(cli: &Cli, args: &[String], local: bool) -> Output {
    let (mut project, locator) = open(cli)?;
    let from = locator.project_path("")?;
    let args = args
        .iter()
        .map(|arg| from_here(&locator, &from, arg))
        .collect::<Result<Vec<_>, _>>()?;
    let outcome = project.unignore(&UnignoreRequest { args, from, local })?;
    Ok(finish(
        cli,
        || ignore_render::unignore_human(&outcome),
        || ignore_render::unignore_json(&outcome),
    ))
}

/// An absolute path as a path relative to `from` (where paper runs), which is what `paper-core`
/// takes. Patterns (`*.aux`, `/data/*.csv`) and relative paths are passed as typed.
fn from_here(locator: &Paths, from: &str, arg: &str) -> Result<String, PaperError> {
    if !Path::new(arg).is_absolute() || arg.contains(['*', '?', '[']) {
        return Ok(arg.to_owned());
    }
    let inside = locator.project_path(arg)?;
    let up = "../".repeat(from.split('/').filter(|part| !part.is_empty()).count());
    Ok(format!("{up}{inside}"))
}

fn init(cli: &Cli, path: Option<&Path>, profile: Option<&str>) -> Output {
    let base = start_dir(cli)?;
    let target = path.map_or_else(|| base.clone(), |p| base.join(p));
    // A `.git` here that git can't open is reported before git settings are read from it.
    if has_damaged_git(&target) {
        return Err(InitError::DamagedGit { path: target }.into());
    }
    let settings = settings::resolve(Some(&target), &target)?;
    let identity = identity(cli, &settings)?;
    // clap only lets the six built-in names through.
    let choice = profile.and_then(Profile::builtin);
    let (_project, initialized) = Project::init_with(
        &target,
        identity,
        &settings.snapshot_options(),
        choice,
        &lock_options(),
    )?;
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

/// `paper activity`: the log, newest first. Opens without snapshotting, so listing adds nothing.
fn activity(cli: &Cli, limit: usize, style: Style) -> Output {
    let (project, _) = open(cli)?;
    let entries = project.activity(limit)?;
    // The note belongs under the listing only when it reached the horizon; a `-n` that cuts
    // the list short says nothing about what lies further back.
    let horizon = ActivityLog::open(project.paper_dir()).horizon()?;
    let trimmed = horizon
        .as_ref()
        .is_some_and(|id| entries.last().is_some_and(|last| last.id == *id));
    Ok(finish(
        cli,
        || {
            let mut text = human::activity(&entries, now(), style);
            if trimmed {
                text.push('\n');
                text.push_str(gc::HORIZON_NOTE);
                text.push('\n');
            }
            text
        },
        || json::activity(&entries),
    ))
}

/// `paper undo` and `paper undo --to <id>`.
fn undo(cli: &Cli, to: Option<&str>) -> Output {
    match to {
        Some(prefix) => undo_family(cli, Verb::GoBack, |project| project.undo_to(prefix)),
        None => undo_family(cli, Verb::Undo, Project::undo),
    }
}

/// `paper gc`: opens without snapshotting; gc records no entry.
fn gc(cli: &Cli, older_than: &str, dry_run: bool) -> Output {
    let age = GcAge::parse(older_than).map_err(PaperError::from)?;
    let (mut project, _) = open(cli)?;
    let outcome = project.gc(age, dry_run)?;
    Ok(finish(cli, || gc::human(&outcome), || gc::json(&outcome)))
}

/// Opens the project (core snapshots first, so the CLI doesn't), runs one undo-family command
/// and renders what it did.
fn undo_family(
    cli: &Cli,
    verb: Verb,
    run: impl FnOnce(&mut Project) -> Result<UndoOutcome, UndoError>,
) -> Output {
    let (mut project, _) = open(cli)?;
    let outcome = run(&mut project)?;
    let trimmed = ActivityLog::open(project.paper_dir()).has_horizon();
    Ok(finish(
        cli,
        || {
            let mut text = undo::human(verb, &outcome);
            if trimmed && outcome == UndoOutcome::NothingToUndo {
                text.push_str(gc::HORIZON_NOTE);
                text.push('\n');
            }
            text
        },
        || undo::json(verb, &outcome),
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

/// The default 5 s wait; debug builds let `PAPER_DEBUG_LOCK_WAIT_MS` shorten it for tests.
pub(crate) fn lock_options() -> LockOptions {
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("PAPER_DEBUG_LOCK_WAIT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        return LockOptions {
            wait: std::time::Duration::from_millis(ms),
        };
    }
    LockOptions::default()
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
    let project = Project::open_with(&workdir, identity, &lock_options())?
        .with_options(settings.snapshot_options());
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

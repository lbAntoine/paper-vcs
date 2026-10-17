//! `paper config get|set|list`.

use std::fmt::Write as _;

use paper_core::config::{ConfigFile, Key, Scope, Settings, Value};
use paper_core::profile::{Profile, gitignore};
use paper_core::project::{LockError, OpenError, ProjectLock};
use paper_core::{ErrorKind, PaperError, Suggestion};
use paper_git::GitRepo;
use serde_json::json;

use crate::cli::{Cli, ConfigAction};
use crate::commands::{finish, lock_options, start_dir};
use crate::settings::{self, PROJECT_FILE};

type Output = Result<String, PaperError>;

pub fn run(cli: &Cli, action: &ConfigAction) -> Output {
    match action {
        ConfigAction::Get { key } => get(cli, key),
        ConfigAction::Set {
            key,
            value,
            project,
        } => set(cli, key, value, *project),
        ConfigAction::List => list(cli),
    }
}

fn effective(cli: &Cli) -> Result<Settings, PaperError> {
    let start = start_dir(cli)?;
    let workdir = GitRepo::enclosing_workdir(&start);
    settings::resolve(workdir.as_deref(), &start)
}

fn get(cli: &Cli, key: &str) -> Output {
    let key = Key::parse(key)?;
    let settings = effective(cli)?;
    let entry = settings.entry(key);
    let Some(value) = &entry.value else {
        return Err(PaperError {
            kind: ErrorKind::User,
            message: format!("`{key}` has no value yet", key = key.as_str()),
            why: if key == Key::Profile {
                "Nothing sets it: not the environment and not this project's paper.toml."
            } else {
                "Nothing sets it: not the environment, your paper settings or your git \
                 configuration."
            }
            .to_owned(),
            next: vec![Suggestion {
                command: format!(
                    "paper config set {}{} {}",
                    if key == Key::Profile {
                        "--project "
                    } else {
                        ""
                    },
                    key.as_str(),
                    key.example()
                ),
                explanation: "give it a value".to_owned(),
            }],
        });
    };
    let mut notes = ignored_notes(&settings, Some(key));
    if key == Key::DiffDefault {
        notes.extend(words_note(&settings));
    }
    emit_notes(cli, &notes);
    Ok(finish(
        cli,
        || format!("{value}\n"),
        || {
            let row = json!({
                "schema": 1,
                "key": key.as_str(),
                "value": json_value(Some(value)),
                "source": entry.source.as_str(),
                "notes": notes,
            });
            format!("{row}\n")
        },
    ))
}

fn set(cli: &Cli, key: &str, text: &str, project: bool) -> Output {
    let key = Key::parse(key)?;
    let scope = if project { Scope::Project } else { Scope::User };
    let fail = |error| PaperError::from_config(&error, scope);
    key.check_scope(scope).map_err(fail)?;
    let value = key.parse_value(text).map_err(fail)?;
    let (path, workdir) = if project {
        let start = start_dir(cli)?;
        let Some(workdir) = GitRepo::enclosing_workdir(&start) else {
            return Err(PaperError::not_a_project(&start, false));
        };
        (workdir.join(PROJECT_FILE), Some(workdir))
    } else {
        (
            settings::user_config_file().ok_or_else(no_settings_folder)?,
            None,
        )
    };
    // A project-level change waits for any running paper command, like a command would.
    let _lock =
        match &workdir {
            Some(workdir) => ProjectLock::acquire_for_workdir(workdir, lock_options().wait)
                .map_err(|error| match error {
                    LockError::Busy { holder } => PaperError::from(OpenError::Busy { holder }),
                    LockError::Io { path, source } => {
                        PaperError::from(OpenError::Io { path, source })
                    }
                })?,
            None => None,
        };
    let updated = ConfigFile::load(&path)?
        .with(key, value.clone())
        .map_err(fail)?;
    // The profile also decides the .gitignore block; a damaged block stops before any write.
    let gitignore_updated = match (&workdir, key, &value) {
        (Some(workdir), Key::Profile, Value::Text(name)) => Profile::builtin(name)
            .map(|profile| gitignore::apply_to_file(workdir, profile))
            .transpose()
            .map_err(|error| {
                let retry = format!("paper config set --project profile {name}");
                PaperError::from_gitignore(&error, &retry)
            })?
            .unwrap_or(false),
        _ => false,
    };
    updated.write(&path)?;
    Ok(finish(
        cli,
        || {
            let mut text = format!("Set {} = {value} in {}\n", key.as_str(), path.display());
            if gitignore_updated {
                text.push_str("Updated the profile block in .gitignore.\n");
            }
            text
        },
        || {
            let row = json!({
                "schema": 1,
                "key": key.as_str(),
                "value": json_value(Some(&value)),
                "file": path.display().to_string(),
            });
            format!("{row}\n")
        },
    ))
}

fn no_settings_folder() -> PaperError {
    let variables = if cfg!(windows) {
        "APPDATA is not set"
    } else {
        "Neither XDG_CONFIG_HOME (an absolute path) nor HOME is set"
    };
    PaperError {
        kind: ErrorKind::User,
        message: "Paper doesn't know where to keep your settings".to_owned(),
        why: format!("{variables}, so there is no folder for the user config."),
        next: vec![Suggestion {
            command: "paper config set --project <setting> <value>".to_owned(),
            explanation: "or keep a project setting in this project instead".to_owned(),
        }],
    }
}

fn list(cli: &Cli) -> Output {
    let settings = effective(cli)?;
    let mut notes = ignored_notes(&settings, None);
    notes.extend(words_note(&settings));
    emit_notes(cli, &notes);
    Ok(finish(
        cli,
        || table(&settings),
        || {
            let rows: Vec<_> = settings
                .entries()
                .iter()
                .map(|e| {
                    json!({
                        "key": e.key.as_str(),
                        "value": json_value(e.value.as_ref()),
                        "source": e.source.as_str(),
                    })
                })
                .collect();
            format!(
                "{}\n",
                json!({"schema": 1, "settings": rows, "notes": notes})
            )
        },
    ))
}

fn json_value(value: Option<&paper_core::config::Value>) -> serde_json::Value {
    match value {
        Some(Value::Text(text)) => json!(text),
        Some(Value::Megabytes(mb)) => json!(mb),
        None => serde_json::Value::Null,
    }
}

fn table(settings: &Settings) -> String {
    let rows: Vec<(&str, String, &str)> = settings
        .entries()
        .iter()
        .map(|e| {
            let value = e
                .value
                .as_ref()
                .map_or_else(|| "(not set)".to_owned(), ToString::to_string);
            let source = if e.value.is_some() {
                e.source.as_str()
            } else {
                "-"
            };
            (e.key.as_str(), value, source)
        })
        .collect();
    let key_width = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
    let value_width = rows.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
    let mut out = String::new();
    for (key, value, source) in rows {
        let _ = writeln!(out, "{key:<key_width$}  {value:<value_width$}  {source}");
    }
    out
}

/// Prints the notes on stderr for people. Under `--json` they travel in the JSON instead (stdout
/// stays the only output), and `--quiet` silences them.
fn emit_notes(cli: &Cli, notes: &[String]) {
    if cli.json || cli.quiet {
        return;
    }
    for note in notes {
        eprintln!("note: {note}");
    }
}

/// A privacy note for personal keys found in `paper.toml` (they are ignored). `only` limits it
/// to one key.
fn ignored_notes(settings: &Settings, only: Option<Key>) -> Vec<String> {
    settings
        .ignored_project_keys()
        .iter()
        .filter(|key| only.is_none_or(|k| k == **key))
        .map(|key| {
            format!(
                "{} in paper.toml is ignored. Identity and display settings never come \
                 from a file that is saved with the project (it may be shared). Remove it \
                 from paper.toml; `paper config set {} <value>` keeps it in your own settings.",
                key.as_str(),
                key.as_str()
            )
        })
        .collect()
}

/// `diff.default = words` is accepted but has no effect before v0.3.
fn words_note(settings: &Settings) -> Option<String> {
    (settings.entry(Key::DiffDefault).value == Some(Value::Text("words".to_owned()))).then(|| {
        "`diff.default = words` has no effect until v0.3; comparisons stay line by line.".to_owned()
    })
}

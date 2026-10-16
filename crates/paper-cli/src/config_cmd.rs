//! `paper config get|set|list`.

use std::fmt::Write as _;

use paper_core::config::{ConfigFile, Key, Scope, Settings};
use paper_core::{ErrorKind, PaperError, Suggestion};
use paper_git::GitRepo;
use serde_json::json;

use crate::cli::{Cli, ConfigAction};
use crate::commands::{finish, start_dir};
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
            why: "Nothing sets it: not the environment, your settings, this project or git."
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
    Ok(finish(
        cli,
        || format!("{value}\n"),
        || {
            let row = json!({
                "schema": 1,
                "key": key.as_str(),
                "value": json_value(Some(value)),
                "source": entry.source.as_str(),
            });
            format!("{row}\n")
        },
    ))
}

fn set(cli: &Cli, key: &str, text: &str, project: bool) -> Output {
    let key = Key::parse(key)?;
    let scope = if project { Scope::Project } else { Scope::User };
    key.check_scope(scope)?;
    let value = key.parse_value(text)?;
    let path = if project {
        let start = start_dir(cli)?;
        let Some(workdir) = GitRepo::enclosing_workdir(&start) else {
            return Err(PaperError::not_a_project(&start, false));
        };
        workdir.join(PROJECT_FILE)
    } else {
        settings::user_config_file().ok_or_else(no_settings_folder)?
    };
    let updated = ConfigFile::load(&path)?.with(key, value.clone())?;
    updated.write(&path)?;
    Ok(finish(
        cli,
        || format!("Set {} = {value} in {}\n", key.as_str(), path.display()),
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
    PaperError {
        kind: ErrorKind::User,
        message: "Paper doesn't know where to keep your settings".to_owned(),
        why: "Neither XDG_CONFIG_HOME nor HOME (APPDATA on Windows) is set.".to_owned(),
        next: vec![Suggestion {
            command: "paper config set --project <setting> <value>".to_owned(),
            explanation: "or keep a project setting in this project instead".to_owned(),
        }],
    }
}

fn list(cli: &Cli) -> Output {
    let settings = effective(cli)?;
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
            format!("{}\n", json!({"schema": 1, "settings": rows}))
        },
    ))
}

fn json_value(value: Option<&paper_core::config::Value>) -> serde_json::Value {
    use paper_core::config::Value;
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
            (e.key.as_str(), value, e.source.as_str())
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

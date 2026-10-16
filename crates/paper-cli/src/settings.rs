//! Loading the settings from the process environment and the two config files.

use std::path::{Path, PathBuf};

use paper_core::PaperError;
use paper_core::config::{ConfigFile, Key, Settings, Sources, Value, user_config_path};
use paper_git::configured_identity;

use crate::cli::{Cli, ColorChoice};

/// The project file in the project's top folder.
pub const PROJECT_FILE: &str = "paper.toml";

/// An environment variable's value, if set and valid Unicode.
pub fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Where the user config lives on this machine, if a base folder is known.
pub fn user_config_file() -> Option<PathBuf> {
    user_config_path(&process_env)
}

/// The user config, empty when it doesn't exist (or no base folder is known).
pub fn load_user() -> Result<ConfigFile, PaperError> {
    match user_config_file() {
        Some(path) => Ok(ConfigFile::load(&path)?),
        None => Ok(ConfigFile::empty()),
    }
}

/// The effective settings for the project at `workdir` (if any). `git_dir` is where git config
/// is read for the identity.
pub fn resolve(workdir: Option<&Path>, git_dir: &Path) -> Result<Settings, PaperError> {
    let project = match workdir {
        Some(dir) => ConfigFile::load(&dir.join(PROJECT_FILE))?,
        None => ConfigFile::empty(),
    };
    let git_identity = configured_identity(git_dir).map_err(|err| {
        PaperError::project_failure("Paper could not read your git settings", &err)
    })?;
    Ok(Settings::resolve(Sources {
        env: process_env,
        project,
        user: load_user()?,
        git_identity,
    })?)
}

/// `--color` if given, else `PAPER_UI_COLOR` or `ui.color` from the user config, else `auto`.
/// A settings problem is left for the command to report; colour falls back to `auto`.
pub fn color_choice(cli: &Cli) -> ColorChoice {
    if let Some(choice) = cli.color {
        return choice;
    }
    let settings = load_user().ok().and_then(|user| {
        Settings::resolve(Sources {
            env: process_env,
            project: ConfigFile::empty(),
            user,
            git_identity: None,
        })
        .ok()
    });
    match settings.map(|s| s.entry(Key::UiColor).value.clone()) {
        Some(Some(Value::Text(text))) if text == "always" => ColorChoice::Always,
        Some(Some(Value::Text(text))) if text == "never" => ColorChoice::Never,
        _ => ColorChoice::Auto,
    }
}

//! Settings: the keys paper knows, the two config files, and how they combine.
//!
//! Everything here is pure or touches only the files it is given; nothing prints or prompts.
//! Precedence, highest first: CLI flag, `PAPER_*` environment, project `paper.toml`, user
//! `config.toml`, git config (identity only), built-in default. See `03-architecture.md`
//! § Configuration.

mod file;
mod keys;
mod settings;

use std::path::{Path, PathBuf};

pub use file::ConfigFile;
pub use keys::{Home, Key, Scope, Source, Value};
pub use settings::{Entry, Settings, Sources};

/// Where the user config lives, from the process environment's point of view: `env` looks up
/// `XDG_CONFIG_HOME`, `HOME` and `APPDATA`. Uses the Windows layout on Windows.
pub fn user_config_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    user_config_path_for(cfg!(windows), env)
}

/// [`user_config_path`] for an explicit platform: `%APPDATA%\paper\config.toml` when `windows`,
/// else `$XDG_CONFIG_HOME/paper/config.toml`, else `~/.config/paper/config.toml`. An empty
/// variable counts as unset. `None` when no base folder is known.
pub fn user_config_path_for(
    windows: bool,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<PathBuf> {
    let set = |name: &str| env(name).filter(|value| !value.is_empty());
    let base = if windows {
        PathBuf::from(set("APPDATA")?)
    } else if let Some(xdg) = set("XDG_CONFIG_HOME").filter(|xdg| {
        // A relative XDG_CONFIG_HOME is invalid per the XDG spec: ignore it.
        xdg.starts_with('/') || Path::new(xdg).is_absolute()
    }) {
        PathBuf::from(xdg)
    } else {
        PathBuf::from(set("HOME")?).join(".config")
    };
    Some(base.join("paper").join("config.toml"))
}

/// Where a bad value came from, when it didn't come from the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// An environment variable, by name.
    Env(String),
    /// A config file.
    File(PathBuf),
}

impl Origin {
    /// The " (from X)" / " (in file)" text appended to messages, empty for no origin.
    pub(crate) fn note(origin: Option<&Origin>) -> String {
        match origin {
            None => String::new(),
            Some(Origin::Env(name)) => format!(" (from {name} in the environment)"),
            Some(Origin::File(path)) => format!(" (in {})", path.display()),
        }
    }
}

/// The other end of an order rule: `large_files.max_mb` must not be below `warn_mb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict {
    pub other: Key,
    pub other_value: u64,
}

/// What can go wrong with settings.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("unknown setting `{key}`")]
    UnknownKey { key: String },
    #[error("`{value}` is not a valid value for `{key}`{}: {expected}", Origin::note(.origin.as_ref()))]
    InvalidValue {
        key: String,
        value: String,
        expected: String,
        origin: Option<Origin>,
        /// Set when the value is fine alone but clashes with the other threshold.
        conflict: Option<Conflict>,
    },
    #[error("`{key}` can't be set {where_}: {reason}")]
    WrongHome {
        key: String,
        where_: String,
        reason: String,
    },
    #[error("`{table}` in the settings file is a plain value, so `{key}` can't be added to it")]
    Blocked {
        path: Option<PathBuf>,
        table: String,
        key: String,
    },
    #[error("{path} is not valid TOML (line {line}): {message}")]
    Malformed {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("could not access {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl ConfigError {
    /// Records where a bad value came from (unless it already says).
    #[must_use]
    pub fn with_origin(self, found: Origin) -> Self {
        match self {
            ConfigError::InvalidValue {
                key,
                value,
                expected,
                origin: None,
                conflict,
            } => ConfigError::InvalidValue {
                key,
                value,
                expected,
                origin: Some(found),
                conflict,
            },
            other => other,
        }
    }
}

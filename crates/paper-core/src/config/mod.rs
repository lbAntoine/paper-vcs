//! Settings: the keys paper knows, the two config files, and how they combine.
//!
//! Everything here is pure or touches only the files it is given; nothing prints or prompts.
//! Precedence, highest first: CLI flag, `PAPER_*` environment, project `paper.toml`, user
//! `config.toml`, git config (identity only), built-in default. See `03-architecture.md`
//! § Configuration.

use std::fmt;
use std::path::{Path, PathBuf};

use paper_git::Identity;

use crate::snapshot::SnapshotOptions;

/// A setting paper knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    UserName,
    UserEmail,
    UiColor,
    LargeFilesWarnMb,
    LargeFilesMaxMb,
    DiffDefault,
    Profile,
}

/// Which file may hold a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Home {
    /// The user config only (identity and looks are never committed).
    UserOnly,
    /// `paper.toml` only (it describes the project).
    ProjectOnly,
    /// Either file; the project's wins.
    Either,
}

/// A file a value is being written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    User,
    Project,
}

/// A typed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Names, emails, `auto|always|never`, `lines`, profile names.
    Text(String),
    /// A whole number of megabytes.
    Megabytes(u64),
}

/// Where an effective value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Env,
    Project,
    User,
    Git,
    Default,
}

impl Key {
    /// Every key, in the order `paper config list` shows them.
    pub const ALL: [Key; 7] = [
        Key::UserName,
        Key::UserEmail,
        Key::UiColor,
        Key::LargeFilesWarnMb,
        Key::LargeFilesMaxMb,
        Key::DiffDefault,
        Key::Profile,
    ];

    /// The dotted name, e.g. `large_files.max_mb`.
    pub fn as_str(self) -> &'static str {
        let _ = self;
        todo!("PV-005: dotted key name")
    }

    /// The environment variable that overrides this key: `PAPER_` plus the dotted name
    /// upper-cased with `.` replaced by `_`.
    pub fn env_name(self) -> String {
        let _ = self;
        todo!("PV-005: environment variable name")
    }

    /// Which file may hold this key.
    pub fn home(self) -> Home {
        let _ = self;
        todo!("PV-005: key home")
    }

    /// Reads a dotted key name.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownKey`], whose message lists the known keys.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn parse(_text: &str) -> Result<Key, ConfigError> {
        todo!("PV-005: parse key")
    }

    /// Checks and types `text` for this key (see the key table in `02-cli.md`).
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] saying what is allowed.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn parse_value(self, _text: &str) -> Result<Value, ConfigError> {
        todo!("PV-005: parse value")
    }

    /// Whether this key may be written to `scope`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::WrongHome`] with the reason (identity is never committed; `profile`
    /// describes the project).
    ///
    /// # Panics
    ///
    /// Never.
    pub fn check_scope(self, _scope: Scope) -> Result<(), ConfigError> {
        todo!("PV-005: key scope")
    }
}

impl fmt::Display for Value {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("PV-005: value display")
    }
}

/// A config file read from disk, edited without losing the user's comments and layout.
#[derive(Debug, Clone, Default)]
pub struct ConfigFile {
    #[allow(dead_code)] // stub: filled in by the implementation
    text: String,
}

impl ConfigFile {
    /// A file with nothing in it.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Reads `path`. A missing file loads as empty.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Malformed`] naming the file and line, or [`ConfigError::Io`].
    ///
    /// # Panics
    ///
    /// Never.
    pub fn load(_path: &Path) -> Result<Self, ConfigError> {
        todo!("PV-005: load config file")
    }

    /// The value stored for `key`, if any.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] if the file holds the wrong type or an unusable value.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn get(&self, _key: Key) -> Result<Option<Value>, ConfigError> {
        todo!("PV-005: read key from file")
    }

    /// A copy with `key` set. Comments, layout and unknown tables are kept.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] if `value` has the wrong type for `key`, or
    /// `large_files.max_mb` would be below `large_files.warn_mb` as written in this file.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn with(&self, _key: Key, _value: Value) -> Result<Self, ConfigError> {
        todo!("PV-005: set key")
    }

    /// The file's text.
    pub fn render(&self) -> String {
        todo!("PV-005: render config file")
    }

    /// Writes the file, creating missing parent folders.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Io`].
    ///
    /// # Panics
    ///
    /// Never.
    pub fn write(&self, _path: &Path) -> Result<(), ConfigError> {
        todo!("PV-005: write config file")
    }
}

/// Everything `Settings::resolve` combines. `env` looks a variable up by name.
#[derive(Debug)]
pub struct Sources<E: Fn(&str) -> Option<String>> {
    pub env: E,
    pub project: ConfigFile,
    pub user: ConfigFile,
    pub git_identity: Option<Identity>,
}

/// One key's effective value and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: Key,
    /// `None` only for identity keys when nobody set them.
    pub value: Option<Value>,
    pub source: Source,
}

/// The effective settings.
#[derive(Debug, Clone)]
pub struct Settings {
    entries: Vec<Entry>,
}

impl Settings {
    /// Combines the sources by precedence.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] for a bad `PAPER_*` value or a bad value in either file.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn resolve<E: Fn(&str) -> Option<String>>(
        _sources: Sources<E>,
    ) -> Result<Self, ConfigError> {
        todo!("PV-005: resolve settings")
    }

    /// Every key in [`Key::ALL`] order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry for `key`.
    pub fn entry(&self, _key: Key) -> &Entry {
        todo!("PV-005: settings entry")
    }

    /// The author for saves, if both name and email are known.
    pub fn identity(&self) -> Option<Identity> {
        todo!("PV-005: settings identity")
    }

    /// The large-file thresholds in bytes. The warning threshold never exceeds the skip one.
    pub fn snapshot_options(&self) -> SnapshotOptions {
        todo!("PV-005: settings snapshot options")
    }
}

/// Where the user config lives, from the process environment's point of view: `env` looks up
/// `XDG_CONFIG_HOME`, `HOME` and `APPDATA`. Uses the Windows layout on Windows.
pub fn user_config_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    user_config_path_for(cfg!(windows), env)
}

/// [`user_config_path`] for an explicit platform: `%APPDATA%\paper\config.toml` when `windows`,
/// else `$XDG_CONFIG_HOME/paper/config.toml`, else `~/.config/paper/config.toml`. An empty
/// variable counts as unset. `None` when no base folder is known.
pub fn user_config_path_for(
    _windows: bool,
    _env: &dyn Fn(&str) -> Option<String>,
) -> Option<PathBuf> {
    todo!("PV-005: user config path")
}

/// What can go wrong with settings.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("unknown setting `{key}`")]
    UnknownKey { key: String },
    #[error("`{value}` is not a valid value for `{key}`: {expected}")]
    InvalidValue {
        key: String,
        value: String,
        expected: String,
    },
    #[error("`{key}` can't be set {where_}: {reason}")]
    WrongHome {
        key: String,
        where_: String,
        reason: String,
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

//! The keys paper knows: their names, types, homes and validation.

use std::fmt;

use super::ConfigError;
use crate::profile::Profile;

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

const COLORS: [&str; 3] = ["auto", "always", "never"];

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
        match self {
            Key::UserName => "user.name",
            Key::UserEmail => "user.email",
            Key::UiColor => "ui.color",
            Key::LargeFilesWarnMb => "large_files.warn_mb",
            Key::LargeFilesMaxMb => "large_files.max_mb",
            Key::DiffDefault => "diff.default",
            Key::Profile => "profile",
        }
    }

    /// The environment variable that overrides this key: `PAPER_` plus the dotted name
    /// upper-cased with `.` replaced by `_`.
    pub fn env_name(self) -> String {
        format!("PAPER_{}", self.as_str().to_uppercase().replace('.', "_"))
    }

    /// Which file may hold this key.
    pub fn home(self) -> Home {
        match self {
            Key::UserName | Key::UserEmail | Key::UiColor => Home::UserOnly,
            Key::LargeFilesWarnMb | Key::LargeFilesMaxMb | Key::DiffDefault => Home::Either,
            Key::Profile => Home::ProjectOnly,
        }
    }

    /// A value that would be accepted, for suggestions.
    pub fn example(self) -> &'static str {
        match self {
            Key::UserName => "\"Your Name\"",
            Key::UserEmail => "you@example.org",
            Key::UiColor => "auto",
            Key::LargeFilesWarnMb => "10",
            Key::LargeFilesMaxMb => "100",
            Key::DiffDefault => "lines",
            Key::Profile => "latex",
        }
    }

    /// Position in [`Key::ALL`].
    pub(super) fn index(self) -> usize {
        Key::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    pub(super) fn is_megabytes(self) -> bool {
        matches!(self, Key::LargeFilesWarnMb | Key::LargeFilesMaxMb)
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
    pub fn parse(text: &str) -> Result<Key, ConfigError> {
        Key::ALL
            .into_iter()
            .find(|key| key.as_str() == text)
            .ok_or_else(|| ConfigError::UnknownKey {
                key: text.to_owned(),
            })
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
    pub fn parse_value(self, text: &str) -> Result<Value, ConfigError> {
        let bad = || self.invalid(text, self.expected());
        let trimmed = text.trim();
        let accepted = match self {
            Key::UserName => !trimmed.is_empty(),
            Key::UserEmail => trimmed
                .split_once('@')
                .is_some_and(|(local, domain)| !local.is_empty() && !domain.is_empty()),
            Key::UiColor => COLORS.contains(&trimmed),
            Key::DiffDefault => trimmed == "lines" || trimmed == "words",
            Key::Profile => Profile::builtin(trimmed).is_some(),
            Key::LargeFilesWarnMb | Key::LargeFilesMaxMb => {
                return match trimmed.parse::<u64>() {
                    Ok(mb) if mb >= 1 => Ok(Value::Megabytes(mb)),
                    _ => Err(bad()),
                };
            }
        };
        if accepted {
            Ok(Value::Text(trimmed.to_owned()))
        } else {
            Err(bad())
        }
    }

    /// What this key accepts, in a sentence.
    pub(super) fn expected(self) -> String {
        match self {
            Key::UserName => "a name, which can't be empty".to_owned(),
            Key::UserEmail => "an email address, like you@example.org".to_owned(),
            Key::UiColor => "one of auto, always or never".to_owned(),
            Key::LargeFilesWarnMb => {
                "a whole number of megabytes, 1 or more (files this big are listed in the draft)"
                    .to_owned()
            }
            Key::LargeFilesMaxMb => {
                "a whole number of megabytes, 1 or more (new files bigger than this are left out \
                 of saves), and not below large_files.warn_mb"
                    .to_owned()
            }
            Key::DiffDefault => {
                "`lines` or `words` (`words` has no effect until v0.3; comparisons \
                                 stay line by line)"
                    .to_owned()
            }
            Key::Profile => format!("one of {}", Profile::names().collect::<Vec<_>>().join(", ")),
        }
    }

    pub(super) fn invalid(self, value: &str, expected: String) -> ConfigError {
        ConfigError::InvalidValue {
            key: self.as_str().to_owned(),
            value: value.to_owned(),
            expected,
            origin: None,
            conflict: None,
        }
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
    pub fn check_scope(self, scope: Scope) -> Result<(), ConfigError> {
        let refuse = |where_: &str, reason: &str| {
            Err(ConfigError::WrongHome {
                key: self.as_str().to_owned(),
                where_: where_.to_owned(),
                reason: reason.to_owned(),
            })
        };
        match (self.home(), scope) {
            (Home::UserOnly, Scope::Project) => refuse(
                "in the project file (paper.toml)",
                "it is personal, and paper.toml is saved with the project, which others (or the \
                 public) may see.",
            ),
            (Home::ProjectOnly, Scope::User) => refuse(
                "in your user settings",
                "it describes one project, so it belongs in that project's paper.toml.",
            ),
            _ => Ok(()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Text(text) => f.write_str(text),
            Value::Megabytes(mb) => write!(f, "{mb}"),
        }
    }
}

impl Source {
    /// `env`, `project`, `user`, `git` or `default`.
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Env => "env",
            Source::Project => "project",
            Source::User => "user",
            Source::Git => "git",
            Source::Default => "default",
        }
    }
}

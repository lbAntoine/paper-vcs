//! The effective settings: every key's value and where it came from.

use paper_git::Identity;

use super::{ConfigError, ConfigFile, Home, Key, Source, Value};
use crate::snapshot::SnapshotOptions;

const MB: u64 = 1024 * 1024;

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
    /// `None` only for identity keys and `profile` when nobody set them.
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
    #[allow(clippy::needless_pass_by_value)] // the sources are built for this one call
    pub fn resolve<E: Fn(&str) -> Option<String>>(
        sources: Sources<E>,
    ) -> Result<Self, ConfigError> {
        let entries = Key::ALL
            .into_iter()
            .map(|key| resolve_key(key, &sources))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { entries })
    }

    /// Every key in [`Key::ALL`] order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry for `key`.
    ///
    /// # Panics
    ///
    /// Never: every key has an entry.
    pub fn entry(&self, key: Key) -> &Entry {
        &self.entries[key.index()]
    }

    fn text(&self, key: Key) -> Option<&str> {
        match &self.entry(key).value {
            Some(Value::Text(text)) => Some(text),
            _ => None,
        }
    }

    /// The author for saves, if both name and email are known.
    pub fn identity(&self) -> Option<Identity> {
        Some(Identity {
            name: self.text(Key::UserName)?.to_owned(),
            email: self.text(Key::UserEmail)?.to_owned(),
        })
    }

    /// The large-file thresholds in bytes. The warning threshold never exceeds the skip one.
    pub fn snapshot_options(&self) -> SnapshotOptions {
        let defaults = SnapshotOptions::default();
        let bytes = |key: Key, fallback: u64| match &self.entry(key).value {
            Some(Value::Megabytes(mb)) => mb.saturating_mul(MB),
            _ => fallback,
        };
        let max = bytes(Key::LargeFilesMaxMb, defaults.max_file_bytes);
        let warn = bytes(Key::LargeFilesWarnMb, defaults.warn_file_bytes);
        SnapshotOptions {
            warn_file_bytes: warn.min(max),
            max_file_bytes: max,
        }
    }
}

fn resolve_key<E: Fn(&str) -> Option<String>>(
    key: Key,
    sources: &Sources<E>,
) -> Result<Entry, ConfigError> {
    let found = |value: Value, source: Source| {
        Ok(Entry {
            key,
            value: Some(value),
            source,
        })
    };
    if let Some(text) = (sources.env)(&key.env_name()).filter(|text| !text.is_empty()) {
        return found(key.parse_value(&text)?, Source::Env);
    }
    if key.home() != Home::UserOnly
        && let Some(value) = sources.project.get(key)?
    {
        return found(value, Source::Project);
    }
    if key.home() != Home::ProjectOnly
        && let Some(value) = sources.user.get(key)?
    {
        return found(value, Source::User);
    }
    let git = sources.git_identity.as_ref();
    let from_git = match key {
        Key::UserName => git.map(|identity| identity.name.clone()),
        Key::UserEmail => git.map(|identity| identity.email.clone()),
        _ => None,
    };
    if let Some(text) = from_git {
        return found(Value::Text(text), Source::Git);
    }
    let default = match key {
        Key::UiColor => Some(Value::Text("auto".to_owned())),
        Key::DiffDefault => Some(Value::Text("lines".to_owned())),
        Key::LargeFilesWarnMb => Some(Value::Megabytes(10)),
        Key::LargeFilesMaxMb => Some(Value::Megabytes(100)),
        Key::UserName | Key::UserEmail | Key::Profile => None,
    };
    Ok(Entry {
        key,
        value: default,
        source: Source::Default,
    })
}

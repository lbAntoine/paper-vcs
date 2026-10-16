//! A config file read from disk and edited without losing the user's comments and layout.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use toml_edit::{DocumentMut, Item, Table};

use super::{ConfigError, Key, Value};

/// A config file (`paper.toml` or the user `config.toml`), kept as TOML text with its comments.
#[derive(Debug, Clone, Default)]
pub struct ConfigFile {
    doc: DocumentMut,
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
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Self::empty()),
            Err(source) => {
                return Err(ConfigError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let doc = text.parse::<DocumentMut>().map_err(|err| {
            let offset = err.span().map_or(0, |span| span.start).min(text.len());
            let line = text.as_bytes()[..offset]
                .split(|byte| *byte == b'\n')
                .count();
            ConfigError::Malformed {
                path: path.to_owned(),
                line,
                message: err.message().to_owned(),
            }
        })?;
        Ok(Self { doc })
    }

    fn item(&self, key: Key) -> Option<&Item> {
        match key.as_str().split_once('.') {
            Some((table, name)) => self.doc.get(table)?.get(name),
            None => self.doc.get(key.as_str()),
        }
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
    pub fn get(&self, key: Key) -> Result<Option<Value>, ConfigError> {
        let Some(item) = self.item(key) else {
            return Ok(None);
        };
        let shown = item.to_string().trim().to_owned();
        let typed = if key.is_megabytes() {
            item.as_integer().and_then(|n| u64::try_from(n).ok())
        } else {
            None
        };
        if key.is_megabytes() {
            return match typed {
                Some(mb) if mb >= 1 => Ok(Some(Value::Megabytes(mb))),
                _ => Err(key.invalid(&shown, key.expected())),
            };
        }
        match item.as_str() {
            Some(text) => key.parse_value(text).map(Some),
            None => Err(key.invalid(&shown, format!("text; {}", key.expected()))),
        }
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
    #[allow(clippy::needless_pass_by_value)] // values are small and built for the call
    pub fn with(&self, key: Key, value: Value) -> Result<Self, ConfigError> {
        let value = match (&value, key.is_megabytes()) {
            (Value::Text(text), false) => key.parse_value(text)?,
            (Value::Megabytes(mb), true) => key.parse_value(&mb.to_string())?,
            _ => return Err(key.invalid(&value.to_string(), key.expected())),
        };
        self.check_order(key, &value)?;
        let toml_value = match &value {
            Value::Text(text) => toml_edit::value(text.clone()),
            Value::Megabytes(mb) => toml_edit::value(i64::try_from(*mb).unwrap_or(i64::MAX)),
        };
        let mut doc = self.doc.clone();
        match key.as_str().split_once('.') {
            Some((table_name, _)) if !doc.contains_key(table_name) => {
                return self.with_new_table(key, &toml_value);
            }
            Some((table_name, name)) => {
                let Some(table) = doc[table_name].as_table_mut() else {
                    return Err(key.invalid(table_name, format!("`{table_name}` must be a table")));
                };
                set_keeping_decor(table, name, toml_value);
            }
            None => set_keeping_decor(doc.as_table_mut(), key.as_str(), toml_value),
        }
        Ok(Self { doc })
    }

    /// Appends `[table]` with the key after the existing text, which stays byte for byte
    /// (editing the parsed document would move the file's trailing blank lines).
    fn with_new_table(&self, key: Key, value: &Item) -> Result<Self, ConfigError> {
        let (table, name) = key.as_str().split_once('.').unwrap_or(("", key.as_str()));
        let mut text = self.render();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if !text.is_empty() && !text.ends_with("\n\n") {
            text.push('\n');
        }
        let _ = writeln!(text, "[{table}]\n{name} = {}", value.to_string().trim());
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|err| key.invalid(&value.to_string(), err.message().to_owned()))?;
        Ok(Self { doc })
    }

    /// `large_files.max_mb` must not be below `large_files.warn_mb` in the same file.
    fn check_order(&self, key: Key, value: &Value) -> Result<(), ConfigError> {
        let Value::Megabytes(new) = value else {
            return Ok(());
        };
        let stored = |k: Key| match self.get(k) {
            Ok(Some(Value::Megabytes(mb))) => Some(mb),
            _ => None,
        };
        match key {
            Key::LargeFilesMaxMb => match stored(Key::LargeFilesWarnMb) {
                Some(warn) if *new < warn => Err(key.invalid(
                    &new.to_string(),
                    format!("it can't be below large_files.warn_mb, which is {warn} here"),
                )),
                _ => Ok(()),
            },
            Key::LargeFilesWarnMb => match stored(Key::LargeFilesMaxMb) {
                Some(max) if *new > max => Err(key.invalid(
                    &new.to_string(),
                    format!("it can't be above large_files.max_mb, which is {max} here"),
                )),
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }

    /// The file's text.
    pub fn render(&self) -> String {
        self.doc.to_string()
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
    pub fn write(&self, path: &Path) -> Result<(), ConfigError> {
        let io_error = |source| ConfigError::Io {
            path: path.to_owned(),
            source,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::write(path, self.render()).map_err(io_error)
    }
}

/// Sets `table[name]`, keeping the comment and spacing around an existing value.
fn set_keeping_decor(table: &mut Table, name: &str, mut new: Item) {
    if let (Some(old), Some(new_value)) =
        (table.get(name).and_then(Item::as_value), new.as_value_mut())
    {
        *new_value.decor_mut() = old.decor().clone();
    }
    table.insert(name, new);
}

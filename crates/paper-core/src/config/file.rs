//! A config file read from disk and edited without losing the user's comments and layout.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item};

use super::{ConfigError, Conflict, Key, Origin, Value};
use crate::fs_util::write_atomic;

/// A config file (`paper.toml` or the user `config.toml`), kept as TOML text with its comments.
#[derive(Debug, Clone, Default)]
pub struct ConfigFile {
    doc: DocumentMut,
    /// Where it was loaded from, to name it in errors.
    path: Option<PathBuf>,
    /// The file's first line ends in CRLF; lines we add do too. Existing lines are never touched.
    crlf: bool,
    /// The text as read, line endings included (`doc` holds it with `\n` endings only).
    raw: String,
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
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(Self {
                    path: Some(path.to_owned()),
                    ..Self::empty()
                });
            }
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
        let crlf = first_line_is_crlf(&text);
        let raw = consistent_raw(text, &doc, crlf);
        Ok(Self {
            doc,
            path: Some(path.to_owned()),
            crlf,
            raw,
        })
    }

    fn item(&self, key: Key) -> Option<&Item> {
        match key.as_str().split_once('.') {
            Some((table, name)) => self.doc.get(table)?.get(name),
            None => self.doc.get(key.as_str()),
        }
    }

    /// Whether the file has anything under `key`, valid or not.
    pub fn has(&self, key: Key) -> bool {
        self.item(key).is_some()
    }

    fn origin(&self) -> Option<Origin> {
        self.path.clone().map(Origin::File)
    }

    /// The value stored for `key`, if any.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] (naming the file) if the file holds the wrong type or an
    /// unusable value.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn get(&self, key: Key) -> Result<Option<Value>, ConfigError> {
        let Some(item) = self.item(key) else {
            return Ok(None);
        };
        let shown = item.to_string().trim().to_owned();
        let result = if key.is_megabytes() {
            match item.as_integer().and_then(|n| u64::try_from(n).ok()) {
                Some(mb) if mb >= 1 => Ok(Value::Megabytes(mb)),
                _ => Err(key.invalid(&shown, key.expected())),
            }
        } else {
            match item.as_str() {
                Some(text) => key.parse_value(text),
                None => Err(key.invalid(&shown, format!("text; {}", key.expected()))),
            }
        };
        match (result, self.origin()) {
            (Err(error), Some(origin)) => Err(error.with_origin(origin)),
            (result, _) => result.map(Some),
        }
    }

    /// A copy with `key` set. Comments, layout and unknown tables are kept.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] if `value` has the wrong type for `key`, or
    /// `large_files.max_mb` would be below `large_files.warn_mb` as written in this file;
    /// [`ConfigError::Blocked`] if the table is a plain value.
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
                let Some(table) = doc[table_name].as_table_like_mut() else {
                    return Err(ConfigError::Blocked {
                        path: self.path.clone(),
                        table: table_name.to_owned(),
                        key: key.as_str().to_owned(),
                    });
                };
                set_keeping_decor(table, name, toml_value);
            }
            None => set_keeping_decor(doc.as_table_mut(), key.as_str(), toml_value),
        }
        Ok(self.with_doc(&doc))
    }

    /// The file with `doc` as its document. Existing lines keep their own ending byte for
    /// byte; a changed line keeps the ending of the line it replaces; extra lines get the
    /// ending of the file's first line.
    fn with_doc(&self, doc: &DocumentMut) -> Self {
        let old_lf = self.doc.to_string();
        let new_lf = doc.to_string();
        let raw_lines: Vec<&str> = self.raw.split_inclusive('\n').collect();
        let old_lines: Vec<&str> = old_lf.split_inclusive('\n').collect();
        let new_lines: Vec<&str> = new_lf.split_inclusive('\n').collect();
        let head = old_lines
            .iter()
            .zip(&new_lines)
            .take_while(|(old, new)| old == new)
            .count();
        let tail = old_lines[head..]
            .iter()
            .rev()
            .zip(new_lines[head..].iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        let mut raw = raw_lines[..head].concat();
        let replaced = &raw_lines[head..old_lines.len() - tail];
        for (at, line) in new_lines[head..new_lines.len() - tail].iter().enumerate() {
            let crlf = replaced
                .get(at)
                .map_or(self.crlf, |old| old.ends_with("\r\n"));
            raw.push_str(&with_ending(line, crlf));
        }
        raw.push_str(&raw_lines[old_lines.len() - tail..].concat());
        Self {
            doc: doc.clone(),
            raw: consistent_raw(raw, doc, self.crlf),
            ..self.clone()
        }
    }

    /// Appends `[table]` with the key after the existing text, which stays byte for byte
    /// (editing the parsed document would move the file's trailing blank lines).
    fn with_new_table(&self, key: Key, value: &Item) -> Result<Self, ConfigError> {
        let (table, name) = key.as_str().split_once('.').unwrap_or(("", key.as_str()));
        let mut text = self.render();
        let eol = if self.crlf { "\r\n" } else { "\n" };
        if !text.is_empty() && !text.ends_with('\n') {
            text.push_str(eol);
        }
        if !text.is_empty() && !text.ends_with(&format!("{eol}{eol}")) {
            text.push_str(eol);
        }
        let _ = write!(
            text,
            "[{table}]{eol}{name} = {}{eol}",
            value.to_string().trim()
        );
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|err| key.invalid(&value.to_string(), err.message().to_owned()))?;
        let raw = consistent_raw(text, &doc, self.crlf);
        Ok(Self {
            doc,
            raw,
            ..self.clone()
        })
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
        let (other, clashes) = match key {
            Key::LargeFilesMaxMb => (Key::LargeFilesWarnMb, below as fn(u64, u64) -> bool),
            Key::LargeFilesWarnMb => (Key::LargeFilesMaxMb, above as fn(u64, u64) -> bool),
            _ => return Ok(()),
        };
        match stored(other) {
            Some(other_value) if clashes(*new, other_value) => {
                let relation = if key == Key::LargeFilesMaxMb {
                    "below"
                } else {
                    "above"
                };
                Err(ConfigError::InvalidValue {
                    key: key.as_str().to_owned(),
                    value: new.to_string(),
                    expected: format!("it can't be {relation} {} ({other_value})", other.as_str()),
                    origin: self.origin(),
                    conflict: Some(Conflict { other, other_value }),
                })
            }
            _ => Ok(()),
        }
    }

    /// The file's text.
    pub fn render(&self) -> String {
        self.raw.clone()
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
        write_atomic(path, self.render().as_bytes()).map_err(io_error)
    }
}

/// Sets `table[name]`, keeping the comment and spacing around an existing value.
fn set_keeping_decor(table: &mut dyn toml_edit::TableLike, name: &str, mut new: Item) {
    if let (Some(old), Some(new_value)) =
        (table.get(name).and_then(Item::as_value), new.as_value_mut())
    {
        *new_value.decor_mut() = old.decor().clone();
    }
    // Replace the item in place: re-inserting the key would drop the comment above it.
    match table.get_mut(name) {
        Some(item) => *item = new,
        None => {
            table.insert(name, new);
        }
    }
}

fn first_line_is_crlf(text: &str) -> bool {
    text.find('\n')
        .is_some_and(|end| end > 0 && text.as_bytes()[end - 1] == b'\r')
}

/// `raw` if it is the document's text apart from CRLF endings; otherwise the document's text
/// with the file's ending. `toml_edit` ends the last line with a newline even when the file
/// did not, so a file without a final newline gets one here (the same ending as the file's
/// first line) rather than letting the next appended key land on the same line.
fn consistent_raw(raw: String, doc: &DocumentMut, crlf: bool) -> String {
    // `toml_edit` keeps the bytes of multi-line strings, so compare with CRLF normalised.
    let lf = doc.to_string().replace("\r\n", "\n");
    if raw.replace("\r\n", "\n") == lf {
        return raw;
    }
    let mut fixed = raw;
    if !fixed.is_empty() && !fixed.ends_with('\n') {
        fixed.push_str(if crlf { "\r\n" } else { "\n" });
    }
    if fixed.replace("\r\n", "\n") != lf {
        // Safe fallback: rebuild from the document, losing only mixed line endings.
        fixed = lf
            .split_inclusive('\n')
            .map(|l| with_ending(l, crlf))
            .collect();
    }
    debug_assert_eq!(fixed.replace("\r\n", "\n"), lf);
    fixed
}

/// `line` (which ends in `\n`, or has no ending) with a CRLF or LF ending.
fn with_ending(line: &str, crlf: bool) -> String {
    match line.strip_suffix('\n') {
        Some(body) => format!(
            "{}{}",
            body.trim_end_matches('\r'),
            if crlf { "\r\n" } else { "\n" }
        ),
        None => line.to_owned(),
    }
}

fn below(new: u64, other: u64) -> bool {
    new < other
}

fn above(new: u64, other: u64) -> bool {
    new > other
}

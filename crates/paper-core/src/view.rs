//! The view: every piece of state that `undo` must cover
//!
//! Every operation in the activity log holds a full view. Format 1 (v0.1) kept a single view
//! in `.git/paper/view.json`; [`View::load`] reads it only to migrate a project to format 2.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use paper_git::CommitId;
use serde::{Deserialize, Serialize};

use crate::name::LineName;

const FILE_NAME: &str = "view.json";

/// A line of work: its last save and its draft
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub tip: CommitId,
    pub draft: CommitId,
}

/// Invariant: `current_line` always has an entry in `lines`
/// Enforced by the constructors and when loading from disk
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawView")]
pub struct View {
    current_line: LineName,
    lines: BTreeMap<LineName, Line>,
}

/// What's on disk, before the invariant is checked
#[derive(Deserialize)]
struct RawView {
    current_line: LineName,
    lines: BTreeMap<LineName, Line>,
}

#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    #[error("could not read or write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is damaged")]
    Corrupt {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl TryFrom<RawView> for View {
    type Error = String;

    fn try_from(raw: RawView) -> Result<Self, Self::Error> {
        if !raw.lines.contains_key(&raw.current_line) {
            return Err(format!("current line '{}' has no entry", raw.current_line));
        }
        Ok(Self {
            current_line: raw.current_line,
            lines: raw.lines,
        })
    }
}

impl View {
    /// A view with a single line, which is the current one
    pub fn new(name: LineName, line: Line) -> Self {
        Self {
            lines: BTreeMap::from([(name.clone(), line)]),
            current_line: name,
        }
    }

    pub fn current_line(&self) -> &LineName {
        &self.current_line
    }

    pub fn current(&self) -> &Line {
        &self.lines[&self.current_line]
    }

    pub fn line(&self, name: &LineName) -> Option<&Line> {
        self.lines.get(name)
    }

    pub fn lines(&self) -> impl Iterator<Item = (&LineName, &Line)> {
        self.lines.iter()
    }

    /// A copy of this view with `name` added or replaced. `self` is left untouched
    #[must_use]
    pub fn with_line(&self, name: LineName, line: Line) -> Self {
        let mut lines = self.lines.clone();
        lines.insert(name, line);
        Self {
            current_line: self.current_line.clone(),
            lines,
        }
    }

    /// Reads `view.json` from `paper_dir`. `Ok(None)` means there is no view yet
    ///
    /// # Errors
    ///
    /// [`ViewError::Io`] if the file exists but can't be read and
    /// [`ViewError::Corrupt`] if it isn't a valid view
    pub fn load(paper_dir: &Path) -> Result<Option<Self>, ViewError> {
        let path = paper_dir.join(FILE_NAME);
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|source| ViewError::Corrupt { path, source }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(ViewError::Io { path, source }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(c: char) -> CommitId {
        c.to_string().repeat(40).parse().unwrap()
    }

    fn line(tip: char, draft: char) -> Line {
        Line {
            tip: commit(tip),
            draft: commit(draft),
        }
    }

    fn name(s: &str) -> LineName {
        s.parse().unwrap()
    }

    #[test]
    fn new_view_has_one_current_line() {
        let view = View::new(LineName::main(), line('a', 'b'));
        assert_eq!(view.current_line(), &LineName::main());
        assert_eq!(view.current(), &line('a', 'b'));
        assert_eq!(view.lines().count(), 1);
    }

    #[test]
    fn with_line_leaves_the_original_untouched() {
        let before = View::new(LineName::main(), line('a', 'b'));
        let after = before.with_line(name("try/intro"), line('c', 'd'));
        assert_eq!(before.lines().count(), 1);
        assert_eq!(after.lines().count(), 2);
        assert_eq!(after.line(&name("try/intro")), Some(&line('c', 'd')));
        assert_eq!(after.current_line(), &LineName::main());
    }

    #[test]
    fn json_format_is_stable() {
        let view = View::new(LineName::main(), line('a', 'b'));
        let expected = format!(
            r#"{{
  "current_line": "main",
  "lines": {{
    "main": {{
      "tip": "{}",
      "draft": "{}"
    }}
  }}
}}"#,
            "a".repeat(40),
            "b".repeat(40)
        );
        assert_eq!(serde_json::to_string_pretty(&view).unwrap(), expected);
        assert_eq!(serde_json::from_str::<View>(&expected).unwrap(), view);
    }

    #[test]
    fn rejects_a_current_line_without_an_entry() {
        let json = format!(
            r#"{{"current_line": "ghost", "lines": {{"main": {{"tip": "{0}", "draft": "{0}"}}}}}}"#,
            "a".repeat(40)
        );
        let err = serde_json::from_str::<View>(&json).unwrap_err();
        assert!(err.to_string().contains("ghost"), "{err}");
    }

    #[test]
    fn loading_a_missing_view_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(View::load(dir.path()).unwrap(), None);
    }

    #[test]
    fn loading_garbage_is_reported_as_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "{ not json").unwrap();
        assert!(matches!(
            View::load(dir.path()),
            Err(ViewError::Corrupt { .. })
        ));
    }
}

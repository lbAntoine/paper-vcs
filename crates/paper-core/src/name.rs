//! Names of the milestones

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Words that mean something else in a point , so they can't name a line
const RESERVED: [&str; 2] = ["last", "HEAD"];

/// The name of a line, e.g. `main` or `try/restructure-intro`
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LineName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{name}' can't be used as a name: {reason}")]
pub struct InvalidNameError {
    pub name: String,
    pub reason: &'static str,
}

impl LineName {
    /// The line every project starts with
    pub fn main() -> Self {
        Self("main".to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for LineName {
    type Err = InvalidNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = |reason| InvalidNameError {
            name: s.to_owned(),
            reason,
        };
        if RESERVED.contains(&s) {
            return Err(invalid("this word is reserved"));
        }
        check(s).map_err(invalid)?;
        Ok(Self(s.to_owned()))
    }
}

impl TryFrom<String> for LineName {
    type Error = InvalidNameError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<LineName> for String {
    fn from(name: LineName) -> Self {
        name.0
    }
}

impl fmt::Display for LineName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Line and milestone names follow a readable subset of git's ref-name rules
/// Any unicode letter or digit is allowed, so `논문` or `thèse` are fine
///
/// Returns the reason when the name is invalid
pub(crate) fn check(name: &str) -> Result<(), &'static str> {
    let allowed = |c: char| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/');
    if name.is_empty() {
        return Err("it can't be empty");
    }
    if !name.chars().all(allowed) {
        return Err("only letters, digits, '-', '_', '.' and '/' are allowed");
    }
    if name.starts_with('-') {
        return Err("it can't start with '-'");
    }
    if name.contains("..") {
        return Err("it can't contain '..'");
    }
    for part in name.split('/') {
        if part.is_empty() {
            return Err("'/' must separate non-empty parts");
        }
        let lock_suffix = Path::new(part)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"));
        if part.starts_with('.') || lock_suffix {
            return Err("parts can't start with '.' or end with '.lock'");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_line_names() {
        for name in ["main", "try/intro", "journal-version", "논문"] {
            assert_eq!(name.parse::<LineName>().unwrap().as_str(), name);
        }
    }

    #[test]
    fn rejects_reserved_words() {
        for reserved in ["last", "HEAD"] {
            let err = reserved.parse::<LineName>().unwrap_err();
            assert_eq!(err.reason, "this word is reserved");
        }
    }

    #[test]
    fn rejects_invalid_names() {
        for bad in ["", "a b", "-x", "try/", "main.lock"] {
            assert!(
                bad.parse::<LineName>().is_err(),
                "{bad:?} should be rejected"
            );
        }
    }
}

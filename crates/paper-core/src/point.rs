//! Points: the user-facing way to name a state
//!
//! This module only parses the syntax. Turning a point into a commit needs the
//! repository and happens at resolution time.

use std::path::Path;
use std::str::FromStr;

use jiff::civil::{Date, DateTime};
use jiff::fmt::strtime;

/// A parsed point, not yet resolved against a repository
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Point {
    /// `@`: the current draft
    Draft,
    /// `last` or `last-N`: the last save, or N saves before it (first parent)
    Last { back: u32 },
    /// `date:...`: the last save on the current line at or before this moment
    Date(DateSpec),
    /// A line, milestone, change-ID prefix or commit-hash prefix
    /// Which one is decided at resolution time, by precedence
    Name(String),
}

/// How precise a `date`: point was. Kept as written so messages can echo it back
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateSpec {
    /// `date:2026-09-01`
    Day(Date),
    /// `date:2026-09-01T14:30`
    Minute(DateTime),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParsePointError {
    #[error("a point can't be empty")]
    Empty,
    #[error("'{0}' is not a number of saves to go back (expected e.g. last~2)")]
    BadAncestorCount(String),
    #[error("'{0}' is not a date (expected date:YYYY-MM-DD or date:YYYY-MM-DDTHH:MM")]
    BadDate(String),
    #[error("'{0}' is not a valid name: {1}")]
    InvalidName(String, &'static str),
    #[error("'{given}' is git/jj syntax; in paper write '{suggestion}'")]
    ForeignSyntax { given: String, suggestion: String },
}

impl std::fmt::Display for DateSpec {
    /// The spelling a researcher types: `2026-09-01` or `2026-09-01T14:30`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Day(day) => write!(f, "{day}"),
            Self::Minute(m) => write!(f, "{}T{:02}:{:02}", m.date(), m.hour(), m.minute()),
        }
    }
}

impl std::fmt::Display for Point {
    /// The spelling a researcher types, so messages can echo a point back.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => f.write_str("@"),
            Self::Last { back: 0 } => f.write_str("last"),
            Self::Last { back } => write!(f, "last~{back}"),
            Self::Date(spec) => write!(f, "date:{spec}"),
            Self::Name(name) => f.write_str(name),
        }
    }
}

impl DateSpec {
    /// The Last instant the spec covers (inclusive). `date:2026-09-01` covers the whole day
    pub fn inclusive_end(self) -> DateTime {
        match self {
            Self::Day(day) => day.at(23, 59, 59, 999_999_999),
            Self::Minute(minute) => {
                minute
                    .date()
                    .at(minute.hour(), minute.minute(), 59, 999_999_999)
            }
        }
    }
}

impl FromStr for Point {
    type Err = ParsePointError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(suggestion) = foreign_syntax_suggestion(s) {
            return Err(ParsePointError::ForeignSyntax {
                given: s.to_owned(),
                suggestion,
            });
        }
        match s {
            "" => Err(ParsePointError::Empty),
            "@" => Ok(Self::Draft),
            "last" => Ok(Self::Last { back: 0 }),
            _ => {
                if let Some(count) = s.strip_prefix("last~") {
                    parse_back(count).map(|back| Self::Last { back })
                } else if let Some(when) = s.strip_prefix("date:") {
                    parse_date(when).map(Self::Date)
                } else {
                    validate_name(s).map(|()| Self::Name(s.to_owned()))
                }
            }
        }
    }
}

/// Git (`HEAD`, `HEAD~N`) and jj (`@-`) spellings we recognise, with the paper equivalent
fn foreign_syntax_suggestion(s: &str) -> Option<String> {
    match s {
        "HEAD" | "@-" => Some("last".to_owned()),
        _ => s.strip_prefix("HEAD~").map(|n| format!("last~{n}")),
    }
}

fn parse_back(count: &str) -> Result<u32, ParsePointError> {
    let bad = || ParsePointError::BadAncestorCount(count.to_owned());
    // u32::from_str would accept "+1"; we only want plain digits
    if count.is_empty() || !count.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    count.parse().map_err(|_| bad())
}

fn parse_date(when: &str) -> Result<DateSpec, ParsePointError> {
    let bad = |_| ParsePointError::BadDate(when.to_owned());
    if when.contains('T') {
        strtime::parse("%Y-%m-%dT%H:%M", when)
            .and_then(|parsed| parsed.to_datetime())
            .map(DateSpec::Minute)
            .map_err(bad)
    } else {
        strtime::parse("%Y-%m-%d", when)
            .and_then(|parsed| parsed.to_date())
            .map(DateSpec::Day)
            .map_err(bad)
    }
}

/// Line and milestone names follow a readable subset of gits ref-name rules
/// Any unicode letter or digit is allowed, so `논문` or `thèse` are fine
fn validate_name(name: &str) -> Result<(), ParsePointError> {
    let invalid = |reason| Err(ParsePointError::InvalidName(name.to_owned(), reason));
    let allowed = |c: char| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/');
    if !name.chars().all(allowed) {
        return invalid("only letters, digits, '-', '_', '.' and '/' are allowed");
    }
    if name.starts_with('-') {
        return invalid("it can't start with '-'");
    }
    if name.contains("..") {
        return invalid("it can't contain '..'");
    }
    for part in name.split('/') {
        if part.is_empty() {
            return invalid("'/' must separate non-empty parts");
        }
        let lock_suffix = Path::new(part)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"));
        if part.starts_with('.') || lock_suffix {
            return invalid("parts can't start with '.' or end with '.lock'");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::{date, datetime};

    #[test]
    fn every_point_parses_back_from_its_display() {
        let points = [
            Point::Draft,
            Point::Last { back: 0 },
            Point::Last { back: 3 },
            Point::Date(DateSpec::Day(date(2026, 9, 1))),
            Point::Date(DateSpec::Minute(datetime(2026, 9, 1, 14, 30, 0, 0))),
            Point::Name("try/intro".to_owned()),
            Point::Name("thèse".to_owned()),
        ];
        for point in points {
            assert_eq!(point.to_string().parse::<Point>().unwrap(), point);
        }
        // Paper's spelling: `last~0` is recorded as `last`.
        assert_eq!("last~0".parse::<Point>().unwrap().to_string(), "last");
    }

    fn parse(s: &str) -> Result<Point, ParsePointError> {
        s.parse()
    }

    #[test]
    fn at_sign_is_the_draft() {
        assert_eq!(parse("@"), Ok(Point::Draft));
    }

    #[test]
    fn last_with_optional_ancestor_count() {
        assert_eq!(parse("last"), Ok(Point::Last { back: 0 }));
        assert_eq!(parse("last~0"), Ok(Point::Last { back: 0 }));
        assert_eq!(parse("last~2"), Ok(Point::Last { back: 2 }));
    }

    #[test]
    fn rejects_bad_ancestor_counts() {
        for bad in ["last~", "last~x", "last~-1", "last~1.5"] {
            assert!(
                matches!(parse(bad), Err(ParsePointError::BadAncestorCount(_))),
                "{bad} should be a BadAncestorCount"
            );
        }
    }

    #[test]
    fn day_date_covers_the_whole_day() {
        let point = parse("date:2026-09-01").unwrap();
        assert_eq!(point, Point::Date(DateSpec::Day(date(2026, 9, 1))));
        let Point::Date(spec) = point else {
            unreachable!()
        };
        assert_eq!(
            spec.inclusive_end(),
            datetime(2026, 9, 1, 23, 59, 59, 999_999_999)
        );
    }

    #[test]
    fn rejects_bad_dates() {
        for bad in [
            "date:",
            "date:2026-13-01",
            "date:yesterday",
            "date:2026-09-01T25:00",
            "date:2026-09-01T14:30:15",
        ] {
            assert!(
                matches!(parse(bad), Err(ParsePointError::BadDate(_))),
                "{bad} should be a BadDate"
            );
        }
    }

    #[test]
    fn accepts_names() {
        for name in [
            "main",
            "try/intro",
            "neurips-submission",
            "v1.2",
            "kmrw",
            "3fa9c1",
            "논문",
            "draft_2",
        ] {
            assert_eq!(parse(name), Ok(Point::Name(name.to_owned())), "{name}");
        }
    }

    #[test]
    fn rejects_invalid_names() {
        for bad in [
            "-x",
            ".hidden",
            "a..b",
            "a b",
            "try/",
            "a//b",
            "main.lock",
            "main.LOCK",
            "what?",
        ] {
            assert!(
                matches!(parse(bad), Err(ParsePointError::InvalidName(..))),
                "{bad} should be an InvalidName"
            );
        }
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(parse(""), Err(ParsePointError::Empty));
    }

    #[test]
    fn translates_git_and_jj_syntax() {
        for (given, suggestion) in [("HEAD", "last"), ("HEAD~3", "last~3"), ("@-", "last")] {
            assert_eq!(
                parse(given),
                Err(ParsePointError::ForeignSyntax {
                    given: given.to_owned(),
                    suggestion: suggestion.to_owned()
                })
            );
        }
    }

    #[test]
    fn minute_date_covers_the_whole_minute() {
        let point = parse("date:2026-09-01T14:30").unwrap();
        assert_eq!(
            point,
            Point::Date(DateSpec::Minute(datetime(2026, 9, 1, 14, 30, 0, 0)))
        );
        let Point::Date(spec) = point else {
            unreachable!()
        };
        assert_eq!(
            spec.inclusive_end(),
            datetime(2026, 9, 1, 14, 30, 59, 999_999_999)
        );
    }
}

//! The error shape front-ends render: what happened, why, and what to run next.
//!
//! Every core error enum converts into a [`PaperError`]. The three parts are written here, per
//! variant, in researcher vocabulary. See `03-architecture.md` § Errors.

use std::path::Path;

use crate::project::{HistoryError, InitError, OpenError, ResolveError, SaveError, ShowError};

/// Who has to act, which decides the CLI exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The request can't be done as asked (bad arguments, unknown point). Exit code 1.
    User,
    /// Something needs the researcher's decision (conflicts, diverged remote). Exit code 2.
    NeedsAttention,
    /// Paper's own state or the machine is at fault (corruption, I/O, git). Exit code 3.
    Project,
}

/// A command the researcher can run next, with what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub command: String,
    pub explanation: String,
}

/// An error in three parts: `message` (what happened), `why`, and `next` (what to run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperError {
    pub kind: ErrorKind,
    pub message: String,
    pub why: String,
    pub next: Vec<Suggestion>,
}

impl PaperError {
    /// No `user.name` / `user.email` in git config; the suggestions are the two
    /// `git config --global user.…` commands.
    pub fn no_identity() -> Self {
        todo!("PV-003")
    }

    /// `path` isn't a paper project. `is_git_repo` says whether a plain git repository was
    /// found there (adopt it with `paper init`) or nothing at all.
    pub fn not_a_project(_path: &Path, _is_git_repo: bool) -> Self {
        todo!("PV-003")
    }
}

impl From<InitError> for PaperError {
    fn from(_error: InitError) -> Self {
        todo!("PV-003")
    }
}

impl From<OpenError> for PaperError {
    fn from(_error: OpenError) -> Self {
        todo!("PV-003")
    }
}

impl From<SaveError> for PaperError {
    fn from(_error: SaveError) -> Self {
        todo!("PV-003")
    }
}

impl From<ResolveError> for PaperError {
    fn from(_error: ResolveError) -> Self {
        todo!("PV-003")
    }
}

impl From<HistoryError> for PaperError {
    fn from(_error: HistoryError) -> Self {
        todo!("PV-003")
    }
}

impl From<ShowError> for PaperError {
    fn from(_error: ShowError) -> Self {
        todo!("PV-003")
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use paper_git::GitError;

    use super::*;
    use crate::point::Point;
    use crate::project::Candidate;
    use crate::snapshot::SnapshotError;
    use crate::view::ViewError;

    fn path() -> PathBuf {
        PathBuf::from("/work/paper")
    }

    fn git_error() -> GitError {
        GitError::Bare { path: path() }
    }

    fn io_error() -> io::Error {
        io::Error::other("disk exploded")
    }

    fn line(name: &str) -> crate::name::LineName {
        name.parse().unwrap()
    }

    fn date_spec() -> crate::point::DateSpec {
        match "date:2026-09-01".parse::<Point>().unwrap() {
            Point::Date(spec) => spec,
            other => panic!("not a date point: {other:?}"),
        }
    }

    fn assert_user(error: &PaperError) {
        assert_eq!(error.kind, ErrorKind::User, "{error:?}");
        assert!(
            !error.next.is_empty(),
            "a user error must say what to run next: {error:?}"
        );
        assert_filled(error);
    }

    fn assert_project(error: &PaperError) {
        assert_eq!(error.kind, ErrorKind::Project, "{error:?}");
        assert_filled(error);
    }

    fn assert_filled(error: &PaperError) {
        assert!(!error.message.trim().is_empty(), "{error:?}");
        assert!(!error.why.trim().is_empty(), "{error:?}");
        for suggestion in &error.next {
            assert!(!suggestion.command.trim().is_empty(), "{error:?}");
            assert!(!suggestion.explanation.trim().is_empty(), "{error:?}");
        }
    }

    #[test]
    fn every_init_error_converts_with_the_right_kind() {
        let user = [
            InitError::AlreadyInitialized { path: path() },
            InitError::InsideAnotherRepository {
                path: path(),
                root: PathBuf::from("/work"),
            },
            InitError::DetachedHead,
            InitError::UnusableBranchName {
                name: "a..b".to_owned(),
            },
            InitError::EmptyCurrentBranch {
                name: "main".to_owned(),
            },
        ];
        for error in user {
            assert_user(&PaperError::from(error));
        }
        assert_project(&PaperError::from(InitError::Git(git_error())));
        assert_project(&PaperError::from(InitError::Snapshot(SnapshotError::Git(
            git_error(),
        ))));
        assert_project(&PaperError::from(InitError::View(ViewError::Io {
            path: path(),
            source: io_error(),
        })));
        assert_project(&PaperError::from(InitError::Io {
            path: path(),
            source: io_error(),
        }));
    }

    #[test]
    fn a_detached_head_error_suggests_git_switch() {
        let error = PaperError::from(InitError::DetachedHead);
        assert!(
            error.next.iter().any(|s| s.command.contains("git switch")),
            "{error:?}"
        );
    }

    #[test]
    fn every_open_error_converts_with_the_right_kind() {
        assert_user(&PaperError::from(OpenError::NotAPaperProject {
            path: path(),
        }));
        assert_project(&PaperError::from(OpenError::Git(git_error())));
        assert_project(&PaperError::from(OpenError::UnsupportedFormat {
            found: "7".to_owned(),
        }));
        assert_project(&PaperError::from(OpenError::Incomplete { path: path() }));
        assert_project(&PaperError::from(OpenError::View(ViewError::Io {
            path: path(),
            source: io_error(),
        })));
        assert_project(&PaperError::from(OpenError::Io {
            path: path(),
            source: io_error(),
        }));
    }

    #[test]
    fn every_save_error_converts_with_the_right_kind() {
        let user = [
            SaveError::MissingMessage,
            SaveError::NoChangesAt {
                paths: vec!["a.tex".to_owned(), "b.tex".to_owned()],
            },
            SaveError::OutOfSyncWithGit { line: line("main") },
            SaveError::HeadOnAnotherBranch {
                line: line("main"),
                found: Some("other".to_owned()),
            },
            SaveError::HeadOnAnotherBranch {
                line: line("main"),
                found: None,
            },
        ];
        for error in user {
            assert_user(&PaperError::from(error));
        }
        assert_project(&PaperError::from(SaveError::Git(git_error())));
        assert_project(&PaperError::from(SaveError::Snapshot(SnapshotError::Io {
            path: path(),
            source: io_error(),
        })));
        assert_project(&PaperError::from(SaveError::View(ViewError::Io {
            path: path(),
            source: io_error(),
        })));
        assert_project(&PaperError::from(SaveError::Io {
            path: path(),
            source: io_error(),
        }));
    }

    #[test]
    fn a_save_with_no_changes_at_paths_lists_every_path() {
        let error = PaperError::from(SaveError::NoChangesAt {
            paths: vec!["a.tex".to_owned(), "b.tex".to_owned()],
        });
        let text = format!("{} {}", error.message, error.why);
        assert!(
            text.contains("a.tex") && text.contains("b.tex"),
            "{error:?}"
        );
    }

    #[test]
    fn a_missing_message_suggests_a_save_with_a_message() {
        let error = PaperError::from(SaveError::MissingMessage);
        assert!(
            error
                .next
                .iter()
                .any(|s| s.command.starts_with("paper save")),
            "{error:?}"
        );
    }

    #[test]
    fn an_out_of_sync_line_never_suggests_a_command_that_drops_commits() {
        let error = PaperError::from(SaveError::OutOfSyncWithGit { line: line("main") });
        for suggestion in &error.next {
            assert!(!suggestion.command.contains("reset"), "{error:?}");
        }
        assert!(
            error.next.iter().any(|s| s.command.contains("git log")),
            "{error:?}"
        );
    }

    #[test]
    fn every_resolve_error_converts_with_the_right_kind() {
        let user = [
            ResolveError::NotEnoughHistory {
                line: line("main"),
                back: 5,
                available: 2,
            },
            ResolveError::NoSaveBefore { date: date_spec() },
            ResolveError::Ambiguous {
                prefix: "ab".to_owned(),
                candidates: vec![Candidate {
                    commit: "0123456789012345678901234567890123456789"
                        .parse::<paper_git::CommitId>()
                        .unwrap(),
                    short_id: "abcd".to_owned(),
                    message: "Start project".to_owned(),
                }],
            },
            ResolveError::UnknownPoint {
                name: "nope".to_owned(),
            },
        ];
        for error in user {
            assert_user(&PaperError::from(error));
        }
        assert_project(&PaperError::from(ResolveError::Git(git_error())));
    }

    #[test]
    fn an_unknown_point_error_names_the_point_and_suggests_history() {
        let error = PaperError::from(ResolveError::UnknownPoint {
            name: "nope".to_owned(),
        });
        assert!(
            format!("{} {}", error.message, error.why).contains("nope"),
            "{error:?}"
        );
        assert!(
            error
                .next
                .iter()
                .any(|s| s.command.contains("paper history")),
            "{error:?}"
        );
    }

    #[test]
    fn history_and_show_errors_convert() {
        assert_project(&PaperError::from(HistoryError::Git(git_error())));
        assert_project(&PaperError::from(ShowError::Git(git_error())));
        assert_user(&PaperError::from(ShowError::Resolve(
            ResolveError::UnknownPoint {
                name: "nope".to_owned(),
            },
        )));
        assert_project(&PaperError::from(ShowError::Resolve(ResolveError::Git(
            git_error(),
        ))));
    }

    #[test]
    fn wrapped_io_failures_name_the_underlying_cause_in_why() {
        let error = PaperError::from(SaveError::Io {
            path: path(),
            source: io_error(),
        });
        assert!(error.why.contains("disk exploded"), "{error:?}");
    }

    #[test]
    fn wrapped_git_failures_name_the_underlying_cause_in_why() {
        let error = PaperError::from(OpenError::Git(git_error()));
        assert!(error.why.contains("bare"), "{error:?}");
    }

    #[test]
    fn no_identity_suggests_both_git_config_commands() {
        let error = PaperError::no_identity();
        assert_user(&error);
        let commands: Vec<&str> = error.next.iter().map(|s| s.command.as_str()).collect();
        assert!(
            commands
                .iter()
                .any(|c| c.starts_with("git config --global user.name")),
            "{commands:?}"
        );
        assert!(
            commands
                .iter()
                .any(|c| c.starts_with("git config --global user.email")),
            "{commands:?}"
        );
    }

    #[test]
    fn not_a_project_in_a_git_repo_says_so_and_suggests_paper_init() {
        let error = PaperError::not_a_project(&path(), true);
        assert_user(&error);
        assert!(
            error.message.contains("git repo") && error.message.contains("not a paper project"),
            "{error:?}"
        );
        assert!(
            error.next.iter().any(|s| s.command == "paper init"),
            "{error:?}"
        );
    }

    #[test]
    fn not_a_project_outside_git_names_the_folder_and_suggests_paper_init() {
        let error = PaperError::not_a_project(&path(), false);
        assert_user(&error);
        assert!(
            format!("{} {}", error.message, error.why).contains("paper"),
            "{error:?}"
        );
        assert!(
            error.next.iter().any(|s| s.command == "paper init"),
            "{error:?}"
        );
    }
}

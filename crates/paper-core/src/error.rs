//! The error shape front-ends render: what happened, why, and what to run next.
//!
//! Every core error enum converts into a [`PaperError`]. The three parts are written here, per
//! variant, in researcher vocabulary. See `03-architecture.md` § Errors.

use std::io;
use std::path::Path;

use crate::activity::ActivityError;
use crate::config::{ConfigError, Conflict, Key, Origin, Scope};
use crate::point::ParsePointError;
use crate::profile::gitignore::GitignoreError;
use crate::project::{
    DiffError, GcError, HistoryError, InitError, OpenError, ResolveError, RestoreError, SaveError,
    ShowError, StatusError, UndoError,
};

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

impl Suggestion {
    fn new(command: impl Into<String>, explanation: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            explanation: explanation.into(),
        }
    }
}

impl PaperError {
    fn user(message: impl Into<String>, why: impl Into<String>, next: Vec<Suggestion>) -> Self {
        Self {
            kind: ErrorKind::User,
            message: message.into(),
            why: why.into(),
            next,
        }
    }

    /// Another paper command held the project for the whole wait.
    fn busy(holder: Option<u32>) -> Self {
        let who = holder.map_or_else(
            || "the other paper command".to_owned(),
            |pid| format!("the other paper command (process {pid})"),
        );
        Self {
            kind: ErrorKind::Project,
            message: "Another paper command is running in this project".to_owned(),
            why: "Paper runs one command at a time in a project so the history stays \
                  consistent, and the other command was still running when paper stopped \
                  waiting for it."
                .to_owned(),
            next: vec![Suggestion::new(
                "run the same command again",
                format!("wait for {who} to finish first"),
            )],
        }
    }

    /// A failure of paper's own state or of the machine; `cause` is named in `why`.
    fn project(message: impl Into<String>, cause: &dyn std::error::Error) -> Self {
        Self::project_failure(message, cause)
    }

    /// A [`ErrorKind::Project`] error: `why` names `cause` and its sources, and `next` asks
    /// for a report at the issue tracker.
    pub fn project_failure(message: impl Into<String>, cause: &dyn std::error::Error) -> Self {
        Self {
            kind: ErrorKind::Project,
            message: message.into(),
            why: cause_chain(cause),
            next: vec![Suggestion::new(
                "https://github.com/lbAntoine/paper-vcs/issues",
                "report it, including what you ran and the message above",
            )],
        }
    }

    /// No `user.name` / `user.email` in paper's settings or git config; the suggestions are
    /// `paper config set …` first, then the two `git config --global user.…` commands.
    pub fn no_identity() -> Self {
        Self::user(
            "Paper doesn't know who you are yet",
            "Every save records its author, and no name or email is set in your paper settings \
             or your git configuration.",
            vec![
                Suggestion::new(
                    "paper config set user.name \"Your Name\"",
                    "set the name saves are made under",
                ),
                Suggestion::new(
                    "paper config set user.email you@example.org",
                    "set your email address",
                ),
                Suggestion::new(
                    "git config --global user.name \"Your Name\"",
                    "or set the name in your git configuration instead",
                ),
                Suggestion::new(
                    "git config --global user.email \"you@example.org\"",
                    "or set the email in your git configuration instead",
                ),
            ],
        )
    }

    /// `path` isn't a paper project. `is_git_repo` says whether a plain git repository was
    /// found there (adopt it with `paper init`) or nothing at all.
    pub fn not_a_project(path: &Path, is_git_repo: bool) -> Self {
        if is_git_repo {
            Self::user(
                "This is a git repo but not a paper project yet",
                format!(
                    "{} has git history, but paper hasn't been set up in it, so it has no \
                     drafts or lines to work with.",
                    path.display()
                ),
                vec![Suggestion::new(
                    "paper init",
                    "adopt this repository; nothing in it is rewritten",
                )],
            )
        } else {
            Self::user(
                "This folder is not a paper project",
                format!(
                    "{} is not inside a paper project, and there is no git repository above it.",
                    path.display()
                ),
                vec![Suggestion::new(
                    "paper init",
                    "make this folder a paper project",
                )],
            )
        }
    }
}

/// `error` and the errors beneath it, as one sentence: `a: b: c`.
fn cause_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        let inner_text = inner.to_string();
        if !text.ends_with(&inner_text) {
            text = format!("{text}: {inner_text}");
        }
        source = inner.source();
    }
    text
}

impl PaperError {
    fn activity_log_in_the_way(path: &Path) -> Self {
        Self::user(
            "This folder has paper data that init won't delete",
            format!(
                "{} has an activity log with more than a first `init` step, but no version \
                 file, so it isn't a project that stopped half way through `paper init`. \
                 Paper won't delete a history it may need.",
                path.display()
            ),
            vec![Suggestion::new(
                "https://github.com/lbAntoine/paper-vcs/issues",
                "report it, including what you ran; keep the folder as it is",
            )],
        )
    }
}

impl From<InitError> for PaperError {
    fn from(error: InitError) -> Self {
        match error {
            InitError::Busy { holder } => Self::busy(holder),
            InitError::AlreadyInitialized { path } => Self::user(
                "This folder is already a paper project",
                format!(
                    "{} already has paper's history, so there is nothing to set up.",
                    path.display()
                ),
                vec![Suggestion::new("paper history", "see its saves")],
            ),
            InitError::ActivityLogInTheWay { path } => Self::activity_log_in_the_way(&path),
            InitError::InsideAnotherRepository { path, root } => Self::user(
                "This folder is inside another project",
                format!(
                    "{} sits inside the git repository at {}, and a project can't be nested in another one.",
                    path.display(),
                    root.display()
                ),
                vec![Suggestion::new(
                    format!("paper init {}", root.display()),
                    "make the outer folder the project instead",
                )],
            ),
            InitError::DetachedHead => Self::user(
                "Git isn't on a branch",
                "HEAD is detached, so paper doesn't know which line to start from.",
                vec![Suggestion::new(
                    "git switch <branch>",
                    "go back to a branch, then run paper init again",
                )],
            ),
            InitError::UnusableBranchName { name } => Self::user(
                "The current branch can't be used as a line",
                format!("'{name}' isn't a name paper can use for a line."),
                vec![Suggestion::new(
                    format!("git branch -m \"{name}\" <new-name>"),
                    "rename the branch, then run paper init again",
                )],
            ),
            InitError::DamagedGit { path } => Self::user(
                "This folder has a damaged .git",
                format!(
                    "{} has a .git that git can't open (it may be empty or half deleted), so \
                     paper can't tell whether this folder is already a repository. Nothing was \
                     changed.",
                    path.display()
                ),
                vec![Suggestion::new(
                    "git status",
                    "inspect what git says about it, or move .git out of the way and run \
                     paper init again",
                )],
            ),
            InitError::EmptyCurrentBranch { name } => Self::user(
                "The current branch has no saves yet",
                format!(
                    "'{name}' has no commits, but other branches do, so paper can't tell where to start."
                ),
                vec![Suggestion::new(
                    "git switch <branch>",
                    "move to a branch that has history, then run paper init again",
                )],
            ),
            InitError::ProfileConflict {
                requested,
                existing,
            } => Self::user(
                format!("This project's profile is '{existing}', not '{requested}'"),
                format!(
                    "paper.toml already says profile = \"{existing}\", and init does not \
                     override it silently."
                ),
                vec![
                    Suggestion::new(
                        format!("paper init --profile {existing}"),
                        "set the project up with the profile it already has",
                    ),
                    Suggestion::new(
                        format!("paper config set --project profile {requested}"),
                        format!("or change the project to '{requested}'"),
                    ),
                ],
            ),
            InitError::Gitignore(error) => Self::from_gitignore(&error, "paper init"),
            InitError::Config(error) => Self::from_config(&error, Scope::Project),
            InitError::Git(_)
            | InitError::Snapshot(_)
            | InitError::Activity(_)
            | InitError::Export(_)
            | InitError::Io { .. } => Self::project("Paper could not set up this folder", &error),
        }
    }
}

impl PaperError {
    /// A `.gitignore` the profile block can't be merged into, or couldn't be written. Nothing
    /// was changed. `retry` is the command that repeats the operation once the file is fixed.
    pub fn from_gitignore(error: &GitignoreError, retry: &str) -> Self {
        match error {
            GitignoreError::UnterminatedBlock { .. } | GitignoreError::DuplicateBlock { .. } => {
                Self::user(
                    "The paper block in .gitignore is damaged",
                    format!(
                        "{error}. Paper keeps its lines between two marker lines, \
                         `# >>> paper-vcs profile: <name> >>>` and `# <<< paper-vcs profile: <name> <<<`, \
                         and needs exactly one such pair. Nothing was changed."
                    ),
                    vec![Suggestion::new(
                        retry,
                        "run it again after fixing .gitignore: delete the stray marker line, or the whole block",
                    )],
                )
            }
            GitignoreError::NotUtf8 { .. } => Self::user(
                "Paper can't read .gitignore",
                format!(
                    "{error}, so paper can't add its block without risking your own lines. \
                     Nothing was changed."
                ),
                vec![Suggestion::new(
                    retry,
                    "run it again after saving .gitignore as UTF-8 text in your editor",
                )],
            ),
            GitignoreError::Read { path, source } | GitignoreError::Write { path, source }
                if source.kind() == io::ErrorKind::PermissionDenied =>
            {
                Self::user(
                    "Paper is not allowed to change .gitignore",
                    format!(
                        "{error}: permission denied. Nothing was changed. Paper needs to be \
                         able to read and write {}.",
                        path.display()
                    ),
                    vec![Suggestion::new(
                        retry,
                        "run it again after making .gitignore readable and writable for you",
                    )],
                )
            }
            GitignoreError::Read { .. } | GitignoreError::Write { .. } => {
                Self::project_failure("Paper could not update .gitignore", error)
            }
        }
    }
}

impl From<ConfigError> for PaperError {
    fn from(error: ConfigError) -> Self {
        Self::from_config(&error, Scope::User)
    }
}

impl PaperError {
    /// A settings error in three parts. `scope` is the file a `set` was aimed at, so a rejected
    /// value suggests the right form of `paper config set` (`--project` for `paper.toml`).
    pub fn from_config(error: &ConfigError, scope: Scope) -> Self {
        match error {
            ConfigError::UnknownKey { key } => {
                let known: Vec<&str> = Key::ALL.iter().map(|k| k.as_str()).collect();
                Self::user(
                    format!("`{key}` is not a setting paper knows"),
                    format!("The settings are: {}.", known.join(", ")),
                    vec![Suggestion::new(
                        "paper config list",
                        "see every setting and its value",
                    )],
                )
            }
            ConfigError::InvalidValue {
                key,
                value,
                expected,
                origin,
                conflict: None,
            } => invalid_value(key, value, expected, origin.as_ref(), scope),
            ConfigError::InvalidValue {
                key,
                value,
                origin,
                conflict: Some(conflict),
                ..
            } => order_conflict(key, value, *conflict, origin.as_ref(), scope),
            ConfigError::WrongHome {
                key,
                where_,
                reason,
            } => {
                let command = if where_.contains("project") {
                    format!("paper config set {key} <value>")
                } else {
                    format!("paper config set --project {key} <value>")
                };
                Self::user(
                    format!("`{key}` can't be set {where_}"),
                    reason.clone(),
                    vec![Suggestion::new(command, "set it where it belongs")],
                )
            }
            ConfigError::Blocked { path, table, key } => {
                let file = path
                    .as_ref()
                    .map_or_else(|| "the file".to_owned(), |p| p.display().to_string());
                Self::user(
                    format!("`{table}` in {file} is a plain value, not a section"),
                    format!(
                        "Paper keeps `{key}` in a `[{table}]` section, and `{table}` is already used for something else."
                    ),
                    vec![Suggestion::new(
                        file,
                        format!(
                            "edit the file: remove `{table}` or turn it into a [{table}] section"
                        ),
                    )],
                )
            }
            ConfigError::Malformed {
                path,
                line,
                message,
            } => Self::user(
                format!("Paper can't read the settings file {}", path.display()),
                format!("It isn't valid TOML (line {line}): {message}."),
                vec![Suggestion::new(
                    path.display().to_string(),
                    format!("fix line {line}, or delete the file to start over"),
                )],
            ),
            ConfigError::Io { path, .. } => Self::project(
                format!("Paper could not use the settings file {}", path.display()),
                error,
            ),
        }
    }
}

fn invalid_value(
    key: &str,
    value: &str,
    expected: &str,
    origin: Option<&Origin>,
    scope: Scope,
) -> PaperError {
    let example = Key::parse(key).map_or("<value>", Key::example);
    let mut next = Vec::new();
    match origin {
        Some(Origin::Env(var)) => next.push(unset_suggestion(var, cfg!(windows))),
        Some(Origin::File(path)) => {
            next.push(Suggestion::new(
                format!(
                    "paper config set {}{key} {example}",
                    set_flag(origin, scope)
                ),
                "replace the bad value",
            ));
            next.push(Suggestion::new(
                path.display().to_string(),
                "or edit the file by hand",
            ));
        }
        None => next.push(Suggestion::new(
            format!(
                "paper config set {}{key} {example}",
                set_flag(origin, scope)
            ),
            "set it to a value paper accepts",
        )),
    }
    PaperError::user(
        format!(
            "`{value}` is not a valid value for `{key}`{}",
            Origin::note(origin)
        ),
        format!("{key} takes {expected}."),
        next,
    )
}

/// The suggestion that removes the environment variable `var`: `unset` on POSIX shells,
/// PowerShell's `Remove-Item Env:` when `windows` is true (with `cmd`'s `set VAR=` named in the
/// explanation).
fn unset_suggestion(var: &str, windows: bool) -> Suggestion {
    if windows {
        Suggestion::new(
            format!("Remove-Item Env:{var}"),
            format!(
                "remove the variable in PowerShell (in cmd.exe, `set {var}=` does the same), \
                 or set it to a value paper accepts"
            ),
        )
    } else {
        Suggestion::new(
            format!("unset {var}"),
            "remove the variable, or set it to a value paper accepts",
        )
    }
}

fn order_conflict(
    key: &str,
    value: &str,
    conflict: Conflict,
    origin: Option<&Origin>,
    scope: Scope,
) -> PaperError {
    let other = conflict.other.as_str();
    let other_value = conflict.other_value;
    let relation = if conflict.other == Key::LargeFilesWarnMb {
        "below"
    } else {
        "above"
    };
    let flag = set_flag(origin, scope);
    let direction = if conflict.other == Key::LargeFilesWarnMb {
        "at least"
    } else {
        "at most"
    };
    let in_file = match origin {
        Some(Origin::File(path)) => format!(" in {}", path.display()),
        _ => String::new(),
    };
    PaperError::user(
        format!("{key} ({value}) can't be {relation} {other} ({other_value}){in_file}"),
        "Files bigger than large_files.max_mb are left out of saves, so the warning \
             level can't be higher than that.",
        vec![
            Suggestion::new(
                format!("paper config set {flag}{key} {other_value}"),
                format!("make {key} {direction} {other_value}, to match {other}"),
            ),
            Suggestion::new(
                format!("paper config set {flag}{other} {value}"),
                format!("or move {other} to {value}, to match {key}"),
            ),
        ],
    )
}

/// `--project ` when the command that fixes this must write `paper.toml`.
fn set_flag(origin: Option<&Origin>, scope: Scope) -> &'static str {
    let project = match origin {
        Some(Origin::File(path)) => path.file_name().is_some_and(|name| name == "paper.toml"),
        Some(Origin::Env(_)) => false,
        None => scope == Scope::Project,
    };
    if project { "--project " } else { "" }
}

impl From<OpenError> for PaperError {
    fn from(error: OpenError) -> Self {
        match error {
            OpenError::Busy { holder } => Self::busy(holder),
            OpenError::NotAPaperProject { path } => Self::not_a_project(&path, true),
            OpenError::UnsupportedFormat { .. } => Self::project(
                "This project was made by a different version of paper",
                &error,
            ),
            OpenError::Git(_)
            | OpenError::View(_)
            | OpenError::Activity(_)
            | OpenError::Export(_)
            | OpenError::Snapshot(_)
            | OpenError::Io { .. } => Self::project("Paper could not open this project", &error),
        }
    }
}

impl From<SaveError> for PaperError {
    fn from(error: SaveError) -> Self {
        match error {
            SaveError::MissingMessage => Self::user(
                "A save needs a message",
                "The message says what changed, so you can find this save later.",
                vec![Suggestion::new(
                    "paper save \"Describe what changed\"",
                    "save with a message",
                )],
            ),
            SaveError::NoChangesAt { paths } => Self::user(
                format!(
                    "None of these paths has unsaved changes: {}",
                    paths.join(", ")
                ),
                "They are the same as in the last save, so there is nothing to save. \
                 A typo in a path is the usual cause.",
                vec![Suggestion::new(
                    "paper show @",
                    "list the files with unsaved changes",
                )],
            ),
            SaveError::OutOfSyncWithGit { line } => Self::user(
                format!("The line '{line}' was changed outside paper"),
                "Commits were added to it, or it was moved, with plain git since paper last saw \
                 it. Paper can't save on top of that yet, and nothing was saved or changed.",
                vec![Suggestion::new(
                    format!("git log {line}"),
                    "see what changed",
                )],
            ),
            SaveError::HeadOnAnotherBranch { line, found } => {
                let place = found.map_or("no branch (detached HEAD)".to_owned(), |branch| {
                    format!("the branch '{branch}'")
                });
                Self::user(
                    format!("Git is on {place}, not on the line '{line}'"),
                    "A save is added to the line you are on, and paper's current line and git's \
                     branch have to agree. Nothing was saved.",
                    vec![Suggestion::new(
                        format!("git switch {line}"),
                        "go back to the line, then save again",
                    )],
                )
            }
            SaveError::Git(_)
            | SaveError::Snapshot(_)
            | SaveError::Activity(_)
            | SaveError::Export(_)
            | SaveError::Io { .. } => Self::project("Paper could not save", &error),
        }
    }
}

impl From<ResolveError> for PaperError {
    fn from(error: ResolveError) -> Self {
        let history = || {
            vec![Suggestion::new(
                "paper history",
                "list the saves on this line",
            )]
        };
        match error {
            ResolveError::NotEnoughHistory {
                line,
                back,
                available,
            } => Self::user(
                format!("`last~{back}` goes further back than this line's saves"),
                format!("The line '{line}' has only {available} saves."),
                history(),
            ),
            ResolveError::NoSaveBefore { date } => Self::user(
                format!("No save was made on or before {date}"),
                "The current line starts later than that date.",
                history(),
            ),
            ResolveError::Ambiguous { prefix, candidates } => {
                let listed: Vec<String> = candidates
                    .iter()
                    .map(|c| format!("{} \"{}\"", c.short_id, c.message))
                    .collect();
                Self::user(
                    format!("'{prefix}' could mean several saves"),
                    format!("It matches {}.", listed.join(", ")),
                    candidates
                        .iter()
                        .map(|c| {
                            Suggestion::new(format!("paper show {}", c.short_id), c.message.clone())
                        })
                        .collect(),
                )
            }
            ResolveError::UnknownPoint { name } => Self::user(
                format!("There is no save, line or date called '{name}'"),
                "A save is named by its short ID, `last`, `@` (the draft), a line name or \
                 `date:YYYY-MM-DD`.",
                history(),
            ),
            ResolveError::Git(_) => Self::project("Paper could not look up that save", &error),
        }
    }
}

impl From<ParsePointError> for PaperError {
    fn from(error: ParsePointError) -> Self {
        Self::user(
            "That is not a way to name a save",
            error.to_string(),
            vec![Suggestion::new(
                "paper history",
                "list the saves and their IDs",
            )],
        )
    }
}

impl From<ActivityError> for PaperError {
    fn from(error: ActivityError) -> Self {
        Self::project("Paper could not read the activity log", &error)
    }
}

impl From<HistoryError> for PaperError {
    fn from(error: HistoryError) -> Self {
        Self::project("Paper could not read the saves", &error)
    }
}

impl From<ShowError> for PaperError {
    fn from(error: ShowError) -> Self {
        match error {
            ShowError::Resolve(inner) => inner.into(),
            ShowError::Git(_) => Self::project("Paper could not read that save", &error),
        }
    }
}

impl From<DiffError> for PaperError {
    fn from(error: DiffError) -> Self {
        match error {
            DiffError::UnknownPaths { paths } => Self::user(
                format!("No file matches: {}", paths.join(", ")),
                "These paths are not in the draft or in either save being compared. \
                 A typo in a path is the usual cause.",
                vec![
                    Suggestion::new("paper status", "list the files in the draft"),
                    Suggestion::new("paper show <point>", "list the files of a save"),
                ],
            ),
            DiffError::Resolve(inner) => inner.into(),
            DiffError::Git(_) => Self::project("Paper could not compare those saves", &error),
        }
    }
}

impl From<StatusError> for PaperError {
    fn from(error: StatusError) -> Self {
        match error {
            StatusError::Save(inner) => inner.into(),
            StatusError::Resolve(inner) => inner.into(),
            StatusError::Git(_) => Self::project("Paper could not read the draft", &error),
        }
    }
}

impl From<UndoError> for PaperError {
    fn from(error: UndoError) -> Self {
        match error {
            UndoError::Save(inner) => inner.into(),
            UndoError::EmptyEntry => Self::user(
                "Say which entry to go back to",
                "`--to` needs the start of an entry id (hexadecimal, as the first column of the \
                 activity list). Nothing was undone or changed.",
                vec![Suggestion::new(
                    "paper activity",
                    "list the entries and their ids",
                )],
            ),
            UndoError::UnknownEntry { prefix } => Self::user(
                format!("No entry in the activity log starts with '{prefix}'"),
                "Nothing was undone or changed. A typo in the id is the usual cause; ids are \
                 made of the digits 0-9 and the letters a-f, and are the first column of the \
                 activity list.",
                vec![Suggestion::new(
                    "paper activity",
                    "list the entries and their ids",
                )],
            ),
            UndoError::AmbiguousEntry { prefix, candidates } => Self::user(
                format!("'{prefix}' starts more than one entry in the activity log"),
                format!(
                    "Nothing was undone or changed. It matches: {}.",
                    candidates
                        .iter()
                        .map(|c| format!("{} ({})", c.short_id, c.description))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                candidates
                    .iter()
                    .map(|c| {
                        Suggestion::new(
                            format!("paper undo --to {}", c.short_id),
                            c.description.clone(),
                        )
                    })
                    .collect(),
            ),
            UndoError::Git(_)
            | UndoError::Activity(_)
            | UndoError::Export(_)
            | UndoError::Io { .. } => Self::project("Paper could not undo or redo", &error),
        }
    }
}

impl From<RestoreError> for PaperError {
    fn from(error: RestoreError) -> Self {
        match error {
            RestoreError::NoPaths => Self::user(
                "Say which files to restore",
                "Restoring puts files back the way they were, so paper needs to know which ones.",
                vec![Suggestion::new(
                    "paper restore main.tex",
                    "restore one file (a folder works too)",
                )],
            ),
            RestoreError::UnknownPaths { paths } => Self::user(
                format!("No file matches: {}", paths.join(", ")),
                "These paths are not in your draft or in that save, so no files were changed. \
                 A typo in a path is the usual cause.",
                vec![
                    Suggestion::new("paper status", "list the files in the draft"),
                    Suggestion::new("paper show <point>", "list the files of a save"),
                ],
            ),
            RestoreError::Save(inner) => inner.into(),
            RestoreError::Resolve(inner) => inner.into(),
            RestoreError::Git(_) => Self::project("Paper could not restore those files", &error),
            RestoreError::Io { .. } => {
                let mut converted = Self::project("Paper could not restore those files", &error);
                converted
                    .next
                    .insert(0, Suggestion::new("paper status", "see where things stand"));
                converted
            }
        }
    }
}

impl From<GcError> for PaperError {
    fn from(error: GcError) -> Self {
        match error {
            GcError::InvalidAge { given } => Self::user(
                format!("'{given}' is not a duration paper understands"),
                if given.starts_with("--") {
                    format!(
                        "`--older-than` needs a duration such as `90d` right after it, but \
                         '{given}' looks like another option. Nothing was removed."
                    )
                } else {
                    "`--older-than` takes a whole number of days or weeks, at least one: `30d`, \
                     `12w`. Nothing was removed."
                        .to_owned()
                },
                vec![
                    Suggestion::new("paper gc --older-than 30d", "keep the last 30 days"),
                    Suggestion::new("paper gc --older-than 12w", "keep the last 12 weeks"),
                ],
            ),
            GcError::Git(_) | GcError::Activity(_) | GcError::Io { .. } => {
                let mut converted = Self::project("Paper could not clean up the history", &error);
                converted.next.insert(
                    0,
                    Suggestion::new("paper gc", "run it again; it finishes what it started"),
                );
                converted
            }
        }
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
        assert!(
            !error.next.is_empty(),
            "every error has all three parts: {error:?}"
        );
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
            InitError::ActivityLogInTheWay { path: path() },
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
            InitError::ProfileConflict {
                requested: "latex".to_owned(),
                existing: "typst".to_owned(),
            },
            InitError::Gitignore(GitignoreError::UnterminatedBlock { line: 3 }),
        ];
        for error in user {
            assert_user(&PaperError::from(error));
        }
        assert_project(&PaperError::from(InitError::Git(git_error())));
        assert_project(&PaperError::from(InitError::Snapshot(SnapshotError::Git(
            git_error(),
        ))));
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
    fn an_unparseable_point_is_a_user_error() {
        let error = "last~x".parse::<Point>().unwrap_err();
        assert_user(&PaperError::from(error));
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

    fn unknown_paths() -> Vec<String> {
        vec!["mian.tex".to_owned(), "sections/nope.tex".to_owned()]
    }

    #[test]
    fn project_failure_puts_the_issue_url_in_command_and_the_prose_in_explanation() {
        let error = PaperError::project_failure("Paper could not save", &io_error());
        assert_eq!(error.kind, ErrorKind::Project);
        assert_eq!(error.next.len(), 1, "{error:?}");
        assert_eq!(
            error.next[0].command,
            "https://github.com/lbAntoine/paper-vcs/issues"
        );
        assert_eq!(
            error.next[0].explanation,
            "report it, including what you ran and the message above"
        );
    }

    #[test]
    fn a_diff_with_unknown_paths_is_a_user_error_naming_every_path() {
        let error = PaperError::from(DiffError::UnknownPaths {
            paths: unknown_paths(),
        });
        assert_user(&error);
        let text = format!("{} {}", error.message, error.why);
        for path in unknown_paths() {
            assert!(text.contains(&path), "{path}: {error:?}");
        }
    }

    #[test]
    fn diff_errors_from_resolving_a_point_keep_their_user_wording() {
        let from_resolve = PaperError::from(ResolveError::UnknownPoint {
            name: "nope".to_owned(),
        });
        let error = PaperError::from(DiffError::Resolve(ResolveError::UnknownPoint {
            name: "nope".to_owned(),
        }));
        assert_eq!(error, from_resolve);
    }

    #[test]
    fn a_git_failure_while_diffing_is_a_project_error() {
        assert_project(&PaperError::from(DiffError::Git(git_error())));
    }

    #[test]
    fn status_errors_convert_with_the_right_kind() {
        assert_project(&PaperError::from(StatusError::Git(git_error())));
        assert_user(&PaperError::from(StatusError::Resolve(
            ResolveError::UnknownPoint {
                name: "nope".to_owned(),
            },
        )));
        assert_user(&PaperError::from(StatusError::Save(
            SaveError::MissingMessage,
        )));
        assert_project(&PaperError::from(StatusError::Save(SaveError::Git(
            git_error(),
        ))));
    }

    #[test]
    fn restore_without_paths_is_a_user_error_that_shows_an_example() {
        let error = PaperError::from(RestoreError::NoPaths);
        assert_user(&error);
        assert!(
            error
                .next
                .iter()
                .any(|s| s.command.starts_with("paper restore")),
            "{error:?}"
        );
    }

    #[test]
    fn restore_with_unknown_paths_names_every_path_and_says_nothing_changed() {
        let error = PaperError::from(RestoreError::UnknownPaths {
            paths: unknown_paths(),
        });
        assert_user(&error);
        let text = format!("{} {}", error.message, error.why);
        for path in unknown_paths() {
            assert!(text.contains(&path), "{path}: {error:?}");
        }
        assert!(
            text.contains("no files were changed") || text.contains("nothing was changed"),
            "{error:?}"
        );
    }

    #[test]
    fn restore_failures_of_the_machine_are_project_errors() {
        assert_project(&PaperError::from(RestoreError::Git(git_error())));
        assert_project(&PaperError::from(RestoreError::Io {
            action: "restore",
            path: "main.tex".to_owned(),
            source: io_error(),
        }));
        assert_project(&PaperError::from(RestoreError::Save(SaveError::Git(
            git_error(),
        ))));
        assert_user(&PaperError::from(RestoreError::Resolve(
            ResolveError::UnknownPoint {
                name: "nope".to_owned(),
            },
        )));
    }

    fn core_messages() -> Vec<String> {
        let unknown = ResolveError::UnknownPoint {
            name: "nope".to_owned(),
        };
        let too_far = ResolveError::NotEnoughHistory {
            line: line("main"),
            back: 3,
            available: 1,
        };
        let no_save = ResolveError::NoSaveBefore { date: date_spec() };
        let ambiguous = ResolveError::Ambiguous {
            prefix: "ab".to_owned(),
            candidates: Vec::new(),
        };
        vec![
            unknown.to_string(),
            too_far.to_string(),
            no_save.to_string(),
            ambiguous.to_string(),
            DiffError::UnknownPaths {
                paths: unknown_paths(),
            }
            .to_string(),
            RestoreError::NoPaths.to_string(),
            RestoreError::UnknownPaths {
                paths: unknown_paths(),
            }
            .to_string(),
            RestoreError::Io {
                action: "restore",
                path: "main.tex".to_owned(),
                source: io_error(),
            }
            .to_string(),
            SaveError::MissingMessage.to_string(),
            SaveError::NoChangesAt {
                paths: unknown_paths(),
            }
            .to_string(),
            SaveError::OutOfSyncWithGit { line: line("main") }.to_string(),
        ]
    }

    #[test]
    fn no_core_message_suggests_a_command_that_does_not_exist_yet() {
        for message in core_messages() {
            assert!(!message.contains("paper lines"), "{message}");
        }
    }

    #[test]
    fn core_messages_do_not_repeat_the_next_hints_the_cli_shows() {
        let diff_and_restore = [
            DiffError::UnknownPaths {
                paths: unknown_paths(),
            }
            .to_string(),
            RestoreError::UnknownPaths {
                paths: unknown_paths(),
            }
            .to_string(),
            RestoreError::Io {
                action: "restore",
                path: "main.tex".to_owned(),
                source: io_error(),
            }
            .to_string(),
            ResolveError::UnknownPoint {
                name: "nope".to_owned(),
            }
            .to_string(),
        ];
        for message in diff_and_restore {
            assert!(
                !message.contains("`paper "),
                "a message states what happened; commands go in `next`: {message}"
            );
        }
    }

    // ---- PV-014 ----

    #[test]
    fn unsetting_a_variable_on_unix_uses_unset() {
        let suggestion = unset_suggestion("PAPER_UI_COLOR", false);
        assert_eq!(suggestion.command, "unset PAPER_UI_COLOR");
    }

    #[test]
    fn unsetting_a_variable_on_windows_uses_powershell_and_names_cmd() {
        let suggestion = unset_suggestion("PAPER_UI_COLOR", true);
        assert_eq!(suggestion.command, "Remove-Item Env:PAPER_UI_COLOR");
        assert!(
            suggestion.explanation.contains("set PAPER_UI_COLOR="),
            "the cmd form is named: {suggestion:?}"
        );
    }

    fn gitignore_path() -> PathBuf {
        PathBuf::from("/work/.gitignore")
    }

    fn permission_denied() -> io::Error {
        io::Error::from(io::ErrorKind::PermissionDenied)
    }

    #[test]
    fn a_gitignore_that_cannot_be_read_says_read() {
        let cause = GitignoreError::Read {
            path: gitignore_path(),
            source: io_error(),
        };
        assert_eq!(
            cause.to_string(),
            "could not read /work/.gitignore",
            "the message names the action that failed"
        );
        let error = PaperError::from_gitignore(&cause, "paper init");
        assert_project(&error);
        assert_eq!(error.message, "Paper could not update .gitignore");
        assert!(error.why.contains("could not read"), "{error:?}");
    }

    #[test]
    fn a_gitignore_that_cannot_be_written_says_write() {
        let cause = GitignoreError::Write {
            path: gitignore_path(),
            source: io_error(),
        };
        assert_eq!(cause.to_string(), "could not write /work/.gitignore");
        let error = PaperError::from_gitignore(&cause, "paper init");
        assert_project(&error);
        assert!(error.why.contains("could not write"), "{error:?}");
    }

    #[test]
    fn permission_denied_on_gitignore_is_a_user_error_naming_the_file() {
        for cause in [
            GitignoreError::Read {
                path: gitignore_path(),
                source: permission_denied(),
            },
            GitignoreError::Write {
                path: gitignore_path(),
                source: permission_denied(),
            },
        ] {
            let error = PaperError::from_gitignore(&cause, "paper init");
            assert_user(&error);
            let text = format!("{} {}", error.message, error.why);
            assert!(text.contains(".gitignore"), "{error:?}");
            assert!(error.why.contains("/work/.gitignore"), "{error:?}");
            assert_eq!(error.next[0].command, "paper init", "{error:?}");
        }
    }
}

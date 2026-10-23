//! Command-line arguments (clap derive).

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// Version control for researchers.
#[derive(Debug, Parser)]
#[command(
    name = "paper",
    bin_name = "paper",
    version,
    disable_help_subcommand = true
)]
#[allow(clippy::struct_excessive_bools)] // global flags are independent switches
pub struct Cli {
    /// Run as if started in this folder.
    #[arg(short = 'C', global = true, value_name = "PATH")]
    pub directory: Option<PathBuf>,
    /// Machine-readable output.
    #[arg(long, global = true)]
    pub json: bool,
    /// Skip the automatic snapshot.
    #[arg(long, global = true)]
    pub no_snapshot: bool,
    /// Print only errors.
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// Add git terms in parentheses.
    #[arg(short, long, global = true)]
    pub verbose: bool,
    /// When to use colour.
    #[arg(long, global = true, value_enum)]
    pub color: Option<ColorChoice>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Make a folder a paper project, or adopt an existing git repository.
    Init {
        /// Kind of project: latex, markdown, typst, quarto, jupyter or generic (default: guessed
        /// from the files). It decides what paper keeps out of saves, through a marked block in
        /// .gitignore.
        #[arg(long, value_parser = ["latex", "markdown", "typst", "quarto", "jupyter", "generic"])]
        profile: Option<String>,
        /// The folder to make a project (default: the current folder).
        path: Option<PathBuf>,
    },
    /// Save the draft (or only some files) with a message.
    Save {
        /// What changed, in a few words (asked for on a terminal when missing).
        message: Option<String>,
        /// Save only these files or folders (relative to where you run paper); the rest stays in
        /// the draft.
        paths: Vec<String>,
        /// Save everything except this file or folder; repeat the flag for more.
        #[arg(long, num_args = 1, action = clap::ArgAction::Append, conflicts_with = "paths")]
        all_but: Vec<String>,
    },
    /// List saves, newest first.
    History {
        /// Only saves that touch this file, or anything under this folder (relative to where you
        /// run paper).
        path: Option<String>,
        /// Show the saves of every line, not just the current one.
        #[arg(long)]
        all: bool,
        /// Show at most this many saves.
        #[arg(short = 'n', value_name = "N")]
        limit: Option<usize>,
        /// Not yet available.
        #[arg(long)]
        graph: bool,
    },
    /// Show one save: metadata, changed files and their diffs.
    Show {
        /// A short save ID, `last`, `last~N`, `@` (the draft), a line name or `date:YYYY-MM-DD`.
        point: String,
        /// Print the per-file counts instead of the diffs.
        #[arg(long)]
        stat: bool,
        /// Compare line by line as a unified diff, whatever `diff.default` says.
        #[arg(long, conflicts_with = "words")]
        lines: bool,
        /// Compare word by word; the default unless `diff.default` is `lines`.
        #[arg(long)]
        words: bool,
    },
    /// List the unsaved changes in the draft.
    Status,
    /// Compare two points, or the last save and the draft.
    Diff {
        /// Up to two points, then files or folders (relative to where you run paper). The first
        /// word that is not a point starts the paths.
        args: Vec<String>,
        /// Files or folders after `--`: always paths, even when named like a point.
        #[arg(last = true)]
        paths: Vec<String>,
        /// Compare line by line as a unified diff, whatever `diff.default` says.
        #[arg(long, conflicts_with = "words")]
        lines: bool,
        /// Compare word by word; the default unless `diff.default` is `lines`.
        #[arg(long)]
        words: bool,
        /// Print the per-file counts instead of the diffs.
        #[arg(long)]
        stat: bool,
        /// Compare this point with the draft; the same as `paper diff <point> @`.
        #[arg(long, value_name = "POINT")]
        since: Option<String>,
    },
    /// Read or change paper's settings.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// List what you and paper did to the project, newest first.
    Activity {
        /// Show at most this many entries.
        #[arg(
            short = 'n',
            value_name = "N",
            default_value_t = 20,
            value_parser = parse_entry_count
        )]
        limit: usize,
    },
    /// Take back the last thing you or paper did, files included.
    Undo {
        /// Go back to the state after this activity entry (an ID from `paper activity`) instead.
        #[arg(long, value_name = "ID")]
        to: Option<String>,
    },
    /// Bring back what you just undid.
    Redo,
    /// Trim the undo history to a recent window, so git can reclaim old drafts and undone work.
    Gc {
        /// Remove entries older than this: a number of days (`30d`) or weeks (`12w`), at
        /// least `1d`.
        #[arg(
            long,
            value_name = "DURATION",
            default_value = "90d",
            allow_hyphen_values = true
        )]
        older_than: String,
        /// Show what would be removed and change nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Stop tracking files or folders, even saved ones; with no arguments, list the ignore rules.
    Ignore {
        /// The files, folders or patterns (like `*.aux`) to ignore; folders and files are
        /// relative to where you run paper.
        paths: Vec<String>,
        /// Go ahead when more than 20 saved files would stop being tracked.
        #[arg(long)]
        yes: bool,
        /// Keep the rules to yourself: write them in `.git/info/exclude`, not `.gitignore`.
        #[arg(long)]
        local: bool,
        /// Show which rule, file and line ignore this path, and change nothing.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["paths", "yes", "local"])]
        why: Option<String>,
    },
    /// Track files or folders again: remove their ignore rule, or add an exception.
    Unignore {
        /// The files, folders or patterns to stop ignoring (relative to where you run paper).
        #[arg(required = true)]
        paths: Vec<String>,
        /// Work on your private rules in `.git/info/exclude`, not `.gitignore`.
        #[arg(long)]
        local: bool,
    },
    /// Put files back the way they were at a point.
    Restore {
        /// The files or folders to restore (relative to where you run paper).
        #[arg(required = true)]
        paths: Vec<String>,
        /// The point to restore from (default: `last`).
        #[arg(long, value_name = "POINT")]
        from: Option<String>,
    },
    /// Explain a command or a topic (try `paper help large-files`).
    Help {
        /// A command (like `save` or `config get`) or a help topic (like `large-files`).
        topic: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the effective value of a setting.
    Get {
        /// The setting, for example `user.name`.
        key: String,
    },
    /// Change a setting in your user settings, or in this project's `paper.toml`.
    Set {
        /// The setting, for example `user.name`.
        key: String,
        /// The new value.
        value: String,
        /// Write to this project's `paper.toml` instead of your user settings.
        #[arg(long)]
        project: bool,
    },
    /// Show every setting with its value and where the value comes from.
    List,
}

/// `-n <N>`: a whole number of entries, at least 1.
fn parse_entry_count(text: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(count) if count >= 1 => Ok(count),
        _ => Err(format!(
            "-n needs a number of entries, at least 1 (you gave '{text}')"
        )),
    }
}

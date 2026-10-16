//! Command-line arguments (clap derive).

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// Version control for researchers.
#[derive(Debug, Parser)]
#[command(name = "paper", version)]
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
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
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
        /// Kind of project: latex, markdown, typst, quarto, jupyter or generic. Accepted and
        /// checked, but it has no effect yet (profiles are not yet implemented).
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
        /// Print the per-file line counts instead of the diffs.
        #[arg(long)]
        stat: bool,
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
        /// Compare line by line (the default).
        #[arg(long, conflicts_with = "words")]
        lines: bool,
        /// Compare word by word. Not yet available (v0.3).
        #[arg(long)]
        words: bool,
        /// Print the per-file line counts instead of the diffs.
        #[arg(long)]
        stat: bool,
        /// Compare this point with the draft; the same as `paper diff <point> @`.
        #[arg(long, value_name = "POINT")]
        since: Option<String>,
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
}

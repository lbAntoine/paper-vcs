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
        /// Save only these files or folders; the rest stays in the draft.
        paths: Vec<String>,
        /// Save everything except these files or folders.
        #[arg(long, num_args = 1.., conflicts_with = "paths")]
        all_but: Vec<String>,
    },
    /// List saves, newest first.
    History {
        /// Only saves that touch this file, or anything under this folder.
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
    /// Show one save: metadata and changed files.
    Show {
        /// A short save ID, `last`, `last~N`, `@` (the draft), a line name or `date:YYYY-MM-DD`.
        point: String,
    },
}

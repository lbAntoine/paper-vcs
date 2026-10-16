//! Command-line arguments (clap derive).
// RED stub: the declarations exist so the crate compiles; GREEN wires them up.
#![allow(dead_code)]

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// Version control for researchers.
#[derive(Debug, Parser)]
#[command(name = "paper", version)]
#[allow(clippy::struct_excessive_bools)] // global flags are independent switches
pub struct Cli {
    /// Run as if started in this folder.
    #[arg(short = 'C', global = true)]
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
        /// Accepted and validated, but has no effect until profiles land.
        #[arg(long)]
        profile: Option<String>,
        path: Option<PathBuf>,
    },
    /// Save the draft (or only some files) with a message.
    Save {
        message: Option<String>,
        paths: Vec<String>,
        #[arg(long, num_args = 1..)]
        all_but: Vec<String>,
    },
    /// List saves, newest first.
    History {
        path: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(short = 'n')]
        limit: Option<usize>,
        #[arg(long)]
        graph: bool,
    },
    /// Show one save: metadata and changed files.
    Show { point: String },
}

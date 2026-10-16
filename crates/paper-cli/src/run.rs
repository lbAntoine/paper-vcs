//! Dispatch: parse arguments, run the command, render the result or the error.

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind as ClapKind;

use paper_core::{ErrorKind, PaperError, Suggestion};

use crate::cli::Cli;
use crate::commands;
use crate::error::{self, exit_code};
use crate::panic;
use crate::render::style::Style;
use crate::settings;

/// Runs the CLI and returns the process exit code.
pub fn run() -> ExitCode {
    panic::install();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return clap_exit(&err),
    };
    panic::debug_trigger();
    let color = settings::color_choice(&cli);
    match commands::execute(&cli, Style::stdout(color)) {
        Ok(text) => {
            print_stdout(&text);
            ExitCode::SUCCESS
        }
        Err(err) => {
            if cli.json {
                print_stdout(&error::json(&err));
            } else {
                eprint!("{}", error::human(&err, Style::stderr(color)));
            }
            ExitCode::from(exit_code(err.kind))
        }
    }
}

/// `--help` and `--version` succeed; every other argument problem is a user error (1), not
/// clap's default 2, which the CLI reserves for "needs attention". Under `--json` the error is
/// the usual JSON object on stdout.
fn clap_exit(err: &clap::Error) -> ExitCode {
    // Anything after `--` is a value, not a flag.
    let wants_json = std::env::args_os()
        .take_while(|arg| arg != "--")
        .any(|arg| arg == "--json");
    match err.kind() {
        ClapKind::DisplayHelp | ClapKind::DisplayVersion => {
            let _ = err.print();
            ExitCode::SUCCESS
        }
        _ if wants_json => {
            print_stdout(&error::json(&argument_error(err)));
            ExitCode::from(1)
        }
        _ => {
            let _ = err.print();
            ExitCode::from(1)
        }
    }
}

/// clap's complaint as a three-part error: its first line says what is wrong.
fn argument_error(err: &clap::Error) -> PaperError {
    let rendered = err.render().to_string();
    let first = rendered
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("the command line could not be understood");
    PaperError {
        kind: ErrorKind::User,
        message: first.trim_start_matches("error: ").to_owned(),
        why: "paper could not understand the command line.".to_owned(),
        next: vec![Suggestion {
            command: "paper --help".to_owned(),
            explanation: "see the commands and options".to_owned(),
        }],
    }
}

fn print_stdout(text: &str) {
    // A closed pipe (`paper history | head`) is not an error worth reporting.
    let _ = io::stdout().write_all(text.as_bytes());
}

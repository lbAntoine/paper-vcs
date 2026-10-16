//! Dispatch: parse arguments, run the command, render the result or the error.

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind as ClapKind;

use crate::cli::Cli;
use crate::commands;
use crate::error::{self, exit_code};
use crate::panic;
use crate::render::style::Style;

/// Runs the CLI and returns the process exit code.
pub fn run() -> ExitCode {
    panic::install();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => return clap_exit(&err),
    };
    panic::debug_trigger();
    match commands::execute(&cli, Style::stdout(cli.color)) {
        Ok(text) => {
            print_stdout(&text);
            ExitCode::SUCCESS
        }
        Err(err) => {
            if cli.json {
                print_stdout(&error::json(&err));
            } else {
                eprint!("{}", error::human(&err, Style::stderr(cli.color)));
            }
            ExitCode::from(exit_code(err.kind))
        }
    }
}

/// `--help` and `--version` succeed; every other argument problem is a user error (1), not
/// clap's default 2, which the CLI reserves for "needs attention".
fn clap_exit(err: &clap::Error) -> ExitCode {
    let _ = err.print();
    match err.kind() {
        ClapKind::DisplayHelp | ClapKind::DisplayVersion => ExitCode::SUCCESS,
        _ => ExitCode::from(1),
    }
}

fn print_stdout(text: &str) {
    // A closed pipe (`paper history | head`) is not an error worth reporting.
    let _ = io::stdout().write_all(text.as_bytes());
}

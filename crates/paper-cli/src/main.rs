//! The `paper` command-line interface: argument parsing, prompts and rendering over `paper-core`.

mod cli;
mod commands;
mod error;
mod panic;
mod paths;
mod prompt;
mod render;
mod run;

fn main() -> std::process::ExitCode {
    run::run()
}

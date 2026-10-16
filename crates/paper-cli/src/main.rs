//! The `paper` command-line interface: argument parsing, prompts and rendering over `paper-core`.

mod cli;
mod commands;
mod config_cmd;
mod error;
mod inspect;
mod panic;
mod paths;
mod prompt;
mod render;
mod run;
mod settings;

fn main() -> std::process::ExitCode {
    run::run()
}

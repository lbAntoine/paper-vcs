//! The `paper` command-line interface: argument parsing, prompts and rendering over `paper-core`.

mod cli;
mod error;
mod render;
mod run;

fn main() -> std::process::ExitCode {
    run::run()
}

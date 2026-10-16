//! Human and JSON rendering.

pub mod human;
pub mod json;
pub mod style;
pub mod time;

/// Extra output from `-v` (`-q` is applied where output is chosen, in `commands`).
#[derive(Debug, Clone, Copy)]
pub struct Verbosity {
    pub verbose: bool,
}

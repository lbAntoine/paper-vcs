//! Human and JSON rendering.

pub mod diff;
pub mod gc;
pub mod human;
pub mod json;
pub mod json_diff;
pub mod status;
pub mod style;
pub mod time;
pub mod undo;

/// Extra output from `-v` (`-q` is applied where output is chosen, in `commands`).
#[derive(Debug, Clone, Copy)]
pub struct Verbosity {
    pub verbose: bool,
}

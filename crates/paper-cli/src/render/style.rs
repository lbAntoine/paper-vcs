//! Colour: only on a terminal, and only when allowed.

use std::io::IsTerminal;

use crate::cli::ColorChoice;

const RESET: &str = "\x1b[0m";

/// Whether to wrap text in ANSI colour codes.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    enabled: bool,
}

impl Style {
    /// `--color always` and `never` decide outright; `auto` needs a terminal and no `NO_COLOR`.
    pub fn new(choice: ColorChoice, stream_is_terminal: bool) -> Self {
        let enabled = match choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                stream_is_terminal
                    && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
            }
        };
        Self { enabled }
    }

    pub fn stdout(choice: ColorChoice) -> Self {
        Self::new(choice, std::io::stdout().is_terminal())
    }

    pub fn stderr(choice: ColorChoice) -> Self {
        Self::new(choice, std::io::stderr().is_terminal())
    }

    fn paint(self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}{RESET}")
        } else {
            text.to_owned()
        }
    }

    /// A save's short ID.
    pub fn id(self, text: &str) -> String {
        self.paint("33", text)
    }

    pub fn error_label(self, text: &str) -> String {
        self.paint("1;31", text)
    }

    /// A removed line in a diff.
    pub fn removed(self, text: &str) -> String {
        self.paint("31", text)
    }

    /// Removed words in a word diff: red and struck through.
    pub fn removed_words(self, text: &str) -> String {
        self.paint("31;9", text)
    }

    /// Whether colour codes are written at all.
    pub fn enabled(self) -> bool {
        self.enabled
    }

    /// An added line in a diff.
    pub fn added(self, text: &str) -> String {
        self.paint("32", text)
    }

    /// Secondary information such as times and git terms.
    pub fn dim(self, text: &str) -> String {
        self.paint("2", text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_leaves_text_alone() {
        assert_eq!(Style::new(ColorChoice::Never, true).id("kmrw"), "kmrw");
    }

    #[test]
    fn always_wraps_text_in_codes() {
        assert_eq!(
            Style::new(ColorChoice::Always, false).id("kmrw"),
            "\x1b[33mkmrw\x1b[0m"
        );
    }

    #[test]
    fn auto_without_a_terminal_is_plain() {
        assert_eq!(Style::new(ColorChoice::Auto, false).dim("x"), "x");
    }
}

//! The save-message prompt, with its input and output injected so it can be unit-tested.

use std::io::{BufRead, Write};

use paper_core::PaperError;

/// Asks `Describe this save: ` on `output` and reads one line from `input`.
///
/// When `is_tty` is false nothing is printed or read and the result is an empty string, which
/// `save` rejects with its own "a save needs a message" error. On a terminal the answer is
/// trimmed; an empty answer is a user error.
///
/// # Errors
///
/// A user error (three parts) if the answer is empty or the input can't be read.
///
/// # Panics
///
/// Never.
#[allow(dead_code)] // wired into `save` in GREEN
pub fn ask_for_message(
    _is_tty: bool,
    _input: &mut dyn BufRead,
    _output: &mut dyn Write,
) -> Result<String, PaperError> {
    todo!("PV-011: save-message prompt")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use paper_core::ErrorKind;

    use super::ask_for_message;

    fn ask(is_tty: bool, answer: &str) -> (Result<String, paper_core::PaperError>, String) {
        let mut input = Cursor::new(answer.as_bytes().to_vec());
        let mut output = Vec::new();
        let result = ask_for_message(is_tty, &mut input, &mut output);
        (result, String::from_utf8(output).unwrap())
    }

    #[test]
    fn the_prompt_asks_to_describe_the_save() {
        let (_, shown) = ask(true, "Fix intro\n");
        assert_eq!(shown, "Describe this save: ");
    }

    #[test]
    fn the_answer_is_trimmed() {
        let (result, _) = ask(true, "  Fix intro \r\n");
        assert_eq!(result.unwrap(), "Fix intro");
    }

    #[test]
    fn an_empty_answer_is_a_three_part_user_error() {
        for answer in ["\n", "   \n", ""] {
            let (result, _) = ask(true, answer);
            let error = result.unwrap_err();
            assert_eq!(error.kind, ErrorKind::User, "{answer:?}");
            assert!(
                !error.message.is_empty() && !error.why.is_empty(),
                "{error:?}"
            );
            assert!(!error.next.is_empty(), "{error:?}");
        }
    }

    #[test]
    fn without_a_terminal_nothing_is_asked_or_read() {
        let (result, shown) = ask(false, "Fix intro\n");
        assert_eq!(result.unwrap(), "");
        assert_eq!(shown, "");
    }
}

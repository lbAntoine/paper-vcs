//! The save-message prompt, with its input and output injected so it can be unit-tested.

use std::io::{BufRead, Write};

use paper_core::{ErrorKind, PaperError, Suggestion};

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
pub fn ask_for_message(
    is_tty: bool,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<String, PaperError> {
    if !is_tty {
        return Ok(String::new());
    }
    // A prompt that can't be shown still lets the answer be read.
    let _ = write!(output, "Describe this save: ");
    let _ = output.flush();
    let mut line = String::new();
    input.read_line(&mut line).map_err(|err| PaperError {
        kind: ErrorKind::User,
        message: "Paper could not read your answer".to_owned(),
        why: format!("Reading from the terminal failed: {err}."),
        next: vec![suggestion()],
    })?;
    let message = line.trim();
    if message.is_empty() {
        return Err(PaperError {
            kind: ErrorKind::User,
            message: "A save needs a message".to_owned(),
            why: "The message says what changed, so you can find this save later.".to_owned(),
            next: vec![suggestion()],
        });
    }
    Ok(message.to_owned())
}

fn suggestion() -> Suggestion {
    Suggestion {
        command: "paper save \"Describe what changed\"".to_owned(),
        explanation: "save with a message".to_owned(),
    }
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

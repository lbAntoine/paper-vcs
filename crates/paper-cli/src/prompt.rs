//! The save-message prompt, with its input and output injected so it can be unit-tested.

use std::io::{BufRead, Write};
use std::path::Path;

use paper_core::config::{ConfigFile, Key};
use paper_core::{ErrorKind, PaperError, Suggestion};
use paper_git::Identity;

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
    input
        .read_line(&mut line)
        .map_err(|err| PaperError::project_failure("Paper could not read your answer", &err))?;
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

/// Asks `Your name: ` then `Your email: ` on `output`, reading one line each from `input`,
/// stores both in the user config at `user_config` (keeping what is already there), says where
/// on `output`, and returns the identity.
///
/// When `is_tty` is false nothing is printed, read or written and the result is `None`, so the
/// caller shows the "who are you" error instead.
///
/// # Errors
///
/// A user error (three parts) if an answer is empty or invalid (an email needs an `@`), or the
/// input can't be read; a project error if the file can't be written. Nothing is written
/// unless both answers are valid.
///
/// # Panics
///
/// Never.
pub fn ask_for_identity(
    is_tty: bool,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    user_config: &Path,
) -> Result<Option<Identity>, PaperError> {
    if !is_tty {
        return Ok(None);
    }
    let name = Key::UserName.parse_value(&ask(input, output, "Your name: ")?)?;
    let email = Key::UserEmail.parse_value(&ask(input, output, "Your email: ")?)?;
    let file = ConfigFile::load(user_config)?
        .with(Key::UserName, name.clone())?
        .with(Key::UserEmail, email.clone())?;
    file.write(user_config)?;
    let _ = writeln!(
        output,
        "Saved your name and email in {} (change them any time with `paper config set`).",
        user_config.display()
    );
    Ok(Some(Identity {
        name: name.to_string(),
        email: email.to_string(),
    }))
}

fn ask(
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    question: &str,
) -> Result<String, PaperError> {
    // A prompt that can't be shown still lets the answer be read.
    let _ = write!(output, "{question}");
    let _ = output.flush();
    let mut line = String::new();
    input
        .read_line(&mut line)
        .map_err(|err| PaperError::project_failure("Paper could not read your answer", &err))?;
    Ok(line)
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

    mod identity {
        use std::io::Cursor;

        use paper_core::ErrorKind;
        use paper_core::config::{ConfigFile, Key, Value};

        use super::super::ask_for_identity;

        fn ask(
            is_tty: bool,
            answers: &str,
            path: &std::path::Path,
        ) -> (
            Result<Option<paper_git::Identity>, paper_core::PaperError>,
            String,
        ) {
            let mut input = Cursor::new(answers.as_bytes().to_vec());
            let mut output = Vec::new();
            let result = ask_for_identity(is_tty, &mut input, &mut output, path);
            (result, String::from_utf8(output).unwrap())
        }

        #[test]
        fn it_asks_for_name_then_email_and_returns_them_trimmed() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("paper").join("config.toml");
            let (result, shown) = ask(true, "  Mina Park \r\nmina@uni.example\n", &path);
            let identity = result.unwrap().unwrap();
            assert_eq!(identity.name, "Mina Park");
            assert_eq!(identity.email, "mina@uni.example");
            assert!(shown.starts_with("Your name: "), "{shown:?}");
            assert!(shown.contains("Your email: "), "{shown:?}");
        }

        #[test]
        fn it_stores_both_values_in_the_user_config_and_says_where() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("paper").join("config.toml");
            let (_, shown) = ask(true, "Mina Park\nmina@uni.example\n", &path);
            let file = ConfigFile::load(&path).unwrap();
            assert_eq!(
                file.get(Key::UserName).unwrap(),
                Some(Value::Text("Mina Park".to_owned()))
            );
            assert_eq!(
                file.get(Key::UserEmail).unwrap(),
                Some(Value::Text("mina@uni.example".to_owned()))
            );
            assert!(shown.contains("config.toml"), "{shown:?}");
        }

        #[test]
        fn it_keeps_what_the_user_config_already_holds() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.toml");
            std::fs::write(&path, "# mine\n[ui]\ncolor = \"never\"\n").unwrap();
            let _ = ask(true, "Mina Park\nmina@uni.example\n", &path);
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(
                text.starts_with("# mine\n[ui]\ncolor = \"never\"\n"),
                "{text}"
            );
        }

        #[test]
        fn without_a_terminal_nothing_is_asked_read_or_written() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("config.toml");
            let (result, shown) = ask(false, "Mina Park\nmina@uni.example\n", &path);
            assert_eq!(result.unwrap(), None);
            assert_eq!(shown, "");
            assert!(!path.exists());
        }

        #[test]
        fn an_empty_name_or_a_bad_email_is_a_three_part_user_error_and_writes_nothing() {
            for answers in [
                "\nmina@uni.example\n",
                "Mina Park\nnobody\n",
                "Mina Park\n\n",
                "",
            ] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("config.toml");
                let (result, _) = ask(true, answers, &path);
                let error = result.unwrap_err();
                assert_eq!(error.kind, ErrorKind::User, "{answers:?}");
                assert!(
                    !error.message.is_empty() && !error.why.is_empty(),
                    "{error:?}"
                );
                assert!(!error.next.is_empty(), "{error:?}");
                assert!(!path.exists(), "{answers:?}");
            }
        }
    }
}

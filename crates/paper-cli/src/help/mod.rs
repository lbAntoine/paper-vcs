//! `paper help [<topic|command>]`: our own help command, replacing clap's, so that topics such
//! as `large-files` sit next to the per-command help.

use std::fmt::Write as _;

use clap::CommandFactory;
use paper_core::{ErrorKind, PaperError, Suggestion};
use serde_json::json;

use crate::cli::Cli;

/// A help topic: a name, a one-line summary and the text.
struct Topic {
    name: &'static str,
    summary: &'static str,
    text: &'static str,
}

/// Every topic. Adding one is adding a row (and its text file).
const TOPICS: [Topic; 1] = [Topic {
    name: "large-files",
    summary: "What happens to big files, and what to do about them",
    text: include_str!("large_files.txt"),
}];

type Output = Result<String, PaperError>;

/// Runs `paper help [<topic|command>...]` and returns the text for stdout.
pub fn run(cli: &Cli, words: &[String]) -> Output {
    let Some((first, rest)) = words.split_first() else {
        return Ok(if cli.json {
            topic_list_json()
        } else {
            overview()
        });
    };
    if rest.is_empty()
        && let Some(topic) = TOPICS.iter().find(|topic| topic.name == first)
    {
        return Ok(if cli.json {
            topic_json(topic)
        } else {
            topic.text.to_owned()
        });
    }
    let mut root = Cli::command();
    root.build();
    let mut command = &mut root;
    for word in words {
        let known = command_names(command);
        command = match command.find_subcommand_mut(word) {
            Some(found) if word != "help" => found,
            _ => return Err(unknown(word, &known)),
        };
    }
    if cli.json {
        return Err(text_only(&words.join(" ")));
    }
    Ok(command.render_help().to_string())
}

fn command_names(command: &clap::Command) -> Vec<String> {
    command
        .get_subcommands()
        .map(|command| command.get_name().to_owned())
        .filter(|name| name != "help")
        .collect()
}

/// The general `--help`, then the topic list.
fn overview() -> String {
    let mut out = Cli::command().render_help().to_string();
    out.push_str("\nHelp topics:\n");
    let width = TOPICS.iter().map(|t| t.name.len()).max().unwrap_or(0);
    for topic in &TOPICS {
        let _ = writeln!(out, "  {:<width$}  {}", topic.name, topic.summary);
    }
    out.push_str(
        "\nRun `paper help <topic>` to read one, or `paper help <command>` for a command.\n",
    );
    out
}

fn topic_list_json() -> String {
    let topics: Vec<_> = TOPICS
        .iter()
        .map(|t| json!({"name": t.name, "summary": t.summary}))
        .collect();
    format!("{}\n", json!({"schema": 1, "topics": topics}))
}

fn topic_json(topic: &Topic) -> String {
    let value = json!({
        "schema": 1,
        "topic": topic.name,
        "summary": topic.summary,
        "text": topic.text,
    });
    format!("{value}\n")
}

fn text_only(command: &str) -> PaperError {
    PaperError {
        kind: ErrorKind::User,
        message: format!("Help for `paper {command}` is available as text only."),
        why: "only help topics have a machine-readable form.".to_owned(),
        next: vec![
            Suggestion {
                command: format!("paper help {command}"),
                explanation: "read the help as text".to_owned(),
            },
            Suggestion {
                command: "paper help --json".to_owned(),
                explanation: "list the help topics as JSON".to_owned(),
            },
        ],
    }
}

fn unknown(word: &str, commands: &[String]) -> PaperError {
    let topics: Vec<&str> = TOPICS.iter().map(|t| t.name).collect();
    PaperError {
        kind: ErrorKind::User,
        message: format!("There is no help topic or command called `{word}`."),
        why: format!(
            "`{word}` is neither a help topic ({}) nor a command.\n        Commands: {}",
            topics.join(", "),
            wrap(&commands.join(", "), 70)
        ),
        next: vec![
            Suggestion {
                command: "paper help".to_owned(),
                explanation: "see the commands and the help topics".to_owned(),
            },
            Suggestion {
                command: format!("paper help {}", topics.first().copied().unwrap_or("save")),
                explanation: "read a topic".to_owned(),
            },
        ],
    }
}

/// Breaks `text` after a comma-and-space once a line passes `width`, indenting continuations.
fn wrap(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut line_len = 0;
    for word in text.split_inclusive(' ') {
        if line_len + word.len() > width {
            out.truncate(out.trim_end().len());
            out.push_str("\n                  ");
            line_len = 0;
        }
        out.push_str(word);
        line_len += word.len();
    }
    out
}

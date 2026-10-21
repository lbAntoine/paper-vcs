//! `paper help [<topic|command>]`: our own help command, replacing clap's, so that topics such
//! as `large-files` sit next to the per-command help.

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

/// Runs `paper help [<topic|command>]` and returns the text for stdout.
pub fn run(cli: &Cli, word: Option<&str>) -> Output {
    let Some(word) = word else {
        return Ok(if cli.json {
            topic_list_json()
        } else {
            overview()
        });
    };
    if let Some(topic) = TOPICS.iter().find(|topic| topic.name == word) {
        return Ok(if cli.json {
            topic_json(topic)
        } else {
            topic.text.to_owned()
        });
    }
    let mut root = Cli::command();
    root.build();
    if let Some(command) = root.find_subcommand_mut(word) {
        if cli.json {
            return Err(text_only(word));
        }
        return Ok(command.render_help().to_string());
    }
    Err(unknown(word, &command_names(&root)))
}

fn command_names(root: &clap::Command) -> Vec<String> {
    root.get_subcommands()
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
        out.push_str(&format!("  {:<width$}  {}\n", topic.name, topic.summary));
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
        message: format!("There is no help topic or command called '{word}'."),
        why: format!(
            "the help topics are: {}. The commands are: {}.",
            topics.join(", "),
            commands.join(", ")
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

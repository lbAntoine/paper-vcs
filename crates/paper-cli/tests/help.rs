//! `paper help`: topics and per-command help, through the binary (PV-043).

mod common;

use common::{Env, Run, assert_keys, binary};

const TOPIC_SUMMARY: &str = "What happens to big files, and what to do about them";

/// Commands that must exist; the loop below also covers whatever `paper --help` lists.
const KNOWN_COMMANDS: [&str; 14] = [
    "init", "save", "history", "show", "status", "diff", "config", "activity", "undo", "redo",
    "gc", "ignore", "unignore", "restore",
];

fn topic(env: &Env) -> Run {
    env.paper(&["help", "large-files"])
}

/// The command names listed under `Commands:` in `paper --help`, without `help` itself.
fn listed_commands(env: &Env) -> Vec<String> {
    let run = env.paper(&["--help"]);
    run.assert_code(0);
    let mut in_commands = false;
    let mut names = Vec::new();
    for line in run.stdout.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
        } else if in_commands {
            if !line.starts_with("  ") {
                break;
            }
            if let Some(name) = line.split_whitespace().next() {
                names.push(name.to_owned());
            }
        }
    }
    names.retain(|name| name != "help");
    names
}

#[test]
fn help_large_files_exits_zero_with_the_topic_on_stdout() {
    let env = Env::new();

    let run = topic(&env);

    run.assert_code(0);
    assert_eq!(run.stderr, "", "{run:#?}");
    assert!(!run.stdout.trim().is_empty(), "{run:#?}");
}

#[test]
fn the_large_files_topic_names_both_limits_with_their_defaults() {
    let env = Env::new();

    let run = topic(&env);

    for needle in [
        "warn_mb",
        "max_mb",
        "large_files.warn_mb",
        "large_files.max_mb",
        "10 MB",
        "100 MB",
    ] {
        assert!(run.stdout.contains(needle), "missing {needle:?}: {run:#?}");
    }
}

#[test]
fn the_large_files_topic_explains_skipped_files_and_the_three_options() {
    let env = Env::new();

    let run = topic(&env);

    for needle in [
        "Skipped",
        "paper ignore <path>",
        "Zenodo",
        "paper config set --project large_files.max_mb <N>",
        "100 MB",
    ] {
        assert!(run.stdout.contains(needle), "missing {needle:?}: {run:#?}");
    }
}

#[test]
fn the_large_files_topic_says_git_lfs_is_planned_after_1_0_in_one_line() {
    let env = Env::new();

    let run = topic(&env);

    let lfs: Vec<&str> = run
        .stdout
        .lines()
        .filter(|line| line.contains("Git LFS"))
        .collect();
    assert_eq!(lfs.len(), 1, "{run:#?}");
    assert!(lfs[0].contains("1.0"), "{run:#?}");
}

#[test]
fn the_large_files_topic_is_25_to_40_lines_wrapped_at_80_columns() {
    let env = Env::new();

    let run = topic(&env);

    let lines = run.stdout.lines().count();
    assert!((25..=40).contains(&lines), "{lines} lines: {run:#?}");
    for line in run.stdout.lines() {
        assert!(line.chars().count() <= 80, "too wide: {line:?}");
    }
}

#[test]
fn help_alone_prints_the_general_help_then_the_topic_list() {
    let env = Env::new();
    let general = env.paper(&["--help"]);

    let run = env.paper(&["help"]);

    run.assert_code(0);
    assert!(
        run.stdout.starts_with(general.stdout.trim_end()),
        "{run:#?}"
    );
    let (_, topics) = run
        .stdout
        .split_once("Help topics:")
        .unwrap_or_else(|| panic!("no 'Help topics:' list: {run:#?}"));
    assert!(topics.contains("large-files"), "{run:#?}");
    assert!(topics.contains(TOPIC_SUMMARY), "{run:#?}");
}

#[test]
fn help_for_a_command_matches_the_command_help_exactly() {
    let env = Env::new();
    let mut commands = listed_commands(&env);
    for known in KNOWN_COMMANDS {
        assert!(
            commands.iter().any(|name| name == known),
            "`paper --help` does not list {known}: {commands:?}"
        );
    }
    commands.sort();
    commands.dedup();

    for command in &commands {
        let via_help = env.paper(&["help", command]);
        let direct = env.paper(&[command, "--help"]);

        direct.assert_code(0);
        via_help.assert_code(0);
        assert_eq!(via_help.stdout, direct.stdout, "paper help {command}");
        assert_eq!(via_help.stderr, direct.stderr, "paper help {command}");
    }
}

#[test]
fn help_for_a_command_works_inside_a_project_too() {
    let env = Env::new();
    env.project_with_three_saves();

    let via_help = env.paper(&["help", "save"]);
    let direct = env.paper(&["save", "--help"]);

    via_help.assert_code(0);
    assert_eq!(via_help.stdout, direct.stdout);
}

#[test]
fn an_unknown_topic_is_a_three_part_error_listing_topics_and_commands() {
    let env = Env::new();

    let run = env.paper(&["help", "nonsense"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("nonsense"), "{run:#?}");
    assert!(run.stderr.contains("large-files"), "{run:#?}");
    for command in ["save", "history", "status", "restore"] {
        assert!(run.stderr.contains(command), "missing {command}: {run:#?}");
    }
}

#[test]
fn the_unknown_topic_error_is_json_under_json() {
    let env = Env::new();

    let run = env.paper(&["help", "nonsense", "--json"]);

    run.assert_code(1);
    assert_eq!(run.stderr, "", "{run:#?}");
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["error"]["kind"], "user");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("nonsense"),
        "{run:#?}"
    );
}

#[test]
fn help_topic_json_has_fixed_keys() {
    let env = Env::new();

    let run = env.paper(&["help", "large-files", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(&json, &["schema", "topic", "summary", "text"]);
    assert_eq!(json["schema"], 1);
    assert_eq!(json["topic"], "large-files");
    assert_eq!(json["summary"], TOPIC_SUMMARY);
    let text = json["text"].as_str().unwrap();
    for needle in ["warn_mb", "max_mb", "paper ignore <path>", "Git LFS"] {
        assert!(text.contains(needle), "missing {needle:?}: {text}");
    }
}

#[test]
fn help_json_lists_the_topics() {
    let env = Env::new();

    let run = env.paper(&["help", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(&json, &["schema", "topics"]);
    assert_eq!(json["schema"], 1);
    let topics = json["topics"].as_array().unwrap();
    let large = topics
        .iter()
        .find(|entry| entry["name"] == "large-files")
        .unwrap_or_else(|| panic!("large-files is not listed: {run:#?}"));
    assert_keys(large, &["name", "summary"]);
    assert_eq!(large["summary"], TOPIC_SUMMARY);
}

#[test]
fn help_for_a_command_under_json_is_a_user_error_saying_it_is_text_only() {
    let env = Env::new();

    let run = env.paper(&["help", "save", "--json"]);

    run.assert_code(1);
    assert_eq!(run.stderr, "", "{run:#?}");
    let json = run.json();
    assert_eq!(json["schema"], 1);
    assert_eq!(json["error"]["kind"], "user");
    let said = format!("{} {}", json["error"]["message"], json["error"]["why"]);
    assert!(said.contains("text"), "{run:#?}");
    assert!(
        !json["error"]["next"].as_array().unwrap().is_empty(),
        "{run:#?}"
    );
}

#[test]
fn dash_h_and_dash_dash_help_still_work() {
    let env = Env::new();

    for args in [
        &["--help"][..],
        &["-h"],
        &["save", "--help"],
        &["save", "-h"],
        &["config", "set", "--help"],
    ] {
        let run = env.paper(args);

        run.assert_code(0);
        assert!(run.stdout.contains("Usage:"), "{args:?}: {run:#?}");
    }
}

const HINT: &str = "  Too large to track? See `paper help large-files`.";

/// A project whose `data/big.bin` (2 MB) is over a 1 MB `large_files.max_mb`.
fn project_with_a_too_large_file() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env.paper(&["config", "set", "--project", "large_files.max_mb", "1"])
        .assert_code(0);
    env.write_bytes("data/big.bin", &binary(2 * 1024 * 1024, 3));
    env
}

#[test]
fn status_ends_the_skipped_section_with_the_large_files_hint() {
    let env = project_with_a_too_large_file();

    let run = env.paper(&["status"]);

    run.assert_code(0);
    assert!(run.stdout.contains("data/big.bin"), "{run:#?}");
    assert!(run.stdout.contains("too large to track"), "{run:#?}");
    let skipped = run.stdout.split_once("Skipped").unwrap().1;
    assert_eq!(skipped.lines().last(), Some(HINT), "{run:#?}");
}

#[test]
fn status_does_not_print_the_hint_for_a_nested_repository_skip() {
    let env = Env::new();
    env.project_with_three_saves();
    env.git_work(&["init", "-q", "vendor"]);
    env.write("vendor/lib.tex", "nested\n");

    let run = env.paper(&["status"]);

    run.assert_code(0);
    assert!(run.stdout.contains("a separate git repository"), "{run:#?}");
    assert!(!run.stdout.contains("paper help large-files"), "{run:#?}");
}

#[test]
fn status_json_for_a_too_large_file_keeps_its_key_set() {
    let env = project_with_a_too_large_file();

    let run = env.paper(&["--json", "status"]);

    run.assert_code(0);
    assert_keys(
        &run.json(),
        &[
            "schema",
            "line",
            "last",
            "changes",
            "skipped",
            "large",
            "outside_paper",
        ],
    );
    assert!(!run.stdout.contains("paper help large-files"), "{run:#?}");
}

#[test]
fn snapshot_help_large_files() {
    let env = Env::new();
    insta::assert_snapshot!("help_large_files", topic(&env).stdout);
}

#[test]
fn snapshot_help_alone() {
    let env = Env::new();
    insta::assert_snapshot!("help_alone", env.paper(&["help"]).stdout);
}

#[test]
fn snapshot_help_unknown_topic() {
    let env = Env::new();
    insta::assert_snapshot!(
        "help_unknown_topic",
        env.paper(&["help", "nonsense"]).stderr
    );
}

#[test]
fn help_for_a_nested_command_matches_its_command_help() {
    let env = Env::new();

    let via_help = env.paper(&["help", "config", "get"]);
    let direct = env.paper(&["config", "get", "--help"]);

    via_help.assert_code(0);
    assert_eq!(via_help.stdout, direct.stdout);
    env.paper(&["help", "config", "nonsense"]).assert_code(1);
}

#[test]
fn snapshot_help_error_cases() {
    let env = Env::new();
    for (name, args) in [
        ("help_help", &["help", "help"][..]),
        ("help_topic_extra", &["help", "large-files", "extra"]),
        ("help_command_extra", &["help", "save", "extra"]),
        ("help_nested_extra", &["help", "config", "get", "extra"]),
        ("help_under_parent", &["help", "config", "nonsense"]),
    ] {
        let run = env.paper(args);
        run.assert_three_part_error(1);
        insta::assert_snapshot!(name, run.stderr);
    }
}

#[test]
fn the_unknown_word_json_error_has_no_layout_padding() {
    let env = Env::new();

    let run = env.paper(&["help", "nonsense", "--json"]);

    run.assert_code(1);
    let json = run.json();
    let why = json["error"]["why"].as_str().unwrap();
    assert!(!why.contains("  "), "{why:?}");
    insta::assert_snapshot!("help_unknown_json", run.stdout);
}

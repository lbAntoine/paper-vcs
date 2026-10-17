//! `paper activity`: the listing, `-n`, `--json`, that it never records or snapshots, and the
//! entries the other commands leave behind.

mod common;

use common::{Env, assert_keys, settings};

/// insta settings that blank op ids (the random ones), relative times and the padding after
/// them, so the snapshots are stable.
fn activity_settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"(?m)^[0-9a-f]{4,32} *", "[ID]  ");
    settings.add_filter(r#""(id|short_id)": "[0-9a-f]{4,32}""#, r#""$1": "[ID]""#);
    settings.add_filter(
        r"(just now|\d+ (?:minute|hour)s? ago|yesterday|\d{4}-\d{2}-\d{2}) *",
        "[TIME]  ",
    );
    settings
}

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

/// The descriptions in `paper --json activity -n 100`, newest first.
fn descriptions(env: &Env) -> Vec<String> {
    let run = env.paper(&["activity", "-n", "100", "--json"]);
    run.assert_code(0);
    run.json()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["description"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn activity_lists_what_was_done_newest_first() {
    let env = project();

    let run = env.paper(&["activity"]);

    run.assert_code(0);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 5, "{run:#?}");
    assert!(lines[0].ends_with("save \"Rewrite intro\""), "{run:#?}");
    assert!(lines[1].ends_with("edited sections/intro.tex"), "{run:#?}");
    assert!(lines[2].ends_with("save \"Add related work\""), "{run:#?}");
    assert!(lines[3].ends_with("edited related.tex"), "{run:#?}");
    assert!(lines[4].ends_with("start the project"), "{run:#?}");
    settings().bind(|| {
        activity_settings().bind(|| {
            insta::assert_snapshot!("activity_human", env.redact(&run.stdout));
        });
    });
}

#[test]
fn activity_lines_start_with_a_unique_hex_short_id_and_then_a_relative_time() {
    let env = project();

    let run = env.paper(&["activity"]);

    run.assert_code(0);
    let mut ids = Vec::new();
    for line in run.stdout.lines() {
        let id = line.split_whitespace().next().unwrap();
        assert!(id.len() >= 4, "{line}");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "{line}");
        let after_id = line[id.len()..].trim_start();
        assert!(after_id.starts_with("just now"), "{line}");
        ids.push(id.to_owned());
    }
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "unique short ids: {ids:?}");
}

#[test]
fn activity_n_limits_the_entries_to_the_newest() {
    let env = project();

    let run = env.paper(&["activity", "-n", "2"]);

    run.assert_code(0);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{run:#?}");
    assert!(lines[0].ends_with("save \"Rewrite intro\""), "{run:#?}");
    assert!(lines[1].ends_with("edited sections/intro.tex"), "{run:#?}");
}

#[test]
fn activity_lists_twenty_entries_by_default() {
    let env = project();
    for n in 0..12 {
        env.write("notes.txt", &format!("note {n}\n"));
        env.paper(&["save", &format!("Note {n}")]).assert_code(0);
    }

    let run = env.paper(&["activity"]);

    run.assert_code(0);
    assert_eq!(run.stdout.lines().count(), 20, "{run:#?}");
}

#[test]
fn activity_json_has_a_fixed_key_set_for_every_entry() {
    let env = project();

    let run = env.paper(&["activity", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(&json, &["schema", "entries"]);
    assert_eq!(json["schema"], 1);
    let entries = json["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 5);
    for entry in entries {
        assert_keys(entry, &["id", "short_id", "time", "kind", "description"]);
        let id = entry["id"].as_str().unwrap();
        let short = entry["short_id"].as_str().unwrap();
        assert_eq!(id.len(), 32, "{entry}");
        assert!(short.len() >= 4 && id.starts_with(short), "{entry}");
        assert!(entry["time"].as_str().unwrap().contains('T'), "{entry}");
    }
    assert_eq!(entries[0]["kind"], "save");
    assert_eq!(entries[1]["kind"], "snapshot");
    assert_eq!(entries[4]["kind"], "init");
    assert_eq!(entries[4]["description"], "start the project");
    settings().bind(|| {
        activity_settings().bind(|| {
            insta::assert_snapshot!("activity_json", env.redact_json(&run.stdout));
        });
    });
}

#[test]
fn activity_json_respects_the_limit_and_works_as_a_global_flag() {
    let env = project();

    let run = env.paper(&["--json", "activity", "-n", "1"]);

    run.assert_code(0);
    assert_eq!(run.json()["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn activity_does_not_snapshot_and_records_nothing() {
    let env = project();
    let before = descriptions(&env);
    env.write("unrecorded.txt", "edit paper has not seen\n");

    env.paper(&["activity"]).assert_code(0);
    env.paper(&["activity", "--json"]).assert_code(0);

    let after = descriptions(&env);
    assert_eq!(after, before, "listing the log must not add to it");
    assert!(after.iter().all(|d| !d.contains("unrecorded")), "{after:?}");
}

#[test]
fn a_status_after_an_unrecorded_edit_adds_it_to_the_activity() {
    let env = project();
    env.write("unrecorded.txt", "edit paper has not seen\n");

    env.paper(&["status"]).assert_code(0);

    let after = descriptions(&env);
    assert_eq!(after[0], "edited unrecorded.txt", "{after:?}");
    assert_eq!(after.len(), 6);
}

#[test]
fn read_only_commands_record_an_edits_entry_only_when_files_changed() {
    let env = project();
    let start = descriptions(&env).len();
    let commands: [&[&str]; 4] = [&["status"], &["diff"], &["history"], &["show", "last"]];

    for command in commands {
        env.paper(command).assert_code(0);
        assert_eq!(
            descriptions(&env).len(),
            start,
            "{command:?} on a clean project"
        );
    }
    let mut expected = start;
    for (n, command) in commands.iter().enumerate() {
        env.write(&format!("edit{n}.txt"), "new\n");
        env.paper(command).assert_code(0);
        expected += 1;
        let entries = descriptions(&env);
        assert_eq!(entries.len(), expected, "{command:?} after an edit");
        assert_eq!(entries[0], format!("edited edit{n}.txt"), "{command:?}");
        env.paper(command).assert_code(0);
        assert_eq!(descriptions(&env).len(), expected, "{command:?} twice");
    }
}

#[test]
fn save_after_edits_leaves_an_edits_entry_and_a_save_entry() {
    let env = project();
    let start = descriptions(&env).len();
    env.write("conclusion.tex", "The end\n");

    env.paper(&["save", "Add conclusion"]).assert_code(0);

    let entries = descriptions(&env);
    assert_eq!(entries.len(), start + 2);
    assert_eq!(entries[0], "save \"Add conclusion\"");
    assert_eq!(entries[1], "edited conclusion.tex");
}

#[test]
fn restore_after_edits_leaves_an_edits_entry_and_a_restore_entry() {
    let env = project();
    let start = descriptions(&env).len();
    env.write("sections/intro.tex", "Intro v3, not saved\n");

    env.paper(&["restore", "sections/intro.tex"]).assert_code(0);

    let entries = descriptions(&env);
    assert_eq!(entries.len(), start + 2);
    assert_eq!(entries[0], "restore sections/intro.tex from last");
    assert_eq!(entries[1], "edited sections/intro.tex");
}

#[test]
fn init_leaves_one_entry_and_adopting_says_so() {
    let fresh = Env::new();
    fresh.write("main.tex", "\\documentclass{article}\n");
    fresh.paper(&["init"]).assert_code(0);
    assert_eq!(descriptions(&fresh), ["start the project"]);

    let adopted = Env::new();
    adopted.plain_git_repo_with_side_branch();
    adopted.paper(&["init"]).assert_code(0);
    assert_eq!(descriptions(&adopted), ["adopt the git repository"]);
}

#[test]
fn activity_outside_a_project_is_a_three_part_error() {
    let env = Env::new();
    env.paper(&["activity"]).assert_three_part_error(1);
}

#[test]
fn activity_on_a_busy_project_exits_3_like_every_command() {
    let env = Env::new();
    let _held = env.project_with_three_saves();

    let run = env.paper_in(
        &env.work,
        &[("PAPER_DEBUG_LOCK_WAIT_MS", "200")],
        &["activity"],
    );

    run.assert_code(3);
    assert!(
        run.stderr
            .contains("Another paper command is running in this project"),
        "{run:#?}"
    );
}

#[test]
fn the_repository_stays_plain_git_after_a_session_of_commands() {
    let env = project();
    env.write("extra.tex", "more\n");
    env.paper(&["save", "More"]).assert_code(0);
    env.paper(&["activity"]).assert_code(0);

    env.fsck();
    assert_eq!(
        env.subjects(),
        ["More", "Rewrite intro", "Add related work", "Start project"]
    );
    assert_eq!(env.git_work(&["status", "--porcelain"]).trim(), "");
}

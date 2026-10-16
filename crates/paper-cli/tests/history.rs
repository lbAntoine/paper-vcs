//! `paper history`.

mod common;

use common::{Env, settings};

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

fn messages(stdout: &str) -> Vec<&str> {
    ["Rewrite intro", "Add related work", "Start project"]
        .into_iter()
        .filter(|message| stdout.contains(message))
        .collect()
}

#[test]
fn history_lists_saves_newest_first() {
    let env = project();

    let run = env.paper(&["history"]);

    run.assert_code(0);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{run:#?}");
    assert!(lines[0].contains("Rewrite intro"), "{run:#?}");
    assert!(lines[1].contains("Add related work"), "{run:#?}");
    assert!(lines[2].contains("Start project"), "{run:#?}");
    let id = env.change_id("HEAD");
    assert!(lines[0].starts_with(&id[..4]), "id first: {run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("history_human", env.redact(&run.stdout));
    });
}

#[test]
fn history_marks_the_line_tip() {
    let env = project();
    let run = env.paper(&["history"]);
    run.assert_code(0);
    assert!(
        run.stdout.lines().next().unwrap().contains("main"),
        "{run:#?}"
    );
}

#[test]
fn history_n_limits_the_number_of_saves() {
    let env = project();
    let run = env.paper(&["history", "-n", "2"]);
    run.assert_code(0);
    assert_eq!(messages(&run.stdout), ["Rewrite intro", "Add related work"]);
}

#[test]
fn history_with_a_path_lists_only_saves_touching_it() {
    let env = project();
    let run = env.paper(&["history", "sections/intro.tex"]);
    run.assert_code(0);
    assert_eq!(messages(&run.stdout), ["Rewrite intro", "Start project"]);
}

#[test]
fn history_with_a_directory_path_matches_files_under_it() {
    let env = project();
    let run = env.paper(&["history", "sections/"]);
    run.assert_code(0);
    assert_eq!(messages(&run.stdout), ["Rewrite intro", "Start project"]);
}

#[test]
fn history_all_includes_the_other_lines() {
    let env = Env::new();
    env.plain_git_repo_with_side_branch();
    env.paper(&["init"]).assert_code(0);

    let current = env.paper(&["history"]);
    let all = env.paper(&["history", "--all"]);

    current.assert_code(0);
    all.assert_code(0);
    assert!(!current.stdout.contains("Side work"), "{current:#?}");
    assert!(all.stdout.contains("Side work"), "{all:#?}");
    assert!(all.stdout.contains("Second"), "{all:#?}");
}

#[test]
fn history_graph_is_not_yet_supported() {
    let env = project();
    let run = env.paper(&["history", "--graph"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("not yet"), "{run:#?}");
    settings().bind(|| {
        insta::assert_snapshot!("history_graph_not_yet", env.redact(&run.stderr));
    });
}

#[test]
fn history_json_has_schema_1_and_the_saves() {
    let env = project();

    let run = env.paper(&["--json", "history"]);

    run.assert_code(0);
    let json = run.json();
    assert_eq!(json["schema"], 1);
    let saves = json["saves"].as_array().unwrap();
    assert_eq!(saves.len(), 3);
    assert_eq!(saves[0]["message"], "Rewrite intro");
    assert_eq!(saves[2]["message"], "Start project");
    settings().bind(|| {
        insta::assert_snapshot!("history_json", env.redact_json(&run.stdout));
    });
}

#[test]
fn history_snapshots_first_without_changing_the_saves() {
    let env = project();
    env.write("unsaved.txt", "draft\n");
    let run = env.paper(&["history"]);
    run.assert_code(0);
    assert!(
        !run.stdout.contains("unsaved"),
        "drafts are not saves: {run:#?}"
    );
    assert_eq!(env.subjects().len(), 3);
}

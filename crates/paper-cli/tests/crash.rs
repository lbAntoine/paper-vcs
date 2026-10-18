//! Crash recovery (PV-025), through the real binary: `PAPER_DEBUG_CRASH_AT=<point>` makes a
//! debug `paper` abort at that point of a command; the next command must repair what was half
//! done. Everything is checked on disk (bytes), with the real `git` binary (`rev-parse`,
//! `symbolic-ref`, `ls-files -s`, `fsck --strict`), in `exported.json` and in `paper activity`.

mod common;

use std::collections::BTreeMap;
use std::fs;

use common::{Env, Run};
use paper_core::activity::ActivityLog;

// ---- helpers (their own sanity tests are at the bottom) ----

/// Runs `paper args` with `PAPER_DEBUG_CRASH_AT=point`, whatever happens.
fn run_with_crash_env(env: &Env, point: &str, args: &[&str]) -> Run {
    env.paper_in(&env.work, &[("PAPER_DEBUG_CRASH_AT", point)], args)
}

/// Runs `paper args` and requires it to die at `point`: a non-success exit (abort codes differ
/// by operating system) and no ordinary result on stdout.
fn crash(env: &Env, point: &str, args: &[&str]) -> Run {
    let run = run_with_crash_env(env, point, args);
    assert_ne!(
        run.code, 0,
        "paper {args:?} should abort at {point}, but it finished: {run:#?}"
    );
    run
}

fn paper_dir(env: &Env) -> std::path::PathBuf {
    env.work.join(".git/paper")
}

fn marker_exists(env: &Env) -> bool {
    paper_dir(env).join("checkout-pending").exists()
}

fn exported(env: &Env) -> serde_json::Value {
    let bytes = fs::read(paper_dir(env).join("exported.json")).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// `(current line, tip, draft)` of the head op's view, read from the activity log.
fn head_view(env: &Env) -> (String, Vec<(String, String, String)>) {
    let log = ActivityLog::open(&paper_dir(env));
    let op = log.read(&log.head().unwrap().unwrap()).unwrap();
    let lines = op
        .view
        .lines()
        .map(|(name, line)| {
            (
                name.to_string(),
                line.tip.as_str().to_owned(),
                line.draft.as_str().to_owned(),
            )
        })
        .collect();
    (op.view.current_line().to_string(), lines)
}

fn head_tip(env: &Env) -> String {
    let (current, lines) = head_view(env);
    lines
        .into_iter()
        .find(|(name, _, _)| *name == current)
        .unwrap()
        .1
}

/// `(mode, object id, path)` of every file in the index.
fn index_listing(env: &Env) -> Vec<(String, String, String)> {
    env.git_work(&["ls-files", "-s"])
        .lines()
        .map(|line| {
            let (meta, path) = line.split_once('\t').unwrap();
            let mut parts = meta.split(' ');
            let mode = parts.next().unwrap().to_owned();
            let id = parts.next().unwrap().to_owned();
            (mode, id, path.to_owned())
        })
        .collect()
}

/// `(mode, object id, path)` of every file in the tree of `rev`.
fn tree_listing(env: &Env, rev: &str) -> Vec<(String, String, String)> {
    env.git_work(&["ls-tree", "-r", rev])
        .lines()
        .map(|line| {
            let (meta, path) = line.split_once('\t').unwrap();
            let mut parts = meta.split(' ');
            let mode = parts.next().unwrap().to_owned();
            let _kind = parts.next().unwrap();
            let id = parts.next().unwrap().to_owned();
            (mode, id, path.to_owned())
        })
        .collect()
}

/// What I4 and I6 promise after any command, crashed or not: git's refs, HEAD, the index and
/// `exported.json` all match the head op's view; no marker is left; fsck is clean.
fn assert_consistent(env: &Env) {
    let (current, lines) = head_view(env);
    let exported = exported(env);
    assert_eq!(exported["head"], current.as_str(), "exported.json head");
    for (name, tip, draft) in &lines {
        for (ref_name, id) in [
            (format!("refs/heads/{name}"), tip),
            (format!("refs/paper/draft/{name}"), draft),
        ] {
            assert_eq!(
                env.git_work(&["rev-parse", &ref_name]).trim(),
                id,
                "{ref_name}"
            );
            assert_eq!(
                exported["refs"][&ref_name],
                id.as_str(),
                "exported.json {ref_name}"
            );
        }
    }
    assert_eq!(
        env.git_work(&["symbolic-ref", "--short", "HEAD"]).trim(),
        current
    );
    let tip = head_tip(env);
    assert_eq!(index_listing(env), tree_listing(env, &tip), "index vs tip");
    assert!(!marker_exists(env), "checkout-pending is gone");
    env.fsck();
}

/// The activity descriptions, newest first.
fn activity(env: &Env) -> Vec<String> {
    let run = env.paper(&["activity", "-n", "100", "--json"]);
    run.assert_code(0);
    run.json()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["description"].as_str().unwrap().to_owned())
        .collect()
}

/// The working files (outside `.git`) by relative path, as bytes.
fn files(env: &Env) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if name == ".git" {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(name, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&env.work, &env.work, &mut out);
    out
}

fn bytes(env: &Env, path: &str) -> Option<Vec<u8>> {
    fs::read(env.work.join(path)).ok()
}

/// Whether some commit reachable from any ref (drafts and the keep chain included) holds
/// `path` with exactly `content`: the work is recoverable.
fn content_is_reachable(env: &Env, path: &str, content: &str) -> bool {
    env.git_work(&["rev-list", "--all"]).lines().any(|commit| {
        let spec = format!("{commit}:{path}");
        let mut command = std::process::Command::new("git");
        let output = command
            .arg("-C")
            .arg(&env.work)
            .args(["cat-file", "blob", &spec])
            .output()
            .unwrap();
        output.status.success() && output.stdout == content.as_bytes()
    })
}

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

/// Three saves, then `sections/intro.tex` and `related.tex` edited to v3 and recorded as an
/// edits entry by a plain `paper status`. The next undo takes both files back.
fn project_with_recorded_edits() -> Env {
    let env = project();
    env.write("sections/intro.tex", "Intro v3\n");
    env.write("related.tex", "Related v3\n");
    env.paper(&["status"]).assert_code(0);
    env
}

const INTRO_V2: &[u8] = b"Intro v2\n";
const RELATED_V2: &[u8] = b"Related work\n";
const INTRO_V3: &[u8] = b"Intro v3\n";
const RELATED_V3: &[u8] = b"Related v3\n";

/// Crash points after which the op is on disk (the head moved).
fn op_committed(point: &str) -> bool {
    matches!(
        point,
        "after-op"
            | "after-index"
            | "after-refs"
            | "after-export"
            | "mid-checkout"
            | "after-checkout"
    )
}

/// Defines `#[test]`s that run `$scenario("<point>")`, skipped in release builds where the
/// hook is compiled out.
macro_rules! crash_tests {
    ($scenario:ident: $($name:ident => $point:literal),+ $(,)?) => {
        $(
            #[test]
            #[cfg_attr(
                not(debug_assertions),
                ignore = "PAPER_DEBUG_CRASH_AT is compiled out of release builds"
            )]
            fn $name() {
                $scenario($point);
            }
        )+
    };
}

// ---- save ----

fn save_survives_a_crash_at(point: &str) {
    let env = project_with_recorded_edits();
    let before_activity = activity(&env);
    let before_files = files(&env);
    let before_tip = env.git_work(&["rev-parse", "refs/heads/main"]);
    let before_log = env.subjects();

    crash(&env, point, &["save", "Third draft"]);

    if point == "after-index" {
        // The deferred PV-022 check: the index is written before the refs and HEAD.
        assert_eq!(
            env.git_work(&["rev-parse", "refs/heads/main"]),
            before_tip,
            "the line ref has not moved yet"
        );
        let intro_blob = env.git_work(&["hash-object", "sections/intro.tex"]);
        assert!(
            index_listing(&env)
                .iter()
                .any(|(_, id, path)| path == "sections/intro.tex" && *id == intro_blob.trim()),
            "the index already has the new tip's content"
        );
        assert_ne!(
            index_listing(&env),
            tree_listing(&env, "refs/heads/main"),
            "the index is ahead of the refs"
        );
    }

    env.paper(&["status"]).assert_code(0);

    assert_consistent(&env);
    assert_eq!(files(&env), before_files, "no work is lost or changed");
    let mut expected = before_activity;
    let mut expected_log = before_log;
    if op_committed(point) {
        expected.insert(0, "save \"Third draft\"".to_owned());
        expected_log.insert(0, "Third draft".to_owned());
    }
    assert_eq!(activity(&env), expected, "no entry besides the save");
    assert_eq!(env.subjects(), expected_log);
    if op_committed(point) {
        assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    }
}

crash_tests! { save_survives_a_crash_at:
    save_crashed_after_snapshot_is_repaired => "after-snapshot",
    save_crashed_after_keep_is_repaired => "after-keep",
    save_crashed_after_op_is_repaired => "after-op",
    save_crashed_after_index_is_repaired => "after-index",
    save_crashed_after_refs_is_repaired => "after-refs",
    save_crashed_after_export_is_repaired => "after-export",
}

// ---- restore ----

fn restore_survives_a_crash_at(point: &str) {
    let env = project();
    env.write("sections/intro.tex", "Intro v3 unsaved\n");
    // Record the edit first, so restore's own snapshot finds nothing and every crash point
    // after it fires in the restore's own op.
    env.paper(&["status"]).assert_code(0);
    let before_activity = activity(&env);

    crash(
        &env,
        point,
        &["restore", "--from", "last~2", "sections", "related.tex"],
    );

    env.paper(&["status"]).assert_code(0);
    assert_consistent(&env);
    let after = activity(&env);
    // Nothing recorded before the crash is lost, and the unsaved edit is recoverable.
    assert!(
        after.ends_with(&before_activity),
        "{after:?} should end with {before_activity:?}"
    );
    assert!(
        content_is_reachable(&env, "sections/intro.tex", "Intro v3 unsaved\n"),
        "the unsaved edit is recorded"
    );
    // The next command settles: asking again records nothing more.
    env.paper(&["status"]).assert_code(0);
    assert_eq!(activity(&env), after);
    assert_consistent(&env);

    match point {
        "after-snapshot" => {
            assert_eq!(
                bytes(&env, "sections/intro.tex").unwrap(),
                b"Intro v3 unsaved\n"
            );
            assert!(env.work.join("related.tex").exists());
            assert_eq!(after, before_activity, "nothing was recorded");
        }
        "mid-checkout" => {
            // Deletions come before writes: related.tex went, intro.tex was not written yet.
            assert!(!env.work.join("related.tex").exists());
            assert_eq!(
                bytes(&env, "sections/intro.tex").unwrap(),
                b"Intro v3 unsaved\n"
            );
        }
        _ => {
            assert!(!env.work.join("related.tex").exists());
            assert_eq!(bytes(&env, "sections/intro.tex").unwrap(), b"Intro v1\n");
        }
    }
    if op_committed(point) && point != "mid-checkout" && point != "after-checkout" {
        assert!(
            after[0].starts_with("restore "),
            "the restore op was committed: {after:?}"
        );
    }
}

crash_tests! { restore_survives_a_crash_at:
    restore_crashed_after_snapshot_is_repaired => "after-snapshot",
    restore_crashed_mid_checkout_is_repaired => "mid-checkout",
    restore_crashed_after_checkout_is_repaired => "after-checkout",
    restore_crashed_after_keep_is_repaired => "after-keep",
    restore_crashed_after_op_is_repaired => "after-op",
    restore_crashed_after_index_is_repaired => "after-index",
    restore_crashed_after_refs_is_repaired => "after-refs",
    restore_crashed_after_export_is_repaired => "after-export",
}

// ---- undo and redo ----

/// Runs `args` (`undo` or `redo`) crashing at `point`, then the next command, and checks that
/// the command either happened completely or not at all. `before_files` are the working files
/// before the command, `after_files` the ones it would leave (`(path, bytes)`).
fn undo_family_survives_a_crash_at(
    env: &Env,
    point: &str,
    command: &str,
    before_files: &[(&str, &[u8])],
    after_files: &[(&str, &[u8])],
) {
    let before_activity = activity(env);
    let before_head = fs::read(paper_dir(env).join("activity/head")).unwrap();

    crash(env, point, &[command]);

    // The deferred PV-023 check: the marker exists from before the op to the end of the
    // checkout, so a crash anywhere in there leaves it for the next command.
    assert_eq!(
        marker_exists(env),
        point != "after-snapshot",
        "checkout-pending after a crash at {point}"
    );
    if point == "mid-checkout" {
        let rewritten = after_files
            .iter()
            .filter(|(path, content)| bytes(env, path).as_deref() == Some(*content))
            .count();
        assert_eq!(rewritten, 1, "exactly the first file was rewritten");
    }

    env.paper(&["status"]).assert_code(0);

    assert_consistent(env);
    let wanted = if op_committed(point) {
        after_files
    } else {
        before_files
    };
    for (path, content) in wanted {
        assert_eq!(bytes(env, path).as_deref(), Some(*content), "{path}");
    }
    let after = activity(env);
    if op_committed(point) {
        assert!(after[0].starts_with(command), "{after:?}");
        assert_eq!(
            after[1..],
            before_activity[..],
            "one new entry, nothing else"
        );
        // After an undo the files equal the tip; after a redo the draft holds the edits again.
        let expected = if command == "redo" {
            " M related.tex\n M sections/intro.tex\n"
        } else {
            ""
        };
        assert_eq!(env.git_work(&["status", "--porcelain"]), expected);
    } else {
        assert_eq!(after, before_activity, "the command never happened");
        assert_eq!(
            fs::read(paper_dir(env).join("activity/head")).unwrap(),
            before_head
        );
    }
    // Settled: another command changes nothing.
    env.paper(&["status"]).assert_code(0);
    assert_eq!(activity(env), after);
}

fn undo_survives_a_crash_at(point: &str) {
    let env = project_with_recorded_edits();
    undo_family_survives_a_crash_at(
        &env,
        point,
        "undo",
        &[
            ("sections/intro.tex", INTRO_V3),
            ("related.tex", RELATED_V3),
        ],
        &[
            ("sections/intro.tex", INTRO_V2),
            ("related.tex", RELATED_V2),
        ],
    );
}

fn redo_survives_a_crash_at(point: &str) {
    let env = project_with_recorded_edits();
    env.paper(&["undo"]).assert_code(0);
    undo_family_survives_a_crash_at(
        &env,
        point,
        "redo",
        &[
            ("sections/intro.tex", INTRO_V2),
            ("related.tex", RELATED_V2),
        ],
        &[
            ("sections/intro.tex", INTRO_V3),
            ("related.tex", RELATED_V3),
        ],
    );
}

crash_tests! { undo_survives_a_crash_at:
    undo_crashed_after_snapshot_is_repaired => "after-snapshot",
    undo_crashed_after_marker_is_repaired => "after-marker",
    undo_crashed_after_keep_is_repaired => "after-keep",
    undo_crashed_after_op_is_repaired => "after-op",
    undo_crashed_after_index_is_repaired => "after-index",
    undo_crashed_after_refs_is_repaired => "after-refs",
    undo_crashed_after_export_is_repaired => "after-export",
    undo_crashed_mid_checkout_is_repaired => "mid-checkout",
    undo_crashed_after_checkout_is_repaired => "after-checkout",
}

crash_tests! { redo_survives_a_crash_at:
    redo_crashed_after_snapshot_is_repaired => "after-snapshot",
    redo_crashed_after_marker_is_repaired => "after-marker",
    redo_crashed_after_keep_is_repaired => "after-keep",
    redo_crashed_after_op_is_repaired => "after-op",
    redo_crashed_after_index_is_repaired => "after-index",
    redo_crashed_after_refs_is_repaired => "after-refs",
    redo_crashed_after_export_is_repaired => "after-export",
    redo_crashed_mid_checkout_is_repaired => "mid-checkout",
    redo_crashed_after_checkout_is_repaired => "after-checkout",
}

// ---- a crash inside a command's own edits entry ----

fn crash_in_the_edits_entry_of(args: &[&str]) {
    let env = project();
    env.write("sections/intro.tex", "Intro v3 unsaved\n");
    // Not recorded yet: the command's own snapshot records it, and the crash hits that op.
    crash(&env, "after-op", args);

    env.paper(&["status"]).assert_code(0);

    assert_eq!(
        bytes(&env, "sections/intro.tex").unwrap(),
        b"Intro v3 unsaved\n",
        "the files are as before the command"
    );
    assert!(env.work.join("related.tex").exists());
    let entries = activity(&env);
    assert_eq!(entries[0], "edited sections/intro.tex", "{entries:?}");
    assert_consistent(&env);
}

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "PAPER_DEBUG_CRASH_AT is compiled out of release builds"
)]
fn a_restore_crashed_in_its_own_edits_entry_leaves_the_files_and_an_edits_entry() {
    crash_in_the_edits_entry_of(&["restore", "--from", "last~2", "sections", "related.tex"]);
}

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "PAPER_DEBUG_CRASH_AT is compiled out of release builds"
)]
fn an_undo_crashed_in_its_own_edits_entry_leaves_the_files_and_an_edits_entry() {
    crash_in_the_edits_entry_of(&["undo"]);
}

// ---- recovery leaves a move made with plain git alone ----

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "PAPER_DEBUG_CRASH_AT is compiled out of release builds"
)]
fn re_export_leaves_a_line_that_plain_git_moved_alone() {
    let env = project();
    env.write("main.tex", "changed\n");
    crash(&env, "after-op", &["save", "Crashed save"]);
    env.git_work(&["commit", "--allow-empty", "-m", "Plain commit"]);
    let plain = env.git_work(&["rev-parse", "refs/heads/main"]);

    env.paper(&["status"]).assert_code(0);

    assert_eq!(env.git_work(&["rev-parse", "refs/heads/main"]), plain);
    assert_eq!(env.subjects()[0], "Plain commit");
    env.write("main.tex", "changed again\n");
    let run = env.paper(&["save", "Refused"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("changed outside paper"), "{run:#?}");
    env.fsck();
}

// ---- an edit between the crash and the next command ----

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "PAPER_DEBUG_CRASH_AT is compiled out of release builds"
)]
fn an_edit_made_after_a_crash_is_kept_and_recorded_as_an_edits_entry() {
    let env = project_with_recorded_edits();
    crash(&env, "after-op", &["undo"]);
    // The undo committed but no file was rewritten; the researcher edits one of them.
    env.write("related.tex", "Written after the crash\n");

    env.paper(&["status"]).assert_code(0);

    assert_eq!(
        bytes(&env, "related.tex").unwrap(),
        b"Written after the crash\n",
        "the edit is never overwritten"
    );
    assert_eq!(
        bytes(&env, "sections/intro.tex").unwrap(),
        INTRO_V2,
        "the untouched file is finished"
    );
    let entries = activity(&env);
    assert_eq!(entries[0], "edited related.tex", "{entries:?}");
    assert!(entries[1].starts_with("undo"), "{entries:?}");
    assert_consistent(&env);
    assert!(content_is_reachable(
        &env,
        "related.tex",
        "Written after the crash\n"
    ));
}

#[test]
#[cfg(not(debug_assertions))]
fn release_builds_ignore_the_crash_variable() {
    let env = project();
    env.write("main.tex", "changed\n");
    let run = run_with_crash_env(&env, "after-op", &["save", "Not crashed"]);
    run.assert_code(0);
    assert_eq!(env.subjects()[0], "Not crashed");
}

// ---- sanity tests for the helpers above: these pass on main today ----

#[test]
fn the_runner_ignores_a_point_that_does_not_exist() {
    let env = project();
    env.write("main.tex", "changed\n");

    let run = run_with_crash_env(&env, "no-such-point", &["save", "Fine"]);

    run.assert_code(0);
    assert_eq!(env.subjects()[0], "Fine");
    assert_consistent(&env);
}

#[test]
#[should_panic(expected = "should abort at no-such-point")]
fn the_crash_helper_demands_a_non_success_exit() {
    let env = project();
    env.write("main.tex", "changed\n");
    crash(&env, "no-such-point", &["save", "Fine"]);
}

#[test]
fn the_consistency_check_accepts_a_healthy_project() {
    let env = project();
    assert_consistent(&env);
    let env = project_with_recorded_edits();
    assert_consistent(&env);
}

#[test]
#[should_panic(expected = "refs/heads/main")]
fn the_consistency_check_rejects_a_line_ref_behind_the_head_op() {
    let env = project();
    env.git_work(&["update-ref", "refs/heads/main", "refs/heads/main~1"]);
    assert_consistent(&env);
}

#[test]
#[should_panic(expected = "index vs tip")]
fn the_consistency_check_rejects_an_index_that_differs_from_the_tip() {
    let env = project();
    env.git_work(&["read-tree", "refs/heads/main~1"]);
    assert_consistent(&env);
}

#[test]
#[should_panic(expected = "checkout-pending")]
fn the_consistency_check_rejects_a_left_over_marker() {
    let env = project();
    fs::write(paper_dir(&env).join("checkout-pending"), "{}").unwrap();
    assert_consistent(&env);
}

#[test]
fn the_state_readers_describe_a_project() {
    let env = project_with_recorded_edits();

    let activity = activity(&env);
    assert!(activity[0].starts_with("edited "), "{activity:?}");
    assert_eq!(
        activity.last().map(String::as_str),
        Some("start the project"),
        "{activity:?}"
    );
    assert_eq!(
        files(&env).get("sections/intro.tex").map(Vec::as_slice),
        Some(INTRO_V3)
    );
    assert!(!marker_exists(&env));
    assert!(content_is_reachable(
        &env,
        "sections/intro.tex",
        "Intro v1\n"
    ));
    assert!(content_is_reachable(
        &env,
        "sections/intro.tex",
        "Intro v3\n"
    ));
    assert!(!content_is_reachable(
        &env,
        "sections/intro.tex",
        "never written\n"
    ));
    assert_eq!(
        head_tip(&env),
        env.git_work(&["rev-parse", "refs/heads/main"]).trim()
    );
    assert_eq!(index_listing(&env), tree_listing(&env, "refs/heads/main"));
    assert!(!op_committed("after-keep") && op_committed("after-op"));
}

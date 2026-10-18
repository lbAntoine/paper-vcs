//! `Project::gc` (PV-027): trimming the activity log to a recent window, the horizon, the
//! rebuilt keep-alive chain and the sweep of crash leftovers. Real temp repositories with
//! hand-dated op files, cross-checked with the real `git` binary.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use paper_core::activity::{ActivityLog, OpId};
use paper_core::profile::Profile;
use paper_core::project::{GcAge, GcOutcome, PathSelection, Project, SaveRequest, UndoOutcome};
use paper_core::snapshot::SnapshotOptions;
use paper_core::{ErrorKind, PaperError};
use paper_git::Identity;
use serde_json::Value;

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn write(dir: &Path, path: &str, content: &str) {
    fs::write(dir.join(path), content).unwrap();
}

fn save_all(project: &mut Project, message: &str) {
    project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
}

/// A project with `saves` saves of `a.txt` (and one extra file each, for more commits). The
/// project stays open, so it holds the lock.
fn history(saves: usize) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "v0\n");
    let (mut project, _) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    for i in 1..=saves {
        write(dir.path(), "a.txt", &format!("v{i}\n"));
        write(dir.path(), &format!("extra{i}.txt"), &format!("e{i}\n"));
        save_all(&mut project, &format!("Save {i}"));
    }
    (dir, project)
}

fn paper(dir: &Path) -> PathBuf {
    dir.join(".git/paper")
}

fn ops_dir(dir: &Path) -> PathBuf {
    paper(dir).join("activity/ops")
}

fn read_op(dir: &Path, id: &str) -> Value {
    serde_json::from_slice(&fs::read(ops_dir(dir).join(format!("{id}.json"))).unwrap()).unwrap()
}

/// The ops from the head back to the first one (or to wherever `parent` stops).
fn chain(dir: &Path) -> Vec<Value> {
    let head = fs::read_to_string(paper(dir).join("activity/head")).unwrap();
    let mut ops = Vec::new();
    let mut next = Some(head.trim().to_owned());
    while let Some(id) = next {
        let op = read_op(dir, &id);
        next = op["parent"].as_str().map(str::to_owned);
        ops.push(op);
        assert!(ops.len() < 5000, "the parent chain loops");
    }
    ops
}

fn id_of(op: &Value) -> String {
    op["id"].as_str().unwrap().to_owned()
}

/// Rewrites the `time` of the op `id` to `days` days ago.
fn backdate(dir: &Path, id: &str, days: i64) {
    let mut op = read_op(dir, id);
    let then = jiff::Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(24 * days))
        .unwrap();
    op["time"] = then.to_string().into();
    let mut json = serde_json::to_vec_pretty(&op).unwrap();
    json.push(b'\n');
    fs::write(ops_dir(dir).join(format!("{id}.json")), json).unwrap();
}

/// Backdates the oldest `count` ops of the chain (the chain is head-first) to `days` ago.
fn backdate_oldest(dir: &Path, count: usize, days: i64) {
    let ops = chain(dir);
    for op in ops.iter().rev().take(count) {
        backdate(dir, &id_of(op), days);
    }
}

/// Every file under `<dir>/.git`, as bytes, by relative path.
fn git_dir_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            } else if entry.file_type().unwrap().is_file() {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(name, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    let git_dir = dir.join(".git");
    walk(&git_dir, &git_dir, &mut out);
    out
}

fn op_ids_on_disk(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(ops_dir(dir))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn activity_ids(project: &Project) -> Vec<String> {
    project
        .activity(10_000)
        .unwrap()
        .into_iter()
        .map(|e| e.id.to_string())
        .collect()
}

fn gc(project: &mut Project, age: &str) -> GcOutcome {
    project.gc(GcAge::parse(age).unwrap(), false).unwrap()
}

/// A project whose oldest `old` ops are 200 days old: `(dir, project, chain head-first)`.
fn aged(saves: usize, old: usize) -> (tempfile::TempDir, Project, Vec<Value>) {
    let (dir, project) = history(saves);
    backdate_oldest(dir.path(), old, 200);
    let ops = chain(dir.path());
    (dir, project, ops)
}

fn commits_of(op: &Value) -> BTreeSet<String> {
    op["view"]["lines"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|line| [line["tip"].clone(), line["draft"].clone()])
        .map(|c| c.as_str().unwrap().to_owned())
        .collect()
}

// ---- the test helpers themselves (these pass without gc) ----

#[test]
fn backdating_changes_only_the_time_of_one_op() {
    let (dir, project) = history(2);
    let ops = chain(dir.path());
    let oldest = ops.last().unwrap();
    backdate(dir.path(), &id_of(oldest), 200);
    let after = read_op(dir.path(), &id_of(oldest));

    let mut expected = oldest.clone();
    expected["time"] = after["time"].clone();
    assert_eq!(after, expected, "only the time field changes");
    let then: jiff::Timestamp = after["time"].as_str().unwrap().parse().unwrap();
    let age = jiff::Timestamp::now().duration_since(then);
    assert!(age.as_hours() >= 24 * 199 && age.as_hours() <= 24 * 201);
    assert_eq!(chain(dir.path()).len(), ops.len(), "the chain still walks");
    let entries = project.activity(100).unwrap();
    assert_eq!(entries.last().unwrap().time, after["time"]);
}

#[test]
fn backdate_oldest_dates_the_oldest_ops_and_leaves_the_rest() {
    let (dir, _project) = history(3);
    let before = chain(dir.path());
    backdate_oldest(dir.path(), 3, 100);
    let after = chain(dir.path());
    let n = before.len();
    for (i, (b, a)) in before.iter().zip(&after).enumerate() {
        assert_eq!(i >= n - 3, b["time"] != a["time"], "op {i} of {n}");
    }
}

#[test]
fn the_byte_snapshot_of_the_git_folder_sees_a_save() {
    let (dir, mut project) = history(1);
    let before = git_dir_bytes(dir.path());
    assert_eq!(before, git_dir_bytes(dir.path()), "stable while idle");
    write(dir.path(), "a.txt", "changed\n");
    save_all(&mut project, "More");
    assert_ne!(before, git_dir_bytes(dir.path()));
}

// ---- --older-than ----

#[test]
fn the_age_parses_days_and_weeks() {
    assert_eq!(GcAge::parse("30d").unwrap().days(), 30);
    assert_eq!(GcAge::parse("1d").unwrap().days(), 1);
    assert_eq!(GcAge::parse("2w").unwrap().days(), 14);
    assert_eq!(GcAge::parse("90d").unwrap(), GcAge::default());
}

#[test]
fn the_default_age_is_90_days() {
    assert_eq!(GcAge::default().days(), 90);
    assert_eq!(GcAge::DEFAULT.days(), 90);
}

#[test]
fn an_age_below_one_day_or_garbage_is_a_three_part_user_error() {
    for bad in [
        "0d", "0w", "", "d", "w", "90", "1h", "-3d", "abc", "1.5d", "3 d", "1d2", "10m",
    ] {
        let error = GcAge::parse(bad).expect_err(bad);
        let error = PaperError::from(error);
        assert_eq!(error.kind, ErrorKind::User, "{bad}: {error:?}");
        assert!(!error.message.trim().is_empty(), "{bad}: {error:?}");
        assert!(!error.why.trim().is_empty(), "{bad}: {error:?}");
        assert!(!error.next.is_empty(), "{bad}: {error:?}");
    }
}

// ---- what is removed and kept ----

#[test]
fn gc_removes_older_entries_keeps_newer_ones_and_writes_the_horizon() {
    let (dir, mut project, ops) = aged(4, 5);
    let total = ops.len();
    assert!(total > 6, "the history has newer ops than the old ones");
    let horizon = ops[total - 6].clone();
    let log = ActivityLog::open(&paper(dir.path()));
    assert!(!log.has_horizon(), "no horizon before the first gc");

    let outcome = gc(&mut project, "30d");

    assert!(!outcome.dry_run);
    assert_eq!(outcome.removed_entries, 5);
    assert_eq!(outcome.kept_entries, total - 5);
    assert_eq!(outcome.horizon.id.to_string(), id_of(&horizon));
    assert_eq!(outcome.horizon.time, horizon["time"].as_str().unwrap());
    assert!(outcome.swept.is_empty(), "{:?}", outcome.swept);

    let kept: BTreeSet<String> = ops[..total - 5]
        .iter()
        .map(|op| format!("{}.json", id_of(op)))
        .collect();
    assert_eq!(op_ids_on_disk(dir.path()), kept);
    assert!(log.has_horizon());
    let file = fs::read_to_string(paper(dir.path()).join("activity/horizon")).unwrap();
    assert_eq!(file.trim(), id_of(&horizon));

    let listed = activity_ids(&project);
    assert_eq!(listed.len(), total - 5, "activity stops at the horizon");
    assert_eq!(listed.last().unwrap(), &id_of(&horizon));
    let head = fs::read_to_string(paper(dir.path()).join("activity/head")).unwrap();
    assert_eq!(head.trim(), id_of(&ops[0]), "head is unchanged");
}

#[test]
fn reading_the_horizon_op_reports_no_parent() {
    let (dir, mut project, ops) = aged(3, 4);
    let outcome = gc(&mut project, "30d");
    let log = ActivityLog::open(&paper(dir.path()));
    let horizon = log.read(&outcome.horizon.id).unwrap();
    assert_eq!(horizon.parent, None);
    assert_eq!(horizon.id.to_string(), id_of(&ops[ops.len() - 5]));
    // The file itself still names its old parent: gc never rewrites an op.
    assert!(read_op(dir.path(), &id_of(&ops[ops.len() - 5]))["parent"].is_string());
}

#[test]
fn the_head_is_kept_even_when_it_is_older_than_the_cutoff() {
    let (dir, mut project, ops) = aged(3, 1000);
    assert!(ops.iter().all(|op| op["time"] != ""), "all ops are old");
    let outcome = gc(&mut project, "30d");
    assert_eq!(outcome.kept_entries, 1);
    assert_eq!(outcome.removed_entries, ops.len() - 1);
    assert_eq!(outcome.horizon.id.to_string(), id_of(&ops[0]));
    assert_eq!(
        op_ids_on_disk(dir.path()),
        BTreeSet::from([format!("{}.json", id_of(&ops[0]))])
    );
    assert_eq!(activity_ids(&project), vec![id_of(&ops[0])]);
}

#[test]
fn a_bigger_cutoff_removes_less() {
    let counts = |age: &str| {
        let (dir, mut project) = history(4);
        let ops = chain(dir.path());
        for op in ops.iter().rev().take(4) {
            backdate(dir.path(), &id_of(op), 100);
        }
        for op in ops.iter().rev().skip(4).take(3) {
            backdate(dir.path(), &id_of(op), 10);
        }
        gc(&mut project, age).removed_entries
    };
    assert_eq!(counts("7d"), 7);
    assert_eq!(counts("30d"), 4);
    assert_eq!(counts("2w"), 4);
    assert_eq!(counts("90d"), 4);
    assert_eq!(counts("15w"), 0);
}

#[test]
fn nothing_old_means_nothing_removed() {
    let (dir, mut project) = history(2);
    let before = chain(dir.path());
    let outcome = gc(&mut project, "30d");
    assert_eq!(outcome.removed_entries, 0);
    assert_eq!(outcome.kept_entries, before.len());
    assert!(outcome.swept.is_empty());
    assert_eq!(op_ids_on_disk(dir.path()).len(), before.len());
    assert_eq!(activity_ids(&project).len(), before.len());
}

#[test]
fn old_redo_stack_entries_are_kept_and_redo_still_works() {
    let (dir, mut project) = history(3);
    assert!(matches!(project.undo().unwrap(), UndoOutcome::Undone(_)));
    assert!(matches!(project.undo().unwrap(), UndoOutcome::Undone(_)));
    let ops = chain(dir.path());
    let head = &ops[0];
    let redo: Vec<String> = head["redo"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(redo.len(), 2, "two undos leave two entries to redo");
    // Everything but the two undo ops is old, redo targets included.
    for op in &ops[2..] {
        backdate(dir.path(), &id_of(op), 200);
    }

    let outcome = gc(&mut project, "30d");

    // The horizon is the oldest of the redo targets and the head's undo position; the position
    // is the op just before the oldest redo target, so it sets the horizon here.
    let position = head["position"].as_str().unwrap();
    let horizon = ops
        .iter()
        .rposition(|op| redo.contains(&id_of(op)) || id_of(op) == position)
        .unwrap();
    assert_eq!(id_of(&ops[horizon]), position, "the position is the oldest");
    assert_eq!(outcome.kept_entries, horizon + 1);
    assert_eq!(outcome.removed_entries, ops.len() - horizon - 1);
    assert_eq!(outcome.horizon.id.to_string(), id_of(&ops[horizon]));
    let on_disk = op_ids_on_disk(dir.path());
    for id in &redo {
        assert!(on_disk.contains(&format!("{id}.json")), "{id} was removed");
    }
    assert_eq!(on_disk.len(), outcome.kept_entries);

    for id in &redo {
        let UndoOutcome::Undone(done) = project.redo().unwrap() else {
            panic!("redo should still work")
        };
        assert_eq!(&done.entry.id.to_string(), id);
    }
    assert!(matches!(
        project.redo().unwrap(),
        UndoOutcome::NothingToRedo
    ));
}

#[test]
fn gc_keeps_the_op_the_next_undo_starts_from_and_undo_still_works() {
    let (dir, mut project) = history(5);
    for _ in 0..3 {
        assert!(matches!(project.undo().unwrap(), UndoOutcome::Undone(_)));
    }
    let ops = chain(dir.path());
    let position = ops[0]["position"].as_str().unwrap().to_owned();
    let position_index = ops.iter().position(|op| id_of(op) == position).unwrap();
    assert!(
        position_index > 3,
        "the position is older than the undo ops"
    );
    // Only the three undo ops are new; the position and everything older are 200 days old.
    for op in &ops[3..] {
        backdate(dir.path(), &id_of(op), 200);
    }

    let outcome = gc(&mut project, "30d");

    assert_eq!(
        outcome.horizon.id.to_string(),
        position,
        "the position is kept"
    );
    assert_eq!(outcome.kept_entries, position_index + 1);
    assert_eq!(outcome.removed_entries, ops.len() - position_index - 1);
    assert!(op_ids_on_disk(dir.path()).contains(&format!("{position}.json")));
    // Undo starts from the kept position: no MissingOp, and it stops at the horizon.
    assert!(matches!(
        project.undo().unwrap(),
        UndoOutcome::NothingToUndo
    ));
    for _ in 0..3 {
        assert!(matches!(project.redo().unwrap(), UndoOutcome::Undone(_)));
    }
}

#[test]
fn undo_after_gc_walks_back_from_the_position_to_the_horizon_and_stops() {
    let (dir, mut project) = history(6);
    for _ in 0..3 {
        assert!(matches!(project.undo().unwrap(), UndoOutcome::Undone(_)));
    }
    let ops = chain(dir.path());
    let position = ops[0]["position"].as_str().unwrap().to_owned();
    let position_index = ops.iter().position(|op| id_of(op) == position).unwrap();
    // The cutoff alone would keep less than the position needs: only the oldest 3 are old.
    backdate_oldest(dir.path(), 3, 200);

    let outcome = gc(&mut project, "30d");

    assert_eq!(outcome.removed_entries, 3);
    assert_eq!(outcome.kept_entries, ops.len() - 3);
    assert!(outcome.kept_entries > position_index + 1);
    let mut steps = 0;
    while let UndoOutcome::Undone(_) = project.undo().unwrap() {
        steps += 1;
        assert!(steps < 100, "undo never reaches the horizon");
    }
    assert!(steps >= 1, "undo can still go back past the position");
    assert!(matches!(
        project.undo().unwrap(),
        UndoOutcome::NothingToUndo
    ));
}

// ---- undo against the horizon ----

#[test]
fn undo_walks_back_to_the_horizon_and_then_has_nothing_to_undo() {
    let (dir, mut project, ops) = aged(4, 5);
    let outcome = gc(&mut project, "30d");
    let horizon_draft = ops[ops.len() - 6]["view"]["lines"]["main"]["draft"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut steps = 0;
    while let UndoOutcome::Undone(_) = project.undo().unwrap() {
        steps += 1;
        assert!(steps < 100, "undo never reaches the horizon");
    }
    assert_eq!(steps, outcome.kept_entries - 1);
    assert!(matches!(
        project.undo().unwrap(),
        UndoOutcome::NothingToUndo
    ));
    let expected = git(
        dir.path(),
        &["cat-file", "-p", &format!("{horizon_draft}:a.txt")],
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        expected
    );
}

// ---- the keep-alive chain ----

#[test]
fn every_kept_commit_stays_reachable_through_a_chain_of_at_most_50_parents_each() {
    // Edit then snapshot twice before the first save: the first snapshot's draft is rewritten
    // by the second and is not an ancestor of anything later, so only the keep chain held it.
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", "v0\n");
    let (mut project, _) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    write(dir.path(), "a.txt", "draft only 1\n");
    project.snapshot().unwrap();
    write(dir.path(), "a.txt", "draft only 2\n");
    project.snapshot().unwrap();
    for i in 1..=40 {
        write(dir.path(), "a.txt", &format!("v{i}\n"));
        write(dir.path(), &format!("extra{i}.txt"), &format!("e{i}\n"));
        save_all(&mut project, &format!("Save {i}"));
    }
    backdate_oldest(dir.path(), 10, 200);
    let ops = chain(dir.path());
    let removed: BTreeSet<String> = ops[ops.len() - 10..].iter().flat_map(commits_of).collect();
    let kept_ops = &ops[..ops.len() - 10];
    let kept: BTreeSet<String> = kept_ops.iter().flat_map(commits_of).collect();
    // Old save tips are git ancestors of newer kept tips, so they stay reachable. Only commits
    // no kept commit leads to (old drafts) can be pruned.
    let kept_args: Vec<&str> = kept.iter().map(String::as_str).collect();
    let mut rev_list = vec!["rev-list"];
    rev_list.extend(&kept_args);
    let reachable_from_kept: BTreeSet<String> = git(dir.path(), &rev_list)
        .lines()
        .map(str::to_owned)
        .collect();
    let only_removed: BTreeSet<&String> = removed
        .iter()
        .filter(|c| !reachable_from_kept.contains(*c))
        .collect();
    assert!(kept.len() > 50, "the test needs more than one group");
    assert!(
        !only_removed.is_empty(),
        "a draft-only commit must be removed, or the prune check tests nothing"
    );

    let outcome = gc(&mut project, "30d");
    assert_eq!(outcome.kept_entries, kept_ops.len());

    let keep_ref = "refs/paper/keep";
    let reachable: BTreeSet<String> = git(dir.path(), &["rev-list", keep_ref])
        .lines()
        .map(str::to_owned)
        .collect();
    for commit in &kept {
        assert!(reachable.contains(commit), "{commit} is not kept alive");
    }
    for commit in &only_removed {
        assert!(!reachable.contains(*commit), "{commit} is still kept alive");
    }
    let keep_commits = git(dir.path(), &["log", "--format=%s\t%P", keep_ref]);
    let mut seen = 0;
    for line in keep_commits.lines() {
        let (subject, parents) = line.split_once('\t').unwrap();
        if subject == "paper keep-alive" {
            seen += 1;
            let count = parents.split_whitespace().count();
            assert!(count <= 51, "a keep commit has {count} parents");
        }
    }
    assert!(
        seen >= 2,
        "{} kept commits need a chain of groups",
        kept.len()
    );

    // Reclaim what git can, then prove undo to the horizon still has what it needs.
    git(dir.path(), &["reflog", "expire", "--expire=now", "--all"]);
    git(dir.path(), &["gc", "--prune=now", "--quiet"]);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
    for commit in &kept {
        git(dir.path(), &["cat-file", "-e", commit]);
    }
    for commit in &only_removed {
        let gone = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["cat-file", "-e", commit])
            .output()
            .unwrap();
        assert!(!gone.status.success(), "{commit} should have been pruned");
    }
    let mut steps = 0;
    while let UndoOutcome::Undone(_) = project.undo().unwrap() {
        steps += 1;
        assert!(steps < 500);
    }
    assert_eq!(steps, outcome.kept_entries - 1);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

// ---- the sweep ----

#[test]
fn leftovers_orphans_and_the_old_view_are_swept() {
    let (dir, mut project) = history(2);
    let activity = paper(dir.path()).join("activity");
    fs::write(activity.join(".tmp-head"), "x").unwrap();
    fs::write(activity.join("ops/.tmp-op"), "x").unwrap();
    fs::write(paper(dir.path()).join("view.json.v1"), "{}").unwrap();
    let head_op = chain(dir.path())[0].clone();
    let orphan = OpId::random().to_string();
    let mut orphan_op = head_op;
    orphan_op["id"] = orphan.clone().into();
    orphan_op["position"] = orphan.clone().into();
    orphan_op["parent"] = Value::Null;
    fs::write(
        ops_dir(dir.path()).join(format!("{orphan}.json")),
        serde_json::to_vec_pretty(&orphan_op).unwrap(),
    )
    .unwrap();
    let kept = chain(dir.path()).len();

    let outcome = gc(&mut project, "30d");

    let mut expected = vec![
        "activity/.tmp-head".to_owned(),
        "activity/ops/.tmp-op".to_owned(),
        format!("activity/ops/{orphan}.json"),
        "view.json.v1".to_owned(),
    ];
    expected.sort();
    assert_eq!(outcome.swept, expected);
    assert_eq!(outcome.removed_entries, 0);
    assert_eq!(outcome.kept_entries, kept);
    assert!(!activity.join(".tmp-head").exists());
    assert!(!activity.join("ops/.tmp-op").exists());
    assert!(!paper(dir.path()).join("view.json.v1").exists());
    assert_eq!(op_ids_on_disk(dir.path()).len(), kept);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn an_op_the_head_names_is_never_swept_even_when_it_is_off_the_chain() {
    let (dir, mut project) = history(2);
    let head = chain(dir.path())[0].clone();
    let (redo_id, position_id) = (OpId::random().to_string(), OpId::random().to_string());
    for id in [&redo_id, &position_id] {
        let mut stray = head.clone();
        stray["id"] = id.clone().into();
        stray["parent"] = Value::Null;
        fs::write(
            ops_dir(dir.path()).join(format!("{id}.json")),
            serde_json::to_vec_pretty(&stray).unwrap(),
        )
        .unwrap();
    }
    // A hand-made inconsistent log: the head names ops that its chain does not contain.
    let mut damaged = head.clone();
    damaged["redo"] = serde_json::json!([redo_id]);
    damaged["position"] = position_id.clone().into();
    fs::write(
        ops_dir(dir.path()).join(format!("{}.json", id_of(&head))),
        serde_json::to_vec_pretty(&damaged).unwrap(),
    )
    .unwrap();

    let outcome = gc(&mut project, "30d");

    assert!(outcome.swept.is_empty(), "{:?}", outcome.swept);
    for id in [&redo_id, &position_id] {
        assert!(ops_dir(dir.path()).join(format!("{id}.json")).is_file());
    }
}

// ---- --dry-run ----

#[test]
fn a_dry_run_changes_nothing_and_reports_what_a_real_run_then_does() {
    let (dir, mut project, ops) = aged(4, 5);
    fs::write(paper(dir.path()).join("activity/ops/.tmp-op"), "x").unwrap();
    fs::write(paper(dir.path()).join("view.json.v1"), "{}").unwrap();
    let before = git_dir_bytes(dir.path());
    let files_before = fs::read_to_string(dir.path().join("a.txt")).unwrap();

    let dry = project.gc(GcAge::parse("30d").unwrap(), true).unwrap();

    assert_eq!(
        before,
        git_dir_bytes(dir.path()),
        "files and refs unchanged"
    );
    assert_eq!(
        files_before,
        fs::read_to_string(dir.path().join("a.txt")).unwrap()
    );
    assert!(dry.dry_run);
    assert_eq!(dry.removed_entries, 5);
    assert_eq!(dry.kept_entries, ops.len() - 5);
    assert_eq!(
        dry.swept,
        vec!["activity/ops/.tmp-op".to_owned(), "view.json.v1".to_owned()]
    );
    assert!(!ActivityLog::open(&paper(dir.path())).has_horizon());

    let real = gc(&mut project, "30d");
    assert!(!real.dry_run);
    assert_eq!(
        GcOutcome {
            dry_run: false,
            ..dry
        },
        real
    );
}

// ---- running twice, interruptions, no entry ----

#[test]
fn a_second_run_removes_nothing_new() {
    let (dir, mut project, _) = aged(4, 5);
    let first = gc(&mut project, "30d");
    let ops_after_first = op_ids_on_disk(dir.path());
    let listed = activity_ids(&project);

    let second = gc(&mut project, "30d");

    assert_eq!(second.removed_entries, 0);
    assert_eq!(second.kept_entries, first.kept_entries);
    assert_eq!(second.horizon, first.horizon);
    assert!(second.swept.is_empty(), "{:?}", second.swept);
    assert_eq!(op_ids_on_disk(dir.path()), ops_after_first);
    assert_eq!(activity_ids(&project), listed);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn running_gc_again_finishes_an_interrupted_run() {
    let (dir, mut project, ops) = aged(4, 5);
    let backup: Vec<(String, Vec<u8>)> = ops[ops.len() - 5..]
        .iter()
        .map(|op| {
            let name = format!("{}.json", id_of(op));
            let bytes = fs::read(ops_dir(dir.path()).join(&name)).unwrap();
            (name, bytes)
        })
        .collect();
    let first = gc(&mut project, "30d");
    // As if the process died before it deleted the old files.
    for (name, bytes) in &backup {
        fs::write(ops_dir(dir.path()).join(name), bytes).unwrap();
    }
    assert_eq!(activity_ids(&project).len(), first.kept_entries);

    let again = gc(&mut project, "30d");

    assert_eq!(again.kept_entries, first.kept_entries);
    let mut expected: Vec<String> = backup
        .iter()
        .map(|(name, _)| format!("activity/ops/{name}"))
        .collect();
    expected.sort();
    assert_eq!(again.swept, expected);
    assert_eq!(op_ids_on_disk(dir.path()).len(), first.kept_entries);
}

#[test]
fn gc_records_no_activity_entry() {
    let (dir, mut project, ops) = aged(3, 4);
    let head = fs::read_to_string(paper(dir.path()).join("activity/head")).unwrap();
    let outcome = gc(&mut project, "30d");
    assert_eq!(
        fs::read_to_string(paper(dir.path()).join("activity/head")).unwrap(),
        head,
        "head did not move"
    );
    assert_eq!(op_ids_on_disk(dir.path()).len(), ops.len() - 4);
    assert_eq!(project.activity(1000).unwrap().len(), outcome.kept_entries);
    assert_eq!(
        project.activity(1).unwrap()[0].id.to_string(),
        id_of(&ops[0])
    );
}

#[test]
fn a_project_reopens_and_keeps_working_after_gc() {
    let (dir, mut project, ops) = aged(3, 4);
    let outcome = gc(&mut project, "30d");
    drop(project);
    let mut project = Project::open(dir.path(), identity()).unwrap();
    write(dir.path(), "a.txt", "after gc\n");
    save_all(&mut project, "After gc");
    let entries = project.activity(1000).unwrap();
    assert_eq!(entries.len(), outcome.kept_entries + 2, "edits and save");
    assert_eq!(entries.last().unwrap().id, outcome.horizon.id);
    assert!(ops.len() > outcome.kept_entries);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn a_gc_with_nothing_to_remove_leaves_ops_and_refs_byte_identical() {
    let (dir, mut project) = history(3);
    let before = git_dir_bytes(dir.path());

    let outcome = gc(&mut project, "30d");

    assert_eq!(outcome.removed_entries, 0);
    assert!(outcome.swept.is_empty());
    assert!(!ActivityLog::open(&paper(dir.path())).has_horizon());
    assert_eq!(before, git_dir_bytes(dir.path()), "no horizon, no new refs");
}

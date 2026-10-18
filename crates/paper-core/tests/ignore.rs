//! `Project::ignore` and `Project::ignore_rules` (PV-018), cross-checked with the real `git`
//! binary (`check-ignore`, `ls-tree`, `log`, `status`, `fsck`) and with the files on disk,
//! which must stay byte-identical.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::activity::OpKind;
use paper_core::error::{ErrorKind, PaperError};
use paper_core::profile::Profile;
use paper_core::project::*;
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

/// Runs `git` in `dir` with a fixed identity, failing the test if git fails.
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

/// `git check-ignore -v --no-index <path>`: the matching rule's line, or `None` if the path is
/// not ignored. `--no-index` judges the rules alone, whatever is tracked.
fn check_ignore(dir: &Path, path: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["check-ignore", "-v", "--no-index", path])
        .output()
        .unwrap();
    match output.status.code() {
        Some(0) => Some(String::from_utf8(output.stdout).unwrap()),
        Some(1) => None,
        _ => panic!(
            "git check-ignore {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

fn write(dir: &Path, path: &str, content: &str) {
    let absolute = dir.join(path);
    fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    fs::write(absolute, content).unwrap();
}

fn text(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

fn project_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        write(dir.path(), path, content);
    }
    let (project, _) = Project::init_with_profile(
        dir.path(),
        identity(),
        &SnapshotOptions::default(),
        Profile::builtin("generic"),
    )
    .unwrap();
    (dir, project)
}

fn save_all(project: &mut Project, message: &str) {
    let outcome = project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
    assert!(matches!(outcome, SaveOutcome::Saved(_)), "{outcome:?}");
}

/// Every file outside `.git`, as bytes, by relative path.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if name == ".git" {
                continue;
            }
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                walk(root, &path, out);
            } else if kind.is_file() {
                out.insert(name, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// `files(dir)` without `.gitignore`, the only file `paper ignore` is allowed to change.
fn files_but_gitignore(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut all = files(dir);
    all.remove(".gitignore");
    all
}

/// The files of the current draft, from git's point of view.
fn draft_files(dir: &Path) -> Vec<String> {
    git(
        dir,
        &["ls-tree", "-r", "--name-only", "refs/paper/draft/main"],
    )
    .lines()
    .map(str::to_owned)
    .collect()
}

fn op_count(dir: &Path) -> usize {
    fs::read_dir(dir.join(".git/paper/activity/ops"))
        .unwrap()
        .count()
}

fn head_op(dir: &Path) -> serde_json::Value {
    let head = fs::read_to_string(dir.join(".git/paper/activity/head")).unwrap();
    let bytes =
        fs::read(dir.join(format!(".git/paper/activity/ops/{}.json", head.trim()))).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn descriptions(project: &Project) -> Vec<String> {
    project
        .activity(100)
        .unwrap()
        .into_iter()
        .map(|e| e.description)
        .collect()
}

fn request(from: &str, args: &[&str], yes: bool) -> IgnoreRequest {
    IgnoreRequest {
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        from: from.to_owned(),
        yes,
    }
}

/// `paper ignore <args>` from the top of the project; panics on an error.
fn ignore(project: &mut Project, args: &[&str]) -> IgnoreOutcome {
    project.ignore(&request("", args, false)).unwrap()
}

fn paper_block(rules: &[&str]) -> String {
    let mut out = "# >>> paper-vcs: ignored >>>\n".to_owned();
    for rule in rules {
        out.push_str(rule);
        out.push('\n');
    }
    out.push_str("# <<< paper-vcs: ignored <<<\n");
    out
}

/// Everything a refused or empty command must leave alone.
fn stored(dir: &Path) -> (String, BTreeMap<String, Vec<u8>>, usize, String, String) {
    (
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        files(dir),
        op_count(dir),
        fs::read_to_string(dir.join(".git/paper/activity/head")).unwrap(),
        git(dir, &["ls-files", "--stage"]),
    )
}

fn raw_data_project() -> (tempfile::TempDir, Project) {
    project_with(&[
        ("main.tex", "\\documentclass{article}\n"),
        ("data/raw/a.csv", "1,2\n"),
        ("data/raw/sub/b.csv", "3,4\n"),
        ("data/keep.csv", "keep\n"),
    ])
}

// ---- helper sanity (passes on main) ----

#[test]
fn the_test_helpers_see_files_ignore_rules_and_the_draft() {
    let (dir, _project) = raw_data_project();
    write(dir.path(), "x.scratch", "x\n");
    append(dir.path(), ".gitignore", "*.scratch\n");

    assert!(files(dir.path()).contains_key("data/raw/sub/b.csv"));
    assert!(!files(dir.path()).contains_key(".git/HEAD"));
    assert!(!files_but_gitignore(dir.path()).contains_key(".gitignore"));
    assert!(check_ignore(dir.path(), "x.scratch").is_some());
    assert!(check_ignore(dir.path(), "main.tex").is_none());
    assert!(draft_files(dir.path()).contains(&"main.tex".to_owned()));
    assert_eq!(op_count(dir.path()), 1);
    assert_eq!(head_op(dir.path())["kind"], "init");
    assert!(paper_block(&["/a/"]).starts_with("# >>>"));
}

fn append(dir: &Path, path: &str, extra: &str) {
    let mut content = fs::read_to_string(dir.join(path)).unwrap_or_default();
    content.push_str(extra);
    write(dir, path, &content);
}

// ---- the headline: an already saved folder ----

#[test]
fn ignoring_a_saved_folder_writes_its_rule_untracks_it_and_keeps_the_files() {
    let (dir, mut project) = raw_data_project();
    let before = files_but_gitignore(dir.path());

    let outcome = ignore(&mut project, &["data/raw"]);

    assert_eq!(outcome.added, ["/data/raw/"]);
    assert!(outcome.already.is_empty());
    assert!(outcome.patterns.is_empty());
    assert_eq!(outcome.untracked, ["data/raw/a.csv", "data/raw/sub/b.csv"]);
    let gitignore = text(dir.path(), ".gitignore");
    assert!(
        gitignore.contains(&paper_block(&["/data/raw/"])),
        "{gitignore}"
    );
    assert!(
        gitignore
            .find("# >>> paper-vcs profile: generic >>>")
            .unwrap()
            < gitignore.find("# >>> paper-vcs: ignored >>>").unwrap(),
        "paper's block follows the profile block: {gitignore}"
    );
    assert_eq!(
        files_but_gitignore(dir.path()),
        before,
        "files stay on disk, byte-identical"
    );
    let draft = draft_files(dir.path());
    assert!(
        !draft.iter().any(|f| f.starts_with("data/raw/")),
        "{draft:?}"
    );
    for kept in ["main.tex", "data/keep.csv", ".gitignore"] {
        assert!(draft.contains(&kept.to_owned()), "{kept} in {draft:?}");
    }
    assert_eq!(
        git(dir.path(), &["show", "refs/paper/draft/main:.gitignore"]),
        gitignore,
        "the draft holds the new .gitignore"
    );
    let rule = check_ignore(dir.path(), "data/raw/a.csv").unwrap();
    assert!(rule.contains("/data/raw/"), "{rule}");
    assert!(check_ignore(dir.path(), "data/raw/sub/b.csv").is_some());
    assert!(check_ignore(dir.path(), "data/keep.csv").is_none());
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn the_next_save_records_the_removal_and_history_keeps_the_old_versions() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);

    save_all(&mut project, "Stop tracking raw data");

    let ls_files = git(dir.path(), &["ls-files"]);
    assert!(!ls_files.contains("data/raw/"), "{ls_files}");
    assert!(ls_files.contains("data/keep.csv"), "{ls_files}");
    let newest = git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]);
    assert!(!newest.contains("data/raw/"), "{newest}");
    let earlier = git(dir.path(), &["ls-tree", "-r", "--name-only", "main~1"]);
    assert!(earlier.contains("data/raw/a.csv"), "{earlier}");
    assert!(earlier.contains("data/raw/sub/b.csv"), "{earlier}");
    let log = git(dir.path(), &["log", "--format=%s", "--", "data/raw/a.csv"]);
    assert!(log.contains("Start project"), "{log}");
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert_eq!(text(dir.path(), "data/raw/a.csv"), "1,2\n");
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn later_snapshots_do_not_bring_the_untracked_files_back() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    write(dir.path(), "data/raw/a.csv", "edited after ignoring\n");
    write(dir.path(), "data/raw/new.csv", "new\n");

    project.snapshot().unwrap();

    let draft = draft_files(dir.path());
    assert!(
        !draft.iter().any(|f| f.starts_with("data/raw/")),
        "{draft:?}"
    );
}

// ---- activity and undo ----

#[test]
fn ignore_records_one_entry_described_with_its_rules() {
    let (dir, mut project) = raw_data_project();
    let ops_before = op_count(dir.path());

    let outcome = ignore(&mut project, &["data/raw", "*.aux"]);

    assert_eq!(outcome.added, ["/data/raw/", "*.aux"]);
    assert_eq!(outcome.patterns, ["*.aux"]);
    let entry = outcome.entry.expect("an entry is recorded");
    assert_eq!(entry.description, "ignore /data/raw/, *.aux");
    assert_eq!(entry.kind, OpKind::Ignore);
    assert_eq!(
        op_count(dir.path()),
        ops_before + 1,
        "one entry per command"
    );
    assert_eq!(head_op(dir.path())["kind"], "ignore");
    let listed = project.activity(1).unwrap();
    assert_eq!(listed[0], entry);
}

#[test]
fn unsaved_edits_are_recorded_first_as_their_own_entry() {
    let (dir, mut project) = raw_data_project();
    write(dir.path(), "main.tex", "edited\n");

    ignore(&mut project, &["data/raw"]);

    let all = descriptions(&project);
    assert_eq!(all[0], "ignore /data/raw/");
    assert!(all[1].starts_with("edited main.tex"), "{all:?}");
}

#[test]
fn undo_restores_the_old_gitignore_and_tracks_the_files_again() {
    let (dir, mut project) = raw_data_project();
    let gitignore_before = fs::read(dir.path().join(".gitignore")).unwrap();
    let files_before = files(dir.path());
    ignore(&mut project, &["data/raw"]);

    let undone = match project.undo().unwrap() {
        UndoOutcome::Undone(undone) => undone,
        other => panic!("expected an undone entry, got {other:?}"),
    };

    assert_eq!(undone.entry.description, "ignore /data/raw/");
    assert!(
        undone.skipped.is_empty(),
        "the files are identical, so none is in the way: {:?}",
        undone.skipped
    );
    assert_eq!(
        fs::read(dir.path().join(".gitignore")).unwrap(),
        gitignore_before
    );
    assert_eq!(
        files(dir.path()),
        files_before,
        "every file is byte-identical"
    );
    let draft = draft_files(dir.path());
    assert!(draft.contains(&"data/raw/a.csv".to_owned()), "{draft:?}");
    assert!(
        draft.contains(&"data/raw/sub/b.csv".to_owned()),
        "{draft:?}"
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert!(check_ignore(dir.path(), "data/raw/a.csv").is_none());
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn redo_ignores_and_untracks_again() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    let after = files(dir.path());
    project.undo().unwrap();

    project.redo().unwrap();

    assert_eq!(files(dir.path()), after);
    let draft = draft_files(dir.path());
    assert!(
        !draft.iter().any(|f| f.starts_with("data/raw/")),
        "{draft:?}"
    );
}

// ---- turning arguments into rules ----

#[test]
fn a_glob_is_written_as_typed_and_applies_everywhere() {
    let (dir, mut project) = project_with(&[
        ("main.tex", "m\n"),
        ("x.scratch", "x\n"),
        ("sections/deep/y.scratch", "y\n"),
    ]);

    let outcome = ignore(&mut project, &["*.scratch"]);

    assert_eq!(outcome.added, ["*.scratch"]);
    assert_eq!(outcome.patterns, ["*.scratch"]);
    assert_eq!(outcome.untracked, ["sections/deep/y.scratch", "x.scratch"]);
    assert!(text(dir.path(), ".gitignore").contains(&paper_block(&["*.scratch"])));
    assert!(check_ignore(dir.path(), "x.scratch").is_some());
    assert!(check_ignore(dir.path(), "sections/deep/y.scratch").is_some());
}

#[test]
fn a_glob_typed_in_a_subfolder_is_not_moved_into_it() {
    let (dir, mut project) = project_with(&[("sections/y.scratch", "y\n"), ("z.scratch", "z\n")]);

    let outcome = project
        .ignore(&request("sections", &["*.scratch"], false))
        .unwrap();

    assert_eq!(outcome.added, ["*.scratch"]);
    assert_eq!(outcome.untracked, ["sections/y.scratch", "z.scratch"]);
    assert!(check_ignore(dir.path(), "z.scratch").is_some());
}

#[test]
fn paths_are_resolved_from_where_paper_runs() {
    let (dir, mut project) = raw_data_project();
    write(dir.path(), "notes.txt", "n\n");

    let outcome = project
        .ignore(&request("data", &["raw", "../notes.txt"], false))
        .unwrap();

    assert_eq!(outcome.added, ["/data/raw/", "/notes.txt"]);
    assert!(check_ignore(dir.path(), "notes.txt").is_some());
    assert!(check_ignore(dir.path(), "data/raw/a.csv").is_some());
}

#[test]
fn a_folder_typed_with_or_without_slash_gets_the_same_anchored_rule() {
    for typed in ["data/raw", "data/raw/", "./data/raw"] {
        let (_dir, mut project) = raw_data_project();
        let outcome = ignore(&mut project, &[typed]);
        assert_eq!(outcome.added, ["/data/raw/"], "typed {typed}");
    }
}

#[test]
fn an_existing_file_becomes_an_anchored_rule_without_a_slash() {
    let (dir, mut project) = project_with(&[("notes/a.log", "a\n"), ("other/a.log", "b\n")]);

    let outcome = ignore(&mut project, &["notes/a.log"]);

    assert_eq!(outcome.added, ["/notes/a.log"]);
    assert_eq!(outcome.untracked, ["notes/a.log"]);
    assert!(check_ignore(dir.path(), "notes/a.log").is_some());
    assert!(
        check_ignore(dir.path(), "other/a.log").is_none(),
        "anchored: a file of the same name elsewhere is not ignored"
    );
}

#[test]
fn an_existing_file_with_brackets_in_its_name_gets_an_escaped_rule() {
    let (dir, mut project) = project_with(&[("notes/a[1].log", "a\n"), ("notes/a1.log", "b\n")]);

    let outcome = ignore(&mut project, &["notes/a[1].log"]);

    assert_eq!(outcome.added, [r"/notes/a\[1\].log"]);
    assert!(outcome.patterns.is_empty(), "it is a path, not a pattern");
    assert_eq!(outcome.untracked, ["notes/a[1].log"]);
    assert!(check_ignore(dir.path(), "notes/a[1].log").is_some());
    assert!(
        check_ignore(dir.path(), "notes/a1.log").is_none(),
        "the brackets are not a character class"
    );
    assert!(draft_files(dir.path()).contains(&"notes/a1.log".to_owned()));
}

#[test]
fn existing_files_starting_with_hash_or_bang_are_ignored_exactly() {
    let (dir, mut project) = project_with(&[
        ("#draft.txt", "d\n"),
        ("!important.txt", "i\n"),
        ("important.txt", "j\n"),
    ]);

    let outcome = ignore(&mut project, &["#draft.txt", "./!important.txt"]);

    assert_eq!(outcome.untracked, ["!important.txt", "#draft.txt"]);
    assert!(check_ignore(dir.path(), "#draft.txt").is_some());
    assert!(check_ignore(dir.path(), "!important.txt").is_some());
    assert!(check_ignore(dir.path(), "important.txt").is_none());
    assert_eq!(
        draft_files(dir.path())
            .iter()
            .filter(|f| f.contains("important"))
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn existing_files_with_star_and_question_mark_in_their_names_get_escaped_rules() {
    let (dir, mut project) = project_with(&[
        ("a*b.log", "1\n"),
        ("aXb.log", "2\n"),
        ("c?.log", "3\n"),
        ("cd.log", "4\n"),
    ]);

    let outcome = ignore(&mut project, &["a*b.log", "c?.log"]);

    assert_eq!(outcome.added, [r"/a\*b.log", r"/c\?.log"]);
    assert_eq!(outcome.untracked, ["a*b.log", "c?.log"]);
    assert!(check_ignore(dir.path(), "aXb.log").is_none());
    assert!(check_ignore(dir.path(), "cd.log").is_none());
    assert!(check_ignore(dir.path(), "a*b.log").is_some());
}

#[test]
fn a_path_that_does_not_exist_yet_is_anchored_and_keeps_a_typed_trailing_slash() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);

    let outcome = ignore(&mut project, &["build/out", "cache/"]);

    assert_eq!(outcome.added, ["/build/out", "/cache/"]);
    assert!(outcome.patterns.is_empty());
    assert!(outcome.untracked.is_empty());
    assert!(check_ignore(dir.path(), "build/out").is_some());
    assert!(check_ignore(dir.path(), "cache/anything.txt").is_some());
    assert!(check_ignore(dir.path(), "other/build/out").is_none());
}

#[test]
fn rules_appear_once_in_the_order_they_were_added() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);

    let first = ignore(&mut project, &["b.scratch", "a.scratch", "b.scratch"]);
    let second = ignore(&mut project, &["c.scratch", "a.scratch"]);

    assert_eq!(first.added, ["b.scratch", "a.scratch"]);
    assert_eq!(second.added, ["c.scratch"]);
    assert_eq!(second.already, ["a.scratch"]);
    assert!(text(dir.path(), ".gitignore").contains(&paper_block(&[
        "b.scratch",
        "a.scratch",
        "c.scratch"
    ])));
}

// ---- the untrack set ----

#[test]
fn only_files_matching_the_rules_named_by_this_command_are_untracked() {
    let (dir, mut project) =
        project_with(&[("x.tmp", "x\n"), ("y.scratch", "y\n"), ("main.tex", "m\n")]);
    // A rule written by hand matches a tracked file; tracked files stay tracked.
    append(dir.path(), ".gitignore", "*.tmp\n");

    let outcome = ignore(&mut project, &["*.scratch"]);

    assert_eq!(outcome.untracked, ["y.scratch"]);
    let draft = draft_files(dir.path());
    assert!(draft.contains(&"x.tmp".to_owned()), "{draft:?}");
    assert!(!draft.contains(&"y.scratch".to_owned()), "{draft:?}");
}

#[test]
fn a_later_negation_in_your_own_lines_keeps_the_file_tracked() {
    let (dir, mut project) = project_with(&[
        ("a.log", "a\n"),
        ("important.log", "i\n"),
        ("main.tex", "m\n"),
    ]);
    append(
        dir.path(),
        ".gitignore",
        &format!("\n{}!important.log\n", paper_block(&["*.log"])),
    );

    let outcome = ignore(&mut project, &["*.log"]);

    assert!(outcome.added.is_empty());
    assert_eq!(outcome.already, ["*.log"]);
    assert_eq!(outcome.untracked, ["a.log"]);
    let draft = draft_files(dir.path());
    assert!(draft.contains(&"important.log".to_owned()), "{draft:?}");
    assert!(!draft.contains(&"a.log".to_owned()), "{draft:?}");
    assert!(
        check_ignore(dir.path(), "important.log")
            .unwrap()
            .contains("!important.log")
    );
    assert_eq!(text(dir.path(), "important.log"), "i\n");
}

#[test]
fn a_negation_before_the_new_rule_loses_because_the_block_comes_later() {
    let (dir, mut project) = project_with(&[("a.log", "a\n"), ("important.log", "i\n")]);
    append(dir.path(), ".gitignore", "!important.log\n");

    let outcome = ignore(&mut project, &["*.log"]);

    assert_eq!(outcome.untracked, ["a.log", "important.log"]);
    assert!(check_ignore(dir.path(), "important.log").is_some());
}

// ---- the threshold ----

fn many_files(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| (format!("big/f{i:02}.txt"), format!("file {i}\n")))
        .collect()
}

fn project_with_owned(files: &[(String, String)]) -> (tempfile::TempDir, Project) {
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    project_with(&borrowed)
}

#[test]
fn more_than_twenty_files_need_yes_and_nothing_is_written_without_it() {
    let (dir, mut project) = project_with_owned(&many_files(21));
    let before = stored(dir.path());

    let error = project.ignore(&request("", &["big"], false)).unwrap_err();

    let paper = PaperError::from(error);
    assert_eq!(paper.kind, ErrorKind::User);
    assert!(!paper.why.is_empty());
    let all = format!(
        "{} {} {}",
        paper.message,
        paper.why,
        paper
            .next
            .iter()
            .map(|s| format!("{} {}", s.command, s.explanation))
            .collect::<Vec<_>>()
            .join(" ")
    );
    assert!(all.contains("21"), "{all}");
    for shown in ["f00", "f01", "f02", "f03", "f04"] {
        assert!(
            all.contains(&format!("big/{shown}.txt")),
            "{shown} in {all}"
        );
    }
    assert!(
        !all.contains("f05.txt"),
        "only the first 5 are named: {all}"
    );
    assert!(
        paper.next.iter().any(|s| s.command.contains("--yes")),
        "{paper:?}"
    );
    assert_eq!(stored(dir.path()), before, "nothing written");
}

#[test]
fn twenty_files_need_no_confirmation() {
    let (dir, mut project) = project_with_owned(&many_files(20));

    let outcome = ignore(&mut project, &["big"]);

    assert_eq!(outcome.untracked.len(), 20);
    assert!(
        !draft_files(dir.path())
            .iter()
            .any(|f| f.starts_with("big/"))
    );
}

#[test]
fn with_yes_more_than_twenty_files_are_untracked() {
    let (dir, mut project) = project_with_owned(&many_files(21));

    let outcome = project.ignore(&request("", &["big"], true)).unwrap();

    assert_eq!(outcome.untracked.len(), 21);
    assert!(
        !draft_files(dir.path())
            .iter()
            .any(|f| f.starts_with("big/"))
    );
    assert_eq!(
        files(dir.path())
            .keys()
            .filter(|f| f.starts_with("big/"))
            .count(),
        21
    );
}

// ---- refusals ----

fn assert_three_part(error: IgnoreError) -> PaperError {
    let paper = PaperError::from(error);
    assert_eq!(paper.kind, ErrorKind::User, "{paper:?}");
    assert!(!paper.message.is_empty(), "{paper:?}");
    assert!(!paper.why.is_empty(), "{paper:?}");
    assert!(!paper.next.is_empty(), "{paper:?}");
    paper
}

#[test]
fn refused_arguments_are_user_errors_and_write_nothing() {
    let (dir, mut project) = raw_data_project();
    assert!(
        dir.path().join("paper.toml").exists(),
        "init writes paper.toml"
    );
    let before = stored(dir.path());

    for refused in [
        "",
        "!x",
        "!/data/raw/keep.csv",
        ".gitignore",
        "./.gitignore",
        "paper.toml",
        ".git",
        ".git/",
        ".git/hooks",
        ".git/info/exclude",
        "../outside",
        "data/../../outside",
    ] {
        let error = project.ignore(&request("", &[refused], false)).unwrap_err();
        assert_three_part(error);
        assert_eq!(stored(dir.path()), before, "after refusing {refused:?}");
    }
}

#[test]
fn refusing_a_leading_bang_points_to_unignore() {
    let (_dir, mut project) = raw_data_project();

    let error = project.ignore(&request("", &["!x"], false)).unwrap_err();

    let paper = assert_three_part(error);
    let all = format!("{} {} {:?}", paper.message, paper.why, paper.next);
    assert!(all.contains("unignore"), "{all}");
}

#[test]
fn one_refused_argument_stops_the_whole_command() {
    let (dir, mut project) = raw_data_project();
    let before = stored(dir.path());

    let error = project
        .ignore(&request("", &["data/raw", "*.aux", ".gitignore"], false))
        .unwrap_err();

    assert_three_part(error);
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn paths_outside_the_project_are_refused_from_a_subfolder_too() {
    let (dir, mut project) = raw_data_project();
    let before = stored(dir.path());

    let error = project
        .ignore(&request("data", &["../../elsewhere"], false))
        .unwrap_err();

    assert_three_part(error);
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn a_damaged_paper_block_stops_the_command_and_writes_nothing() {
    let (dir, mut project) = raw_data_project();
    append(
        dir.path(),
        ".gitignore",
        "\n# >>> paper-vcs: ignored >>>\n/a/\n",
    );
    project.snapshot().unwrap();
    let before = stored(dir.path());

    let error = project
        .ignore(&request("", &["data/raw"], false))
        .unwrap_err();

    assert!(matches!(error, IgnoreError::Gitignore(_)), "{error:?}");
    assert_eq!(stored(dir.path()), before);
}

// ---- re-running, no-ops, crash safety ----

#[test]
fn rerunning_finishes_the_job_when_the_rule_is_there_but_the_files_are_still_tracked() {
    let (dir, mut project) = raw_data_project();
    // What a crash after the .gitignore write leaves: the rule, with the files tracked.
    append(
        dir.path(),
        ".gitignore",
        &format!("\n{}", paper_block(&["/data/raw/"])),
    );
    let ops_before = op_count(dir.path());

    let outcome = ignore(&mut project, &["data/raw"]);

    assert!(outcome.added.is_empty());
    assert_eq!(outcome.already, ["/data/raw/"]);
    assert_eq!(outcome.untracked, ["data/raw/a.csv", "data/raw/sub/b.csv"]);
    let entry = outcome.entry.expect("untracking is recorded");
    assert_eq!(entry.kind, OpKind::Ignore);
    assert!(op_count(dir.path()) > ops_before);
    assert!(
        !draft_files(dir.path())
            .iter()
            .any(|f| f.starts_with("data/raw/"))
    );
    assert_eq!(
        text(dir.path(), ".gitignore").matches("/data/raw/").count(),
        1
    );
}

#[test]
fn running_it_again_is_a_no_op_with_no_entry() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    let before = stored(dir.path());

    let outcome = ignore(&mut project, &["data/raw"]);

    assert!(outcome.added.is_empty());
    assert_eq!(outcome.already, ["/data/raw/"]);
    assert!(outcome.untracked.is_empty());
    assert_eq!(outcome.entry, None, "no entry for nothing to do");
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn ignoring_a_new_rule_that_matches_nothing_still_records_the_gitignore_edit() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);

    let outcome = ignore(&mut project, &["*.nothing"]);

    assert_eq!(outcome.added, ["*.nothing"]);
    assert!(outcome.untracked.is_empty());
    assert!(outcome.entry.is_some());
    assert_eq!(
        git(dir.path(), &["show", "refs/paper/draft/main:.gitignore"]),
        text(dir.path(), ".gitignore")
    );
}

#[test]
fn ignore_does_not_refuse_when_plain_git_moved_the_line() {
    let (dir, mut project) = raw_data_project();
    git(dir.path(), &["commit", "--allow-empty", "-m", "plain git"]);
    let moved = git(dir.path(), &["rev-parse", "main"]);

    let outcome = project.ignore(&request("", &["data/raw"], false)).unwrap();

    assert_eq!(outcome.untracked.len(), 2);
    assert_eq!(
        git(dir.path(), &["rev-parse", "main"]),
        moved,
        "ignore writes only paper's draft, never the line"
    );
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

// ---- hand-written lines, profiles, line endings ----

#[test]
fn hand_written_lines_stay_byte_identical_and_a_crlf_file_stays_crlf() {
    let (dir, mut project) = project_with(&[("a.scratch", "a\n")]);
    let original = "mine.txt\r\n*.bak\r\n  keep me  \r\n";
    write(dir.path(), ".gitignore", original);

    ignore(&mut project, &["*.scratch"]);

    let after = text(dir.path(), ".gitignore");
    assert!(after.starts_with(original), "{after:?}");
    assert!(
        after.contains("# >>> paper-vcs: ignored >>>\r\n*.scratch\r\n"),
        "{after:?}"
    );
    assert!(!after.replace("\r\n", "").contains('\n'), "{after:?}");
}

#[test]
fn a_project_without_a_gitignore_gets_one_holding_just_paper_block() {
    let (dir, mut project) = raw_data_project();
    fs::remove_file(dir.path().join(".gitignore")).unwrap();

    ignore(&mut project, &["data/raw"]);

    assert_eq!(text(dir.path(), ".gitignore"), paper_block(&["/data/raw/"]));
}

// ---- listing ----

fn find<'a>(rules: &'a [IgnoreRule], source: IgnoreSource, rule: &str) -> &'a IgnoreRule {
    rules
        .iter()
        .find(|r| r.source == source && r.rule == rule)
        .unwrap_or_else(|| panic!("no {source:?} rule {rule:?} in {rules:#?}"))
}

fn line_of(content: &str, rule: &str) -> usize {
    content
        .lines()
        .position(|line| line == rule)
        .unwrap_or_else(|| panic!("{rule:?} not in {content:?}"))
        + 1
}

#[test]
fn source_names_are_the_words_the_listing_groups_by() {
    assert_eq!(IgnoreSource::Paper.name(), "paper");
    assert_eq!(IgnoreSource::Profile.name(), "profile");
    assert_eq!(IgnoreSource::Yours.name(), "yours");
    assert_eq!(IgnoreSource::Local.name(), "local");
    assert_eq!(IgnoreSource::Global.name(), "global");
}

#[test]
fn the_listing_has_every_source_with_its_file_and_line() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);
    append(dir.path(), ".gitignore", "\n# my notes\n\nmine.txt\n");
    write(dir.path(), "docs/.gitignore", "# nested\n*.nested\n");
    append(dir.path(), ".git/info/exclude", "local.tmp\n");
    let global_dir = tempfile::tempdir().unwrap();
    let global = global_dir.path().join("global-ignore");
    fs::write(&global, "# global\n\nglobal.tmp\n").unwrap();
    git(
        dir.path(),
        &["config", "core.excludesFile", global.to_str().unwrap()],
    );
    ignore(&mut project, &["*.scratch"]);
    let gitignore = text(dir.path(), ".gitignore");

    let rules = project.ignore_rules();

    let paper = find(&rules, IgnoreSource::Paper, "*.scratch");
    assert_eq!(
        (paper.file.as_str(), paper.line),
        (".gitignore", line_of(&gitignore, "*.scratch"))
    );
    let profile = find(&rules, IgnoreSource::Profile, ".DS_Store");
    assert_eq!(
        (profile.file.as_str(), profile.line),
        (".gitignore", line_of(&gitignore, ".DS_Store"))
    );
    let yours = find(&rules, IgnoreSource::Yours, "mine.txt");
    assert_eq!(
        (yours.file.as_str(), yours.line),
        (".gitignore", line_of(&gitignore, "mine.txt"))
    );
    let nested = find(&rules, IgnoreSource::Yours, "*.nested");
    assert_eq!((nested.file.as_str(), nested.line), ("docs/.gitignore", 2));
    let exclude = text(dir.path(), ".git/info/exclude");
    let local = find(&rules, IgnoreSource::Local, "local.tmp");
    assert_eq!(
        (local.file.as_str(), local.line),
        (".git/info/exclude", line_of(&exclude, "local.tmp"))
    );
    let global_rule = find(&rules, IgnoreSource::Global, "global.tmp");
    assert!(
        global_rule.file.ends_with("global-ignore"),
        "{}",
        global_rule.file
    );
    assert_eq!(global_rule.line, 3);
    for rule in &rules {
        assert!(!rule.rule.is_empty(), "{rule:?}");
        assert!(
            !rule.rule.starts_with('#'),
            "comments are skipped: {rule:?}"
        );
        assert!(
            !rule.rule.contains("paper-vcs"),
            "markers are not rules: {rule:?}"
        );
        assert!(rule.line >= 1);
    }
}

#[test]
fn the_listing_is_grouped_paper_profile_yours_local_global() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);
    append(dir.path(), ".gitignore", "mine.txt\n");
    append(dir.path(), ".git/info/exclude", "local.tmp\n");
    let global_dir = tempfile::tempdir().unwrap();
    let global = global_dir.path().join("global-ignore");
    fs::write(&global, "global.tmp\n").unwrap();
    git(
        dir.path(),
        &["config", "core.excludesFile", global.to_str().unwrap()],
    );
    ignore(&mut project, &["*.scratch"]);

    let rules = project.ignore_rules();

    let rank = |source: IgnoreSource| match source {
        IgnoreSource::Paper => 0,
        IgnoreSource::Profile => 1,
        IgnoreSource::Yours => 2,
        IgnoreSource::Local => 3,
        IgnoreSource::Global => 4,
    };
    let ranks: Vec<u8> = rules.iter().map(|r| rank(r.source)).collect();
    let mut sorted = ranks.clone();
    sorted.sort_unstable();
    assert_eq!(ranks, sorted, "{rules:#?}");
    assert_eq!(
        sorted
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        5,
        "every source is present: {rules:#?}"
    );
}

#[test]
fn listing_reads_a_crlf_gitignore_without_the_carriage_returns() {
    let (dir, project) = project_with(&[("main.tex", "m\n")]);
    write(dir.path(), ".gitignore", "mine.log\r\n");

    let rules = project.ignore_rules();

    let yours = find(&rules, IgnoreSource::Yours, "mine.log");
    assert_eq!(yours.line, 1);
}

#[test]
fn a_project_with_no_rules_lists_none_of_its_own() {
    let (dir, project) = project_with(&[("main.tex", "m\n")]);
    fs::remove_file(dir.path().join(".gitignore")).unwrap();
    write(dir.path(), ".git/info/exclude", "# nothing\n\n");

    let own: Vec<IgnoreRule> = project
        .ignore_rules()
        .into_iter()
        .filter(|r| r.source != IgnoreSource::Global)
        .collect();

    assert!(own.is_empty(), "{own:#?}");
}

#[test]
fn listing_changes_nothing() {
    let (dir, project) = raw_data_project();
    let before = stored(dir.path());

    let _ = project.ignore_rules();

    assert_eq!(stored(dir.path()), before);
}

// ---- review round 1 ----

fn entry_with(project: &Project, description_start: &str) -> String {
    project
        .activity(100)
        .unwrap()
        .into_iter()
        .find(|e| e.description.starts_with(description_start))
        .unwrap_or_else(|| panic!("no entry starting {description_start:?}"))
        .short_id
}

/// A project whose newest save dropped a hand-written `*.log` line and added `x.log`.
fn project_that_stopped_ignoring_logs() -> (tempfile::TempDir, Project) {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);
    append(dir.path(), ".gitignore", "*.log\n");
    save_all(&mut project, "Ignore logs by hand");
    let without = text(dir.path(), ".gitignore").replace("*.log\n", "");
    write(dir.path(), ".gitignore", &without);
    write(dir.path(), "x.log", "log\n");
    save_all(&mut project, "Track logs");
    (dir, project)
}

#[test]
fn restore_of_a_file_the_older_save_did_not_have_still_removes_it_even_if_ignored_there() {
    for paths in [vec!["x.log"], vec![""]] {
        let (dir, mut project) = project_that_stopped_ignoring_logs();
        assert!(draft_files(dir.path()).contains(&"x.log".to_owned()));

        let restored = project
            .restore(&RestoreRequest {
                paths: paths.iter().map(|p| (*p).to_owned()).collect(),
                from: Some("last~1".parse().unwrap()),
            })
            .unwrap();

        assert!(
            restored.changes.iter().any(|c| c.path == "x.log"),
            "{paths:?}: {restored:?}"
        );
        assert!(!dir.path().join("x.log").exists(), "{paths:?}");
        assert!(
            !draft_files(dir.path()).contains(&"x.log".to_owned()),
            "{paths:?}"
        );
        git(dir.path(), &["fsck", "--strict", "--no-progress"]);
    }
}

#[test]
fn undo_to_a_state_whose_nested_gitignore_ignores_a_file_keeps_it_on_disk() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);
    write(dir.path(), "docs/.gitignore", "*.pdf\n");
    write(dir.path(), "docs/a.pdf", "pdf\n");
    save_all(&mut project, "Nested ignore");
    write(dir.path(), "docs/.gitignore", "\n");
    save_all(&mut project, "Track the pdf");
    assert!(draft_files(dir.path()).contains(&"docs/a.pdf".to_owned()));
    let target = entry_with(&project, "save \"Nested ignore\"");

    project.undo_to(&target).unwrap();

    assert_eq!(text(dir.path(), "docs/a.pdf"), "pdf\n");
    assert!(!draft_files(dir.path()).contains(&"docs/a.pdf".to_owned()));
    assert_eq!(text(dir.path(), "docs/.gitignore"), "*.pdf\n");
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn a_bare_name_starting_with_hash_is_escaped_so_it_is_not_a_comment() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);

    let outcome = ignore(&mut project, &["#later"]);

    assert_eq!(outcome.added, [r"\#later"]);
    assert_eq!(outcome.patterns, [r"\#later"]);
    assert!(check_ignore(dir.path(), "#later").is_some());
    assert!(check_ignore(dir.path(), "sub/#later").is_some());
}

#[test]
fn patterns_that_could_never_match_are_refused() {
    let (dir, mut project) = raw_data_project();
    let before = stored(dir.path());

    for (from, arg) in [
        ("", "../*.aux"),
        ("data", "../../*.aux"),
        ("", ".git/*.lock"),
        ("", "./.git/h[o]oks"),
    ] {
        let error = project.ignore(&request(from, &[arg], false)).unwrap_err();
        assert_three_part(error);
        assert_eq!(stored(dir.path()), before, "after refusing {arg:?}");
    }
}

#[test]
fn an_existing_file_starting_with_a_bang_is_ignored_not_refused() {
    let (dir, mut project) = project_with(&[("!bang", "b\n"), ("bang", "c\n")]);

    let outcome = ignore(&mut project, &["!bang"]);

    assert_eq!(outcome.untracked, ["!bang"]);
    assert!(check_ignore(dir.path(), "!bang").is_some());
    assert!(check_ignore(dir.path(), "bang").is_none());
}

#[test]
fn a_folder_typed_in_another_case_gets_the_spelling_on_disk() {
    let (dir, mut project) = raw_data_project();
    // Only a filesystem that ignores case (the default on macOS and Windows) finds it.
    if !dir.path().join("DATA/raw").exists() {
        return;
    }

    let outcome = ignore(&mut project, &["DATA/RAW"]);

    assert_eq!(outcome.added, ["/data/raw/"]);
    assert_eq!(outcome.untracked, ["data/raw/a.csv", "data/raw/sub/b.csv"]);
}

// ---- review round 2: case-only renames and restore ----

/// Names in `dir` (top level) whose lowercase is `lower`.
fn spellings(dir: &Path, lower: &str) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.to_lowercase() == lower)
        .collect()
}

/// A project saved with `old` spelled one way, then renamed (same bytes) to `new` and saved.
/// The rename goes through a temporary name, as it must on a filesystem that ignores case.
fn project_after_case_rename(old: &str, new: &str, file: &str) -> (tempfile::TempDir, Project) {
    let (dir, mut project) = project_with(&[(&format!("{old}/{file}"), "body\n")]);
    let tmp = dir.path().join("t-rename");
    fs::rename(dir.path().join(old), &tmp).unwrap();
    fs::rename(&tmp, dir.path().join(new)).unwrap();
    save_all(&mut project, "Renamed");
    (dir, project)
}

fn assert_survives(dir: &Path, folder: &str, file: &str) {
    let found = spellings(dir, folder);
    assert_eq!(
        found.len(),
        1,
        "exactly one spelling of {folder}: {found:?}"
    );
    assert_eq!(text(dir, &format!("{}/{file}", found[0])), "body\n");
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn restore_over_a_case_only_rename_of_a_folder_keeps_the_files() {
    let (dir, mut project) = project_after_case_rename("Notes", "notes", "a.txt");

    project
        .restore(&RestoreRequest {
            paths: vec!["Notes".to_owned(), "notes".to_owned()],
            from: Some("last~1".parse().unwrap()),
        })
        .unwrap();

    assert_survives(dir.path(), "notes", "a.txt");
}

#[test]
fn undo_of_a_case_only_rename_of_a_folder_keeps_the_files() {
    let (dir, mut project) = project_after_case_rename("Notes", "notes", "a.txt");

    project.undo().unwrap();
    project.undo().unwrap();

    assert_survives(dir.path(), "notes", "a.txt");
}

#[test]
fn restore_over_a_case_only_rename_of_a_file_keeps_the_file() {
    let (dir, mut project) = project_with(&[("Notes.tex", "body\n")]);
    let tmp = dir.path().join("t-rename");
    fs::rename(dir.path().join("Notes.tex"), &tmp).unwrap();
    fs::rename(&tmp, dir.path().join("notes.tex")).unwrap();
    save_all(&mut project, "Renamed");

    project
        .restore(&RestoreRequest {
            paths: vec!["Notes.tex".to_owned(), "notes.tex".to_owned()],
            from: Some("last~1".parse().unwrap()),
        })
        .unwrap();

    let found = spellings(dir.path(), "notes.tex");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(text(dir.path(), &found[0]), "body\n");
}

#[test]
fn undo_of_a_case_only_rename_of_a_file_keeps_the_file() {
    let (dir, mut project) = project_with(&[("Notes.tex", "body\n")]);
    let tmp = dir.path().join("t-rename");
    fs::rename(dir.path().join("Notes.tex"), &tmp).unwrap();
    fs::rename(&tmp, dir.path().join("notes.tex")).unwrap();
    save_all(&mut project, "Renamed");

    project.undo().unwrap();
    project.undo().unwrap();

    let found = spellings(dir.path(), "notes.tex");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(text(dir.path(), &found[0]), "body\n");
}

#[test]
fn restoring_an_ignored_untracked_file_with_the_saved_bytes_is_still_listed_as_blocked() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    save_all(&mut project, "Stop tracking raw data");

    let restored = project
        .restore(&RestoreRequest {
            paths: vec!["data/raw/a.csv".to_owned()],
            from: Some("last~1".parse().unwrap()),
        })
        .unwrap();

    assert!(
        restored
            .skipped
            .iter()
            .any(|s| s.path == "data/raw/a.csv" && s.reason == RestoreSkip::Blocked),
        "{:?}",
        restored.skipped
    );
    assert_eq!(text(dir.path(), "data/raw/a.csv"), "1,2\n");
}

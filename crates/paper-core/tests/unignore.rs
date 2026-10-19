//! `Project::unignore`, `Project::why_ignored` and `--local` for `Project::ignore` (PV-032),
//! cross-checked with the real `git` binary (`check-ignore -v`, `ls-tree`, `status`, `fsck`)
//! and with the files on disk, which must stay byte-identical.

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

/// What `git check-ignore -v --no-index <path>` reports.
#[derive(Debug, PartialEq, Eq)]
struct GitRule {
    /// The path is ignored. Git exits 0 for a negation too when `-v` is given, so a `!` rule
    /// is read as "not ignored" here.
    ignored: bool,
    file: String,
    line: usize,
    pattern: String,
}

/// The rule git says decides `path` (negations included), or `None` if no rule matches.
fn check_verbose(dir: &Path, path: &str) -> Option<GitRule> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["check-ignore", "-v", "--no-index", path])
        .output()
        .unwrap();
    let code = output.status.code();
    assert!(
        matches!(code, Some(0 | 1)),
        "git check-ignore {path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let source = stdout.split('\t').next().unwrap_or_default();
    if source.is_empty() {
        return None;
    }
    // `file:line:pattern`; the file may hold ':' on Windows, so split from the right.
    let mut parts = source.rsplitn(3, ':');
    let pattern = parts.next().unwrap().to_owned();
    let line = parts.next().unwrap().parse().unwrap();
    let file = parts.next().unwrap().to_owned();
    Some(GitRule {
        ignored: code == Some(0) && !pattern.starts_with('!'),
        file,
        line,
        pattern,
    })
}

fn write(dir: &Path, path: &str, content: &str) {
    let absolute = dir.join(path);
    fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    fs::write(absolute, content).unwrap();
}

fn text(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

fn append(dir: &Path, path: &str, extra: &str) {
    let mut content = fs::read_to_string(dir.join(path)).unwrap_or_default();
    content.push_str(extra);
    write(dir, path, &content);
}

/// `.git/info/exclude`, or the empty string when it does not exist.
fn exclude(dir: &Path) -> String {
    fs::read_to_string(dir.join(".git/info/exclude")).unwrap_or_default()
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

/// `files(dir)` without `.gitignore`, the only file these commands may change.
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

fn paper_block(rules: &[&str]) -> String {
    let mut out = "# >>> paper-vcs: ignored >>>\n".to_owned();
    for rule in rules {
        out.push_str(rule);
        out.push('\n');
    }
    out.push_str("# <<< paper-vcs: ignored <<<\n");
    out
}

fn line_of(content: &str, rule: &str) -> usize {
    content
        .lines()
        .position(|line| line == rule)
        .unwrap_or_else(|| panic!("{rule:?} not in {content:?}"))
        + 1
}

/// Everything a refused or empty command must leave alone.
fn stored(
    dir: &Path,
) -> (
    String,
    BTreeMap<String, Vec<u8>>,
    String,
    usize,
    String,
    String,
) {
    (
        git(dir, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        files(dir),
        exclude(dir),
        op_count(dir),
        fs::read_to_string(dir.join(".git/paper/activity/head")).unwrap(),
        git(dir, &["ls-files", "--stage"]),
    )
}

fn ignore_req(args: &[&str], local: bool) -> IgnoreRequest {
    IgnoreRequest {
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        from: String::new(),
        yes: false,
        local,
    }
}

fn unignore_req(from: &str, args: &[&str], local: bool) -> UnignoreRequest {
    UnignoreRequest {
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        from: from.to_owned(),
        local,
    }
}

/// `paper ignore <args>` from the top of the project; panics on an error.
fn ignore(project: &mut Project, args: &[&str]) -> IgnoreOutcome {
    project.ignore(&ignore_req(args, false)).unwrap()
}

fn ignore_local(project: &mut Project, args: &[&str]) -> IgnoreOutcome {
    project.ignore(&ignore_req(args, true)).unwrap()
}

/// `paper unignore <args>` from the top of the project; panics on an error.
fn unignore(project: &mut Project, args: &[&str]) -> UnignoreOutcome {
    project
        .unignore(&unignore_req("", args, false))
        .unwrap_or_else(|error| panic!("{error:?}"))
}

fn unignore_local(project: &mut Project, args: &[&str]) -> UnignoreOutcome {
    project
        .unignore(&unignore_req("", args, true))
        .unwrap_or_else(|error| panic!("{error:?}"))
}

fn undone(outcome: UndoOutcome) -> Undone {
    match outcome {
        UndoOutcome::Undone(undone) => undone,
        other => panic!("expected an undone entry, got {other:?}"),
    }
}

/// The text of a refusal, after checking it has the three parts and is the user's to fix.
fn refusal_text(error: UnignoreError) -> String {
    let paper = PaperError::from(error);
    assert_eq!(paper.kind, ErrorKind::User, "{paper:?}");
    assert!(!paper.message.is_empty(), "{paper:?}");
    assert!(!paper.why.is_empty(), "{paper:?}");
    assert!(!paper.next.is_empty(), "{paper:?}");
    format!("{} {} {:?}", paper.message, paper.why, paper.next)
}

fn raw_data_project() -> (tempfile::TempDir, Project) {
    project_with(&[
        ("main.tex", "\\documentclass{article}\n"),
        ("data/raw/a.csv", "1,2\n"),
        ("data/raw/sub/b.csv", "3,4\n"),
        ("data/keep.csv", "keep\n"),
    ])
}

/// Your own `*.log` line ignores `notes/keep.log`, which exists on disk but is not saved.
fn log_project() -> (tempfile::TempDir, Project) {
    let (dir, project) = project_with(&[("main.tex", "m\n"), (".gitignore", "*.log\n")]);
    write(dir.path(), "notes/keep.log", "keep\n");
    write(dir.path(), "notes/other.log", "other\n");
    (dir, project)
}

const RAW_FILES: [&str; 2] = ["data/raw/a.csv", "data/raw/sub/b.csv"];

/// `why_ignored("", path)` must report what `git check-ignore -v` reports.
fn assert_why_matches_git(dir: &Path, project: &Project, path: &str) -> IgnoreWhy {
    let why = project.why_ignored("", path).unwrap();
    let git = check_verbose(dir, path);
    match (&why.rule, &git) {
        (None, None) => assert!(!why.ignored, "{why:?}"),
        (Some(rule), Some(git)) => {
            assert_eq!(rule.rule, git.pattern, "{why:?} vs {git:?}");
            assert_eq!(rule.file, git.file, "{why:?} vs {git:?}");
            assert_eq!(rule.line, git.line, "{why:?} vs {git:?}");
            assert_eq!(rule.negated, git.pattern.starts_with('!'), "{why:?}");
            assert_eq!(why.ignored, git.ignored, "{why:?} vs {git:?}");
        }
        other => panic!("paper and git disagree on {path}: {other:?}"),
    }
    why
}

// ---- helper sanity (passes on main) ----

#[test]
fn the_check_helper_reports_plain_rules_negations_and_no_rule() {
    let (dir, _project) = project_with(&[("main.tex", "m\n"), (".gitignore", "*.log\n")]);
    append(dir.path(), ".gitignore", "!keep.log\n");
    append(dir.path(), ".git/info/exclude", "local.tmp\n");

    let plain = check_verbose(dir.path(), "a.log").unwrap();
    assert!(plain.ignored);
    assert_eq!(
        (plain.file.as_str(), plain.pattern.as_str()),
        (".gitignore", "*.log")
    );
    assert_eq!(plain.line, 1);

    let negated = check_verbose(dir.path(), "keep.log").unwrap();
    assert!(!negated.ignored, "{negated:?}");
    assert_eq!(negated.pattern, "!keep.log");

    let local = check_verbose(dir.path(), "local.tmp").unwrap();
    assert_eq!(local.file, ".git/info/exclude");
    assert!(local.line >= 1);

    assert_eq!(check_verbose(dir.path(), "main.tex"), None);
}

#[test]
fn the_other_helpers_see_exclude_files_blocks_and_the_draft() {
    let (dir, _project) = raw_data_project();
    assert!(!exclude(dir.path()).contains("zzz"));
    append(dir.path(), ".git/info/exclude", "zzz\n");
    assert!(exclude(dir.path()).contains("zzz"));
    assert!(paper_block(&["/a/", "!/b"]).starts_with("# >>> paper-vcs: ignored >>>\n/a/\n!/b\n"));
    assert_eq!(line_of("a\nb\nc\n", "c"), 3);
    assert!(draft_files(dir.path()).contains(&"main.tex".to_owned()));
    assert!(files(dir.path()).contains_key("data/raw/a.csv"));
    assert!(!files_but_gitignore(dir.path()).contains_key(".gitignore"));
    assert_eq!(head_op(dir.path())["kind"], "init");
    let before = stored(dir.path());
    assert_eq!(stored(dir.path()), before);
    assert_eq!(descriptions(&raw_data_project().1).len(), 1);
    assert_eq!(
        ignore_req(&["x"], true),
        IgnoreRequest {
            args: vec!["x".to_owned()],
            from: String::new(),
            yes: false,
            local: true
        }
    );
}

// ---- removing paper's rule ----

#[test]
fn unignore_removes_paper_rule_and_tracks_the_files_again_in_one_entry() {
    let (dir, mut project) = raw_data_project();
    let disk_before = files_but_gitignore(dir.path());
    ignore(&mut project, &["data/raw"]);
    let ops_before = op_count(dir.path());

    let outcome = unignore(&mut project, &["data/raw"]);

    assert_eq!(outcome.removed, ["/data/raw/"]);
    assert!(outcome.negated.is_empty(), "{outcome:?}");
    assert_eq!(outcome.tracked, RAW_FILES);
    assert!(!outcome.local);
    let entry = outcome.entry.expect("an entry is recorded");
    assert_eq!(entry.kind, OpKind::Unignore);
    assert_eq!(entry.description, "unignore /data/raw/");
    assert_eq!(
        op_count(dir.path()),
        ops_before + 1,
        "one entry per command"
    );
    assert_eq!(head_op(dir.path())["kind"], "unignore");
    assert_eq!(project.activity(1).unwrap()[0], entry);

    assert!(!text(dir.path(), ".gitignore").contains("/data/raw/"));
    assert_eq!(check_verbose(dir.path(), "data/raw/a.csv"), None);
    let draft = draft_files(dir.path());
    for file in RAW_FILES {
        assert!(draft.contains(&file.to_owned()), "{draft:?}");
    }
    assert_eq!(files_but_gitignore(dir.path()), disk_before);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn the_files_unignored_are_part_of_the_next_save() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    save_all(&mut project, "Stop tracking raw data");
    assert!(!git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]).contains("data/raw/"));
    unignore(&mut project, &["data/raw"]);

    save_all(&mut project, "Track raw data again");

    let tracked = git(dir.path(), &["ls-tree", "-r", "--name-only", "main"]);
    for file in RAW_FILES {
        assert!(tracked.contains(file), "{tracked}");
    }
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn undo_of_an_unignore_restores_the_old_gitignore_and_takes_the_files_out_of_the_draft() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    let gitignore_after_ignore = fs::read(dir.path().join(".gitignore")).unwrap();
    let disk_after_ignore = files(dir.path());
    unignore(&mut project, &["data/raw"]);

    let undone = undone(project.undo().unwrap());

    assert_eq!(undone.entry.kind, OpKind::Unignore);
    assert_eq!(undone.entry.description, "unignore /data/raw/");
    assert!(!undone.local_rules());
    assert!(undone.skipped.is_empty(), "{:?}", undone.skipped);
    assert_eq!(
        fs::read(dir.path().join(".gitignore")).unwrap(),
        gitignore_after_ignore
    );
    assert_eq!(
        files(dir.path()),
        disk_after_ignore,
        "the files stay on disk"
    );
    let draft = draft_files(dir.path());
    assert!(
        !draft.iter().any(|f| f.starts_with("data/raw/")),
        "{draft:?}"
    );
    assert!(check_verbose(dir.path(), "data/raw/a.csv").is_some());
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);

    // Undoing the ignore as well gets back to the very start.
    project.undo().unwrap();
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

#[test]
fn redo_of_an_unignore_tracks_the_files_again() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    unignore(&mut project, &["data/raw"]);
    let gitignore_after = fs::read(dir.path().join(".gitignore")).unwrap();
    project.undo().unwrap();

    project.redo().unwrap();

    assert_eq!(
        fs::read(dir.path().join(".gitignore")).unwrap(),
        gitignore_after
    );
    let draft = draft_files(dir.path());
    for file in RAW_FILES {
        assert!(draft.contains(&file.to_owned()), "{draft:?}");
    }
    assert_eq!(check_verbose(dir.path(), "data/raw/a.csv"), None);
}

#[test]
fn unsaved_edits_are_recorded_first_as_their_own_entry() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    write(dir.path(), "main.tex", "edited\n");

    unignore(&mut project, &["data/raw"]);

    let all = descriptions(&project);
    assert_eq!(all[0], "unignore /data/raw/");
    assert!(all[1].starts_with("edited main.tex"), "{all:?}");
}

// ---- negating ----

#[test]
fn unignore_adds_a_negation_when_your_own_line_ignores_the_file() {
    let (dir, mut project) = log_project();
    let disk_before = files_but_gitignore(dir.path());
    let hand_written = text(dir.path(), ".gitignore");

    let outcome = unignore(&mut project, &["notes/keep.log"]);

    assert!(outcome.removed.is_empty(), "{outcome:?}");
    assert_eq!(outcome.negated, ["!/notes/keep.log"]);
    assert_eq!(outcome.tracked, ["notes/keep.log"]);
    let entry = outcome.entry.expect("an entry is recorded");
    assert_eq!(entry.kind, OpKind::Unignore);
    assert_eq!(entry.description, "unignore /notes/keep.log");

    let gitignore = text(dir.path(), ".gitignore");
    assert!(gitignore.starts_with(&hand_written), "{gitignore}");
    assert!(
        gitignore.contains(&paper_block(&["!/notes/keep.log"])),
        "{gitignore}"
    );
    let rule = check_verbose(dir.path(), "notes/keep.log").unwrap();
    assert!(!rule.ignored, "{rule:?}");
    assert_eq!(rule.pattern, "!/notes/keep.log");
    assert!(
        check_verbose(dir.path(), "notes/other.log")
            .unwrap()
            .ignored
    );
    let draft = draft_files(dir.path());
    assert!(draft.contains(&"notes/keep.log".to_owned()), "{draft:?}");
    assert!(!draft.contains(&"notes/other.log".to_owned()), "{draft:?}");
    assert_eq!(files_but_gitignore(dir.path()), disk_before);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn the_negation_goes_last_in_paper_block_so_it_wins() {
    let (dir, mut project) = log_project();
    ignore(&mut project, &["draft.tmp", "*.log"]);

    unignore(&mut project, &["notes/keep.log"]);

    let gitignore = text(dir.path(), ".gitignore");
    let block = gitignore
        .lines()
        .skip_while(|l| *l != "# >>> paper-vcs: ignored >>>")
        .take_while(|l| *l != "# <<< paper-vcs: ignored <<<")
        .collect::<Vec<_>>();
    assert_eq!(block.last(), Some(&"!/notes/keep.log"), "{gitignore}");
    assert!(block.contains(&"*.log"), "{gitignore}");
    assert!(!check_verbose(dir.path(), "notes/keep.log").unwrap().ignored);
}

#[test]
fn a_pattern_argument_is_negated_as_typed() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n"), (".gitignore", "*.scratch\n")]);
    write(dir.path(), "a.scratch", "a\n");
    write(dir.path(), "sub/b.scratch", "b\n");

    let outcome = unignore(&mut project, &["*.scratch"]);

    assert_eq!(outcome.negated, ["!*.scratch"]);
    assert_eq!(outcome.tracked, ["a.scratch", "sub/b.scratch"]);
    assert!(text(dir.path(), ".gitignore").contains(&paper_block(&["!*.scratch"])));
    assert_eq!(
        check_verbose(dir.path(), "a.scratch").map(|r| r.ignored),
        Some(false)
    );
}

#[test]
fn a_profile_rule_is_negated_too() {
    let (dir, mut project) = raw_data_project();
    write(dir.path(), ".DS_Store", "finder\n");
    assert!(check_verbose(dir.path(), ".DS_Store").unwrap().ignored);

    let outcome = unignore(&mut project, &[".DS_Store"]);

    assert_eq!(outcome.negated, ["!/.DS_Store"]);
    assert_eq!(outcome.tracked, [".DS_Store"]);
    assert!(!check_verbose(dir.path(), ".DS_Store").unwrap().ignored);
    assert!(text(dir.path(), ".gitignore").contains("paper-vcs profile"));
}

#[test]
fn a_rule_in_info_exclude_is_beaten_by_a_negation_in_gitignore() {
    let (dir, mut project) = raw_data_project();
    append(dir.path(), ".git/info/exclude", "*.tmp\n");
    write(dir.path(), "x.tmp", "x\n");
    let exclude_before = exclude(dir.path());

    let outcome = unignore(&mut project, &["x.tmp"]);

    assert_eq!(outcome.negated, ["!/x.tmp"]);
    assert_eq!(outcome.tracked, ["x.tmp"]);
    assert_eq!(
        exclude(dir.path()),
        exclude_before,
        "the shared file is the one that changes"
    );
    assert!(text(dir.path(), ".gitignore").contains(&paper_block(&["!/x.tmp"])));
    assert!(!check_verbose(dir.path(), "x.tmp").unwrap().ignored);
}

#[test]
fn a_rule_and_its_negation_never_both_stay_in_paper_block() {
    let (dir, mut project) = log_project();
    unignore(&mut project, &["notes/keep.log"]);
    assert!(text(dir.path(), ".gitignore").contains("!/notes/keep.log"));

    let outcome = ignore(&mut project, &["notes/keep.log"]);

    let gitignore = text(dir.path(), ".gitignore");
    assert!(!gitignore.contains("/notes/keep.log"), "{gitignore}");
    assert_eq!(outcome.untracked, ["notes/keep.log"]);
    assert_eq!(outcome.entry.expect("an entry").kind, OpKind::Ignore);
    assert!(check_verbose(dir.path(), "notes/keep.log").unwrap().ignored);
    let draft = draft_files(dir.path());
    assert!(!draft.contains(&"notes/keep.log".to_owned()), "{draft:?}");
    assert_eq!(text(dir.path(), "notes/keep.log"), "keep\n");
}

#[test]
fn ignore_after_unignore_adds_a_rule_when_removing_the_negation_is_not_enough() {
    let (dir, mut project) = log_project();
    unignore(&mut project, &["notes/keep.log"]);
    // Your own `*.log` line goes away by hand: the negation was the only thing left to undo.
    let without = text(dir.path(), ".gitignore").replace("*.log\n", "");
    write(dir.path(), ".gitignore", &without);

    ignore(&mut project, &["notes/keep.log"]);

    let gitignore = text(dir.path(), ".gitignore");
    assert!(!gitignore.contains("!/notes/keep.log"), "{gitignore}");
    assert!(
        gitignore.contains(&paper_block(&["/notes/keep.log"])),
        "{gitignore}"
    );
    assert!(check_verbose(dir.path(), "notes/keep.log").unwrap().ignored);
    assert!(!draft_files(dir.path()).contains(&"notes/keep.log".to_owned()));
}

// ---- refusals ----

#[test]
fn a_file_in_an_excluded_folder_is_refused_naming_the_rule_and_nothing_is_written() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n"), (".gitignore", "/archive/\n")]);
    write(dir.path(), "archive/old.txt", "old\n");
    let gitignore = text(dir.path(), ".gitignore");
    let line = line_of(&gitignore, "/archive/");
    let before = stored(dir.path());

    let error = project
        .unignore(&unignore_req("", &["archive/old.txt"], false))
        .unwrap_err();

    let all = refusal_text(error);
    assert!(all.contains("/archive/"), "{all}");
    assert!(all.contains(".gitignore"), "{all}");
    assert!(all.contains(&line.to_string()), "{all}");
    assert!(
        all.contains("unignore archive"),
        "the way out is named: {all}"
    );
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn unignoring_the_excluded_folder_is_the_way_out() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n"), (".gitignore", "/archive/\n")]);
    write(dir.path(), "archive/old.txt", "old\n");

    let outcome = unignore(&mut project, &["archive"]);

    assert_eq!(outcome.negated, ["!/archive/"]);
    assert_eq!(outcome.tracked, ["archive/old.txt"]);
    // Git reports the negation for the folder; the file under it has no rule of its own.
    assert_eq!(
        check_verbose(dir.path(), "archive").map(|r| (r.ignored, r.pattern)),
        Some((false, "!/archive/".to_owned()))
    );
    assert_eq!(check_verbose(dir.path(), "archive/old.txt"), None);
    assert!(draft_files(dir.path()).contains(&"archive/old.txt".to_owned()));
}

#[test]
fn a_nested_gitignore_beats_the_root_so_the_path_is_refused_naming_that_file() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n"), ("docs/.gitignore", "*.tmp\n")]);
    write(dir.path(), "docs/x.tmp", "x\n");
    let before = stored(dir.path());

    let error = project
        .unignore(&unignore_req("", &["docs/x.tmp"], false))
        .unwrap_err();

    let all = refusal_text(error);
    assert!(all.contains("docs/.gitignore"), "{all}");
    assert!(all.contains("*.tmp"), "{all}");
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn a_later_line_of_your_own_in_the_same_file_keeps_the_path_ignored() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n")]);
    ignore(&mut project, &["draft.tmp"]);
    append(dir.path(), ".gitignore", "*.log\n");
    project.snapshot().unwrap();
    write(dir.path(), "notes/keep.log", "keep\n");
    let before = stored(dir.path());

    let error = project
        .unignore(&unignore_req("", &["notes/keep.log"], false))
        .unwrap_err();

    let all = refusal_text(error);
    assert!(all.contains("*.log"), "{all}");
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn one_refused_argument_stops_the_whole_command() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n"), (".gitignore", "/archive/\n")]);
    write(dir.path(), "archive/old.txt", "old\n");
    write(dir.path(), "a.tmp", "a\n");
    ignore(&mut project, &["a.tmp"]);
    let before = stored(dir.path());

    let error = project
        .unignore(&unignore_req("", &["a.tmp", "archive/old.txt"], false))
        .unwrap_err();

    refusal_text(error);
    assert_eq!(
        stored(dir.path()),
        before,
        "the fine argument was not applied either"
    );
}

#[test]
fn unignore_refuses_what_ignore_refuses_and_a_leading_bang() {
    let (dir, mut project) = raw_data_project();
    let before = stored(dir.path());

    for refused in [
        "",
        "!x",
        "!/data/raw/keep.csv",
        ".gitignore",
        "./.gitignore",
        "paper.toml",
        ".git",
        ".git/hooks",
        ".git/info/exclude",
        "../outside",
        "data/../../outside",
        ".",
    ] {
        let error = project
            .unignore(&unignore_req("", &[refused], false))
            .unwrap_err();
        refusal_text(error);
        assert_eq!(stored(dir.path()), before, "after refusing {refused:?}");
    }
}

// ---- nothing to do ----

#[test]
fn a_path_that_is_not_ignored_is_a_no_op_with_no_entry() {
    let (dir, mut project) = raw_data_project();
    let before = stored(dir.path());

    let outcome = unignore(&mut project, &["data/keep.csv", "main.tex"]);

    assert!(outcome.removed.is_empty(), "{outcome:?}");
    assert!(outcome.negated.is_empty(), "{outcome:?}");
    assert!(outcome.tracked.is_empty(), "{outcome:?}");
    assert!(outcome.entry.is_none(), "{outcome:?}");
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn unignoring_twice_is_a_no_op_the_second_time() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    unignore(&mut project, &["data/raw"]);
    let before = stored(dir.path());

    let outcome = unignore(&mut project, &["data/raw"]);

    assert!(outcome.entry.is_none(), "{outcome:?}");
    assert!(
        outcome.removed.is_empty() && outcome.negated.is_empty(),
        "{outcome:?}"
    );
    assert_eq!(stored(dir.path()), before);
}

// ---- --local ----

#[test]
fn ignore_local_writes_paper_block_in_info_exclude_and_leaves_gitignore_alone() {
    let (dir, mut project) = raw_data_project();
    let gitignore_before = fs::read(dir.path().join(".gitignore")).unwrap();
    let disk_before = files_but_gitignore(dir.path());
    let ops_before = op_count(dir.path());

    let outcome = ignore_local(&mut project, &["data/raw"]);

    assert!(outcome.local);
    assert_eq!(outcome.added, ["/data/raw/"]);
    assert_eq!(outcome.untracked, RAW_FILES);
    assert_eq!(
        fs::read(dir.path().join(".gitignore")).unwrap(),
        gitignore_before,
        "co-authors' .gitignore is unchanged"
    );
    assert!(
        exclude(dir.path()).contains(&paper_block(&["/data/raw/"])),
        "{}",
        exclude(dir.path())
    );
    let rule = check_verbose(dir.path(), "data/raw/a.csv").unwrap();
    assert_eq!(rule.file, ".git/info/exclude");
    assert!(rule.ignored);
    let draft = draft_files(dir.path());
    assert!(
        !draft.iter().any(|f| f.starts_with("data/raw/")),
        "{draft:?}"
    );
    assert_eq!(files_but_gitignore(dir.path()), disk_before);
    assert_eq!(op_count(dir.path()), ops_before + 1, "one entry");
    let entry = outcome.entry.expect("an entry is recorded");
    assert_eq!(entry.kind, OpKind::Ignore);
    assert!(
        entry.description.starts_with("ignore /data/raw/"),
        "{entry:?}"
    );
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn ignore_local_creates_info_exclude_and_its_folder_when_missing() {
    let (dir, mut project) = raw_data_project();
    fs::remove_dir_all(dir.path().join(".git/info")).unwrap();

    ignore_local(&mut project, &["data/raw"]);

    assert!(exclude(dir.path()).contains(&paper_block(&["/data/raw/"])));
    assert!(check_verbose(dir.path(), "data/raw/a.csv").unwrap().ignored);
}

#[test]
fn ignore_local_keeps_lines_you_wrote_in_info_exclude() {
    let (dir, mut project) = raw_data_project();
    append(dir.path(), ".git/info/exclude", "# mine\nmine.tmp\n");
    let before = exclude(dir.path());

    ignore_local(&mut project, &["data/raw"]);

    let after = exclude(dir.path());
    assert!(after.starts_with(&before), "{after}");
    assert!(after.contains(&paper_block(&["/data/raw/"])), "{after}");
}

#[test]
fn ignore_local_again_is_a_no_op() {
    let (dir, mut project) = raw_data_project();
    ignore_local(&mut project, &["data/raw"]);
    let before = stored(dir.path());

    let outcome = ignore_local(&mut project, &["data/raw"]);

    assert!(outcome.entry.is_none(), "{outcome:?}");
    assert_eq!(outcome.already, ["/data/raw/"]);
    assert!(outcome.local);
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn undo_of_a_local_ignore_tracks_the_files_again_and_leaves_info_exclude_alone() {
    let (dir, mut project) = raw_data_project();
    let gitignore_before = fs::read(dir.path().join(".gitignore")).unwrap();
    let disk_before = files(dir.path());
    ignore_local(&mut project, &["data/raw"]);
    let exclude_after = exclude(dir.path());

    let undone = undone(project.undo().unwrap());

    assert_eq!(undone.entry.kind, OpKind::Ignore);
    assert!(undone.local_rules(), "the undo note needs to know");
    assert_eq!(
        exclude(dir.path()),
        exclude_after,
        ".git/info/exclude is outside history"
    );
    assert_eq!(
        fs::read(dir.path().join(".gitignore")).unwrap(),
        gitignore_before
    );
    assert_eq!(files(dir.path()), disk_before);
    let draft = draft_files(dir.path());
    for file in RAW_FILES {
        assert!(draft.contains(&file.to_owned()), "{draft:?}");
    }
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn redo_of_a_local_ignore_untracks_again_and_leaves_info_exclude_alone() {
    let (dir, mut project) = raw_data_project();
    ignore_local(&mut project, &["data/raw"]);
    project.undo().unwrap();
    let exclude_before = exclude(dir.path());

    let redone = undone(project.redo().unwrap());

    assert!(redone.local_rules());
    assert_eq!(exclude(dir.path()), exclude_before);
    let draft = draft_files(dir.path());
    assert!(
        !draft.iter().any(|f| f.starts_with("data/raw/")),
        "{draft:?}"
    );
}

#[test]
fn undo_of_a_shared_ignore_does_not_claim_local_rules() {
    let (_dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);

    let undone = undone(project.undo().unwrap());

    assert!(!undone.local_rules());
}

#[test]
fn unignore_local_removes_the_rule_from_info_exclude_and_tracks_the_files() {
    let (dir, mut project) = raw_data_project();
    ignore_local(&mut project, &["data/raw"]);
    let gitignore_before = fs::read(dir.path().join(".gitignore")).unwrap();

    let outcome = unignore_local(&mut project, &["data/raw"]);

    assert!(outcome.local);
    assert_eq!(outcome.removed, ["/data/raw/"]);
    assert_eq!(outcome.tracked, RAW_FILES);
    let entry = outcome.entry.expect("an entry is recorded");
    assert_eq!(entry.kind, OpKind::Unignore);
    assert!(
        !exclude(dir.path()).contains("/data/raw/"),
        "{}",
        exclude(dir.path())
    );
    assert_eq!(
        fs::read(dir.path().join(".gitignore")).unwrap(),
        gitignore_before
    );
    assert_eq!(check_verbose(dir.path(), "data/raw/a.csv"), None);
    let draft = draft_files(dir.path());
    for file in RAW_FILES {
        assert!(draft.contains(&file.to_owned()), "{draft:?}");
    }
}

#[test]
fn undo_of_a_local_unignore_untracks_and_leaves_info_exclude_as_it_is() {
    let (dir, mut project) = raw_data_project();
    ignore_local(&mut project, &["data/raw"]);
    unignore_local(&mut project, &["data/raw"]);
    let exclude_after = exclude(dir.path());

    let undone = undone(project.undo().unwrap());

    assert_eq!(undone.entry.kind, OpKind::Unignore);
    assert!(undone.local_rules());
    assert_eq!(exclude(dir.path()), exclude_after);
    let draft = draft_files(dir.path());
    assert!(
        !draft.iter().any(|f| f.starts_with("data/raw/")),
        "{draft:?}"
    );
}

#[test]
fn unignore_local_adds_a_negation_to_info_exclude_for_a_rule_written_there() {
    let (dir, mut project) = raw_data_project();
    append(dir.path(), ".git/info/exclude", "*.tmp\n");
    write(dir.path(), "x.tmp", "x\n");
    let gitignore_before = fs::read(dir.path().join(".gitignore")).unwrap();

    let outcome = unignore_local(&mut project, &["x.tmp"]);

    assert_eq!(outcome.negated, ["!/x.tmp"]);
    assert_eq!(outcome.tracked, ["x.tmp"]);
    assert!(
        exclude(dir.path()).contains(&paper_block(&["!/x.tmp"])),
        "{}",
        exclude(dir.path())
    );
    assert_eq!(
        fs::read(dir.path().join(".gitignore")).unwrap(),
        gitignore_before
    );
    let rule = check_verbose(dir.path(), "x.tmp").unwrap();
    assert_eq!(
        (rule.ignored, rule.file.as_str()),
        (false, ".git/info/exclude")
    );
}

#[test]
fn unignore_local_of_a_path_the_shared_gitignore_ignores_is_refused_with_a_hint() {
    let (dir, mut project) = log_project();
    let before = stored(dir.path());

    let error = project
        .unignore(&unignore_req("", &["notes/keep.log"], true))
        .unwrap_err();

    let all = refusal_text(error);
    assert!(all.contains("*.log"), "{all}");
    assert!(all.contains(".gitignore"), "{all}");
    assert!(all.contains("without"), "{all}");
    assert!(all.contains("--local"), "{all}");
    assert_eq!(
        stored(dir.path()),
        before,
        "info/exclude is not touched either"
    );
}

#[test]
fn unignore_local_of_a_shared_rule_is_refused_even_when_paper_wrote_it() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    let before = stored(dir.path());

    let error = project
        .unignore(&unignore_req("", &["data/raw"], true))
        .unwrap_err();

    let all = refusal_text(error);
    assert!(all.contains("/data/raw/"), "{all}");
    assert_eq!(stored(dir.path()), before);
}

// ---- --why ----

/// One rule from each place a rule can live, and one path for each.
fn why_project() -> (tempfile::TempDir, Project) {
    let (dir, mut project) = project_with(&[
        ("main.tex", "m\n"),
        ("data/raw/a.csv", "1,2\n"),
        ("docs/.gitignore", "# nested\n*.nested\n"),
        (".gitignore", "# mine\n\nmine.txt\n/archive/\n"),
    ]);
    append(dir.path(), ".git/info/exclude", "# local\nlocal.tmp\n");
    write(dir.path(), ".DS_Store", "finder\n");
    write(dir.path(), "mine.txt", "mine\n");
    write(dir.path(), "docs/x.nested", "n\n");
    write(dir.path(), "local.tmp", "l\n");
    write(dir.path(), "archive/old.txt", "old\n");
    ignore(&mut project, &["data/raw"]);
    (dir, project)
}

#[test]
fn why_names_the_rule_source_file_and_line_for_each_source() {
    let (dir, project) = why_project();
    let gitignore = text(dir.path(), ".gitignore");
    let exclude_text = exclude(dir.path());

    let cases: [(&str, &str, IgnoreSource, &str, usize); 6] = [
        (
            "data/raw/a.csv",
            "/data/raw/",
            IgnoreSource::Paper,
            ".gitignore",
            line_of(&gitignore, "/data/raw/"),
        ),
        (
            ".DS_Store",
            ".DS_Store",
            IgnoreSource::Profile,
            ".gitignore",
            line_of(&gitignore, ".DS_Store"),
        ),
        (
            "mine.txt",
            "mine.txt",
            IgnoreSource::Yours,
            ".gitignore",
            line_of(&gitignore, "mine.txt"),
        ),
        (
            "docs/x.nested",
            "*.nested",
            IgnoreSource::Yours,
            "docs/.gitignore",
            2,
        ),
        (
            "local.tmp",
            "local.tmp",
            IgnoreSource::Local,
            ".git/info/exclude",
            line_of(&exclude_text, "local.tmp"),
        ),
        (
            "archive/old.txt",
            "/archive/",
            IgnoreSource::Yours,
            ".gitignore",
            line_of(&gitignore, "/archive/"),
        ),
    ];
    for (path, rule, source, file, line) in cases {
        let why = assert_why_matches_git(dir.path(), &project, path);
        assert_eq!(why.path, path);
        assert!(why.ignored, "{why:?}");
        assert!(!why.tracked, "{path} was never saved: {why:?}");
        let found = why.rule.unwrap_or_else(|| panic!("no rule for {path}"));
        assert_eq!(found.rule, rule, "{path}");
        assert_eq!(found.source, source, "{path}");
        assert_eq!(found.file, file, "{path}");
        assert_eq!(found.line, line, "{path}");
        assert!(!found.negated, "{path}");
    }
}

#[test]
fn why_reports_a_folder_given_as_the_folder_itself() {
    let (dir, project) = why_project();

    let why = assert_why_matches_git(dir.path(), &project, "data/raw");

    assert!(why.ignored, "{why:?}");
    assert_eq!(why.rule.unwrap().rule, "/data/raw/");
}

#[test]
fn why_says_not_ignored_with_no_rule_for_a_path_no_rule_touches() {
    let (dir, project) = why_project();

    let tracked = assert_why_matches_git(dir.path(), &project, "main.tex");
    assert_eq!(
        tracked,
        IgnoreWhy {
            path: "main.tex".to_owned(),
            ignored: false,
            tracked: true,
            rule: None
        }
    );
    let absent = project.why_ignored("", "nothing/here.txt").unwrap();
    assert_eq!(
        absent,
        IgnoreWhy {
            path: "nothing/here.txt".to_owned(),
            ignored: false,
            tracked: false,
            rule: None
        }
    );
}

#[test]
fn why_reports_the_negation_that_decides() {
    let (dir, mut project) = log_project();
    unignore(&mut project, &["notes/keep.log"]);
    let gitignore = text(dir.path(), ".gitignore");

    let why = assert_why_matches_git(dir.path(), &project, "notes/keep.log");

    assert!(!why.ignored, "{why:?}");
    assert!(why.tracked, "{why:?}");
    let rule = why.rule.expect("the negation decides");
    assert_eq!(rule.rule, "!/notes/keep.log");
    assert!(rule.negated);
    assert_eq!(rule.source, IgnoreSource::Paper);
    assert_eq!(rule.file, ".gitignore");
    assert_eq!(rule.line, line_of(&gitignore, "!/notes/keep.log"));
    // The sibling is still ignored by the plain rule.
    let other = assert_why_matches_git(dir.path(), &project, "notes/other.log");
    assert!(other.ignored);
    assert_eq!(other.rule.unwrap().rule, "*.log");
}

#[test]
fn why_says_a_saved_file_is_still_tracked_under_a_later_rule() {
    let (dir, project) = raw_data_project();
    append(dir.path(), ".gitignore", "*.csv\n");

    let why = assert_why_matches_git(dir.path(), &project, "data/keep.csv");

    assert!(why.ignored, "{why:?}");
    assert!(why.tracked, "saved before the rule: {why:?}");
    assert_eq!(why.rule.unwrap().rule, "*.csv");
}

#[test]
fn why_resolves_the_path_from_where_paper_runs() {
    let (_dir, project) = why_project();

    let why = project.why_ignored("data", "raw/a.csv").unwrap();

    assert_eq!(why.path, "data/raw/a.csv");
    assert!(why.ignored);
    let up = project.why_ignored("data", "../mine.txt").unwrap();
    assert_eq!(up.path, "mine.txt");
    assert_eq!(up.rule.unwrap().rule, "mine.txt");
}

#[test]
fn why_reads_only_and_records_nothing_even_with_unsaved_edits() {
    let (dir, project) = why_project();
    write(dir.path(), "main.tex", "an unsaved edit\n");
    let before = stored(dir.path());

    project.why_ignored("", "data/raw/a.csv").unwrap();
    project.why_ignored("", "main.tex").unwrap();

    assert_eq!(stored(dir.path()), before);
}

#[test]
fn why_refuses_an_empty_name_a_path_inside_git_and_one_outside_the_project() {
    let (_dir, project) = why_project();

    assert!(matches!(
        project.why_ignored("", ""),
        Err(IgnoreError::Empty)
    ));
    assert!(matches!(
        project.why_ignored("", ".git/config"),
        Err(IgnoreError::InsideGit { .. })
    ));
    assert!(matches!(
        project.why_ignored("", "../outside"),
        Err(IgnoreError::Outside { .. })
    ));
}

// ---- undo and redo change tracking, never files ----

#[test]
fn undo_and_redo_of_a_local_unignore_leave_every_file_on_disk() {
    let (dir, mut project) = raw_data_project();
    ignore_local(&mut project, &["data/raw"]);
    unignore_local(&mut project, &["data/raw"]);
    let disk = files(dir.path());
    let exclude_text = exclude(dir.path());

    let back = undone(project.undo().unwrap());
    assert!(
        !back.files_match,
        "the files are left alone, and say so: {back:?}"
    );
    assert!(back.skipped.is_empty(), "{back:?}");
    assert_eq!(files(dir.path()), disk);
    assert_eq!(exclude(dir.path()), exclude_text);

    // The files the undo left alone are remembered, so redo still works and records no edit.
    let edited = descriptions(&project)
        .iter()
        .filter(|d| d.starts_with("edited"))
        .count();
    let redone = undone(project.redo().unwrap());
    assert!(redone.files_match, "{redone:?}");
    assert_eq!(redone.entry.kind, OpKind::Unignore);
    assert_eq!(files(dir.path()), disk);
    assert_eq!(
        descriptions(&project)
            .iter()
            .filter(|d| d.starts_with("edited"))
            .count(),
        edited
    );
    let draft = draft_files(dir.path());
    for file in RAW_FILES {
        assert!(draft.contains(&file.to_owned()), "{draft:?}");
    }
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

#[test]
fn the_next_command_records_the_files_an_undone_local_unignore_left() {
    let (dir, mut project) = raw_data_project();
    ignore_local(&mut project, &["data/raw"]);
    unignore_local(&mut project, &["data/raw"]);
    project.undo().unwrap();
    assert!(
        !draft_files(dir.path())
            .iter()
            .any(|f| f.starts_with("data/raw/"))
    );

    project.snapshot().unwrap();

    let draft = draft_files(dir.path());
    for file in RAW_FILES {
        assert!(draft.contains(&file.to_owned()), "{draft:?}");
    }
}

#[test]
fn undo_and_redo_of_a_local_ignore_leave_every_file_on_disk() {
    let (dir, mut project) = raw_data_project();
    let disk = files(dir.path());
    ignore_local(&mut project, &["data/raw"]);

    let back = undone(project.undo().unwrap());
    assert!(back.files_match, "{back:?}");
    assert_eq!(files(dir.path()), disk);

    let redone = undone(project.redo().unwrap());
    assert!(redone.files_match, "{redone:?}");
    assert_eq!(files(dir.path()), disk);
}

#[test]
fn undo_to_across_ignore_entries_keeps_the_usual_rules() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    let target = project.activity(1).unwrap()[0].short_id.clone();
    unignore(&mut project, &["data/raw"]);
    let disk = files(dir.path());

    let outcome = project.undo_to(&target).unwrap();

    let reached = undone(outcome);
    assert_eq!(reached.entry.kind, OpKind::Ignore);
    assert_eq!(files_but_gitignore(dir.path()), {
        let mut d = disk;
        d.remove(".gitignore");
        d
    });
    assert!(check_verbose(dir.path(), "data/raw/a.csv").is_some());
}

// ---- review round 1 ----

#[test]
fn an_ignore_entry_in_the_chain_does_not_break_the_leftovers_of_an_undo() {
    let (dir, mut project) = project_with(&[
        (".gitignore", "*.log\n"),
        ("chap", "a file\n"),
        ("keep.tex", "v0\n"),
    ]);
    ignore(&mut project, &["scratch.tmp"]);
    write(dir.path(), "keep.tex", "v1\n");
    save_all(&mut project, "one");
    fs::remove_file(dir.path().join("chap")).unwrap();
    write(dir.path(), "chap/one.tex", "1\n");
    save_all(&mut project, "folder");
    write(dir.path(), "chap/cache.log", "my ignored log\n");
    let edited = |project: &Project| {
        descriptions(project)
            .iter()
            .filter(|d| d.starts_with("edited chap"))
            .count()
    };
    let before = edited(&project);

    for _ in 0..5 {
        undone(project.undo().unwrap());
    }
    let mut kinds = Vec::new();
    for _ in 0..5 {
        kinds.push(undone(project.redo().unwrap()).entry.kind);
    }
    assert!(kinds.contains(&OpKind::Ignore), "{kinds:?}");
    assert!(matches!(
        project.redo().unwrap(),
        UndoOutcome::NothingToRedo
    ));
    assert_eq!(edited(&project), before, "no stray edits entry");
    assert_eq!(text(dir.path(), "chap/cache.log"), "my ignored log\n");
}

#[test]
fn removing_paper_rule_adds_the_exception_when_another_rule_still_ignores_the_path() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n"), (".gitignore", "notes/*.log\n")]);
    write(dir.path(), "notes/x.log", "x\n");
    ignore(&mut project, &["notes/x.log"]);

    let outcome = unignore(&mut project, &["notes/x.log"]);

    assert_eq!(outcome.removed, ["/notes/x.log"]);
    assert_eq!(outcome.negated, ["!/notes/x.log"]);
    assert_eq!(outcome.tracked, ["notes/x.log"]);
    let gitignore = text(dir.path(), ".gitignore");
    assert!(
        gitignore.contains(&paper_block(&["!/notes/x.log"])),
        "only the negation stays: {gitignore}"
    );
    assert!(!check_verbose(dir.path(), "notes/x.log").unwrap().ignored);
}

#[test]
fn why_finds_the_on_disk_spelling_on_a_case_insensitive_filesystem() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    if fs::metadata(dir.path().join("DATA")).is_err() {
        return; // a case-sensitive filesystem: nothing to resolve
    }

    let why = project.why_ignored("", "DATA/RAW/a.csv").unwrap();

    assert_eq!(why.path, "data/raw/a.csv");
    assert!(why.ignored, "{why:?}");
    assert_eq!(why.rule.unwrap().rule, "/data/raw/");
}

#[test]
fn ignore_local_is_refused_when_a_shared_negation_keeps_the_path_visible() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n"), (".gitignore", "*.log\n")]);
    write(dir.path(), "notes/keep.log", "keep\n");
    unignore(&mut project, &["notes/keep.log"]);
    let before = stored(dir.path());

    let error = project
        .ignore(&ignore_req(&["notes/keep.log"], true))
        .unwrap_err();

    let paper = PaperError::from(error);
    assert_eq!(paper.kind, ErrorKind::User);
    let all = format!("{} {} {:?}", paper.message, paper.why, paper.next);
    assert!(all.contains("!/notes/keep.log"), "{all}");
    assert!(all.contains("without `--local`"), "{all}");
    assert_eq!(stored(dir.path()), before);
}

#[test]
fn a_leading_bang_on_an_existing_file_suggests_dot_slash() {
    let (dir, mut project) = raw_data_project();
    write(dir.path(), "!odd.txt", "odd\n");

    let error = project
        .unignore(&unignore_req("", &["!odd.txt"], false))
        .unwrap_err();

    let all = refusal_text(error);
    assert!(all.contains("./!odd.txt"), "{all}");
}

#[test]
fn undo_to_across_a_local_unignore_follows_the_usual_rules() {
    let (dir, mut project) = raw_data_project();
    ignore_local(&mut project, &["data/raw"]);
    let ignored = project.activity(1).unwrap()[0].short_id.clone();
    let outcome = unignore_local(&mut project, &["data/raw"]);
    let unignored = outcome.entry.unwrap().short_id;

    undone(project.undo_to(&ignored).unwrap());

    for file in RAW_FILES {
        assert!(
            !dir.path().join(file).exists(),
            "{file} was added after the target, so it is removed like any added file"
        );
    }
    undone(project.undo_to(&unignored).unwrap());
    for file in RAW_FILES {
        assert!(dir.path().join(file).exists(), "{file} is still in history");
    }
}

// ---- review round 2 ----

fn edited_entries(project: &Project) -> usize {
    descriptions(project)
        .iter()
        .filter(|d| d.starts_with("edited"))
        .count()
}

/// Whether any commit reachable from a ref (paper keeps drafts alive) holds `needle` in a file.
fn history_holds(dir: &Path, needle: &str) -> bool {
    let revs = git(dir, &["rev-list", "--all"]);
    let mut args = vec!["grep", "-l", needle];
    args.extend(revs.lines());
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(&args)
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn a_file_edited_while_ignored_becomes_an_edit_at_the_next_command_which_ends_redo() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    write(dir.path(), "data/raw/a.csv", "EDITED while ignored\n");
    let edited = edited_entries(&project);

    let back = undone(project.undo().unwrap());
    assert_eq!(back.entry.kind, OpKind::Ignore);
    assert!(!back.files_match && !back.remembered, "{back:?}");
    assert_eq!(text(dir.path(), "data/raw/a.csv"), "EDITED while ignored\n");
    assert!(matches!(
        project.redo().unwrap(),
        UndoOutcome::NothingToRedo
    ));

    assert_eq!(edited_entries(&project), edited + 1, "recorded as an edit");
    assert_eq!(text(dir.path(), "data/raw/a.csv"), "EDITED while ignored\n");
    assert!(history_holds(dir.path(), "EDITED while ignored"));
}

#[test]
fn undoing_twice_never_loses_a_file_edited_while_ignored() {
    let (dir, mut project) = raw_data_project();
    write(dir.path(), "main.tex", "second\n");
    save_all(&mut project, "second");
    ignore(&mut project, &["data/raw"]);
    write(dir.path(), "data/raw/a.csv", "EDITED\n");

    undone(project.undo().unwrap());
    let second = undone(project.undo().unwrap());

    assert!(
        second
            .entry
            .description
            .starts_with("edited data/raw/a.csv"),
        "{second:?}"
    );
    assert!(
        history_holds(dir.path(), "EDITED"),
        "the edit is in history"
    );
    undone(project.redo().unwrap());
    assert_eq!(text(dir.path(), "data/raw/a.csv"), "EDITED\n");
}

#[test]
fn a_file_edited_while_ignored_after_an_undone_unignore_is_recorded_by_the_next_command() {
    let (dir, mut project) = project_with(&[("main.tex", "m\n"), (".gitignore", "*.tmp\n")]);
    write(dir.path(), "s.tmp", "first\n");
    unignore(&mut project, &["s.tmp"]);
    undone(project.undo().unwrap());
    write(dir.path(), "s.tmp", "EDITED\n");
    let edited = edited_entries(&project);

    undone(project.redo().unwrap());
    let back = undone(project.undo().unwrap());

    assert!(
        back.entry.description.starts_with("edited s.tmp"),
        "{back:?}"
    );
    assert_eq!(edited_entries(&project), edited + 1);
    assert!(history_holds(dir.path(), "EDITED"));
    undone(project.redo().unwrap());
    assert_eq!(text(dir.path(), "s.tmp"), "EDITED\n");
}

#[test]
fn a_real_edit_to_a_visible_file_is_still_recorded_as_an_edits_entry() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    write(dir.path(), "main.tex", "real edit\n");
    let edited = edited_entries(&project);

    let back = undone(project.undo().unwrap());

    assert_eq!(edited_entries(&project), edited + 1);
    assert_eq!(back.entry.kind, OpKind::Snapshot);
    assert_eq!(text(dir.path(), "main.tex"), "\\documentclass{article}\n");
    undone(project.redo().unwrap());
    assert_eq!(text(dir.path(), "main.tex"), "real edit\n");
}

#[test]
fn ignore_local_blocked_by_a_nested_negation_points_to_that_file() {
    let (dir, mut project) = project_with(&[
        ("main.tex", "t\n"),
        (".gitignore", "*.log\n"),
        ("sub/.gitignore", "!keep.log\n"),
    ]);
    write(dir.path(), "sub/keep.log", "k\n");
    let error = project
        .ignore(&ignore_req(&["sub/keep.log"], true))
        .unwrap_err();

    let paper = PaperError::from(error);
    let all = format!("{} {} {:?}", paper.message, paper.why, paper.next);
    assert!(all.contains("sub/.gitignore"), "{all}");
    assert!(!all.contains("without `--local`"), "{all}");
}

#[test]
fn leftovers_are_not_all_remembered_when_one_was_edited_while_ignored() {
    let (dir, mut project) = raw_data_project();
    ignore(&mut project, &["data/raw"]);
    write(dir.path(), "data/raw/a.csv", "EDITED\n");
    fs::remove_file(dir.path().join("data/raw/sub/b.csv")).unwrap();

    let back = undone(project.undo().unwrap());

    assert!(!back.files_match, "{back:?}");
    assert!(
        !back.remembered,
        "the edited file is not in history, so the next command records an edit: {back:?}"
    );
}

// ---- PV-038: undo and redo through `undo:` and `redo:` entries ----

/// The four commands whose entries must only ever change tracking.
#[derive(Debug, Clone, Copy)]
enum Rule {
    LocalUnignore,
    SharedUnignore,
    LocalIgnore,
    SharedIgnore,
}

const ALL_RULES: [Rule; 4] = [
    Rule::LocalUnignore,
    Rule::SharedUnignore,
    Rule::LocalIgnore,
    Rule::SharedIgnore,
];

/// Undoing an ignore leaves no untracked files behind, so the next command records no edit
/// of them. (After undoing an unignore it does, by design, and undoing that ordinary edit
/// removes the files it recorded, which is not what these tests are about.) Stepping over
/// an `undo:` entry of an ignore is tracking-only on main already; these pin it.
const IGNORES: [Rule; 2] = [Rule::LocalIgnore, Rule::SharedIgnore];

const NEVER_SAVED: &str = "data/raw/never-saved.csv";

/// Runs the rule's command on `data/raw`, with a file that was never saved sitting in it.
/// An unignore is preceded by the matching ignore, so the files are untracked before it.
fn run_rule(dir: &Path, project: &mut Project, rule: Rule) {
    match rule {
        Rule::LocalUnignore => {
            ignore_local(project, &["data/raw"]);
            write(dir, NEVER_SAVED, "created while ignored\n");
            unignore_local(project, &["data/raw"]);
        }
        Rule::SharedUnignore => {
            ignore(project, &["data/raw"]);
            write(dir, NEVER_SAVED, "created while ignored\n");
            unignore(project, &["data/raw"]);
        }
        Rule::LocalIgnore => {
            write(dir, NEVER_SAVED, "created before\n");
            ignore_local(project, &["data/raw"]);
        }
        Rule::SharedIgnore => {
            write(dir, NEVER_SAVED, "created before\n");
            ignore(project, &["data/raw"]);
        }
    }
}

/// The bytes of every file under `data/raw/`.
fn raw_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut all = files(dir);
    all.retain(|path, _| path.starts_with("data/raw/"));
    all
}

/// Records an edit to a file outside `data/raw`, as `paper status` does.
fn edit_main_and_status(dir: &Path, project: &mut Project) {
    write(dir, "main.tex", "edited again\n");
    project.status(&StatusOptions::default()).unwrap();
}

/// One undo or redo whose entry description starts with `prefix`, with `data/raw` untouched.
fn step_keeps_raw(
    dir: &Path,
    raw: &BTreeMap<String, Vec<u8>>,
    outcome: UndoOutcome,
    prefix: &str,
) -> Undone {
    let step = undone(outcome);
    assert!(
        step.entry.description.starts_with(prefix),
        "expected a step over an entry starting with {prefix:?}: {:?}",
        step.entry
    );
    assert_eq!(&raw_files(dir), raw, "a step over {:?}", step.entry);
    assert!(raw.contains_key(NEVER_SAVED));
    step
}

fn assert_raw_tracked(dir: &Path, tracked: bool) {
    let draft = draft_files(dir);
    for file in RAW_FILES {
        assert_eq!(
            draft.contains(&file.to_owned()),
            tracked,
            "{file}: {draft:?}"
        );
    }
}

#[test]
fn undoing_a_redo_of_each_ignore_and_unignore_changes_tracking_only() {
    for rule in ALL_RULES {
        let (dir, mut project) = raw_data_project();
        run_rule(dir.path(), &mut project, rule);
        let tracked_after = matches!(rule, Rule::LocalUnignore | Rule::SharedUnignore);
        project.undo().unwrap();
        project.redo().unwrap();
        edit_main_and_status(dir.path(), &mut project);
        let raw = raw_files(dir.path());

        step_keeps_raw(dir.path(), &raw, project.undo().unwrap(), "edited");
        let wrapper = step_keeps_raw(dir.path(), &raw, project.undo().unwrap(), "redo:");

        assert!(wrapper.skipped.is_empty(), "{rule:?}: {wrapper:?}");
        assert_raw_tracked(dir.path(), !tracked_after);
        assert!(descriptions(&project)[0].starts_with("undo: redo:"));
        git(dir.path(), &["fsck", "--strict", "--no-progress"]);
        git(dir.path(), &["status", "--porcelain"]);
    }
}

#[test]
fn redoing_again_after_undoing_a_redo_brings_the_rule_back_without_touching_files() {
    for rule in ALL_RULES {
        let (dir, mut project) = raw_data_project();
        run_rule(dir.path(), &mut project, rule);
        let tracked_after = matches!(rule, Rule::LocalUnignore | Rule::SharedUnignore);
        project.undo().unwrap();
        project.redo().unwrap();
        edit_main_and_status(dir.path(), &mut project);
        let raw = raw_files(dir.path());
        project.undo().unwrap();
        step_keeps_raw(dir.path(), &raw, project.undo().unwrap(), "redo:");

        step_keeps_raw(dir.path(), &raw, project.redo().unwrap(), "redo:");

        assert_raw_tracked(dir.path(), tracked_after);
        git(dir.path(), &["fsck", "--strict", "--no-progress"]);
    }
}

#[test]
fn undoing_and_redoing_an_undo_of_each_ignore_and_unignore_changes_tracking_only() {
    for rule in IGNORES {
        let (dir, mut project) = raw_data_project();
        run_rule(dir.path(), &mut project, rule);
        let raw = raw_files(dir.path());
        step_keeps_raw(dir.path(), &raw, project.undo().unwrap(), "ignore");
        edit_main_and_status(dir.path(), &mut project);

        step_keeps_raw(dir.path(), &raw, project.undo().unwrap(), "edited");
        // Takes back the `undo:` entry: the state of the rule itself again.
        step_keeps_raw(dir.path(), &raw, project.undo().unwrap(), "undo:");
        assert_raw_tracked(dir.path(), false);
        // Brings the `undo:` entry back.
        step_keeps_raw(dir.path(), &raw, project.redo().unwrap(), "undo:");
        assert_raw_tracked(dir.path(), true);

        git(dir.path(), &["fsck", "--strict", "--no-progress"]);
        git(dir.path(), &["status", "--porcelain"]);
    }
}

/// gc keeps a `redo:` entry but not the op its position names: the step over it must not fail
/// on the missing op, it just uses the normal rules.
#[test]
fn undoing_over_a_redo_entry_whose_target_gc_removed_still_works() {
    let (dir, mut project) = raw_data_project();
    run_rule(dir.path(), &mut project, Rule::LocalUnignore);
    project.undo().unwrap();
    project.redo().unwrap();
    edit_main_and_status(dir.path(), &mut project);
    project.undo().unwrap();
    project.undo().unwrap();
    fs::write(dir.path().join("m.tex"), "m3\n").unwrap();
    project.status(&StatusOptions::default()).unwrap();

    let ops = dir.path().join(".git/paper/activity/ops");
    let head = fs::read_to_string(dir.path().join(".git/paper/activity/head")).unwrap();
    let head = head.trim();
    let head_op: serde_json::Value =
        serde_json::from_slice(&fs::read(ops.join(format!("{head}.json"))).unwrap()).unwrap();
    let newest_parent = head_op["parent"].as_str().unwrap().to_owned();
    for entry in fs::read_dir(&ops).unwrap() {
        let path = entry.unwrap().path();
        let id = path.file_stem().unwrap().to_string_lossy().into_owned();
        if id == head || id == newest_parent {
            continue;
        }
        let mut op: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        op["time"] = serde_json::json!("2020-01-01T00:00:00Z");
        fs::write(&path, serde_json::to_vec_pretty(&op).unwrap()).unwrap();
    }
    project.gc(GcAge::parse("1d").unwrap(), false).unwrap();

    project.undo().unwrap();
    project.undo().unwrap();
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
}

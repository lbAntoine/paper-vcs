//! Profiles in `Project::init_with_profile`, cross-checked with the real `git` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::profile::{Profile, gitignore};
use paper_core::project::{InitError, Initialized, ProfileSource, Project};
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

fn init(path: &Path, choice: Option<&str>) -> Result<(Project, Initialized), InitError> {
    let profile = choice.map(|name| Profile::builtin(name).unwrap());
    Project::init_with_profile(path, identity(), &SnapshotOptions::default(), profile)
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
    let absolute = dir.join(path);
    fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    fs::write(absolute, content).unwrap();
}

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap()
}

fn tree(dir: &Path, rev: &str) -> Vec<String> {
    git(dir, &["ls-tree", "-r", "--name-only", rev])
        .lines()
        .map(str::to_owned)
        .collect()
}

fn draft_files(project: &Project) -> Vec<String> {
    let draft = &project.view().current().draft;
    let tree = project.repo().read_commit(draft).unwrap().tree;
    project
        .repo()
        .tree_files(&tree)
        .unwrap()
        .into_keys()
        .collect()
}

fn latex_folder(dir: &Path) {
    write(dir, "main.tex", "\\documentclass{article}\n");
    write(dir, "sections/intro.tex", "Intro\n");
    write(dir, "main.aux", "junk");
    write(dir, "main.log", "junk");
    write(dir, "main.pdf", "%PDF compiled");
}

fn fsck(dir: &Path) {
    git(dir, &["fsck", "--strict", "--no-progress"]);
}

fn block_start(name: &str) -> String {
    format!("# >>> paper-vcs profile: {name} >>>")
}

// ---- new project ----

#[test]
fn init_in_a_new_latex_folder_saves_the_block_and_paper_toml_and_none_of_the_junk() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());

    let (_project, initialized) = init(dir.path(), None).unwrap();

    assert_eq!(initialized.profile, "latex");
    assert_eq!(initialized.profile_source, ProfileSource::Detected);
    assert!(!initialized.adopted);
    assert_eq!(initialized.written, [".gitignore", "paper.toml"]);
    assert!(!initialized.tips.is_empty());
    assert_eq!(initialized.tracked_files, 5);
    assert_eq!(
        tree(dir.path(), "main"),
        [
            ".gitignore",
            "main.pdf",
            "main.tex",
            "paper.toml",
            "sections/intro.tex"
        ]
    );
    assert!(read(dir.path(), ".gitignore").contains(&block_start("latex")));
    assert!(read(dir.path(), "paper.toml").contains("profile = \"latex\""));
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    git(dir.path(), &["check-ignore", "main.aux"]);
    fsck(dir.path());
}

#[test]
fn init_in_a_new_folder_keeps_the_users_own_gitignore_lines() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());
    write(dir.path(), ".gitignore", "secret-notes.txt\n");
    write(dir.path(), "secret-notes.txt", "private");

    init(dir.path(), None).unwrap();

    let gitignore = read(dir.path(), ".gitignore");
    assert!(gitignore.starts_with("secret-notes.txt\n\n"), "{gitignore}");
    assert!(gitignore.contains(&block_start("latex")));
    assert!(!tree(dir.path(), "main").contains(&"secret-notes.txt".to_owned()));
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

#[test]
fn a_chosen_profile_beats_detection_and_is_reported_as_chosen() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());

    let (_project, initialized) = init(dir.path(), Some("markdown")).unwrap();

    assert_eq!(initialized.profile, "markdown");
    assert_eq!(initialized.profile_source, ProfileSource::Chosen);
    let gitignore = read(dir.path(), ".gitignore");
    assert!(gitignore.contains(&block_start("markdown")));
    assert!(!gitignore.contains(&block_start("latex")));
    assert!(read(dir.path(), "paper.toml").contains("profile = \"markdown\""));
    // markdown does not ignore LaTeX junk, so it is saved.
    assert!(tree(dir.path(), "main").contains(&"main.aux".to_owned()));
}

#[test]
fn detection_only_looks_at_files_the_walk_sees() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".gitignore", "*.tex\n");
    write(dir.path(), "main.tex", "ignored by the user");
    write(dir.path(), "notes.md", "# Notes\n");

    let (_project, initialized) = init(dir.path(), None).unwrap();

    assert_eq!(initialized.profile, "markdown");
    assert_eq!(initialized.profile_source, ProfileSource::Detected);
}

#[test]
fn an_empty_folder_gets_the_generic_profile() {
    let dir = tempfile::tempdir().unwrap();

    let (_project, initialized) = init(dir.path(), None).unwrap();

    assert_eq!(initialized.profile, "generic");
    assert_eq!(initialized.profile_source, ProfileSource::Detected);
    assert_eq!(tree(dir.path(), "main"), [".gitignore", "paper.toml"]);
    fsck(dir.path());
}

#[test]
fn a_malformed_block_stops_init_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());
    let broken = "# >>> paper-vcs profile: latex >>>\n*.aux\n";
    write(dir.path(), ".gitignore", broken);

    let error = init(dir.path(), None).unwrap_err();

    assert!(matches!(error, InitError::Gitignore(_)), "{error:?}");
    assert_eq!(read(dir.path(), ".gitignore"), broken);
    assert!(!dir.path().join("paper.toml").exists());
    assert!(!dir.path().join(".git/paper/version").exists());
}

#[test]
fn a_gitignore_that_is_not_utf8_stops_init_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    latex_folder(dir.path());
    fs::write(dir.path().join(".gitignore"), b"caf\xe9\n").unwrap();

    let error = init(dir.path(), None).unwrap_err();

    assert!(
        matches!(
            error,
            InitError::Gitignore(gitignore::GitignoreError::NotUtf8 { .. })
        ),
        "{error:?}"
    );
    assert_eq!(
        fs::read(dir.path().join(".gitignore")).unwrap(),
        b"caf\xe9\n"
    );
    assert!(!dir.path().join("paper.toml").exists());
}

// ---- adoption ----

fn committed_repo(dir: &Path) {
    git(dir, &["init", "--quiet", "--initial-branch=main"]);
    write(dir, "main.tex", "v1\n");
    write(
        dir,
        "build/out.aux",
        "committed before the profile existed\n",
    );
    git(dir, &["add", "."]);
    git(dir, &["commit", "--quiet", "-m", "First draft"]);
}

#[test]
fn adoption_puts_the_block_and_paper_toml_in_the_draft_and_leaves_history_alone() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    let log_before = git(dir.path(), &["log", "--format=%H %s", "--branches"]);

    let (project, initialized) = init(dir.path(), None).unwrap();

    assert!(initialized.adopted);
    assert_eq!(initialized.profile, "latex");
    assert_eq!(initialized.profile_source, ProfileSource::Detected);
    assert_eq!(initialized.written, [".gitignore", "paper.toml"]);
    assert_eq!(
        git(dir.path(), &["log", "--format=%H %s", "--branches"]),
        log_before
    );
    // Committed files that match the new ignores stay tracked.
    assert_eq!(tree(dir.path(), "main"), ["build/out.aux", "main.tex"]);
    let draft = draft_files(&project);
    for expected in [".gitignore", "paper.toml", "build/out.aux", "main.tex"] {
        assert!(draft.contains(&expected.to_owned()), "{draft:?}");
    }
    // The two files are unsaved changes, as far as git can tell too.
    let status = git(dir.path(), &["status", "--porcelain"]);
    assert!(status.contains(".gitignore"), "{status}");
    assert!(status.contains("paper.toml"), "{status}");
    fsck(dir.path());
}

#[test]
fn adoption_keeps_an_existing_profile_in_paper_toml_and_does_not_rewrite_the_file() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    let toml = "# chosen by my supervisor\nprofile = \"typst\"\n";
    write(dir.path(), "paper.toml", toml);

    let (_project, initialized) = init(dir.path(), None).unwrap();

    assert_eq!(initialized.profile, "typst");
    assert_eq!(initialized.profile_source, ProfileSource::PaperToml);
    assert_eq!(initialized.written, [".gitignore"]);
    assert_eq!(read(dir.path(), "paper.toml"), toml);
    assert!(read(dir.path(), ".gitignore").contains(&block_start("typst")));
}

#[test]
fn adoption_agreeing_with_paper_toml_is_fine() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    write(dir.path(), "paper.toml", "profile = \"typst\"\n");

    let (_project, initialized) = init(dir.path(), Some("typst")).unwrap();

    assert_eq!(initialized.profile, "typst");
}

#[test]
fn adoption_with_a_conflicting_choice_is_an_error_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    let toml = "profile = \"typst\"\n";
    write(dir.path(), "paper.toml", toml);

    let error = init(dir.path(), Some("latex")).unwrap_err();

    assert!(
        matches!(&error, InitError::ProfileConflict { requested, existing }
            if requested == "latex" && existing == "typst"),
        "{error:?}"
    );
    assert_eq!(read(dir.path(), "paper.toml"), toml);
    assert!(!dir.path().join(".gitignore").exists());
    assert!(!dir.path().join(".git/paper/version").exists());
}

#[test]
fn adoption_adds_the_profile_to_a_paper_toml_that_has_none_keeping_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    write(
        dir.path(),
        "paper.toml",
        "# my settings\n[large_files]\nwarn_mb = 5\n",
    );

    let (_project, initialized) = init(dir.path(), Some("markdown")).unwrap();

    assert_eq!(initialized.written, [".gitignore", "paper.toml"]);
    let toml = read(dir.path(), "paper.toml");
    assert!(toml.contains("# my settings"), "{toml}");
    assert!(toml.contains("warn_mb = 5"), "{toml}");
    assert!(toml.contains("profile = \"markdown\""), "{toml}");
}

// ---- gitignore::apply_to_file against real git ----

#[test]
fn apply_to_file_keeps_git_happy_and_ignores_the_junk() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet"]);
    write(dir.path(), ".gitignore", "mine.txt\n");
    let latex = Profile::builtin("latex").unwrap();
    let typst = Profile::builtin("typst").unwrap();

    assert!(gitignore::apply_to_file(dir.path(), latex).unwrap());
    git(dir.path(), &["check-ignore", "main.aux"]);
    git(dir.path(), &["check-ignore", "mine.txt"]);
    assert!(read(dir.path(), ".gitignore").starts_with("mine.txt\n"));

    assert!(gitignore::apply_to_file(dir.path(), typst).unwrap());
    let text = read(dir.path(), ".gitignore");
    assert!(text.contains(&block_start("typst")), "{text}");
    assert!(!text.contains(&block_start("latex")), "{text}");
    assert!(text.starts_with("mine.txt\n"));
    assert!(!gitignore::apply_to_file(dir.path(), typst).unwrap());
}

#[test]
fn adoption_with_a_malformed_block_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    committed_repo(dir.path());
    let broken = "# >>> paper-vcs profile: latex >>>\n*.aux\n";
    write(dir.path(), ".gitignore", broken);
    let status_before = git(dir.path(), &["status", "--porcelain"]);

    let error = init(dir.path(), None).unwrap_err();

    assert!(matches!(error, InitError::Gitignore(_)), "{error:?}");
    assert_eq!(read(dir.path(), ".gitignore"), broken);
    assert!(!dir.path().join("paper.toml").exists());
    assert!(!dir.path().join(".git/paper/version").exists());
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), status_before);
}

// ---- PV-018: paper's own second block ----

const PAPER_START: &str = "# >>> paper-vcs: ignored >>>";
const PAPER_END: &str = "# <<< paper-vcs: ignored <<<";

fn paper_block(rules: &[&str], eol: &str) -> String {
    let mut out = format!("{PAPER_START}{eol}");
    for rule in rules {
        out.push_str(rule);
        out.push_str(eol);
    }
    out.push_str(PAPER_END);
    out.push_str(eol);
    out
}

fn rules(list: &[&str]) -> Vec<String> {
    list.iter().map(|rule| (*rule).to_owned()).collect()
}

fn latex_profile_block(eol: &str) -> String {
    let latex = Profile::builtin("latex").unwrap();
    let mut out = format!("# >>> paper-vcs profile: latex >>>{eol}");
    for line in &latex.ignore {
        out.push_str(line);
        out.push_str(eol);
    }
    out.push_str("# <<< paper-vcs profile: latex <<<");
    out.push_str(eol);
    out
}

/// True when every line of `original` appears in `result`, unchanged and in the same order.
fn keeps_lines_in_order(original: &str, result: &str) -> bool {
    let mut remaining = result.split_inclusive('\n');
    original
        .split_inclusive('\n')
        .all(|line| remaining.any(|candidate| candidate == line))
}

#[test]
fn add_rules_to_a_missing_file_makes_just_paper_block() {
    let result = gitignore::add_rules(None, &rules(&["/data/raw/", "*.aux"])).unwrap();
    assert_eq!(result, Some(paper_block(&["/data/raw/", "*.aux"], "\n")));
}

#[test]
fn add_rules_appends_the_block_after_a_blank_line_keeping_every_byte() {
    let existing = "mine.txt\n  keep me  ";
    let result = gitignore::add_rules(Some(existing), &rules(&["/x/"]))
        .unwrap()
        .unwrap();
    assert!(result.starts_with(existing), "{result:?}");
    assert!(result.ends_with(&paper_block(&["/x/"], "\n")), "{result:?}");
    assert!(
        result[existing.len()..].starts_with("\n\n# >>> paper-vcs: ignored >>>"),
        "{result:?}"
    );
}

#[test]
fn add_rules_puts_the_block_after_the_profile_block() {
    let existing = format!("mine.txt\n\n{}", latex_profile_block("\n"));
    let result = gitignore::add_rules(Some(&existing), &rules(&["/x/"]))
        .unwrap()
        .unwrap();
    assert!(result.starts_with(&existing), "{result:?}");
    assert!(result.ends_with(&paper_block(&["/x/"], "\n")), "{result:?}");
}

#[test]
fn add_rules_with_hand_written_lines_after_the_profile_block_keeps_them_all() {
    let existing = format!("{}after-1\nafter-2\n", latex_profile_block("\n"));
    let result = gitignore::add_rules(Some(&existing), &rules(&["/x/"]))
        .unwrap()
        .unwrap();
    assert!(keeps_lines_in_order(&existing, &result), "{result:?}");
    let profile_end = result.find("# <<< paper-vcs profile: latex <<<").unwrap();
    assert!(
        result.find(PAPER_START).unwrap() > profile_end,
        "{result:?}"
    );
}

#[test]
fn add_rules_appends_new_rules_to_the_block_in_order_and_skips_repeats() {
    let existing = format!("mine\n{}", paper_block(&["# note", "/a/", "*.aux"], "\n"));
    let result = gitignore::add_rules(Some(&existing), &rules(&["/b/", "/a/", "/c/"]))
        .unwrap()
        .unwrap();
    assert_eq!(
        result,
        format!(
            "mine\n{}",
            paper_block(&["# note", "/a/", "*.aux", "/b/", "/c/"], "\n")
        )
    );
}

#[test]
fn add_rules_repeats_within_one_call_are_written_once() {
    let result = gitignore::add_rules(None, &rules(&["/a/", "/a/"]))
        .unwrap()
        .unwrap();
    assert_eq!(result, paper_block(&["/a/"], "\n"));
}

#[test]
fn add_rules_that_are_all_present_change_nothing() {
    let existing = format!("mine\n{}", paper_block(&["/a/", "*.aux"], "\n"));
    assert_eq!(
        gitignore::add_rules(Some(&existing), &rules(&["*.aux", "/a/"])).unwrap(),
        None
    );
}

#[test]
fn add_rules_with_no_rules_never_writes_an_empty_block() {
    assert_eq!(gitignore::add_rules(None, &[]).unwrap(), None);
    assert_eq!(gitignore::add_rules(Some("mine\n"), &[]).unwrap(), None);
}

#[test]
fn add_rules_in_a_crlf_file_writes_crlf_and_keeps_hand_written_bytes() {
    let existing = format!("mine.txt\r\n*.bak\r\n\r\n{}", latex_profile_block("\r\n"));
    let result = gitignore::add_rules(Some(&existing), &rules(&["/x/", "*.aux"]))
        .unwrap()
        .unwrap();
    assert!(result.starts_with(&existing), "{result:?}");
    assert!(
        result.ends_with(&paper_block(&["/x/", "*.aux"], "\r\n")),
        "{result:?}"
    );
    assert!(!result.replace("\r\n", "").contains('\n'), "{result:?}");
}

#[test]
fn paper_rules_reads_the_block_skipping_comments_and_blank_lines() {
    let existing = format!(
        "other\n{}after\n",
        paper_block(&["# note", "/a/", "", "*.aux"], "\n")
    );
    assert_eq!(
        gitignore::paper_rules(Some(&existing)).unwrap(),
        ["/a/", "*.aux"]
    );
    assert!(gitignore::paper_rules(Some("other\n")).unwrap().is_empty());
    assert!(gitignore::paper_rules(None).unwrap().is_empty());
}

#[test]
fn paper_rules_reads_a_crlf_block_without_the_carriage_returns() {
    let existing = paper_block(&["/a/", "*.aux"], "\r\n");
    assert_eq!(
        gitignore::paper_rules(Some(&existing)).unwrap(),
        ["/a/", "*.aux"]
    );
}

#[test]
fn two_paper_blocks_are_a_duplicate_block_error() {
    let existing = format!(
        "{}\n{}",
        paper_block(&["/a/"], "\n"),
        paper_block(&["/b/"], "\n")
    );
    assert!(matches!(
        gitignore::add_rules(Some(&existing), &rules(&["/c/"])).unwrap_err(),
        gitignore::GitignoreError::DuplicateBlock { .. }
    ));
    assert!(matches!(
        gitignore::paper_rules(Some(&existing)).unwrap_err(),
        gitignore::GitignoreError::DuplicateBlock { .. }
    ));
}

#[test]
fn a_paper_block_without_an_end_marker_is_an_unterminated_block_error() {
    let existing = format!("mine\n{PAPER_START}\n/a/\n");
    assert!(matches!(
        gitignore::add_rules(Some(&existing), &rules(&["/c/"])).unwrap_err(),
        gitignore::GitignoreError::UnterminatedBlock { line: 2 }
    ));
}

#[test]
fn a_profile_block_and_a_paper_block_together_are_not_a_duplicate() {
    let existing = format!(
        "{}\n{}",
        latex_profile_block("\n"),
        paper_block(&["/a/"], "\n")
    );
    let latex = Profile::builtin("latex").unwrap();
    assert_eq!(gitignore::apply(Some(&existing), latex).unwrap(), None);
    assert!(gitignore::add_rules(Some(&existing), &rules(&["/b/"])).is_ok());
}

#[test]
fn applying_a_profile_replaces_only_the_profile_block_and_keeps_paper_block() {
    let existing = format!(
        "mine\n{}\n{}after\n",
        latex_profile_block("\n"),
        paper_block(&["/a/", "*.aux"], "\n")
    );
    let markdown = Profile::builtin("markdown").unwrap();

    let result = gitignore::apply(Some(&existing), markdown)
        .unwrap()
        .unwrap();

    assert!(
        result.contains(&paper_block(&["/a/", "*.aux"], "\n")),
        "{result:?}"
    );
    assert!(!result.contains("profile: latex"), "{result:?}");
    assert!(result.starts_with("mine\n"));
    assert!(result.ends_with("after\n"));
    assert!(
        result
            .find("# >>> paper-vcs profile: markdown >>>")
            .unwrap()
            < result.find(PAPER_START).unwrap(),
        "paper's block stays after the profile block: {result:?}"
    );
}

#[test]
fn applying_a_profile_to_a_file_with_only_paper_block_puts_the_profile_block_first() {
    let existing = format!("mine\n\n{}", paper_block(&["/a/"], "\n"));
    let latex = Profile::builtin("latex").unwrap();

    let result = gitignore::apply(Some(&existing), latex).unwrap().unwrap();

    assert!(result.contains(&paper_block(&["/a/"], "\n")), "{result:?}");
    assert!(result.starts_with("mine\n"));
    assert!(
        result.find("# >>> paper-vcs profile: latex >>>").unwrap()
            < result.find(PAPER_START).unwrap(),
        "{result:?}"
    );
}

#[test]
fn applying_a_profile_to_a_crlf_file_with_paper_block_keeps_crlf() {
    let existing = format!(
        "mine\r\n{}{}",
        latex_profile_block("\r\n"),
        paper_block(&["/a/"], "\r\n")
    );
    let markdown = Profile::builtin("markdown").unwrap();

    let result = gitignore::apply(Some(&existing), markdown)
        .unwrap()
        .unwrap();

    assert!(
        result.contains(&paper_block(&["/a/"], "\r\n")),
        "{result:?}"
    );
    assert!(!result.replace("\r\n", "").contains('\n'), "{result:?}");
}

#[test]
fn reapplying_a_profile_on_disk_keeps_paper_block_and_git_still_ignores_both() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--quiet", "-b", "main"]);
    write(dir.path(), ".gitignore", "mine.txt\n");
    let latex = Profile::builtin("latex").unwrap();
    let typst = Profile::builtin("typst").unwrap();
    assert!(gitignore::apply_to_file(dir.path(), latex).unwrap());
    let with_profile = read(dir.path(), ".gitignore");
    let with_block = gitignore::add_rules(Some(&with_profile), &rules(&["/data/raw/"]))
        .unwrap()
        .unwrap();
    write(dir.path(), ".gitignore", &with_block);

    assert!(gitignore::apply_to_file(dir.path(), typst).unwrap());

    let text = read(dir.path(), ".gitignore");
    assert!(text.starts_with("mine.txt\n"), "{text}");
    assert!(text.contains(&paper_block(&["/data/raw/"], "\n")), "{text}");
    assert!(text.contains(&block_start("typst")), "{text}");
    assert!(!text.contains(&block_start("latex")), "{text}");
    git(
        dir.path(),
        &["check-ignore", "--no-index", "data/raw/a.csv"],
    );
    git(dir.path(), &["check-ignore", "--no-index", "mine.txt"]);
    assert!(!gitignore::apply_to_file(dir.path(), typst).unwrap());
}

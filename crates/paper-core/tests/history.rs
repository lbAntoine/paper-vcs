//! `Project::history` and `Project::show`, cross-checked with the real `git` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::name::LineName;
use paper_core::point::Point;
use paper_core::project::{
    HistoryEntry, HistoryRequest, PathSelection, Project, SaveRequest, ShowError,
};
use paper_core::snapshot::{Change, ChangeKind, SnapshotOptions};
use paper_git::{ChangeId, CommitId, GitRepo, GitTime, Identity, NewCommit, Signature, TreeEdit};
use tempfile::TempDir;

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
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn git_lines(dir: &Path, args: &[&str]) -> Vec<String> {
    git(dir, args).lines().map(str::to_owned).collect()
}

fn write(dir: &Path, path: &str, content: &str) {
    let absolute = dir.join(path);
    fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    fs::write(absolute, content).unwrap();
}

fn save(project: &mut Project, dir: &Path, message: &str, files: &[(&str, &str)]) {
    for (path, content) in files {
        write(dir, path, content);
    }
    project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
}

/// A thesis project: a first save with two files, then three more saves.
fn thesis() -> (TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.tex", "main");
    write(dir.path(), "sections/intro.tex", "intro");
    let (mut project, _) =
        Project::init(dir.path(), identity(), &SnapshotOptions::default()).unwrap();
    save(
        &mut project,
        dir.path(),
        "Edit intro",
        &[("sections/intro.tex", "intro 2")],
    );
    save(
        &mut project,
        dir.path(),
        "Edit main",
        &[("main.tex", "main 2")],
    );
    save(
        &mut project,
        dir.path(),
        "Add methods",
        &[("sections/methods.tex", "m")],
    );
    (dir, project)
}

fn everything() -> HistoryRequest {
    HistoryRequest::default()
}

fn history(project: &Project, request: &HistoryRequest) -> Vec<HistoryEntry> {
    project.history(request).unwrap()
}

fn commits(entries: &[HistoryEntry]) -> Vec<String> {
    entries.iter().map(|e| e.commit.to_string()).collect()
}

fn messages(entries: &[HistoryEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.message.as_str()).collect()
}

fn change(path: &str, kind: ChangeKind) -> Change {
    Change {
        path: path.to_owned(),
        kind,
    }
}

fn point(text: &str) -> Point {
    text.parse().unwrap()
}

fn line(name: &str) -> LineName {
    name.parse().unwrap()
}

#[test]
fn history_lists_the_current_lines_saves_newest_first_without_the_draft() {
    let (dir, project) = thesis();
    let entries = history(&project, &everything());

    assert_eq!(
        messages(&entries),
        ["Add methods", "Edit main", "Edit intro", "Start project"]
    );
    assert_eq!(
        commits(&entries),
        git_lines(dir.path(), &["rev-list", "main"])
    );
    let draft = project.view().current().draft.to_string();
    assert!(!commits(&entries).contains(&draft));
}

#[test]
fn history_entries_carry_the_author_message_time_and_line_tips() {
    let (dir, project) = thesis();
    let entries = history(&project, &everything());
    let newest = &entries[0];

    assert_eq!(newest.author, identity());
    assert_eq!(newest.tip_of, [line("main")]);
    assert!(entries[1..].iter().all(|entry| entry.tip_of.is_empty()));
    let seconds = git(dir.path(), &["log", "-1", "--format=%ct", "main"]);
    assert_eq!(newest.time.seconds.to_string(), seconds);
    let change_id = project
        .repo()
        .read_commit(&newest.commit)
        .unwrap()
        .change_id;
    assert_eq!(newest.change_id, change_id);
}

#[test]
fn history_limit_truncates() {
    let (_dir, project) = thesis();
    let limited = history(
        &project,
        &HistoryRequest {
            limit: Some(2),
            ..everything()
        },
    );
    assert_eq!(messages(&limited), ["Add methods", "Edit main"]);

    let roomy = history(
        &project,
        &HistoryRequest {
            limit: Some(50),
            ..everything()
        },
    );
    assert_eq!(roomy.len(), 4);
}

fn for_path(path: &str) -> HistoryRequest {
    HistoryRequest {
        path: Some(path.to_owned()),
        ..everything()
    }
}

#[test]
fn history_of_a_file_lists_only_the_saves_that_touch_it() {
    let (dir, project) = thesis();
    for path in ["sections/intro.tex", "main.tex", "sections/methods.tex"] {
        let entries = history(&project, &for_path(path));
        let expected = git_lines(
            dir.path(),
            &["log", "--first-parent", "--format=%H", "main", "--", path],
        );
        assert_eq!(commits(&entries), expected, "history of {path}");
    }
    assert_eq!(
        messages(&history(&project, &for_path("sections/intro.tex"))),
        ["Edit intro", "Start project"]
    );
}

#[test]
fn history_of_a_directory_lists_the_saves_that_touch_anything_under_it() {
    let (_dir, project) = thesis();
    let expected = ["Add methods", "Edit intro", "Start project"];
    assert_eq!(
        messages(&history(&project, &for_path("sections"))),
        expected
    );
    assert_eq!(
        messages(&history(&project, &for_path("sections/"))),
        expected
    );
}

#[test]
fn history_paths_match_whole_components_only() {
    let (_dir, project) = thesis();
    assert!(history(&project, &for_path("sec")).is_empty());
    assert!(history(&project, &for_path("main")).is_empty());
    assert!(history(&project, &for_path("sections/intro")).is_empty());
}

#[test]
fn history_of_a_path_combines_with_a_limit() {
    let (_dir, project) = thesis();
    let entries = history(
        &project,
        &HistoryRequest {
            path: Some("sections".to_owned()),
            limit: Some(1),
            ..everything()
        },
    );
    assert_eq!(messages(&entries), ["Add methods"]);
}

/// A repository made without paper: no change IDs. Committer times are 1000, 2000, ...
struct Plain {
    dir: TempDir,
    repo: GitRepo,
}

impl Plain {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path(), &identity()).unwrap();
        repo.set_head_branch("main", "test").unwrap();
        Self { dir, repo }
    }

    fn commit(
        &self,
        files: &[(&str, &str)],
        message: &str,
        parents: &[&CommitId],
        seconds: i64,
        change_id: Option<&str>,
    ) -> CommitId {
        let edits: Vec<_> = files
            .iter()
            .map(|(path, content)| TreeEdit::Upsert {
                path: (*path).to_owned(),
                blob: self.repo.write_blob(content.as_bytes()).unwrap(),
                executable: false,
            })
            .collect();
        let tree = self
            .repo
            .edit_tree(&self.repo.empty_tree().unwrap(), &edits)
            .unwrap();
        let signature = Signature {
            identity: identity(),
            time: GitTime {
                seconds,
                offset_seconds: 0,
            },
        };
        self.repo
            .write_commit(&NewCommit {
                tree,
                parents: parents.iter().map(|id| (*id).clone()).collect(),
                author: signature.clone(),
                committer: signature,
                message: message.to_owned(),
                change_id: change_id.map(|text| text.parse::<ChangeId>().unwrap()),
            })
            .unwrap()
    }

    fn branch(&self, name: &str, commit: &CommitId) {
        self.repo
            .set_ref(&format!("refs/heads/{name}"), commit, "test")
            .unwrap();
    }

    fn adopt(self, current_files: &[(&str, &str)]) -> (TempDir, Project) {
        for (path, content) in current_files {
            write(self.dir.path(), path, content);
        }
        let (project, _) =
            Project::init(self.dir.path(), identity(), &SnapshotOptions::default()).unwrap();
        (self.dir, project)
    }
}

/// main: root <- m1 <- m2.  side: root <- m1 <- s1.  Times interleave: root 1000, m1 2000,
/// s1 3000, m2 4000.
fn two_lines() -> (TempDir, Project, [CommitId; 4]) {
    let plain = Plain::new();
    let root = plain.commit(&[("a.txt", "0")], "Root", &[], 1_000, None);
    let m1 = plain.commit(&[("a.txt", "1")], "Shared", &[&root], 2_000, None);
    let s1 = plain.commit(&[("a.txt", "s")], "On side", &[&m1], 3_000, None);
    let m2 = plain.commit(&[("a.txt", "2")], "On main", &[&m1], 4_000, None);
    plain.branch("main", &m2);
    plain.branch("side", &s1);
    let (dir, project) = plain.adopt(&[("a.txt", "2")]);
    (dir, project, [root, m1, s1, m2])
}

#[test]
fn history_defaults_to_the_current_line_only() {
    let (_dir, project, _) = two_lines();
    assert_eq!(
        messages(&history(&project, &everything())),
        ["On main", "Shared", "Root"]
    );
}

#[test]
fn history_of_all_lines_merges_them_without_duplicates() {
    let (dir, project, [root, m1, s1, m2]) = two_lines();
    let entries = history(
        &project,
        &HistoryRequest {
            all_lines: true,
            ..everything()
        },
    );

    assert_eq!(
        commits(&entries),
        [m2.clone(), s1.clone(), m1, root]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let mut from_git = git_lines(dir.path(), &["rev-list", "--branches"]);
    let mut listed = commits(&entries);
    from_git.sort();
    listed.sort();
    assert_eq!(listed, from_git);

    assert_eq!(entries[0].tip_of, [line("main")]);
    assert_eq!(entries[1].tip_of, [line("side")]);
}

#[test]
fn history_of_all_lines_respects_limit_and_path() {
    let (_dir, project, _) = two_lines();
    let limited = history(
        &project,
        &HistoryRequest {
            all_lines: true,
            limit: Some(2),
            ..everything()
        },
    );
    assert_eq!(messages(&limited), ["On main", "On side"]);
}

#[test]
fn commits_made_with_plain_git_show_a_hash_based_short_id() {
    let (_dir, project, [root, ..]) = two_lines();
    let entries = history(&project, &everything());
    let first = entries.last().unwrap();

    assert_eq!(first.commit, root);
    assert_eq!(first.change_id, None);
    assert_eq!(first.short_change_id, &root.as_str()[..7]);
}

#[test]
fn short_change_ids_are_at_least_four_letters_and_unique_across_the_project() {
    let (_dir, project) = thesis();
    let entries = history(&project, &everything());

    let mut known: Vec<ChangeId> = entries.iter().filter_map(|e| e.change_id).collect();
    known.push(
        project
            .repo()
            .read_commit(&project.view().current().draft)
            .unwrap()
            .change_id
            .unwrap(),
    );
    for entry in &entries {
        let full = entry.change_id.as_ref().unwrap().to_string();
        assert!(entry.short_change_id.len() >= 4);
        assert!(full.starts_with(&entry.short_change_id));
        let sharing = known
            .iter()
            .filter(|id| id.to_string().starts_with(&entry.short_change_id))
            .count();
        assert_eq!(sharing, 1, "{} is not unique", entry.short_change_id);
    }
}

#[test]
fn short_change_ids_grow_until_they_tell_similar_ids_apart() {
    // Ten shared letters: a random ID (the draft) sharing them is a 1 in 16^10 chance.
    let plain = Plain::new();
    let first_id = format!("kmrwpqstvw{}", "k".repeat(22));
    let second_id = format!("kmrwpqstvw{}", "m".repeat(22));
    let first = plain.commit(&[("a.txt", "1")], "One", &[], 1_000, Some(&first_id));
    let second = plain.commit(&[("a.txt", "2")], "Two", &[&first], 2_000, Some(&second_id));
    plain.branch("main", &second);
    let (_dir, project) = plain.adopt(&[("a.txt", "2")]);

    let entries = history(&project, &everything());
    assert_eq!(entries[0].short_change_id, "kmrwpqstvwm");
    assert_eq!(entries[1].short_change_id, "kmrwpqstvwk");
}

#[test]
fn short_change_ids_count_the_ids_of_every_line() {
    let plain = Plain::new();
    let main_id = format!("kmrwpqstvw{}", "k".repeat(22));
    let side_id = format!("kmrwpqstvw{}", "m".repeat(22));
    let root = plain.commit(&[("a.txt", "1")], "Root", &[], 1_000, Some(&main_id));
    let side = plain.commit(&[("a.txt", "2")], "Side", &[&root], 2_000, Some(&side_id));
    plain.branch("main", &root);
    plain.branch("side", &side);
    let (_dir, project) = plain.adopt(&[("a.txt", "1")]);

    // Only `main` is listed, but the other line's ID still has to be told apart.
    let entries = history(&project, &everything());
    assert_eq!(entries[0].short_change_id, "kmrwpqstvwk");
}

#[test]
fn show_lists_a_saves_changes_against_its_parent() {
    let (_dir, project) = thesis();
    let shown = project.show(&point("last")).unwrap();
    assert_eq!(shown.entry.message, "Add methods");
    assert_eq!(
        shown.changes,
        [change("sections/methods.tex", ChangeKind::Added)]
    );

    let earlier = project.show(&point("last~2")).unwrap();
    assert_eq!(earlier.entry.message, "Edit intro");
    assert_eq!(
        earlier.changes,
        [change("sections/intro.tex", ChangeKind::Modified)]
    );
}

#[test]
fn show_of_the_first_save_lists_every_file_as_added() {
    let (_dir, project) = thesis();
    let shown = project.show(&point("last~3")).unwrap();
    assert_eq!(shown.entry.message, "Start project");
    assert_eq!(
        shown.changes,
        [
            change("main.tex", ChangeKind::Added),
            change("sections/intro.tex", ChangeKind::Added),
        ]
    );
}

#[test]
fn show_of_a_merge_compares_with_the_first_parent() {
    let plain = Plain::new();
    let root = plain.commit(&[("a.txt", "1")], "Root", &[], 1_000, None);
    let side = plain.commit(
        &[("a.txt", "1"), ("b.txt", "b")],
        "Side",
        &[&root],
        2_000,
        None,
    );
    let main = plain.commit(&[("a.txt", "2")], "Main", &[&root], 3_000, None);
    let merge = plain.commit(
        &[("a.txt", "2"), ("b.txt", "b")],
        "Merge",
        &[&main, &side],
        4_000,
        None,
    );
    plain.branch("main", &merge);
    let (_dir, project) = plain.adopt(&[("a.txt", "2"), ("b.txt", "b")]);

    let shown = project.show(&point("last")).unwrap();
    assert_eq!(shown.changes, [change("b.txt", ChangeKind::Added)]);
}

#[test]
fn show_at_lists_the_drafts_unsaved_changes_against_the_last_save() {
    let (dir, mut project) = thesis();
    write(dir.path(), "main.tex", "main 3");
    write(dir.path(), "notes.md", "todo");
    project.snapshot().unwrap();

    let shown = project.show(&point("@")).unwrap();
    assert_eq!(shown.entry.commit, project.view().current().draft);
    assert_eq!(
        shown.changes,
        [
            change("main.tex", ChangeKind::Modified),
            change("notes.md", ChangeKind::Added),
        ]
    );
}

#[test]
fn show_by_change_id_and_by_line_name_finds_the_same_save() {
    let (_dir, project) = thesis();
    let by_line = project.show(&point("main")).unwrap();
    let by_id = project
        .show(&point(&by_line.entry.short_change_id))
        .unwrap();
    assert_eq!(by_line, by_id);
}

#[test]
fn show_of_an_unknown_point_is_a_resolve_error() {
    let (_dir, project) = thesis();
    assert!(matches!(
        project.show(&point("nothing-here")),
        Err(ShowError::Resolve(_))
    ));
}

#[test]
fn history_and_show_leave_the_repository_untouched() {
    let (dir, project) = thesis();
    let before = git(
        dir.path(),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    );
    history(&project, &everything());
    project.show(&point("last")).unwrap();
    let after = git(
        dir.path(),
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    );
    assert_eq!(before, after);
    git(dir.path(), &["fsck", "--strict", "--no-progress"]);
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

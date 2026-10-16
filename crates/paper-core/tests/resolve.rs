//! `Project::resolve`: points to commits, by the precedence in 01-concepts.md § Points.
//! Cross-checked with the real `git` binary.

use std::fs;
use std::path::Path;
use std::process::Command;

use jiff::tz::TimeZone;
use paper_core::name::LineName;
use paper_core::point::{DateSpec, Point};
use paper_core::project::{PathSelection, PointKind, Project, ResolveError, Resolved, SaveRequest};
use paper_core::snapshot::SnapshotOptions;
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

fn point(text: &str) -> Point {
    text.parse().unwrap()
}

fn line(name: &str) -> LineName {
    name.parse().unwrap()
}

fn commit_id(text: &str) -> CommitId {
    text.parse().unwrap()
}

/// A fresh project whose line `main` has `saves` saves after the first one
/// (so `saves + 1` in total), each changing `notes.txt`.
fn project_with_saves(saves: usize) -> (TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), "start").unwrap();
    let (mut project, _) =
        Project::init(dir.path(), identity(), &SnapshotOptions::default()).unwrap();
    for i in 1..=saves {
        fs::write(dir.path().join("notes.txt"), format!("version {i}")).unwrap();
        project
            .save(&SaveRequest {
                message: format!("Save {i}"),
                paths: PathSelection::All,
            })
            .unwrap();
    }
    (dir, project)
}

/// Like `project_with_saves(2)`, but retried until the tip's first `length` hash characters
/// match no other commit in the repository (hashes of fresh saves are random-ish, so a
/// short prefix can otherwise clash: about 1 in 65 000 per other commit for four characters).
fn project_with_unambiguous_tip(length: usize) -> (TempDir, Project) {
    loop {
        let (dir, project) = project_with_saves(2);
        let tip = project.view().current().tip.clone();
        let objects = git(
            dir.path(),
            &[
                "cat-file",
                "--batch-all-objects",
                "--batch-check=%(objectname) %(objecttype)",
            ],
        );
        let sharing = objects
            .lines()
            .filter(|l| l.ends_with(" commit") && l.starts_with(&tip.as_str()[..length]))
            .count();
        if sharing == 1 {
            return (dir, project);
        }
    }
}

/// A repository built commit by commit, then adopted as a project.
struct Plain {
    dir: TempDir,
    repo: GitRepo,
}

fn at(seconds: i64) -> GitTime {
    GitTime {
        seconds,
        offset_seconds: 0,
    }
}

impl Plain {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path(), &identity()).unwrap();
        repo.set_head_branch("main", "test").unwrap();
        Self { dir, repo }
    }

    /// A commit holding `files`, made at committer time `committer`; the author time is
    /// always long before, to prove that only the committer time counts.
    fn commit(
        &self,
        files: &[(&str, &str)],
        message: &str,
        parents: &[&CommitId],
        committer: GitTime,
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
        self.repo
            .write_commit(&NewCommit {
                tree,
                parents: parents.iter().map(|id| (*id).clone()).collect(),
                author: Signature {
                    identity: identity(),
                    time: at(1_577_836_800),
                },
                committer: Signature {
                    identity: identity(),
                    time: committer,
                },
                message: message.to_owned(),
                change_id: change_id.map(|text| text.parse().unwrap()),
            })
            .unwrap()
    }

    fn branch(&self, name: &str, commit: &CommitId) {
        self.repo
            .set_ref(&format!("refs/heads/{name}"), commit, "test")
            .unwrap();
    }

    /// Writes the files of the current branch to the folder, then adopts the repository.
    fn adopt(self, current_files: &[(&str, &str)]) -> (TempDir, Project) {
        for (path, content) in current_files {
            fs::write(self.dir.path().join(path), content).unwrap();
        }
        let (project, _) =
            Project::init(self.dir.path(), identity(), &SnapshotOptions::default()).unwrap();
        (self.dir, project)
    }
}

/// A change ID of 32 letters: `head` followed by `fill`.
fn change_id(head: &str, fill: char) -> String {
    let mut text = head.to_owned();
    while text.len() < ChangeId::LEN {
        text.push(fill);
    }
    text
}

fn resolved(project: &Project, text: &str) -> Resolved {
    project.resolve(&point(text)).unwrap()
}

#[test]
fn at_resolves_to_the_current_draft() {
    let (_dir, project) = project_with_saves(1);
    let found = resolved(&project, "@");
    assert_eq!(found.commit, project.view().current().draft);
    assert_eq!(found.matched, PointKind::Draft);
}

#[test]
fn last_resolves_to_the_tip_and_last_tilde_n_goes_back_n_saves() {
    let (dir, project) = project_with_saves(3);
    let tip = resolved(&project, "last");
    assert_eq!(tip.commit, project.view().current().tip);
    assert_eq!(tip.matched, PointKind::Last);

    let two_back = resolved(&project, "last~2");
    assert_eq!(
        two_back.commit,
        commit_id(&git(dir.path(), &["rev-parse", "main~2"]))
    );
    assert_eq!(two_back.matched, PointKind::Last);
}

#[test]
fn last_past_the_first_save_says_how_many_saves_the_line_has() {
    let (dir, project) = project_with_saves(2);
    let total: usize = git(dir.path(), &["rev-list", "--count", "main"])
        .parse()
        .unwrap();
    assert_eq!(total, 3);

    // The first save is still reachable; one step more is not.
    assert_eq!(
        resolved(&project, "last~2").commit,
        commit_id(&git(dir.path(), &["rev-parse", "main~2"]))
    );
    match project.resolve(&point("last~3")) {
        Err(ResolveError::NotEnoughHistory {
            back, available, ..
        }) => {
            assert_eq!(back, 3);
            assert_eq!(available, total);
        }
        other => panic!("expected NotEnoughHistory, got {other:?}"),
    }
    let message = project.resolve(&point("last~9")).unwrap_err().to_string();
    assert!(
        message.contains('3'),
        "message should give the count: {message}"
    );
}

#[test]
fn a_line_name_resolves_to_its_tip() {
    let plain = Plain::new();
    let first = plain.commit(&[("a.txt", "a")], "First", &[], at(1_000), None);
    let second = plain.commit(&[("a.txt", "b")], "Second", &[&first], at(2_000), None);
    plain.branch("main", &second);
    plain.branch("journal", &first);
    let (_dir, project) = plain.adopt(&[("a.txt", "b")]);

    let found = resolved(&project, "journal");
    assert_eq!(found.commit, first);
    assert_eq!(found.matched, PointKind::Line(line("journal")));
    assert_eq!(resolved(&project, "main").commit, second);
}

#[test]
fn a_line_wins_over_a_change_id_prefix_and_a_hash_prefix_with_the_same_spelling() {
    let plain = Plain::new();
    let root = plain.commit(&[("a.txt", "a")], "Root", &[], at(1_000), None);
    let wanted = change_id("kmrw", 'p');
    let by_id = plain.commit(
        &[("a.txt", "b")],
        "Has the change ID",
        &[&root],
        at(2_000),
        Some(&wanted),
    );
    plain.branch("main", &by_id);
    // Lines named like a change-ID prefix and like the hash prefix of `root`.
    plain.branch("kmrw", &root);
    let hash_spelling = root.as_str()[..6].to_owned();
    plain.branch(&hash_spelling, &by_id);
    let (_dir, project) = plain.adopt(&[("a.txt", "b")]);

    let by_line = resolved(&project, "kmrw");
    assert_eq!(by_line.commit, root);
    assert_eq!(by_line.matched, PointKind::Line(line("kmrw")));

    let by_hash_line = resolved(&project, &hash_spelling);
    assert_eq!(by_hash_line.commit, by_id);
    assert_eq!(by_hash_line.matched, PointKind::Line(line(&hash_spelling)));
}

#[test]
fn a_unique_change_id_prefix_resolves_including_the_drafts() {
    let (_dir, project) = project_with_saves(2);
    let tip = &project.view().current().tip;
    let tip_id = project.repo().read_commit(tip).unwrap().change_id.unwrap();
    let draft = &project.view().current().draft;
    let draft_id = project
        .repo()
        .read_commit(draft)
        .unwrap()
        .change_id
        .unwrap();

    let by_save = resolved(&project, &tip_id.to_string()[..8]);
    assert_eq!(&by_save.commit, tip);
    assert_eq!(by_save.matched, PointKind::ChangeId);

    let by_draft = resolved(&project, &draft_id.to_string()[..8]);
    assert_eq!(&by_draft.commit, draft);
    assert_eq!(by_draft.matched, PointKind::ChangeId);
}

#[test]
fn an_ambiguous_change_id_prefix_lists_its_candidates() {
    // The two fixed IDs share nine letters and differ at the tenth. The project also holds
    // random IDs (the draft); one starting with the nine shared letters is a 1 in 16^9
    // (about 7e10) chance per random ID, so the lookups below can't realistically clash.
    const SHARED: &str = "kmqrstuvw";
    let plain = Plain::new();
    let root = plain.commit(&[("a.txt", "a")], "Root", &[], at(1_000), None);
    let left = plain.commit(
        &[("a.txt", "l")],
        "Left",
        &[&root],
        at(2_000),
        Some(&change_id(&format!("{SHARED}n"), 'k')),
    );
    let right = plain.commit(
        &[("a.txt", "r")],
        "Right",
        &[&root],
        at(3_000),
        Some(&change_id(&format!("{SHARED}p"), 'k')),
    );
    plain.branch("main", &left);
    plain.branch("side", &right);
    let (_dir, project) = plain.adopt(&[("a.txt", "l")]);

    match project.resolve(&point(SHARED)) {
        Err(ResolveError::Ambiguous { prefix, candidates }) => {
            assert_eq!(prefix, SHARED);
            let mut ids: Vec<_> = candidates.iter().map(|c| c.commit.clone()).collect();
            ids.sort();
            let mut expected = vec![left.clone(), right.clone()];
            expected.sort();
            assert_eq!(ids, expected);
            let messages: Vec<_> = candidates.iter().map(|c| c.message.as_str()).collect();
            assert!(messages.contains(&"Left") && messages.contains(&"Right"));
            assert!(candidates.iter().all(|c| c.short_id.len() >= 4));
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
    // A longer prefix tells them apart.
    assert_eq!(resolved(&project, &format!("{SHARED}n")).commit, left);
    assert_eq!(resolved(&project, &format!("{SHARED}p")).commit, right);
}

#[test]
fn commits_sharing_one_change_id_resolve_to_the_newest() {
    let plain = Plain::new();
    // Eight letters are looked up below: a random ID matching them is a 1 in 16^8 chance.
    let same = change_id("knnnrstw", 'q');
    let older = plain.commit(&[("a.txt", "a")], "Older", &[], at(1_000), Some(&same));
    let newer = plain.commit(
        &[("a.txt", "b")],
        "Newer",
        &[&older],
        at(2_000),
        Some(&same),
    );
    plain.branch("main", &newer);
    plain.branch("old", &older);
    let (_dir, project) = plain.adopt(&[("a.txt", "b")]);

    let found = resolved(&project, "knnnrstw");
    assert_eq!(found.commit, newer);
    assert_eq!(found.matched, PointKind::ChangeId);
}

#[test]
fn a_unique_hash_prefix_of_at_least_four_characters_resolves() {
    let (_dir, project) = project_with_unambiguous_tip(4);
    let tip = project.view().current().tip.clone();

    for length in [4, 7, 40] {
        let found = resolved(&project, &tip.as_str()[..length]);
        assert_eq!(found.commit, tip, "prefix of {length} characters");
        assert_eq!(found.matched, PointKind::CommitHash);
    }
}

#[test]
fn a_hash_prefix_shorter_than_four_characters_is_not_a_hash() {
    let (_dir, project) = project_with_saves(2);
    let tip = project.view().current().tip.clone();
    // Fewer than four characters is never a hash, whatever else shares them.
    let short = &tip.as_str()[..3];
    match project.resolve(&point(short)) {
        Err(ResolveError::UnknownPoint { name }) => assert_eq!(name, short),
        other => panic!("expected UnknownPoint, got {other:?}"),
    }
}

#[test]
fn an_ambiguous_hash_prefix_lists_its_candidates() {
    // Grow a history until two commits share their first four hash characters.
    let plain = Plain::new();
    let mut chain: Vec<CommitId> = Vec::new();
    let (first, second, prefix) = loop {
        let parents: Vec<&CommitId> = chain.last().into_iter().collect();
        let next = plain.commit(
            &[("a.txt", "a")],
            &format!("Save {}", chain.len()),
            &parents,
            at(1_000 + i64::try_from(chain.len()).unwrap()),
            None,
        );
        let head = next.as_str()[..4].to_owned();
        if let Some(twin) = chain.iter().find(|id| id.as_str()[..4] == head) {
            break (twin.clone(), next, head);
        }
        chain.push(next);
        assert!(chain.len() < 5_000, "no collision found");
    };
    plain.branch("main", &second);
    let (_dir, project) = plain.adopt(&[("a.txt", "a")]);

    match project.resolve(&point(&prefix)) {
        Err(ResolveError::Ambiguous {
            prefix: given,
            candidates,
        }) => {
            assert_eq!(given, prefix);
            let ids: Vec<_> = candidates.iter().map(|c| c.commit.clone()).collect();
            assert!(ids.contains(&first) && ids.contains(&second));
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

#[test]
fn unknown_names_are_reported_with_the_name() {
    let (_dir, project) = project_with_saves(1);
    for name in ["nothing-here", "zzzzzzzzzz", "ffffffffff"] {
        match project.resolve(&point(name)) {
            Err(ResolveError::UnknownPoint { name: given }) => assert_eq!(given, name),
            other => panic!("expected UnknownPoint for {name}, got {other:?}"),
        }
    }
}

/// Committer time of `civil` in the system's local zone (what `date:` is read in).
fn local(civil: jiff::civil::DateTime) -> GitTime {
    let zoned = civil.to_zoned(TimeZone::system()).unwrap();
    GitTime {
        seconds: zoned.timestamp().as_second(),
        offset_seconds: zoned.offset().seconds(),
    }
}

/// Three saves at 18:00 local time on 1, 10 and 20 September 2026, plus a save on another
/// line on the 12th. The author time of every commit is in 2020.
fn dated_project() -> (TempDir, Project, [CommitId; 3]) {
    let plain = Plain::new();
    let day = |d: i8| local(jiff::civil::date(2026, 9, d).at(18, 0, 0, 0));
    let first = plain.commit(&[("a.txt", "1")], "Day 1", &[], day(1), None);
    let second = plain.commit(&[("a.txt", "10")], "Day 10", &[&first], day(10), None);
    let third = plain.commit(&[("a.txt", "20")], "Day 20", &[&second], day(20), None);
    let elsewhere = plain.commit(&[("a.txt", "12")], "Day 12", &[&second], day(12), None);
    plain.branch("main", &third);
    plain.branch("side", &elsewhere);
    let (dir, project) = plain.adopt(&[("a.txt", "20")]);
    (dir, project, [first, second, third])
}

#[test]
fn a_date_resolves_to_the_newest_save_at_or_before_the_end_of_that_day() {
    let (_dir, project, [first, second, third]) = dated_project();

    // The save is at 18:00 that day: a day-only date still counts it.
    let same_day = resolved(&project, "date:2026-09-10");
    assert_eq!(same_day.commit, second);
    assert_eq!(same_day.matched, PointKind::Date);

    assert_eq!(resolved(&project, "date:2026-09-15").commit, second);
    assert_eq!(resolved(&project, "date:2026-09-01").commit, first);
    // Later than every save: the last save, never the draft.
    assert_eq!(resolved(&project, "date:2099-01-01").commit, third);
}

#[test]
fn a_date_with_a_time_covers_the_whole_minute() {
    let (_dir, project, [first, second, _third]) = dated_project();
    assert_eq!(resolved(&project, "date:2026-09-10T17:59").commit, first);
    assert_eq!(resolved(&project, "date:2026-09-10T18:00").commit, second);
}

#[test]
fn a_date_only_looks_at_the_current_line() {
    let (_dir, project, [_first, second, _third]) = dated_project();
    // The save on `side` (the 12th) is newer than the 11th but isn't on `main`.
    assert_eq!(resolved(&project, "date:2026-09-13").commit, second);
}

#[test]
fn a_date_before_the_first_save_finds_nothing() {
    let (_dir, project, _) = dated_project();
    match project.resolve(&point("date:2026-08-31")) {
        Err(ResolveError::NoSaveBefore { date }) => {
            assert!(matches!(date, DateSpec::Day(_)));
        }
        other => panic!("expected NoSaveBefore, got {other:?}"),
    }
}

#[test]
fn extreme_dates_never_panic() {
    let (_dir, project, [_first, _second, third]) = dated_project();
    // Past the latest representable time: no limit, so the last save.
    assert_eq!(resolved(&project, "date:9999-12-31").commit, third);
    // Before the earliest: nothing.
    assert!(matches!(
        project.resolve(&point("date:-9999-01-01")),
        Err(ResolveError::NoSaveBefore { .. })
    ));
}

#[test]
fn no_save_before_reads_the_date_back_plainly() {
    let (_dir, project, _) = dated_project();
    let message = project
        .resolve(&point("date:2026-08-31T10:05"))
        .unwrap_err()
        .to_string();
    assert!(message.contains("2026-08-31T10:05"), "{message}");
}

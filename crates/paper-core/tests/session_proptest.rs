//! PV-026 (I4 acceptance): in a random 200-step session of edits and commands, undoing step
//! by step restores the exact state at every step, and redoing replays every step exactly.
//!
//! A state is the view (every line's tip and draft, and the current line) plus the working
//! files (path to bytes, with empty folders). The oracle records the state each op produced,
//! then walks the whole log back with `undo` and forward again with `redo`.
//!
//! The default is 3 cases. `PROPTEST_CASES=200 cargo test -p paper-core --test
//! session_proptest` runs a long one. Nothing is written to the source tree on failure: the
//! minimal session is printed instead.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::process::Command;

use paper_core::project::*;
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::Config;

const SESSION_LEN: usize = 200;
const DEFAULT_CASES: u32 = 3;

/// The paths a session writes: some at the top, some in subfolders.
const POOL: [&str; 8] = [
    "a.txt",
    "b.txt",
    "notes.md",
    "data.bin",
    "top.tex",
    "sec/one.tex",
    "sec/two.tex",
    "sec/deep/x.txt",
];

/// What a save or restore may name: the pool, a folder, and a path nothing ever has.
const NAMEABLE: [&str; 10] = [
    "a.txt",
    "b.txt",
    "notes.md",
    "data.bin",
    "top.tex",
    "sec/one.tex",
    "sec/two.tex",
    "sec/deep/x.txt",
    "sec",
    "ghost.txt",
];

#[derive(Debug, Clone)]
enum Step {
    Write { path: usize, content: Vec<u8> },
    Delete { path: usize },
    FileToFolder { path: usize },
    FolderToFile { path: usize },
    SaveAll,
    SaveOnly { paths: Vec<usize> },
    Restore { paths: Vec<usize>, back: u8 },
    Undo,
    Redo,
    UndoTo { pick: usize },
    Status,
}

fn content() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        1 => Just(Vec::new()),
        4 => "[a-z \n]{1,12}".prop_map(String::into_bytes),
        2 => vec(any::<u8>(), 1..8),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        30 => (0..POOL.len(), content()).prop_map(|(path, content)| Step::Write { path, content }),
        6 => (0..POOL.len()).prop_map(|path| Step::Delete { path }),
        4 => (0..POOL.len()).prop_map(|path| Step::FileToFolder { path }),
        4 => (0..POOL.len()).prop_map(|path| Step::FolderToFile { path }),
        10 => Just(Step::SaveAll),
        5 => vec(0..NAMEABLE.len(), 1..3).prop_map(|paths| Step::SaveOnly { paths }),
        6 => (vec(0..NAMEABLE.len(), 1..3), 0u8..4)
            .prop_map(|(paths, back)| Step::Restore { paths, back }),
        12 => Just(Step::Undo),
        8 => Just(Step::Redo),
        5 => any::<usize>().prop_map(|pick| Step::UndoTo { pick }),
        5 => Just(Step::Status),
    ]
}

/// A working file or an empty folder. Folders that hold files are implied by their files.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    File(Vec<u8>),
    EmptyFolder,
}

/// The view and the working files at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    view: String,
    files: BTreeMap<String, Node>,
}

fn identity() -> Identity {
    Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    }
}

fn collect(root: &Path, dir: &Path, out: &mut BTreeMap<String, Node>) {
    let mut empty = true;
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap();
        if relative.to_str() == Some(".git") {
            continue;
        }
        empty = false;
        if path.is_dir() {
            collect(root, &path, out);
        } else {
            out.insert(
                relative.to_str().unwrap().replace('\\', "/"),
                Node::File(fs::read(&path).unwrap()),
            );
        }
    }
    if empty && dir != root {
        let relative = dir.strip_prefix(root).unwrap();
        out.insert(
            relative.to_str().unwrap().replace('\\', "/"),
            Node::EmptyFolder,
        );
    }
}

/// The working files: path to bytes, plus the empty folders.
fn capture_files(dir: &Path) -> BTreeMap<String, Node> {
    let mut files = BTreeMap::new();
    collect(dir, dir, &mut files);
    files
}

/// A canonical text of a view: the same for the live view and for one read from an op file.
fn canonical_view(view: &serde_json::Value) -> String {
    serde_json::to_string(view).unwrap()
}

fn capture(project: &Project, dir: &Path) -> State {
    State {
        view: canonical_view(&serde_json::to_value(project.view()).unwrap()),
        files: capture_files(dir),
    }
}

/// `Ok` when the two states are equal, otherwise what differs.
fn compare(expected: &State, actual: &State) -> Result<(), String> {
    let mut problems = Vec::new();
    if expected.view != actual.view {
        problems.push(format!(
            "view differs\n  expected {}\n  actual   {}",
            expected.view, actual.view
        ));
    }
    for (path, node) in &expected.files {
        match actual.files.get(path) {
            None => problems.push(format!("{path}: missing, expected {node:?}")),
            Some(found) if found != node => {
                problems.push(format!("{path}: expected {node:?}, found {found:?}"));
            }
            Some(_) => {}
        }
    }
    for (path, node) in &actual.files {
        if !expected.files.contains_key(path) {
            problems.push(format!("{path}: unexpected {node:?}"));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
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

fn head_id(dir: &Path) -> String {
    fs::read_to_string(dir.join(".git/paper/activity/head"))
        .unwrap()
        .trim()
        .to_owned()
}

fn op_json(dir: &Path, id: &str) -> serde_json::Value {
    let bytes = fs::read(dir.join(format!(".git/paper/activity/ops/{id}.json"))).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

// Filesystem steps: the researcher's own edits, made outside paper.

fn write_file(dir: &Path, path: &str, bytes: &[u8]) {
    let mut ancestor = dir.to_path_buf();
    let parts: Vec<&str> = path.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        ancestor.push(part);
        if ancestor.is_file() {
            fs::remove_file(&ancestor).unwrap();
        }
    }
    let target = dir.join(path);
    if target.is_dir() {
        fs::remove_dir_all(&target).unwrap();
    }
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, bytes).unwrap();
}

fn prune_empty_parents(dir: &Path, path: &str) {
    let mut current = dir.join(path);
    while let Some(parent) = current.parent().map(Path::to_path_buf) {
        if parent == dir || fs::remove_dir(&parent).is_err() {
            break;
        }
        current = parent;
    }
}

fn delete_file(dir: &Path, path: &str) {
    let target = dir.join(path);
    if target.is_file() {
        fs::remove_file(&target).unwrap();
        prune_empty_parents(dir, path);
    }
}

fn file_to_folder(dir: &Path, path: &str) {
    let target = dir.join(path);
    if target.is_file() {
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("inner.txt"), b"inner").unwrap();
    }
}

fn folder_to_file(dir: &Path, path: &str) {
    let target = dir.join(path);
    if target.is_dir() {
        fs::remove_dir_all(&target).unwrap();
        fs::write(target, b"was a folder").unwrap();
    }
}

fn names(indices: &[usize]) -> Vec<String> {
    indices.iter().map(|i| NAMEABLE[*i].to_owned()).collect()
}

/// A project in a temp folder, with the state every op produced.
struct Session {
    dir: tempfile::TempDir,
    project: Project,
    /// Op id to the state it produced.
    states: HashMap<String, State>,
    steps_done: usize,
}

impl Session {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"seed").unwrap();
        let (project, _) = Project::init(dir.path(), identity(), &SnapshotOptions::default())
            .expect("init a project");
        let mut session = Self {
            dir,
            project,
            states: HashMap::new(),
            steps_done: 0,
        };
        let before = capture_files(session.dir.path());
        session.record(&before);
        session
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Records the state of every op added since the last call. The newest op is the head,
    /// so its state is the project as it stands. An older new op is an edits entry recorded
    /// at the start of the command: the new draft, with the files as they were before it.
    fn record(&mut self, files_before: &BTreeMap<String, Node>) {
        let entries = self.project.activity(1_000_000).unwrap();
        for (index, entry) in entries.iter().enumerate() {
            let id = entry.id.to_string();
            if self.states.contains_key(&id) {
                break;
            }
            let state = if index == 0 {
                capture(&self.project, self.path())
            } else {
                State {
                    view: canonical_view(&op_json(self.path(), &id)["view"]),
                    files: files_before.clone(),
                }
            };
            self.states.insert(id, state);
        }
    }

    /// The working files and view must equal the state of the op the head's position names.
    fn check_position(&self) -> Result<(), String> {
        let position = op_json(self.path(), &head_id(self.path()))["position"]
            .as_str()
            .unwrap()
            .to_owned();
        let expected = self
            .states
            .get(&position)
            .ok_or_else(|| format!("the head's position {position} is an op nobody recorded"))?;
        compare(expected, &capture(&self.project, self.path()))
            .map_err(|e| format!("state differs from op {position}:\n{e}"))
    }

    fn check_undone(&self, undone: &Undone) -> Result<(), String> {
        if !undone.files_match {
            return Err(format!("files_match is false after {:?}", undone.entry));
        }
        self.check_position()
    }

    fn position(&self) -> String {
        op_json(self.path(), &head_id(self.path()))["position"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn apply(&mut self, step: &Step) -> Result<(), String> {
        let dir = self.path().to_path_buf();
        let before = capture_files(&dir);
        self.steps_done += 1;
        let n = self.steps_done;
        match step {
            Step::Write { path, content } => write_file(&dir, POOL[*path], content),
            Step::Delete { path } => delete_file(&dir, POOL[*path]),
            Step::FileToFolder { path } => file_to_folder(&dir, POOL[*path]),
            Step::FolderToFile { path } => folder_to_file(&dir, POOL[*path]),
            Step::SaveAll => {
                self.project
                    .save(&SaveRequest {
                        message: format!("save {n}"),
                        paths: PathSelection::All,
                    })
                    .map_err(|e| format!("save all failed: {e}"))?;
            }
            Step::SaveOnly { paths } => {
                match self.project.save(&SaveRequest {
                    message: format!("save {n}"),
                    paths: PathSelection::Only(names(paths)),
                }) {
                    Ok(_) | Err(SaveError::NoChangesAt { .. }) => {}
                    Err(e) => return Err(format!("save of a selection failed: {e}")),
                }
            }
            Step::Restore { paths, back } => {
                let from = if *back == 0 {
                    None
                } else {
                    Some(format!("last~{back}").parse().unwrap())
                };
                match self.project.restore(&RestoreRequest {
                    paths: names(paths),
                    from,
                }) {
                    Ok(_) | Err(RestoreError::UnknownPaths { .. } | RestoreError::Resolve(_)) => {}
                    Err(e) => return Err(format!("restore failed: {e}")),
                }
            }
            Step::Undo => {
                let outcome = self
                    .project
                    .undo()
                    .map_err(|e| format!("undo failed: {e}"))?;
                self.record(&before);
                if let UndoOutcome::Undone(undone) = outcome {
                    return self.check_undone(&undone);
                }
            }
            Step::Redo => {
                let outcome = self
                    .project
                    .redo()
                    .map_err(|e| format!("redo failed: {e}"))?;
                self.record(&before);
                if let UndoOutcome::Undone(undone) = outcome {
                    return self.check_undone(&undone);
                }
            }
            Step::UndoTo { pick } => {
                let entries = self.project.activity(1_000_000).unwrap();
                let id = entries[pick % entries.len()].id.to_string();
                let outcome = self
                    .project
                    .undo_to(&id)
                    .map_err(|e| format!("undo --to {id} failed: {e}"))?;
                self.record(&before);
                if let UndoOutcome::Undone(undone) = outcome {
                    return self.check_undone(&undone);
                }
            }
            Step::Status => {
                self.project
                    .status(&StatusOptions::default())
                    .map_err(|e| format!("status failed: {e}"))?;
            }
        }
        self.record(&before);
        Ok(())
    }

    /// Undoes to the start, then redoes to the end, checking every state.
    fn walk_back_and_forth(&mut self) -> Result<(), String> {
        // Record pending edits first, so the walk starts from a recorded state.
        self.apply(&Step::Status)?;
        let final_state = capture(&self.project, self.path());
        let bound = self.states.len() + 5;

        let mut back = vec![self.position()];
        loop {
            match self
                .project
                .undo()
                .map_err(|e| format!("undo failed: {e}"))?
            {
                UndoOutcome::NothingToUndo => break,
                UndoOutcome::NothingToRedo => return Err("undo said NothingToRedo".to_owned()),
                UndoOutcome::Undone(undone) => {
                    self.check_undone(&undone)
                        .map_err(|e| format!("undo #{}: {e}", back.len()))?;
                    back.push(self.position());
                }
            }
            if back.len() > bound {
                return Err(format!(
                    "undo did not reach the first entry in {bound} steps"
                ));
            }
        }

        // A session can end with redo entries still on the stack; redo stops where the
        // session ended, which the walk must reach.
        let end = back[0].clone();
        let mut forward = vec![self.position()];
        while forward.last() != Some(&end) {
            match self
                .project
                .redo()
                .map_err(|e| format!("redo failed: {e}"))?
            {
                UndoOutcome::NothingToRedo => {
                    return Err(format!(
                        "redo ran out before reaching the session's end {end}"
                    ));
                }
                UndoOutcome::NothingToUndo => return Err("redo said NothingToUndo".to_owned()),
                UndoOutcome::Undone(undone) => {
                    self.check_undone(&undone)
                        .map_err(|e| format!("redo #{}: {e}", forward.len()))?;
                    forward.push(self.position());
                }
            }
            if forward.len() > bound {
                return Err(format!("redo did not stop in {bound} steps"));
            }
        }

        let mut expected_path = back.clone();
        expected_path.reverse();
        if forward != expected_path {
            return Err(format!(
                "redo did not retrace the undo walk\n  undo went through {back:?}\n  redo went through {forward:?}"
            ));
        }
        compare(&final_state, &capture(&self.project, self.path())).map_err(|e| {
            format!("after the last redo, the state is not the session's final one:\n{e}")
        })?;
        git(self.path(), &["fsck", "--strict", "--no-progress"]);
        Ok(())
    }
}

fn run_session(steps: &[Step]) -> Result<(), String> {
    let mut session = Session::new();
    for (index, step) in steps.iter().enumerate() {
        session
            .apply(step)
            .map_err(|e| format!("step {index} ({step:?}): {e}"))?;
    }
    session.walk_back_and_forth()
}

/// How to reproduce a failure locally: the failing run prints its seed (`PROPTEST_RNG_SEED=...`)
/// and proptest prints the minimal failing session. Re-run with the same seed, for example
/// `PROPTEST_RNG_SEED=123 PROPTEST_CASES=3 cargo test -p paper-core --test session_proptest`.
/// Shrinking is bounded to 16 steps (about 20 s each); `PROPTEST_MAX_SHRINK_ITERS` raises it.
fn config() -> Config {
    use proptest::test_runner::RngSeed;
    let mut config = Config::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = DEFAULT_CASES;
    }
    if std::env::var_os("PROPTEST_MAX_SHRINK_ITERS").is_none() {
        config.max_shrink_iters = 16;
    }
    if matches!(config.rng_seed, RngSeed::Random) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() ^ u64::from(d.subsec_nanos()));
        config.rng_seed = RngSeed::Fixed(seed);
    }
    if let RngSeed::Fixed(seed) = config.rng_seed {
        eprintln!("session_proptest: PROPTEST_RNG_SEED={seed}");
    }
    config.failure_persistence = None;
    config
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn a_random_200_step_session_undoes_and_redoes_exactly(
        steps in vec(step(), SESSION_LEN..=SESSION_LEN)
    ) {
        run_session(&steps).map_err(TestCaseError::fail)?;
    }
}

// Sanity tests: the oracle sees what it should, and flags what it should.

#[test]
fn state_capture_sees_a_one_byte_edit_and_a_new_empty_folder() {
    let session = Session::new();
    let dir = session.path();
    let start = capture(&session.project, dir);
    assert_eq!(
        start.files.get("a.txt"),
        Some(&Node::File(b"seed".to_vec()))
    );

    fs::write(dir.join("a.txt"), b"seee").unwrap();
    let edited = capture(&session.project, dir);
    assert!(
        compare(&start, &edited).is_err(),
        "a one-byte edit went unseen"
    );

    fs::write(dir.join("a.txt"), b"seed").unwrap();
    fs::create_dir(dir.join("empty")).unwrap();
    let with_folder = capture(&session.project, dir);
    assert_eq!(with_folder.files.get("empty"), Some(&Node::EmptyFolder));
    assert!(
        compare(&start, &with_folder).is_err(),
        "a new empty folder went unseen"
    );
}

#[test]
fn compare_flags_a_wrong_expected_state() {
    let session = Session::new();
    let actual = capture(&session.project, session.path());
    compare(&actual, &actual).unwrap();

    let mut wrong_file = actual.clone();
    wrong_file
        .files
        .insert("a.txt".to_owned(), Node::File(b"other".to_vec()));
    assert!(compare(&wrong_file, &actual).unwrap_err().contains("a.txt"));

    let mut extra_file = actual.clone();
    extra_file
        .files
        .insert("ghost".to_owned(), Node::File(Vec::new()));
    assert!(compare(&extra_file, &actual).unwrap_err().contains("ghost"));

    let mut wrong_view = actual.clone();
    wrong_view.view = wrong_view.view.replace('a', "b");
    assert!(
        compare(&wrong_view, &actual)
            .unwrap_err()
            .contains("view differs")
    );
}

#[test]
fn a_fixed_short_session_passes_the_same_checks() {
    let steps = [
        Step::Write {
            path: 1,
            content: b"one".to_vec(),
        },
        Step::SaveAll,
        Step::Write {
            path: 1,
            content: b"two".to_vec(),
        },
        Step::Write {
            path: 5,
            content: Vec::new(),
        },
        Step::Undo,
        Step::Redo,
        Step::SaveAll,
        Step::FileToFolder { path: 1 },
        Step::Undo,
        Step::Undo,
        Step::Redo,
    ];
    run_session(&steps).unwrap();
}

#[test]
fn the_position_check_flags_a_working_file_changed_behind_undos_back() {
    let mut session = Session::new();
    for step in [
        Step::Write {
            path: 1,
            content: b"one".to_vec(),
        },
        Step::SaveAll,
        Step::Write {
            path: 1,
            content: b"two".to_vec(),
        },
    ] {
        session.apply(&step).unwrap();
    }
    let UndoOutcome::Undone(undone) = session.project.undo().unwrap() else {
        panic!("expected an undone entry");
    };
    let before = capture_files(session.path());
    session.record(&before);
    session.check_undone(&undone).unwrap();

    fs::write(session.path().join("b.txt"), b"tampered").unwrap();
    let error = session.check_position().unwrap_err();
    assert!(error.contains("b.txt"), "{error}");
}

#[test]
fn a_session_of_only_unsaved_edits_undoes_to_the_first_entry_and_back() {
    let steps = [
        Step::Write {
            path: 0,
            content: b"changed".to_vec(),
        },
        Step::Status,
    ];
    run_session(&steps).unwrap();
}

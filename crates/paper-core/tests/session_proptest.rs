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
const DEFAULT_CASES: u32 = 1;

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

/// What a file-to-folder or folder-to-file swap may target: the pool and two folders.
const SWAPPABLE: [&str; 10] = [
    "a.txt",
    "b.txt",
    "notes.md",
    "data.bin",
    "top.tex",
    "sec/one.tex",
    "sec/two.tex",
    "sec/deep/x.txt",
    "sec",
    "sec/deep",
];

#[derive(Debug, Clone)]
enum Step {
    Write {
        path: usize,
        content: Vec<u8>,
    },
    Delete {
        path: usize,
    },
    FileToFolder {
        path: usize,
    },
    FolderToFile {
        path: usize,
    },
    SaveAll,
    /// `smart`: name files that really changed instead of names from the fixed list.
    SaveOnly {
        paths: Vec<usize>,
        smart: bool,
    },
    /// `smart`: name files of the save being restored.
    Restore {
        paths: Vec<usize>,
        back: u8,
        smart: bool,
    },
    Undo,
    Redo,
    /// `from_redo`: pick from the redo stack when it is not empty.
    UndoTo {
        pick: usize,
        from_redo: bool,
    },
    /// Undo `undos` times, then redo `redos` times.
    Burst {
        undos: u8,
        redos: u8,
    },
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
        28 => (0..POOL.len(), content()).prop_map(|(path, content)| Step::Write { path, content }),
        6 => (0..POOL.len()).prop_map(|path| Step::Delete { path }),
        4 => (0..SWAPPABLE.len()).prop_map(|path| Step::FileToFolder { path }),
        4 => (0..SWAPPABLE.len()).prop_map(|path| Step::FolderToFile { path }),
        10 => Just(Step::SaveAll),
        5 => (vec(0..NAMEABLE.len(), 1..3), any::<bool>())
            .prop_map(|(paths, smart)| Step::SaveOnly { paths, smart }),
        6 => (vec(0..NAMEABLE.len(), 1..3), 0u8..4, any::<bool>())
            .prop_map(|(paths, back, smart)| Step::Restore { paths, back, smart }),
        10 => Just(Step::Undo),
        6 => Just(Step::Redo),
        5 => (any::<usize>(), any::<bool>())
            .prop_map(|(pick, from_redo)| Step::UndoTo { pick, from_redo }),
        3 => any::<usize>().prop_map(|pick| Step::UndoTo { pick, from_redo: true }),
        4 => (1u8..=8, 0u8..=8).prop_map(|(undos, redos)| Step::Burst { undos, redos }),
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

/// What an undo-family command asks for.
#[derive(Debug, Clone)]
enum Aim {
    Undo,
    Redo,
    To(String),
}

/// The file nodes of a capture, without the empty folders (the snapshot never sees those).
fn tracked(files: &BTreeMap<String, Node>) -> BTreeMap<&String, &Node> {
    files
        .iter()
        .filter(|(_, node)| matches!(node, Node::File(_)))
        .collect()
}

/// A project in a temp folder, with the state every op produced, and an independent model of
/// where undo and redo should be.
struct Session {
    dir: tempfile::TempDir,
    project: Project,
    /// Op id to the state it produced.
    states: HashMap<String, State>,
    /// Op id to the head that was current right before it was recorded (read from the head
    /// file, never from the op's own `parent` field).
    prev: HashMap<String, Option<String>>,
    /// The op the project should be at.
    model_pos: String,
    /// The redo stack the head should hold, next entry first.
    model_redo: Vec<String>,
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
            prev: HashMap::new(),
            model_pos: String::new(),
            model_redo: Vec::new(),
            steps_done: 0,
        };
        let before = capture_files(session.dir.path());
        let fresh = session.record(&before, None);
        session
            .model_pos
            .clone_from(fresh.last().expect("init records an op"));
        session
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Records the state of every op added since the last call, oldest first, and returns
    /// their ids. The newest op is the head, so its state is the project as it stands. An
    /// older new op is an edits entry recorded at the start of the command: the new draft,
    /// with the files as they were before it. `head_before` is the head the command started
    /// from.
    fn record(
        &mut self,
        files_before: &BTreeMap<String, Node>,
        head_before: Option<String>,
    ) -> Vec<String> {
        // Only the newest few entries can be new: widen the window until it reaches an op
        // already recorded.
        let mut limit = 8;
        let entries = loop {
            let entries = self.project.activity(limit).unwrap();
            let reached_known = entries
                .iter()
                .any(|entry| self.states.contains_key(&entry.id.to_string()));
            if reached_known || entries.len() < limit {
                break entries;
            }
            limit *= 4;
        };
        let mut fresh = Vec::new();
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
            self.states.insert(id.clone(), state);
            fresh.push(id);
        }
        fresh.reverse();
        let mut before = head_before;
        for id in &fresh {
            self.prev.insert(id.clone(), before);
            before = Some(id.clone());
        }
        fresh
    }

    fn position(&self) -> String {
        op_json(self.path(), &head_id(self.path()))["position"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn head_redo(&self) -> Vec<String> {
        op_json(self.path(), &head_id(self.path()))["redo"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    }

    /// The head's `position` and `redo` must be what the model says.
    fn check_head(&self) -> Result<(), String> {
        let position = self.position();
        if position != self.model_pos {
            return Err(format!(
                "the head's position is {position}, the model expects {}",
                self.model_pos
            ));
        }
        let redo = self.head_redo();
        if redo != self.model_redo {
            return Err(format!(
                "the head's redo stack is {redo:?}, the model expects {:?}",
                self.model_redo
            ));
        }
        Ok(())
    }

    /// The working files and view must equal the state of the op the head's position names.
    fn check_position(&self) -> Result<(), String> {
        let position = self.position();
        let expected = self
            .states
            .get(&position)
            .ok_or_else(|| format!("the head's position {position} is an op nobody recorded"))?;
        compare(expected, &capture(&self.project, self.path()))
            .map_err(|e| format!("state differs from op {position}:\n{e}"))
    }

    /// Runs undo, redo or `undo --to`, and checks it against the model: I1 (edits made since
    /// the last command become exactly one entry first, which clears the redo stack), the
    /// outcome variant, the head's position and redo stack, `files_match`, and the state.
    fn undo_family(&mut self, aim: &Aim) -> Result<UndoOutcome, String> {
        let dir = self.path().to_path_buf();
        let before = capture_files(&dir);
        let head_before = head_id(&dir);
        let expected_edits = tracked(&before) != tracked(&self.states[&self.model_pos].files);
        let outcome = match aim {
            Aim::Undo => self.project.undo(),
            Aim::Redo => self.project.redo(),
            Aim::To(id) => self.project.undo_to(id),
        }
        .map_err(|e| format!("{aim:?} failed: {e}"))?;
        let fresh = self.record(&before, Some(head_before));

        // I1 first: unsaved edits become one entry of their own, then the transition.
        let transitioned = matches!(outcome, UndoOutcome::Undone(_));
        let expected_ops = usize::from(expected_edits) + usize::from(transitioned);
        if fresh.len() != expected_ops {
            return Err(format!(
                "I1: {aim:?} recorded {} op(s), expected {expected_ops} (unsaved edits: {expected_edits}, state changed: {transitioned})",
                fresh.len()
            ));
        }
        if expected_edits {
            self.model_pos.clone_from(&fresh[0]);
            self.model_redo.clear();
        }

        let expect_change = match aim {
            Aim::Undo => match self.prev[&self.model_pos].clone() {
                None => false,
                Some(parent) => {
                    self.model_redo.insert(0, self.model_pos.clone());
                    self.model_pos = parent;
                    true
                }
            },
            Aim::Redo => {
                if self.model_redo.is_empty() {
                    false
                } else {
                    self.model_pos = self.model_redo.remove(0);
                    true
                }
            }
            Aim::To(id) => {
                self.model_pos.clone_from(id);
                self.model_redo.clear();
                true
            }
        };
        let variant_ok = match &outcome {
            UndoOutcome::Undone(_) => expect_change,
            UndoOutcome::NothingToUndo => !expect_change && !matches!(aim, Aim::Redo),
            UndoOutcome::NothingToRedo => !expect_change && matches!(aim, Aim::Redo),
        };
        if !variant_ok {
            return Err(format!(
                "{aim:?} answered {outcome:?}, the model expected {}",
                if expect_change {
                    "a change"
                } else {
                    "nothing to do"
                }
            ));
        }
        self.check_head()?;
        if let UndoOutcome::Undone(undone) = &outcome {
            if !undone.files_match {
                return Err(format!("files_match is false after {:?}", undone.entry));
            }
            self.check_position()?;
        }
        Ok(outcome)
    }

    /// Names for a save selection or a restore: half the time from `candidates` (paths that
    /// really changed or exist), otherwise from the fixed list.
    fn pick_names(indices: &[usize], smart: bool, candidates: &[String]) -> Vec<String> {
        indices
            .iter()
            .map(|i| {
                if smart && !candidates.is_empty() {
                    candidates[i % candidates.len()].clone()
                } else {
                    NAMEABLE[i % NAMEABLE.len()].to_owned()
                }
            })
            .collect()
    }

    /// Files whose bytes differ between the working files and the state the project is at.
    fn changed_paths(&self) -> Vec<String> {
        let now = capture_files(self.path());
        let at = &self.states[&self.model_pos].files;
        let (now, at) = (tracked(&now), tracked(at));
        let mut paths: Vec<String> = now
            .iter()
            .filter(|(path, node)| at.get(*path) != Some(node))
            .map(|(path, _)| (*path).clone())
            .chain(
                at.keys()
                    .filter(|p| !now.contains_key(*p))
                    .map(|p| (*p).clone()),
            )
            .collect();
        paths.sort();
        paths
    }

    /// Files of the save `last~back` on the current line (empty when there is none).
    fn saved_paths(&self, back: u8) -> Vec<String> {
        let tip = self.project.view().current().tip.to_string();
        let output = Command::new("git")
            .arg("-C")
            .arg(self.path())
            .args(["ls-tree", "-r", "--name-only", &format!("{tip}~{back}")])
            .output()
            .unwrap();
        if !output.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn apply(&mut self, step: &Step) -> Result<(), String> {
        let dir = self.path().to_path_buf();
        let mut before = capture_files(&dir);
        let head_before = head_id(&dir);
        self.steps_done += 1;
        let n = self.steps_done;
        match step {
            Step::Write { path, content } => write_file(&dir, POOL[*path], content),
            Step::Delete { path } => delete_file(&dir, POOL[*path]),
            Step::FileToFolder { path } => file_to_folder(&dir, SWAPPABLE[*path]),
            Step::FolderToFile { path } => folder_to_file(&dir, SWAPPABLE[*path]),
            Step::SaveAll => {
                self.project
                    .save(&SaveRequest {
                        message: format!("save {n}"),
                        paths: PathSelection::All,
                    })
                    .map_err(|e| format!("save all failed: {e}"))?;
            }
            Step::SaveOnly { paths, smart } => {
                if *smart && self.changed_paths().is_empty() {
                    // Nothing changed: edit a file so the selection makes a real save.
                    write_file(&dir, POOL[paths[0] % POOL.len()], b"selective");
                    before = capture_files(&dir);
                }
                let names = Self::pick_names(paths, *smart, &self.changed_paths());
                match self.project.save(&SaveRequest {
                    message: format!("save {n}"),
                    paths: PathSelection::Only(names),
                }) {
                    Ok(_) | Err(SaveError::NoChangesAt { .. }) => {}
                    Err(e) => return Err(format!("save of a selection failed: {e}")),
                }
            }
            Step::Restore { paths, back, smart } => {
                let from = if *back == 0 {
                    None
                } else {
                    Some(format!("last~{back}").parse().unwrap())
                };
                let names = Self::pick_names(paths, *smart, &self.saved_paths(*back));
                match self.project.restore(&RestoreRequest { paths: names, from }) {
                    Ok(_) | Err(RestoreError::UnknownPaths { .. } | RestoreError::Resolve(_)) => {}
                    Err(e) => return Err(format!("restore failed: {e}")),
                }
            }
            Step::Undo => {
                self.undo_family(&Aim::Undo)?;
                return Ok(());
            }
            Step::Redo => {
                self.undo_family(&Aim::Redo)?;
                return Ok(());
            }
            Step::UndoTo { pick, from_redo } => {
                let id = if *from_redo && !self.model_redo.is_empty() {
                    self.model_redo[pick % self.model_redo.len()].clone()
                } else {
                    let entries = self.project.activity(1_000_000).unwrap();
                    entries[pick % entries.len()].id.to_string()
                };
                self.undo_family(&Aim::To(id))?;
                return Ok(());
            }
            Step::Burst { undos, redos } => {
                for _ in 0..*undos {
                    self.undo_family(&Aim::Undo)?;
                }
                for _ in 0..*redos {
                    self.undo_family(&Aim::Redo)?;
                }
                return Ok(());
            }
            Step::Status => {
                self.project
                    .status(&StatusOptions::default())
                    .map_err(|e| format!("status failed: {e}"))?;
            }
        }
        // Any other command that recorded something: its newest op is the position, and the
        // redo stack is empty.
        let fresh = self.record(&before, Some(head_before));
        if let Some(newest) = fresh.last() {
            self.model_pos.clone_from(newest);
            self.model_redo.clear();
        }
        self.check_head()
    }

    /// Undoes to the start, then redoes to the end, checking every state.
    fn walk_back_and_forth(&mut self) -> Result<(), String> {
        // Record pending edits first, so the walk starts from a recorded state.
        self.apply(&Step::Status)?;
        let final_state = capture(&self.project, self.path());
        let redo_at_end = self.model_redo.clone();
        let bound = self.states.len() + 5;

        let mut back = vec![self.position()];
        loop {
            match self
                .undo_family(&Aim::Undo)
                .map_err(|e| format!("undo #{}: {e}", back.len()))?
            {
                UndoOutcome::NothingToUndo => break,
                UndoOutcome::NothingToRedo => return Err("undo said NothingToRedo".to_owned()),
                UndoOutcome::Undone(_) => back.push(self.position()),
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
                .undo_family(&Aim::Redo)
                .map_err(|e| format!("redo #{}: {e}", forward.len()))?
            {
                UndoOutcome::NothingToRedo => {
                    return Err(format!(
                        "redo ran out before reaching the session's end {end}"
                    ));
                }
                UndoOutcome::NothingToUndo => return Err("redo said NothingToUndo".to_owned()),
                UndoOutcome::Undone(_) => forward.push(self.position()),
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
        if self.head_redo() != redo_at_end || self.model_redo != redo_at_end {
            return Err(format!(
                "at the session's end the redo stack is {:?} (model {:?}), it was {redo_at_end:?}",
                self.head_redo(),
                self.model_redo
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

/// One case by default: a case is 200 steps on a real repository and takes a minute or more
/// on a slow runner. Every CI job seeds from its own clock, so a pull request still runs one
/// different session per OS. `PROPTEST_CASES` raises the count and `PROPTEST_RNG_SEED` replays
/// a run (the seed is printed).
fn config() -> Config {
    use proptest::test_runner::RngSeed;
    let mut config = Config::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = DEFAULT_CASES;
    }
    if std::env::var_os("PROPTEST_MAX_SHRINK_TIME").is_none() {
        config.max_shrink_time = 120_000;
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
    session.undo_family(&Aim::Undo).unwrap();

    fs::write(session.path().join("b.txt"), b"tampered").unwrap();
    let error = session.check_position().unwrap_err();
    assert!(error.contains("b.txt"), "{error}");
}

#[test]
fn the_model_flags_a_head_whose_position_or_redo_stack_disagrees() {
    let mut session = Session::new();
    for step in [
        Step::Write {
            path: 1,
            content: b"one".to_vec(),
        },
        Step::SaveAll,
        Step::Undo,
    ] {
        session.apply(&step).unwrap();
    }
    session.check_head().unwrap();
    assert_eq!(session.model_redo.len(), 1);

    let saved_redo = session.model_redo.clone();
    session.model_redo = Vec::new();
    assert!(session.check_head().unwrap_err().contains("redo stack"));
    session.model_redo = saved_redo;

    let start = session.model_pos.clone();
    session.model_pos = session.model_redo[0].clone();
    assert!(session.check_head().unwrap_err().contains("position"));
    session.model_pos = start;
    session.check_head().unwrap();
}

#[test]
fn undo_after_unsaved_edits_records_them_first_and_clears_the_redo_stack() {
    let steps = [
        Step::Write {
            path: 1,
            content: b"one".to_vec(),
        },
        Step::SaveAll,
        Step::Undo,
        Step::Write {
            path: 2,
            content: b"unsaved".to_vec(),
        },
        Step::Redo,
        Step::Undo,
        Step::UndoTo {
            pick: 0,
            from_redo: true,
        },
        Step::Burst { undos: 3, redos: 2 },
    ];
    run_session(&steps).unwrap();
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

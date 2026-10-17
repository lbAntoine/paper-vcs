//! `paper init`: start a new project, or adopt an existing git repository
//! (02-cli.md § paper init).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use paper_git::{ChangeId, CommitId, GitError, GitRepo, Identity, NewCommit, TreeId};

use super::export::{ExportError, Mode, export, write_keep};
use super::init_profile::{ProfilePlan, ProfileReport};
use super::{FORMAT_VERSION, Project, VERSION_FILE, paper_dir, signature_now, write_draft};
use super::{LockError, LockOptions, ProjectLock};
use crate::activity::{ActivityError, ActivityLog, OpKind};
use crate::config::ConfigError;
use crate::name::LineName;
use crate::profile::{Profile, gitignore::GitignoreError};
use crate::snapshot::{Skipped, SnapshotError, SnapshotOptions, StatCache, snapshot};
use crate::view::{Line, View};

/// How `init` came to use its profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    /// Detected from the files in the folder.
    Detected,
    /// Chosen by the researcher (`--profile`, or the prompt).
    Chosen,
    /// Read from an existing `profile` in `paper.toml`.
    PaperToml,
}

/// What `init` did, for the front-end to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Initialized {
    /// `true` when an existing git repository was adopted rather than created.
    pub adopted: bool,
    /// Files recorded in the current line's draft.
    pub tracked_files: usize,
    /// Files the first snapshot deliberately left out.
    pub skipped: Vec<Skipped>,
    /// Large tracked files, with their size.
    pub large: Vec<(String, u64)>,
    /// Existing branches whose names can't be used as line names; they were left alone.
    pub skipped_branches: Vec<String>,
    /// Name of the profile in effect: `latex`, `markdown`, ...
    pub profile: String,
    /// How the profile was picked.
    pub profile_source: ProfileSource,
    /// Files `init` wrote in the working folder, relative to it: `.gitignore` and/or `paper.toml`.
    pub written: Vec<String>,
    /// `true` when `written` went into the first save, `false` when it is left in the draft
    /// as unsaved changes (adopting a repository that already has saves).
    pub written_in_first_save: bool,
    /// The profile's tips.
    pub tips: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("another paper command is running in this project")]
    Busy {
        /// The holder's process id, when known.
        holder: Option<u32>,
    },
    #[error("{path} is already a paper project")]
    AlreadyInitialized { path: PathBuf },
    #[error("{path} holds an activity log but no version file")]
    ActivityLogInTheWay { path: PathBuf },
    #[error("{path} is inside the git repository at {root}")]
    InsideAnotherRepository { path: PathBuf, root: PathBuf },
    #[error("{path} has a .git that git cannot open")]
    DamagedGit { path: PathBuf },
    #[error("HEAD is detached, so there is no current branch to start from")]
    DetachedHead,
    #[error("the current branch '{name}' can't be used as a line name")]
    UnusableBranchName { name: String },
    #[error("the current branch '{name}' has no commits yet, but other branches do")]
    EmptyCurrentBranch { name: String },
    #[error("paper.toml says the profile is '{existing}', but '{requested}' was asked for")]
    ProfileConflict { requested: String, existing: String },
    #[error(transparent)]
    Gitignore(#[from] GitignoreError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    Activity(#[from] ActivityError),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error("could not write {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Makes `path` a paper project: creates a new repository with a first save, or
    /// adopts the existing repository at `path` without rewriting anything.
    ///
    /// `.git/paper/version` is written last, so an interrupted `init` can simply be run again.
    ///
    /// # Errors
    ///
    /// See [`InitError`]. In particular, `path` must not be a subfolder of another
    /// repository, and an adopted repository must have a current branch.
    pub fn init(
        path: &Path,
        identity: Identity,
        options: &SnapshotOptions,
    ) -> Result<(Self, Initialized), InitError> {
        Self::init_with_profile(path, identity, options, None)
    }
}

impl Project {
    /// Like [`Project::init`], with the profile choice: `Some(profile)` is "chosen" (reported
    /// with `detected == false`), `None` detects it from the files the snapshot walk sees.
    ///
    /// New project: `.gitignore` (profile block) and `paper.toml` (`profile = "<name>"`) are
    /// written before the first save, so the save contains both and none of the ignored files.
    /// Adoption: history is never rewritten; the block and `paper.toml` (when it has no
    /// `profile` yet) are written to the working files and show up in the draft. An existing
    /// `profile` in `paper.toml` wins over detection.
    ///
    /// # Errors
    ///
    /// Everything [`Project::init`] reports, plus [`InitError::ProfileConflict`] when `choice`
    /// disagrees with the `profile` in `paper.toml`, and [`InitError::Gitignore`] for a
    /// malformed paper-vcs block; in both cases nothing is written.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn init_with_profile(
        path: &Path,
        identity: Identity,
        options: &SnapshotOptions,
        choice: Option<&Profile>,
    ) -> Result<(Self, Initialized), InitError> {
        Self::init_with(path, identity, options, choice, &LockOptions::default())
    }

    /// Like [`Project::init_with_profile`], waiting at most `lock.wait` for another paper
    /// command. The lock is taken once `.git/paper/` exists, before anything else is checked
    /// (so a busy project reports `Busy` rather than `AlreadyInitialized`), and the returned
    /// project holds it until dropped.
    ///
    /// # Errors
    ///
    /// [`InitError::Busy`] when another command holds the project after the wait, and
    /// everything [`Project::init_with_profile`] reports.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn init_with(
        path: &Path,
        identity: Identity,
        options: &SnapshotOptions,
        choice: Option<&Profile>,
        lock: &LockOptions,
    ) -> Result<(Self, Initialized), InitError> {
        fs::create_dir_all(path).map_err(|source| io_error(path, source))?;
        if has_damaged_git(path) {
            return Err(InitError::DamagedGit {
                path: path.to_owned(),
            });
        }
        match GitRepo::enclosing_workdir(path) {
            None => {
                let plan = ProfilePlan::prepare(path, choice)?;
                let repo = GitRepo::init(path, &identity)?;
                let paper_dir = create_paper_dir(&repo)?;
                let held = take_lock(&paper_dir, lock)?;
                // Another init may have finished while this one waited for the lock.
                refuse_if_initialized(&held, path)?;
                let report = plan.reprepare(path, choice)?.write(path)?;
                start_fresh(
                    repo,
                    identity,
                    options,
                    LineName::main(),
                    false,
                    report,
                    held,
                )
            }
            Some(root) if same_dir(&root, path) => adopt(path, identity, options, choice, lock),
            Some(root) => Err(InitError::InsideAnotherRepository {
                path: path.to_owned(),
                root,
            }),
        }
    }
}

fn adopt(
    path: &Path,
    identity: Identity,
    options: &SnapshotOptions,
    choice: Option<&Profile>,
    lock: &LockOptions,
) -> Result<(Project, Initialized), InitError> {
    let repo = GitRepo::open(path, &identity)?;
    let dir = paper_dir(&repo);
    // An existing `.git/paper/` is locked first, so a busy project reports `Busy` rather than
    // `AlreadyInitialized`. Otherwise every read-only check runs before the folder is created:
    // a refused adoption leaves nothing behind, and the lock file is never deleted.
    let early = if dir.is_dir() {
        let held = take_lock(&dir, lock)?;
        refuse_if_initialized(&held, path)?;
        Some(held)
    } else {
        None
    };
    let current = repo.head_branch()?.ok_or(InitError::DetachedHead)?;
    let current_line: LineName = current
        .parse()
        .map_err(|_| InitError::UnusableBranchName { name: current })?;
    let branches = repo.local_branches()?;
    if !branches.is_empty()
        && !branches
            .iter()
            .any(|(branch, _)| *branch == current_line.as_str())
    {
        return Err(InitError::EmptyCurrentBranch {
            name: current_line.to_string(),
        });
    }
    let plan = ProfilePlan::prepare(path, choice)?;
    let held = if let Some(held) = early {
        held
    } else {
        let held = take_lock(&create_paper_dir(&repo)?, lock)?;
        // Another init may have finished while this one was checking.
        refuse_if_initialized(&held, path)?;
        held
    };
    // A `paper config set --project` may have landed since `prepare`: plan again, now locked.
    let plan = plan.reprepare(path, choice)?;
    // The block and `paper.toml` go to the working files before the snapshot, so they are part
    // of the draft; nothing is rewritten in history.
    let report = plan.write(path)?;
    if branches.is_empty() {
        // A repository without commits yet: start like a new project, on its current branch.
        return start_fresh(repo, identity, options, current_line, true, report, held);
    }

    let signature = signature_now(&identity);
    let mut lines = Vec::new();
    let mut skipped_branches = Vec::new();
    let mut first_snapshot = None;
    for (branch, tip) in branches {
        let Ok(line) = branch.parse::<LineName>() else {
            skipped_branches.push(branch);
            continue;
        };
        let tip_tree = repo.read_commit(&tip)?.tree;
        // Uncommitted work belongs to the current line's draft; other lines start clean.
        let draft_tree = if line == current_line {
            let snap = snapshot(&repo, &tip_tree, &StatCache::default(), options)?;
            let tree = snap.tree.clone();
            first_snapshot = Some(snap);
            tree
        } else {
            tip_tree
        };
        let draft = write_draft(&repo, &draft_tree, &tip, &signature)?;
        lines.push((line, Line { tip, draft }));
    }

    let snap = first_snapshot.expect("the current branch exists, so it was snapshotted");
    let (current, others): (Vec<_>, Vec<_>) = lines
        .into_iter()
        .partition(|(name, _)| *name == current_line);
    let (current_name, current) = current
        .into_iter()
        .next()
        .expect("the current branch exists, so it was adopted");
    let view = others
        .into_iter()
        .fold(View::new(current_name, current), |view, (name, line)| {
            view.with_line(name, line)
        });

    // An adopted repository keeps the index it has (staged changes stay staged).
    let reset_index = false;
    let initialized = Initialized {
        adopted: true,
        tracked_files: repo.tree_files(&snap.tree)?.len(),
        skipped: snap.skipped,
        large: snap.large,
        skipped_branches,
        profile: report.profile,
        profile_source: report.profile_source,
        written: report.written,
        written_in_first_save: false,
        tips: report.tips,
    };
    finish(
        repo,
        identity,
        *options,
        held,
        First {
            view,
            cache: &snap.cache,
            reset_index,
        },
        initialized,
    )
}

/// First save for a repository without commits: everything in the folder becomes
/// "Start project", on top of which an empty draft is created.
fn start_fresh(
    repo: GitRepo,
    identity: Identity,
    options: &SnapshotOptions,
    line: LineName,
    adopted: bool,
    report: ProfileReport,
    held: Held,
) -> Result<(Project, Initialized), InitError> {
    let empty = repo.empty_tree()?;
    let snap = snapshot(&repo, &empty, &StatCache::default(), options)?;
    let signature = signature_now(&identity);
    let save = first_save(&repo, &snap.tree, &signature)?;
    let draft = write_draft(&repo, &snap.tree, &save, &signature)?;

    // The first save is the whole folder, so the index is made to match it.
    let reset_index = true;
    let initialized = Initialized {
        adopted,
        tracked_files: repo.tree_files(&snap.tree)?.len(),
        skipped: snap.skipped,
        large: snap.large,
        skipped_branches: Vec::new(),
        profile: report.profile,
        profile_source: report.profile_source,
        written: report.written,
        written_in_first_save: true,
        tips: report.tips,
    };
    let view = View::new(line, Line { tip: save, draft });
    finish(
        repo,
        identity,
        *options,
        held,
        First {
            view,
            cache: &snap.cache,
            reset_index,
        },
        initialized,
    )
}

fn first_save(
    repo: &GitRepo,
    tree: &TreeId,
    signature: &paper_git::Signature,
) -> Result<CommitId, GitError> {
    repo.write_commit(&NewCommit {
        tree: tree.clone(),
        parents: Vec::new(),
        author: signature.clone(),
        committer: signature.clone(),
        message: "Start project".to_owned(),
        change_id: Some(ChangeId::random()),
    })
}

/// What the first op records, and how git is brought in line with it.
struct First<'a> {
    view: View,
    cache: &'a StatCache,
    /// Whether the index is rewritten from the first tip (not for an adopted repository, which
    /// keeps the index it has).
    reset_index: bool,
}

/// Records the first op and the cache, then writes the version file: the point of no return.
fn finish(
    repo: GitRepo,
    identity: Identity,
    options: SnapshotOptions,
    held: Held,
    first: First<'_>,
    initialized: Initialized,
) -> Result<(Project, Initialized), InitError> {
    let Held { paper_dir, lock } = held;
    let First {
        view,
        cache,
        reset_index,
    } = first;
    // A previous, interrupted `init` may have left its op behind (checked safe to remove by
    // `refuse_if_initialized`); this run starts the log over.
    let activity = ActivityLog::open(&paper_dir);
    if activity.root().exists() {
        fs::remove_dir_all(activity.root()).map_err(|source| io_error(activity.root(), source))?;
    }
    let description = if initialized.adopted {
        "adopt the git repository"
    } else {
        "start the project"
    };
    let keep = write_keep(&repo, &identity, None, &view).map_err(ExportError::from)?;
    activity.append(OpKind::Init, &view, description)?;
    activity.make_durable()?;
    export(
        &repo,
        &paper_dir,
        &view,
        &keep,
        &Mode::Fresh { reset_index },
    )?;
    cache
        .save(&paper_dir)
        .map_err(|source| io_error(&paper_dir, source))?;
    let version_path = paper_dir.join(VERSION_FILE);
    fs::write(&version_path, format!("{FORMAT_VERSION}\n"))
        .map_err(|source| io_error(&version_path, source))?;
    let project = Project {
        repo,
        paper_dir,
        view,
        identity,
        options,
        _lock: lock,
    };
    Ok((project, initialized))
}

/// `.git/paper/` together with the lock taken on it.
struct Held {
    paper_dir: PathBuf,
    lock: ProjectLock,
}

/// Refuses a folder that is already a project, or whose `activity/` is more than the leftover
/// of an interrupted `init` (no head, or a head that is a lone `init` op): that log is real
/// history, and `init` would delete it.
fn refuse_if_initialized(held: &Held, path: &Path) -> Result<(), InitError> {
    if held.paper_dir.join(VERSION_FILE).exists() {
        return Err(InitError::AlreadyInitialized {
            path: path.to_owned(),
        });
    }
    let log = ActivityLog::open(&held.paper_dir);
    let leftover = match log.head() {
        Ok(None) => log.op_file_count() <= 1,
        Ok(Some(tip)) => log
            .read(&tip)
            .is_ok_and(|op| op.kind == OpKind::Init && op.parent.is_none()),
        Err(_) => false,
    };
    if leftover {
        Ok(())
    } else {
        Err(InitError::ActivityLogInTheWay {
            path: log.root().to_owned(),
        })
    }
}

fn take_lock(paper_dir: &Path, options: &LockOptions) -> Result<Held, InitError> {
    let lock = ProjectLock::acquire(paper_dir, options.wait).map_err(|error| match error {
        LockError::Busy { holder } => InitError::Busy { holder },
        LockError::Io { path, source } => InitError::Io { path, source },
    })?;
    Ok(Held {
        paper_dir: paper_dir.to_owned(),
        lock,
    })
}

fn create_paper_dir(repo: &GitRepo) -> Result<PathBuf, InitError> {
    let dir = paper_dir(repo);
    fs::create_dir_all(&dir).map_err(|source| io_error(&dir, source))?;
    Ok(dir)
}

/// Whether `target` has its own `.git` that git cannot open (empty, half deleted, or a file
/// pointing at a missing folder). Git's search skips such a `.git` and finds a repository
/// further up, so the folder looks like it is inside another project; this tells the two
/// apart: `target/.git` exists, yet the repository found from `target` does not start at
/// `target`. (Reading the git settings fails the same way, see `open_repository` in
/// `paper-git`, so callers check this before reading them.)
pub fn has_damaged_git(target: &Path) -> bool {
    target.join(".git").exists()
        && !GitRepo::enclosing_workdir(target).is_some_and(|root| same_dir(&root, target))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn io_error(path: &Path, source: io::Error) -> InitError {
    InitError::Io {
        path: path.to_owned(),
        source,
    }
}

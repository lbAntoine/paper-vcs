//! `paper init`: start a new project, or adopt an existing git repository
//! (02-cli.md § paper init).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use paper_git::{ChangeId, CommitId, GitError, GitRepo, Identity, NewCommit, TreeId};

use super::{
    FORMAT_VERSION, Project, VERSION_FILE, draft_ref, line_ref, paper_dir, signature_now,
    write_draft,
};
use crate::config::ConfigError;
use crate::name::LineName;
use crate::profile::{Profile, gitignore::GitignoreError};
use crate::snapshot::{Skipped, SnapshotError, SnapshotOptions, StatCache, snapshot};
use crate::view::{Line, View, ViewError};

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
    /// `true` when the profile was detected from the files, `false` when chosen (by the caller
    /// or by an existing `profile` in `paper.toml`).
    pub detected: bool,
    /// Files `init` wrote in the working folder, relative to it: `.gitignore` and/or `paper.toml`.
    pub written: Vec<String>,
    /// The profile's tips.
    pub tips: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("{path} is already a paper project")]
    AlreadyInitialized { path: PathBuf },
    #[error("{path} is inside the git repository at {root}")]
    InsideAnotherRepository { path: PathBuf, root: PathBuf },
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
    View(#[from] ViewError),
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
        // RED stub: GREEN makes this `init_with_profile(.., None)`.
        fs::create_dir_all(path).map_err(|source| io_error(path, source))?;
        match GitRepo::enclosing_workdir(path) {
            None => {
                let repo = GitRepo::init(path, &identity)?;
                start_fresh(repo, identity, options, LineName::main(), false)
            }
            Some(root) if same_dir(&root, path) => adopt(path, identity, options),
            Some(root) => Err(InitError::InsideAnotherRepository {
                path: path.to_owned(),
                root,
            }),
        }
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
        _path: &Path,
        _identity: Identity,
        _options: &SnapshotOptions,
        _choice: Option<&Profile>,
    ) -> Result<(Self, Initialized), InitError> {
        todo!("PV-004: profiles in init")
    }
}

fn adopt(
    path: &Path,
    identity: Identity,
    options: &SnapshotOptions,
) -> Result<(Project, Initialized), InitError> {
    let repo = GitRepo::open(path, &identity)?;
    if paper_dir(&repo).join(VERSION_FILE).exists() {
        return Err(InitError::AlreadyInitialized {
            path: path.to_owned(),
        });
    }
    let current = repo.head_branch()?.ok_or(InitError::DetachedHead)?;
    let current_line: LineName = current
        .parse()
        .map_err(|_| InitError::UnusableBranchName { name: current })?;
    let branches = repo.local_branches()?;
    if branches.is_empty() {
        // A repository without commits yet: start like a new project, on its current branch.
        return start_fresh(repo, identity, options, current_line, true);
    }
    if !branches
        .iter()
        .any(|(branch, _)| *branch == current_line.as_str())
    {
        return Err(InitError::EmptyCurrentBranch {
            name: current_line.to_string(),
        });
    }

    let paper_dir = create_paper_dir(&repo)?;
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
        repo.set_ref(&draft_ref(&line), &draft, "paper: adopt branch")?;
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

    let initialized = Initialized {
        adopted: true,
        tracked_files: repo.tree_files(&snap.tree)?.len(),
        skipped: snap.skipped,
        large: snap.large,
        skipped_branches,
        // RED stub: GREEN fills these in.
        profile: String::new(),
        detected: false,
        written: Vec::new(),
        tips: Vec::new(),
    };
    finish(
        repo,
        identity,
        *options,
        paper_dir,
        view,
        &snap.cache,
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
) -> Result<(Project, Initialized), InitError> {
    let paper_dir = create_paper_dir(&repo)?;
    let empty = repo.empty_tree()?;
    let snap = snapshot(&repo, &empty, &StatCache::default(), options)?;
    let signature = signature_now(&identity);
    let save = first_save(&repo, &snap.tree, &signature)?;
    let draft = write_draft(&repo, &snap.tree, &save, &signature)?;

    repo.set_ref(&line_ref(&line), &save, "paper: start project")?;
    repo.set_ref(&draft_ref(&line), &draft, "paper: start project")?;
    repo.set_head_branch(line.as_str(), "paper: start project")?;
    repo.reset_index_to(&snap.tree)?;

    let initialized = Initialized {
        adopted,
        tracked_files: repo.tree_files(&snap.tree)?.len(),
        skipped: snap.skipped,
        large: snap.large,
        skipped_branches: Vec::new(),
        // RED stub: GREEN fills these in.
        profile: String::new(),
        detected: false,
        written: Vec::new(),
        tips: Vec::new(),
    };
    let view = View::new(line, Line { tip: save, draft });
    finish(
        repo,
        identity,
        *options,
        paper_dir,
        view,
        &snap.cache,
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

/// Persists the view and cache, then writes the version file: the point of no return.
fn finish(
    repo: GitRepo,
    identity: Identity,
    options: SnapshotOptions,
    paper_dir: PathBuf,
    view: View,
    cache: &StatCache,
    initialized: Initialized,
) -> Result<(Project, Initialized), InitError> {
    view.save(&paper_dir)?;
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
    };
    Ok((project, initialized))
}

fn create_paper_dir(repo: &GitRepo) -> Result<PathBuf, InitError> {
    let dir = paper_dir(repo);
    fs::create_dir_all(&dir).map_err(|source| io_error(&dir, source))?;
    Ok(dir)
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

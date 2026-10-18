//! `paper ignore`: stop tracking files and folders, even saved ones, by writing rules in
//! paper's own `.gitignore` block (02-cli.md § paper ignore).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use paper_git::{GitError, NewCommit, TreeEdit};

use super::{ActivityEntry, ExportError, Project, SaveError, signature_now};
use crate::activity::{ActivityLog, OpKind, describe, short_ids_among};
use crate::fs_util::write_atomic;
use crate::profile::gitignore::{self, BlockKind, GitignoreError};
use crate::view::Line;

/// Gitignore metacharacters that make an argument a pattern rather than a name.
const GLOB_CHARS: [char; 3] = ['*', '?', '['];

/// More files than this leaving the draft need `--yes`.
const UNTRACK_WITHOUT_YES: usize = 20;

/// What to ignore, and from where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreRequest {
    /// The arguments as typed: paths (relative to `from`) and patterns such as `*.aux`.
    pub args: Vec<String>,
    /// The folder paper runs in, relative to the project folder with `/` separators; empty at
    /// the top.
    pub from: String,
    /// Go ahead when more than 20 saved files would stop being tracked.
    pub yes: bool,
}

/// What `paper ignore <args>` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreOutcome {
    /// Rules written to paper's block, in the order added.
    pub added: Vec<String>,
    /// Rules that were already in paper's block.
    pub already: Vec<String>,
    /// The rules of `added` and `already` that came from a pattern, not a path: they apply to
    /// the whole project wherever paper runs.
    pub patterns: Vec<String>,
    /// Saved files that left the draft (still on disk, byte-identical), sorted.
    pub untracked: Vec<String>,
    /// The activity entry recorded; `None` when there was nothing to do.
    pub entry: Option<ActivityEntry>,
}

/// Where an ignore rule comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreSource {
    /// Paper's own block.
    Paper,
    /// The profile block.
    Profile,
    /// Other lines of the root `.gitignore`, and nested `.gitignore` files.
    Yours,
    /// `.git/info/exclude`.
    Local,
    /// Git's `core.excludesFile`, or the XDG default `git/ignore`.
    Global,
}

impl IgnoreSource {
    /// `paper`, `profile`, `yours`, `local` or `global`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Profile => "profile",
            Self::Yours => "yours",
            Self::Local => "local",
            Self::Global => "global",
        }
    }
}

/// One ignore rule that applies, with where it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreRule {
    /// The rule as written, without the line ending.
    pub rule: String,
    pub source: IgnoreSource,
    /// Relative to the project folder for `.gitignore` files (`docs/.gitignore`) and
    /// `.git/info/exclude`; the path of git's global ignore file for `global`.
    pub file: String,
    /// 1-based line in `file`.
    pub line: usize,
}

/// Every ignore rule that applies, plus the sources paper could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreListing {
    /// Grouped paper, profile, yours, local, global.
    pub rules: Vec<IgnoreRule>,
    /// Files that exist but could not be read (not UTF-8 text, no permission): their rules are
    /// not in `rules`. The source and the file, as in [`IgnoreRule`].
    pub unreadable: Vec<(IgnoreSource, String)>,
}

/// Why `paper ignore` did nothing.
#[derive(Debug, thiserror::Error)]
pub enum IgnoreError {
    #[error("an empty name can't be ignored")]
    Empty,
    #[error("{arg} starts with '!', which would stop ignoring something")]
    Unignore { arg: String },
    #[error("{path} can't be ignored: paper needs it")]
    Protected { path: String },
    #[error("{arg} is inside .git, which paper never tracks")]
    InsideGit { arg: String },
    #[error("{arg} is outside this project")]
    Outside { arg: String },
    #[error("the whole project can't be ignored")]
    WholeProject,
    #[error("{count} saved files would stop being tracked")]
    TooMany {
        count: usize,
        /// The first five, sorted.
        first: Vec<String>,
        /// The arguments as typed, so the error can suggest the same command with `--yes`.
        args: Vec<String>,
    },
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Gitignore(#[from] GitignoreError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Export(#[from] ExportError),
    #[error("could not read {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl Project {
    /// Writes a rule for each argument in paper's `.gitignore` block, and takes the saved
    /// files that now match out of the draft (they stay on disk). Records one `ignore` entry.
    ///
    /// Order: arguments are checked, unsaved edits are recorded (I1), everything is decided
    /// (rules, files to untrack, the threshold), then `.gitignore` is written atomically and
    /// one entry is recorded. A crash in between leaves the rule on disk with the files still
    /// tracked; running the same command again finishes the job.
    ///
    /// # Errors
    ///
    /// [`IgnoreError`]; nothing is written when it is returned (unsaved edits recorded before
    /// a refusal that needs the draft are the exception, as for every command).
    ///
    /// # Panics
    ///
    /// Never.
    pub fn ignore(&mut self, request: &IgnoreRequest) -> Result<IgnoreOutcome, IgnoreError> {
        let workdir = self.repo.workdir().to_owned();
        let parsed = request
            .args
            .iter()
            .map(|arg| Arg::parse(arg, &request.from, &workdir))
            .collect::<Result<Vec<_>, _>>()?;

        // I1: unsaved edits become their own entry first.
        self.snapshot()?;
        crate::fault::crash_point("after-snapshot");
        let plan = self.plan_ignore(request, &parsed)?;
        if plan.added.is_empty() && plan.untracked.is_empty() {
            return Ok(plan.outcome(None));
        }
        let entry = self.write_ignore(&plan)?;
        Ok(plan.outcome(Some(entry)))
    }

    /// Decides everything (rules, files to untrack, the threshold) without writing.
    fn plan_ignore(
        &self,
        request: &IgnoreRequest,
        parsed: &[Arg],
    ) -> Result<IgnorePlan, IgnoreError> {
        let workdir = self.repo.workdir();
        let draft = self.repo.read_commit(&self.view.current().draft)?;
        let draft_files = self.repo.tree_files(&draft.tree)?;

        let mut named: Vec<(String, bool)> = Vec::new();
        for arg in parsed {
            let (rule, is_pattern) = arg.rule(workdir, &draft_files);
            if !named.iter().any(|(seen, _)| *seen == rule) {
                named.push((rule, is_pattern));
            }
        }
        let existing = gitignore::read(workdir)?;
        let present = gitignore::paper_rules(existing.as_deref())?;
        let (already, added): (Vec<_>, Vec<_>) = named
            .iter()
            .map(|(rule, _)| rule.clone())
            .partition(|rule| present.contains(rule));
        let patterns = named
            .iter()
            .filter(|(_, is_pattern)| *is_pattern)
            .map(|(rule, _)| rule.clone())
            .collect();
        let new_text = gitignore::add_rules(existing.as_deref(), &added)?;

        let rules_text = new_text
            .as_deref()
            .or(existing.as_deref())
            .unwrap_or_default();
        let layers = Layers::build(&self.repo, rules_text);
        let wanted: BTreeSet<&str> = named.iter().map(|(rule, _)| rule.as_str()).collect();
        let mut memo = HashMap::new();
        let untracked: Vec<String> = draft_files
            .keys()
            .filter(|path| !is_protected_file(path))
            .filter(|path| {
                layers
                    .decide(path, false, &mut memo)
                    .is_some_and(|rule| wanted.contains(rule.as_str()))
            })
            .cloned()
            .collect();
        if untracked.len() > UNTRACK_WITHOUT_YES && !request.yes {
            return Err(IgnoreError::TooMany {
                count: untracked.len(),
                first: untracked.iter().take(5).cloned().collect(),
                args: request.args.clone(),
            });
        }
        Ok(IgnorePlan {
            added,
            already,
            patterns,
            untracked,
            new_text,
            draft,
        })
    }

    /// Writes `.gitignore`, then records the new draft as one `ignore` entry.
    fn write_ignore(&mut self, plan: &IgnorePlan) -> Result<ActivityEntry, IgnoreError> {
        let mut edits: Vec<TreeEdit> = plan
            .untracked
            .iter()
            .map(|path| TreeEdit::Remove { path: path.clone() })
            .collect();
        if let Some(text) = &plan.new_text {
            edits.push(TreeEdit::Upsert {
                path: GITIGNORE.to_owned(),
                blob: self.repo.write_blob(text.as_bytes())?,
                executable: false,
            });
        }
        let draft = &plan.draft;
        let rewritten = self.repo.write_commit(&NewCommit {
            tree: self.repo.edit_tree(&draft.tree, &edits)?,
            parents: draft.parents.clone(),
            author: draft.author.clone(),
            committer: signature_now(&self.identity),
            message: draft.message.clone(),
            change_id: draft.change_id,
        })?;

        if let Some(text) = &plan.new_text {
            let path = self.repo.workdir().join(GITIGNORE);
            write_atomic(&path, text.as_bytes())
                .map_err(|source| GitignoreError::Write { path, source })?;
        }

        let view = self.view.with_line(
            self.view.current_line().clone(),
            Line {
                tip: self.view.current().tip.clone(),
                draft: rewritten,
            },
        );
        let spoken = if plan.added.is_empty() {
            &plan.already
        } else {
            &plan.added
        };
        let op = self.record_at(OpKind::Ignore, view, &describe::ignored(spoken), None)?;
        let log = ActivityLog::open(&self.paper_dir);
        let short = short_ids_among(
            std::slice::from_ref(&op.id),
            &log.op_file_names().map_err(ExportError::Activity)?,
        );
        Ok(ActivityEntry {
            id: op.id,
            short_id: short.into_iter().next().unwrap_or_default(),
            time: op.time,
            kind: op.kind,
            description: op.description,
        })
    }

    /// Every ignore rule that applies, grouped by source (paper, profile, yours, local,
    /// global), each with its file and line. Comments and blank lines are skipped. Sources
    /// that can't be read are left out; [`Project::ignore_listing`] names them.
    #[must_use]
    pub fn ignore_rules(&self) -> Vec<IgnoreRule> {
        self.ignore_listing().rules
    }

    /// [`Project::ignore_rules`], and the files that exist but could not be read.
    #[must_use]
    pub fn ignore_listing(&self) -> IgnoreListing {
        let workdir = self.repo.workdir();
        let mut buckets: [Vec<IgnoreRule>; 5] = Default::default();
        let mut unreadable = Vec::new();

        match gitignore::read(workdir) {
            Ok(Some(text)) => {
                for line in gitignore::rule_lines(&text) {
                    let source = match line.block {
                        Some(BlockKind::Paper) => IgnoreSource::Paper,
                        Some(BlockKind::Profile) => IgnoreSource::Profile,
                        None => IgnoreSource::Yours,
                    };
                    buckets[source as usize].push(IgnoreRule {
                        rule: line.rule,
                        source,
                        file: GITIGNORE.to_owned(),
                        line: line.line,
                    });
                }
            }
            Ok(None) => {}
            Err(_) => unreadable.push((IgnoreSource::Yours, GITIGNORE.to_owned())),
        }
        for dir in nested_gitignore_dirs(workdir) {
            let file = format!("{dir}/{GITIGNORE}");
            read_plain(
                &workdir.join(&file),
                &file,
                IgnoreSource::Yours,
                &mut buckets,
                &mut unreadable,
            );
        }
        read_plain(
            &self.repo.git_dir().join("info").join("exclude"),
            ".git/info/exclude",
            IgnoreSource::Local,
            &mut buckets,
            &mut unreadable,
        );
        if let Some(global) = self.repo.global_excludes_file() {
            read_plain(
                &global,
                &global.display().to_string(),
                IgnoreSource::Global,
                &mut buckets,
                &mut unreadable,
            );
        }
        IgnoreListing {
            rules: buckets.into_iter().flatten().collect(),
            unreadable,
        }
    }
}

const GITIGNORE: &str = ".gitignore";

/// Everything `paper ignore` decided before writing.
struct IgnorePlan {
    added: Vec<String>,
    already: Vec<String>,
    patterns: Vec<String>,
    untracked: Vec<String>,
    /// The new root `.gitignore`; `None` when the rules were all present.
    new_text: Option<String>,
    /// The draft commit the plan starts from (after the I1 snapshot).
    draft: paper_git::Commit,
}

impl IgnorePlan {
    fn outcome(&self, entry: Option<ActivityEntry>) -> IgnoreOutcome {
        IgnoreOutcome {
            added: self.added.clone(),
            already: self.already.clone(),
            patterns: self.patterns.clone(),
            untracked: self.untracked.clone(),
            entry,
        }
    }
}

/// Reads one plain rules file (no paper blocks) into its bucket. A missing file is no rules.
fn read_plain(
    path: &Path,
    shown: &str,
    source: IgnoreSource,
    buckets: &mut [Vec<IgnoreRule>; 5],
    unreadable: &mut Vec<(IgnoreSource, String)>,
) {
    let text = match fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes).ok(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return,
        Err(_) => None,
    };
    let Some(text) = text else {
        unreadable.push((source, shown.to_owned()));
        return;
    };
    for line in gitignore::rule_lines(&text) {
        buckets[source as usize].push(IgnoreRule {
            rule: line.rule,
            source,
            file: shown.to_owned(),
            line: line.line,
        });
    }
}

/// Files `paper ignore` never takes out of the draft, whatever the rules match: paper's settings
/// and every `.gitignore` (without them the rules would be lost).
fn is_protected_file(path: &str) -> bool {
    path == "paper.toml" || path == GITIGNORE || path.ends_with("/.gitignore")
}

/// One command-line argument, checked.
enum Arg {
    /// Has no gitignore metacharacters (or does, and may still name a file): a name.
    Name {
        parts: Vec<String>,
        typed_slash: bool,
        typed: String,
    },
    /// Has `*`, `?` or `[`: a pattern unless a file of that name exists.
    Maybe {
        parts: Option<Vec<String>>,
        typed: String,
    },
}

impl Arg {
    fn parse(arg: &str, from: &str, workdir: &Path) -> Result<Self, IgnoreError> {
        if arg.is_empty() {
            return Err(IgnoreError::Empty);
        }
        if arg.starts_with('!') {
            return Err(IgnoreError::Unignore {
                arg: arg.to_owned(),
            });
        }
        let parts = resolve(arg, from, workdir);
        if arg.contains(GLOB_CHARS) {
            return Ok(Self::Maybe {
                parts: parts.ok(),
                typed: arg.to_owned(),
            });
        }
        let parts = parts?;
        match parts.as_slice() {
            [] => return Err(IgnoreError::WholeProject),
            [first, ..] if first.eq_ignore_ascii_case(".git") => {
                return Err(IgnoreError::InsideGit {
                    arg: arg.to_owned(),
                });
            }
            [only] if only == GITIGNORE || only == "paper.toml" => {
                return Err(IgnoreError::Protected { path: only.clone() });
            }
            _ => {}
        }
        Ok(Self::Name {
            typed: arg.to_owned(),
            parts,
            typed_slash: arg.ends_with('/') || (cfg!(windows) && arg.ends_with('\\')),
        })
    }

    /// The rule this argument stands for, and whether it is a pattern.
    fn rule(
        &self,
        workdir: &Path,
        draft: &BTreeMap<String, paper_git::TreeFile>,
    ) -> (String, bool) {
        match self {
            Self::Name {
                parts,
                typed_slash,
                typed,
            } => {
                let rule = match exists(workdir, parts, draft) {
                    Some(is_dir) => path_rule(parts, is_dir),
                    // Not there yet: a bare name matches at any depth, as it does in
                    // .gitignore; with a slash in it, it is anchored.
                    None if parts.len() == 1 && !*typed_slash && !typed.contains('/') => {
                        escape(&parts[0])
                    }
                    None => path_rule(parts, *typed_slash),
                };
                (rule, false)
            }
            Self::Maybe { parts, typed } => {
                if let Some(parts) = parts.as_ref().filter(|p| !p.is_empty())
                    && let Some(is_dir) = exists(workdir, parts, draft)
                {
                    return (path_rule(parts, is_dir), false);
                }
                (typed.clone(), true)
            }
        }
    }
}

/// `/a/b/` for a folder, `/a/b` for a file, with metacharacters escaped.
fn path_rule(parts: &[String], folder: bool) -> String {
    let joined = parts
        .iter()
        .map(|p| escape(p))
        .collect::<Vec<_>>()
        .join("/");
    if folder {
        format!("/{joined}/")
    } else {
        format!("/{joined}")
    }
}

/// Backslash-escapes what gitignore reads as syntax inside a name. A leading `!` or `#` needs
/// no escape: every rule starts with the anchoring `/`.
fn escape(name: &str) -> String {
    let body = name.trim_end_matches(' ');
    let mut out = String::with_capacity(name.len());
    for c in body.chars() {
        if matches!(c, '\\' | '*' | '?' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    for _ in 0..name.len() - body.len() {
        out.push_str("\\ ");
    }
    out
}

/// Whether the path exists (on disk, or in the draft), and if so whether it is a folder.
fn exists(
    workdir: &Path,
    parts: &[String],
    draft: &BTreeMap<String, paper_git::TreeFile>,
) -> Option<bool> {
    let joined = parts.join("/");
    if let Ok(meta) = fs::symlink_metadata(workdir.join(&joined)) {
        return Some(meta.is_dir());
    }
    if draft.contains_key(&joined) {
        return Some(false);
    }
    let prefix = format!("{joined}/");
    draft
        .range(prefix.clone()..)
        .next()
        .is_some_and(|(path, _)| path.starts_with(&prefix))
        .then_some(true)
}

/// `arg` typed in `from`, as components relative to the project folder.
fn resolve(arg: &str, from: &str, workdir: &Path) -> Result<Vec<String>, IgnoreError> {
    let outside = || IgnoreError::Outside {
        arg: arg.to_owned(),
    };
    let typed = if cfg!(windows) {
        arg.replace('\\', "/")
    } else {
        arg.to_owned()
    };
    let (mut parts, rest): (Vec<String>, String) = if Path::new(&typed).is_absolute() {
        let inside = Path::new(&typed)
            .strip_prefix(workdir)
            .map_err(|_| outside())?;
        (Vec::new(), inside.to_string_lossy().replace('\\', "/"))
    } else {
        (
            from.split('/')
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect(),
            typed,
        )
    };
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or_else(outside)?;
            }
            other => parts.push(other.to_owned()),
        }
    }
    Ok(parts)
}

/// Directories below the project folder that hold a `.gitignore`, sorted.
fn nested_gitignore_dirs(workdir: &Path) -> Vec<String> {
    let walker = WalkBuilder::new(workdir)
        .standard_filters(false)
        .hidden(false)
        .follow_links(false)
        .sort_by_file_name(Ord::cmp)
        .filter_entry(|entry| {
            entry.file_name() != ".git"
                && !(entry.depth() > 0
                    && entry.file_type().is_some_and(|kind| kind.is_dir())
                    && entry.path().join(".git").exists())
        })
        .build();
    let mut dirs = Vec::new();
    for entry in walker.flatten() {
        if entry.depth() < 2
            || entry.file_name() != GITIGNORE
            || !entry.file_type().is_some_and(|kind| kind.is_file())
        {
            continue;
        }
        let relative = entry
            .path()
            .parent()
            .and_then(|dir| dir.strip_prefix(workdir).ok())
            .and_then(Path::to_str);
        if let Some(dir) = relative {
            dirs.push(dir.replace('\\', "/"));
        }
    }
    dirs
}

/// All the matchers git consults, in git's order of precedence: nested `.gitignore` files
/// (deepest first), the root one, `.git/info/exclude`, then the global file.
struct Layers {
    workdir: PathBuf,
    nested: BTreeMap<String, Gitignore>,
    root: Gitignore,
    exclude: Gitignore,
    global: Option<Gitignore>,
}

impl Layers {
    /// `root_text` is the root `.gitignore` as it will be after the command.
    fn build(repo: &paper_git::GitRepo, root_text: &str) -> Self {
        let workdir = repo.workdir().to_owned();
        let mut root = GitignoreBuilder::new(&workdir);
        for line in root_text.split('\n') {
            // An unparsable line is skipped, as git skips it.
            let _ = root.add_line(None, line.trim_end_matches('\r'));
        }
        let from_file = |base: &Path, file: &Path| {
            let mut builder = GitignoreBuilder::new(base);
            let _ = builder.add(file);
            builder.build().unwrap_or_else(|_| Gitignore::empty())
        };
        let nested = nested_gitignore_dirs(&workdir)
            .into_iter()
            .map(|dir| {
                let base = workdir.join(&dir);
                let matcher = from_file(&base, &base.join(GITIGNORE));
                (dir, matcher)
            })
            .collect();
        Self {
            nested,
            root: root.build().unwrap_or_else(|_| Gitignore::empty()),
            exclude: from_file(&workdir, &repo.git_dir().join("info").join("exclude")),
            global: repo
                .global_excludes_file()
                .map(|file| from_file(&workdir, &file)),
            workdir,
        }
    }

    /// The text of the rule that makes `rel` ignored (itself, or a folder above it), or `None`.
    /// `memo` remembers the answer for folders.
    fn decide(
        &self,
        rel: &str,
        is_dir: bool,
        memo: &mut HashMap<String, Option<String>>,
    ) -> Option<String> {
        if is_dir && let Some(known) = memo.get(rel) {
            return known.clone();
        }
        let parent = rel.rfind('/').map(|at| &rel[..at]);
        let answer = parent
            .and_then(|parent| self.decide(parent, true, memo))
            .or_else(|| match self.check(rel, is_dir) {
                Match::Ignore(glob) => Some(glob.original().to_owned()),
                Match::None | Match::Whitelist(_) => None,
            });
        if is_dir {
            memo.insert(rel.to_owned(), answer.clone());
        }
        answer
    }

    /// The first matcher with an opinion wins, deepest `.gitignore` first.
    fn check(&self, rel: &str, is_dir: bool) -> Match<&ignore::gitignore::Glob> {
        let absolute = self.workdir.join(rel);
        let mut dir = rel;
        while let Some(at) = dir.rfind('/') {
            dir = &dir[..at];
            if let Some(matcher) = self.nested.get(dir) {
                let found = matcher.matched(&absolute, is_dir);
                if !found.is_none() {
                    return found;
                }
            }
        }
        for matcher in [Some(&self.root), Some(&self.exclude), self.global.as_ref()]
            .into_iter()
            .flatten()
        {
            let found = matcher.matched(&absolute, is_dir);
            if !found.is_none() {
                return found;
            }
        }
        Match::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_escaped_where_gitignore_reads_syntax() {
        assert_eq!(escape("a[1].log"), r"a\[1\].log");
        assert_eq!(escape("a*b?c"), r"a\*b\?c");
        assert_eq!(escape(r"back\slash"), r"back\\slash");
        assert_eq!(escape("plain.txt"), "plain.txt");
        assert_eq!(escape("end  "), r"end\ \ ");
    }

    #[test]
    fn a_path_is_resolved_from_where_paper_runs() {
        let root = Path::new("/project");
        let parts = |arg, from| resolve(arg, from, root).unwrap();
        assert_eq!(parts("raw", "data"), ["data", "raw"]);
        assert_eq!(parts("../notes.txt", "data"), ["notes.txt"]);
        assert_eq!(parts("./a//b/", ""), ["a", "b"]);
        assert!(parts(".", "").is_empty());
        assert!(resolve("../x", "", root).is_err());
        assert!(resolve("../../x", "data", root).is_err());
    }

    #[test]
    fn protected_files_are_never_untracked() {
        for path in [".gitignore", "docs/.gitignore", "paper.toml"] {
            assert!(is_protected_file(path), "{path}");
        }
        assert!(!is_protected_file("docs/paper.toml"));
        assert!(!is_protected_file("notes.gitignore"));
    }
}

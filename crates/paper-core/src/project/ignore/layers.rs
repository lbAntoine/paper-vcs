//! The matchers git consults to decide whether a path is ignored.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use paper_git::GitError;

use super::listing::nested_gitignore_dirs;
use super::{GITIGNORE, IgnoreMatch, IgnoreSource};
use crate::profile::gitignore::{self, BlockKind};
use crate::project::Project;

impl Project {
    /// The files of `current` that `wanted` does not track and that the `.gitignore` in
    /// `wanted` ignores: undo and redo leave these on disk.
    pub(in crate::project) fn ignored_in(
        &self,
        current: &BTreeMap<String, paper_git::TreeFile>,
        wanted: &BTreeMap<String, paper_git::TreeFile>,
    ) -> Result<BTreeSet<String>, GitError> {
        let gone: Vec<&String> = current
            .keys()
            .filter(|p| !wanted.contains_key(*p))
            .collect();
        if gone.is_empty() {
            return Ok(BTreeSet::new());
        }
        let layers = Layers::from_tree(&self.repo, wanted, &Overrides::default())?;
        let mut memo = HashMap::new();
        Ok(gone
            .into_iter()
            .filter(|path| layers.decide(path, false, &mut memo).is_some())
            .cloned()
            .collect())
    }
}

/// Texts to use in place of what is on disk, to ask what the rules would decide after a change.
#[derive(Default)]
pub(super) struct Overrides<'a> {
    /// The root `.gitignore`.
    pub root: Option<&'a str>,
    /// `.git/info/exclude`.
    pub exclude: Option<&'a str>,
}

/// The rules files, as text, in one place.
struct Texts {
    /// Nested `.gitignore` files by folder.
    nested: BTreeMap<String, String>,
    root: String,
    exclude: String,
    /// Git's global excludes file, as shown, with its text.
    global: Option<(String, String)>,
}

/// All the matchers git consults, in git's order of precedence: nested `.gitignore` files
/// (deepest first), the root one, `.git/info/exclude`, then the global file.
pub(super) struct Layers {
    workdir: PathBuf,
    nested: BTreeMap<String, Gitignore>,
    root: Gitignore,
    exclude: Gitignore,
    global: Gitignore,
    texts: Texts,
}

pub(super) fn matcher(base: &Path, text: &str) -> Gitignore {
    let mut builder = GitignoreBuilder::new(base);
    for line in text.split('\n') {
        // An unparsable line is skipped, as git skips it.
        let _ = builder.add_line(None, line.trim_end_matches('\r'));
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

fn read_lossy(path: &Path) -> String {
    fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

impl Layers {
    /// The matchers for the `.gitignore` files in `files` (a tree's files), with the root
    /// file and `.git/info/exclude` replaced where `overrides` says so, plus git's global
    /// excludes as they are on disk (the same layers the snapshot walker uses).
    pub(super) fn from_tree(
        repo: &paper_git::GitRepo,
        files: &BTreeMap<String, paper_git::TreeFile>,
        overrides: &Overrides<'_>,
    ) -> Result<Self, GitError> {
        let mut nested = BTreeMap::new();
        let mut root = String::new();
        for (path, file) in files {
            if path == GITIGNORE {
                root = String::from_utf8_lossy(&repo.read_blob(&file.blob)?).into_owned();
            } else if let Some(dir) = path.strip_suffix("/.gitignore") {
                let text = String::from_utf8_lossy(&repo.read_blob(&file.blob)?).into_owned();
                nested.insert(dir.to_owned(), text);
            }
        }
        Ok(Self::from_texts(repo, nested, root, overrides))
    }

    /// The matchers for the files as they are on disk now (what `git check-ignore` reads).
    pub(super) fn from_disk(repo: &paper_git::GitRepo, overrides: &Overrides<'_>) -> Self {
        let workdir = repo.workdir();
        let nested = nested_gitignore_dirs(workdir)
            .into_iter()
            .map(|dir| {
                let text = read_lossy(&workdir.join(&dir).join(GITIGNORE));
                (dir, text)
            })
            .collect();
        let root = read_lossy(&workdir.join(GITIGNORE));
        Self::from_texts(repo, nested, root, overrides)
    }

    fn from_texts(
        repo: &paper_git::GitRepo,
        nested: BTreeMap<String, String>,
        root: String,
        overrides: &Overrides<'_>,
    ) -> Self {
        let workdir = repo.workdir().to_owned();
        let root = overrides.root.map_or(root, str::to_owned);
        let exclude = overrides.exclude.map_or_else(
            || read_lossy(&repo.git_dir().join("info").join("exclude")),
            str::to_owned,
        );
        let global = repo.global_excludes_file().map(|path| {
            let text = read_lossy(&path);
            (path.display().to_string(), text)
        });
        Self {
            nested: nested
                .iter()
                .map(|(dir, text)| (dir.clone(), matcher(&workdir.join(dir), text)))
                .collect(),
            root: matcher(&workdir, &root),
            exclude: matcher(&workdir, &exclude),
            global: Gitignore::global().0,
            texts: Texts {
                nested,
                root,
                exclude,
                global,
            },
            workdir,
        }
    }

    /// The text of the rule that makes `rel` ignored (itself, or a folder above it), or `None`.
    /// `memo` remembers the answer for folders.
    pub(super) fn decide(
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
        for matcher in [&self.root, &self.exclude] {
            let found = matcher.matched(&absolute, is_dir);
            if !found.is_none() {
                return found;
            }
        }
        // The global matcher is rooted at "": it takes the path relative to the project.
        self.global.matched(rel, is_dir)
    }

    /// The rule, file and line that decide `rel`, as `git check-ignore -v` reports them: the
    /// folder rule for a file in an ignored folder, or the negation that brings a path back.
    /// `None` when no rule has an opinion.
    pub(super) fn explain(&self, rel: &str, is_dir: bool) -> Option<IgnoreMatch> {
        let parent = rel.rfind('/').map(|at| &rel[..at]);
        if let Some(found) = parent
            .and_then(|parent| self.explain(parent, true))
            .filter(|found| !found.negated)
        {
            return Some(found);
        }
        self.explain_one(rel, is_dir)
    }

    fn explain_one(&self, rel: &str, is_dir: bool) -> Option<IgnoreMatch> {
        let absolute = self.workdir.join(rel);
        let mut ancestors: Vec<&str> = Vec::new();
        let mut dir = rel;
        while let Some(at) = dir.rfind('/') {
            dir = &dir[..at];
            if self.texts.nested.contains_key(dir) {
                ancestors.push(dir);
            }
        }
        let mut files: Vec<RulesFile<'_>> = ancestors
            .into_iter()
            .map(|dir| RulesFile {
                file: format!("{dir}/{GITIGNORE}"),
                text: &self.texts.nested[dir],
                base: self.workdir.join(dir),
                path: &absolute,
                blocks: false,
                source: IgnoreSource::Yours,
            })
            .collect();
        files.push(RulesFile {
            file: GITIGNORE.to_owned(),
            text: &self.texts.root,
            base: self.workdir.clone(),
            path: &absolute,
            blocks: true,
            source: IgnoreSource::Yours,
        });
        files.push(RulesFile {
            file: ".git/info/exclude".to_owned(),
            text: &self.texts.exclude,
            base: self.workdir.clone(),
            path: &absolute,
            blocks: true,
            source: IgnoreSource::Local,
        });
        let global_path = Path::new(rel);
        if let Some((shown, text)) = &self.texts.global {
            files.push(RulesFile {
                file: shown.clone(),
                text,
                base: PathBuf::new(),
                path: global_path,
                blocks: false,
                source: IgnoreSource::Global,
            });
        }
        files.iter().find_map(|file| file.decide(is_dir))
    }
}

/// One rules file, asked about one path.
struct RulesFile<'a> {
    file: String,
    text: &'a str,
    base: PathBuf,
    path: &'a Path,
    /// Lines may sit in paper's or the profile's block (and are then told apart).
    blocks: bool,
    /// The source of lines outside a block.
    source: IgnoreSource,
}

impl RulesFile<'_> {
    /// The last line of this file that has an opinion on the path.
    fn decide(&self, is_dir: bool) -> Option<IgnoreMatch> {
        for line in gitignore::rule_lines(self.text).into_iter().rev() {
            let one = matcher(&self.base, &line.rule);
            let negated = match one.matched(self.path, is_dir) {
                Match::None => continue,
                Match::Ignore(_) => false,
                Match::Whitelist(_) => true,
            };
            let source = match line.block {
                Some(BlockKind::Paper) if self.blocks => IgnoreSource::Paper,
                Some(BlockKind::Profile) if self.blocks && self.source == IgnoreSource::Yours => {
                    IgnoreSource::Profile
                }
                _ => self.source,
            };
            return Some(IgnoreMatch {
                rule: line.rule,
                source,
                file: self.file.clone(),
                line: line.line,
                negated,
            });
        }
        None
    }
}

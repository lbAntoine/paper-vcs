//! The matchers git consults to decide whether a path is ignored.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use paper_git::GitError;

use super::GITIGNORE;
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
        let layers = Layers::from_tree(&self.repo, wanted, None)?;
        let mut memo = HashMap::new();
        Ok(gone
            .into_iter()
            .filter(|path| layers.decide(path, false, &mut memo).is_some())
            .cloned()
            .collect())
    }
}

/// All the matchers git consults, in git's order of precedence: nested `.gitignore` files
/// (deepest first), the root one, `.git/info/exclude`, then the global file.
pub(super) struct Layers {
    workdir: PathBuf,
    nested: BTreeMap<String, Gitignore>,
    root: Gitignore,
    exclude: Gitignore,
    global: Gitignore,
}

impl Layers {
    /// The matchers for the `.gitignore` files in `files` (a tree's files), with the root one
    /// replaced by `root_text` when given, plus `.git/info/exclude` and git's global excludes
    /// as they are on disk (the same layers the snapshot walker uses).
    pub(super) fn from_tree(
        repo: &paper_git::GitRepo,
        files: &BTreeMap<String, paper_git::TreeFile>,
        root_text: Option<&str>,
    ) -> Result<Self, GitError> {
        let workdir = repo.workdir().to_owned();
        let from_text = |base: &Path, text: &str| {
            let mut builder = GitignoreBuilder::new(base);
            for line in text.split('\n') {
                // An unparsable line is skipped, as git skips it.
                let _ = builder.add_line(None, line.trim_end_matches('\r'));
            }
            builder.build().unwrap_or_else(|_| Gitignore::empty())
        };
        let mut nested = BTreeMap::new();
        let mut root_from_tree = String::new();
        for (path, file) in files {
            if path == GITIGNORE {
                root_from_tree = String::from_utf8_lossy(&repo.read_blob(&file.blob)?).into_owned();
            } else if let Some(dir) = path.strip_suffix("/.gitignore") {
                let text = String::from_utf8_lossy(&repo.read_blob(&file.blob)?).into_owned();
                nested.insert(dir.to_owned(), from_text(&workdir.join(dir), &text));
            }
        }
        let mut exclude = GitignoreBuilder::new(&workdir);
        let _ = exclude.add(repo.git_dir().join("info").join("exclude"));
        Ok(Self {
            nested,
            root: from_text(&workdir, root_text.unwrap_or(&root_from_tree)),
            exclude: exclude.build().unwrap_or_else(|_| Gitignore::empty()),
            global: Gitignore::global().0,
            workdir,
        })
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
}

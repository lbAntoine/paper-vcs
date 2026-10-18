//! `paper unignore`: remove paper's rule or add a negation, then track the files again.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use paper_git::{GitError, TreeFile};

use super::args::{Arg, rule_target};
use super::layers::{Layers, Overrides, matcher};
use super::store::Store;
use super::{IgnoreError, IgnoreMatch, IgnoreSource};
use crate::activity::{OpKind, describe};
use crate::profile::gitignore::{self, GitignoreError};
use crate::project::{ActivityEntry, ExportError, Project, SaveError, Undone};

/// What to unignore, and from where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnignoreRequest {
    /// The arguments as typed: paths (relative to `from`) and patterns such as `*.aux`.
    pub args: Vec<String>,
    /// The folder paper runs in, relative to the project folder with `/` separators; empty at
    /// the top.
    pub from: String,
    /// Work on paper's block in `.git/info/exclude` instead of the root `.gitignore`.
    pub local: bool,
}

/// What `paper unignore <args>` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnignoreOutcome {
    /// Rules taken out of paper's block, in the order given.
    pub removed: Vec<String>,
    /// Negations added to paper's block (`!/notes/keep.log`), in the order given.
    pub negated: Vec<String>,
    /// Files that entered the draft because they are no longer ignored, sorted.
    pub tracked: Vec<String>,
    /// The activity entry recorded; `None` when nothing was ignored.
    pub entry: Option<ActivityEntry>,
    /// Whether the rules live in `.git/info/exclude` (`--local`).
    pub local: bool,
}

/// Why `paper unignore` did nothing.
#[derive(Debug, thiserror::Error)]
pub enum UnignoreError {
    /// The refusals shared with `paper ignore` (empty name, `.git`, outside, protected files,
    /// the whole project) and I/O failures.
    #[error(transparent)]
    Refused(#[from] IgnoreError),
    /// The argument starts with `!`: `paper unignore` adds the exception itself.
    #[error("{arg} starts with '!'")]
    LeadingBang {
        arg: String,
        /// A file with exactly that name exists.
        exists: bool,
    },
    /// After the change the path would still be ignored.
    #[error("{path} would still be ignored")]
    StillIgnored {
        path: String,
        /// The rule that keeps it ignored.
        rule: IgnoreMatch,
        /// The folder to unignore first, when a folder above the path is what ignores it.
        folder: Option<String>,
        /// The command was run with `--local`.
        local: bool,
    },
}

impl From<SaveError> for UnignoreError {
    fn from(error: SaveError) -> Self {
        Self::Refused(error.into())
    }
}

impl From<GitError> for UnignoreError {
    fn from(error: GitError) -> Self {
        Self::Refused(error.into())
    }
}

impl From<GitignoreError> for UnignoreError {
    fn from(error: GitignoreError) -> Self {
        Self::Refused(error.into())
    }
}

impl From<ExportError> for UnignoreError {
    fn from(error: ExportError) -> Self {
        Self::Refused(error.into())
    }
}

/// What `paper unignore` decided before writing.
struct UnignorePlan {
    removed: Vec<String>,
    negated: Vec<String>,
    /// The rules named, as the entry says them.
    spoken: Vec<String>,
    new_text: Option<String>,
    store: Store,
    draft_files: BTreeMap<String, TreeFile>,
}

/// What must hold once the rules are changed.
enum Expect {
    /// This path (folder or not) is not ignored.
    Free(String, bool),
    /// These paths, ignored now, are not ignored.
    Freed(Vec<String>),
}

impl Project {
    /// Removes paper's rule for each argument, or adds a negation to paper's block when
    /// another rule still ignores it, and tracks the files again. Records one `unignore`
    /// entry whose draft holds the changed rules file (when it is `.gitignore`) and the files.
    ///
    /// Order: arguments are checked, unsaved edits are recorded (I1), everything is decided
    /// and simulated, then the rules file is written atomically and the files that are no
    /// longer ignored enter the draft in the same entry. Nothing is written when any argument
    /// would still be ignored afterwards.
    ///
    /// # Errors
    ///
    /// [`UnignoreError`]; nothing is written when it is returned.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn unignore(
        &mut self,
        request: &UnignoreRequest,
    ) -> Result<UnignoreOutcome, UnignoreError> {
        let workdir = self.repo.workdir().to_owned();
        let parsed = request
            .args
            .iter()
            .map(|arg| {
                let parsed = Arg::parse(arg, &request.from, &workdir);
                if arg.starts_with('!') {
                    return Err(UnignoreError::LeadingBang {
                        arg: arg.clone(),
                        exists: parsed.is_ok(),
                    });
                }
                Ok(parsed?)
            })
            .collect::<Result<Vec<_>, UnignoreError>>()?;

        // I1: unsaved edits become their own entry first.
        self.snapshot()?;
        crate::fault::crash_point("after-snapshot");
        let Some(plan) = self.plan_unignore(request, &parsed)? else {
            return Ok(UnignoreOutcome {
                removed: Vec::new(),
                negated: Vec::new(),
                tracked: Vec::new(),
                entry: None,
                local: request.local,
            });
        };
        self.write_unignore(plan)
    }

    /// Decides everything and simulates the result, without writing. `None` when no argument
    /// is ignored.
    fn plan_unignore(
        &self,
        request: &UnignoreRequest,
        parsed: &[Arg],
    ) -> Result<Option<UnignorePlan>, UnignoreError> {
        let workdir = self.repo.workdir();
        let store = Store::of(request.local);
        let draft = self.repo.read_commit(&self.view.current().draft)?;
        let draft_files = self.repo.tree_files(&draft.tree)?;

        let mut named: Vec<(String, bool)> = Vec::new();
        for arg in parsed {
            let (rule, is_pattern) = arg.rule(workdir, &draft_files);
            if !named.iter().any(|(seen, _)| *seen == rule) {
                named.push((rule, is_pattern));
            }
        }
        let existing = store.read(&self.repo)?;
        let present = gitignore::paper_rules(existing.as_deref())?;
        let now = Layers::from_tree(&self.repo, &draft_files, &Overrides::default())?;
        let mut memo = HashMap::new();

        let (mut removed, mut negated, mut spoken) = (Vec::new(), Vec::new(), Vec::new());
        let mut expect: Vec<(String, bool, Expect)> = Vec::new();
        for (rule, is_pattern) in &named {
            let target = if *is_pattern { None } else { rule_target(rule) };
            let (hits, ignored) = if let Some((path, folder)) = &target {
                (Vec::new(), now.decide(path, *folder, &mut memo).is_some())
            } else {
                let hits = self.ignored_matching(rule, &now);
                let any = !hits.is_empty();
                (hits, any)
            };
            let in_block = present.contains(rule);
            if !in_block && !ignored {
                continue;
            }
            if in_block {
                removed.push(rule.clone());
            } else {
                negated.push(format!("!{rule}"));
            }
            spoken.push(rule.clone());
            expect.push((
                rule.clone(),
                in_block,
                match target {
                    Some((path, folder)) => Expect::Free(path, folder),
                    None => Expect::Freed(hits),
                },
            ));
        }
        if spoken.is_empty() {
            return Ok(None);
        }

        // A rule of paper's that goes away may leave the path ignored by an earlier or
        // lower-precedence rule: then the exception is added in its place (only the
        // negation stays in the block).
        let without = gitignore::edit_rules(existing.as_deref(), &removed, &[])?;
        let text = without
            .as_deref()
            .or(existing.as_deref())
            .unwrap_or_default();
        let mid = Layers::from_tree(&self.repo, &draft_files, &store.overriding(text))?;
        let mut memo = HashMap::new();
        for (rule, in_block, wanted) in &expect {
            if *in_block && first_blocker(&mid, wanted, &mut memo).is_some() {
                negated.push(format!("!{rule}"));
            }
        }

        let new_text = gitignore::edit_rules(existing.as_deref(), &removed, &negated)?;
        let text = new_text
            .as_deref()
            .or(existing.as_deref())
            .unwrap_or_default();
        let after = Layers::from_tree(&self.repo, &draft_files, &store.overriding(text))?;
        let mut memo = HashMap::new();
        for (_, _, wanted) in &expect {
            if let Some((path, folder, blocker)) = first_blocker(&after, wanted, &mut memo) {
                return Err(still_ignored_by(
                    &after,
                    &path,
                    folder,
                    &blocker,
                    request.local,
                ));
            }
        }
        Ok(Some(UnignorePlan {
            removed,
            negated,
            spoken,
            new_text,
            store,
            draft_files,
        }))
    }

    /// Writes the rules file, then lets the snapshot walk pick up the files that are no longer
    /// ignored, all as one `unignore` entry.
    fn write_unignore(&mut self, plan: UnignorePlan) -> Result<UnignoreOutcome, UnignoreError> {
        if let Some(text) = &plan.new_text {
            plan.store.write(&self.repo, text)?;
        }
        crate::fault::crash_point("after-write");

        let mut description = describe::unignored(&plan.spoken);
        if plan.store == Store::Exclude {
            description = describe::local(&description);
        }
        let before = self.view.current().draft.clone();
        self.snapshot_as(OpKind::Unignore, Some(&description))?;
        if self.view.current().draft == before {
            // Nothing entered the draft (a private rule, or a folder with nothing in it): the
            // command still is one entry.
            self.record_at(OpKind::Unignore, self.view.clone(), &description, None)?;
        }

        let draft = self.repo.read_commit(&self.view.current().draft)?;
        let files = self.repo.tree_files(&draft.tree)?;
        let tracked = files
            .keys()
            .filter(|path| !plan.draft_files.contains_key(*path))
            .filter(|path| path.as_str() != ".gitignore")
            .cloned()
            .collect();
        let entry = self
            .activity(1)
            .map_err(ExportError::Activity)?
            .into_iter()
            .next();
        Ok(UnignoreOutcome {
            removed: plan.removed,
            negated: plan.negated,
            tracked,
            entry,
            local: plan.store == Store::Exclude,
        })
    }

    /// The paths a pattern matches that `layers` ignore (a folder counts once; what is inside
    /// an ignored folder is not listed).
    fn ignored_matching(&self, pattern: &str, layers: &Layers) -> Vec<String> {
        let workdir = self.repo.workdir();
        let single = matcher(workdir, pattern);
        let mut memo = HashMap::new();
        let mut hits = Vec::new();
        visit(workdir, "", &mut |rel, is_dir| {
            let ignored = layers.decide(rel, is_dir, &mut memo).is_some();
            if ignored && single.matched(workdir.join(rel), is_dir).is_ignore() {
                hits.push(rel.to_owned());
            }
            !(is_dir && ignored)
        });
        hits.sort();
        hits
    }
}

/// The first path of `wanted` that `layers` still ignore, with the deciding rule's text.
fn first_blocker(
    layers: &Layers,
    wanted: &Expect,
    memo: &mut HashMap<String, Option<String>>,
) -> Option<(String, bool, String)> {
    let (paths, folder) = match wanted {
        Expect::Free(path, folder) => (vec![path.clone()], *folder),
        Expect::Freed(paths) => (paths.clone(), false),
    };
    paths.into_iter().find_map(|path| {
        layers
            .decide(&path, folder, memo)
            .map(|blocker| (path, folder, blocker))
    })
}

/// The error for `path` that `blocker` (the deciding rule's text) keeps ignored.
fn still_ignored_by(
    layers: &Layers,
    path: &str,
    folder: bool,
    blocker: &str,
    local: bool,
) -> UnignoreError {
    let rule = layers
        .explain(path, folder)
        .filter(|found| !found.negated)
        .unwrap_or_else(|| IgnoreMatch {
            rule: blocker.to_owned(),
            source: IgnoreSource::Yours,
            file: ".gitignore".to_owned(),
            line: 0,
            negated: false,
        });
    // The shallowest folder above the path that is ignored is the one to unignore first.
    let mut upper = String::new();
    let mut ignored_folder = None;
    for part in path.split('/').take(path.matches('/').count()) {
        if !upper.is_empty() {
            upper.push('/');
        }
        upper.push_str(part);
        if layers
            .explain(&upper, true)
            .is_some_and(|found| !found.negated)
        {
            ignored_folder = Some(upper.clone());
            break;
        }
    }
    UnignoreError::StillIgnored {
        path: path.to_owned(),
        rule,
        folder: ignored_folder,
        local,
    }
}

/// Calls `visit(relative path, is_dir)` for everything below `dir`, skipping `.git` and other
/// repositories. A folder is entered only when `visit` returns `true` for it.
fn visit(dir: &Path, prefix: &str, visit_one: &mut impl FnMut(&str, bool) -> bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let is_dir = kind.is_dir();
        if name == ".git" || (is_dir && entry.path().join(".git").exists()) {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if visit_one(&rel, is_dir) && is_dir {
            visit(&entry.path(), &rel, visit_one);
        }
    }
}

impl Undone {
    /// Whether the undone, redone or reached entry was written with `--local`: its rule lives
    /// in `.git/info/exclude`, outside history, and undo left that file as it was.
    ///
    /// # Panics
    ///
    /// Never.
    #[must_use]
    pub fn local_rules(&self) -> bool {
        matches!(self.entry.kind, OpKind::Ignore | OpKind::Unignore)
            && self.entry.description.ends_with(describe::LOCAL_SUFFIX)
    }
}

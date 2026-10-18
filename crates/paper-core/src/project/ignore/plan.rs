//! `Project::ignore`: decide everything, then write one rule block and record one entry.

use std::collections::{BTreeSet, HashMap};

use paper_git::{NewCommit, TreeEdit};

use super::args::{Arg, is_protected_file};
use super::layers::Layers;
use super::{GITIGNORE, IgnoreError, IgnoreOutcome, IgnoreRequest};
use crate::activity::{ActivityLog, OpKind, describe, short_ids_among};
use crate::fs_util::write_atomic;
use crate::profile::gitignore::{self, GitignoreError};
use crate::project::{ActivityEntry, ExportError, Project, signature_now};
use crate::view::Line;

/// More files than this leaving the draft need `--yes`.
const UNTRACK_WITHOUT_YES: usize = 20;

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
        if request.local {
            todo!("PV-032: `--local` writes paper's block in .git/info/exclude");
        }
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
        let layers = Layers::from_tree(&self.repo, &draft_files, Some(rules_text))?;
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
}

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
            local: false,
        }
    }
}

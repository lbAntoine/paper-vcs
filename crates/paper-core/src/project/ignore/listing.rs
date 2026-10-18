//! Listing every ignore rule that applies, with its file and line.

use std::fs;
use std::io;
use std::path::Path;

use ignore::WalkBuilder;

use super::{GITIGNORE, IgnoreListing, IgnoreRule, IgnoreSource};
use crate::profile::gitignore::{self, BlockKind};
use crate::project::Project;

impl Project {
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

/// Directories below the project folder that hold a `.gitignore`, sorted.
pub(super) fn nested_gitignore_dirs(workdir: &Path) -> Vec<String> {
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

//! Snapshot behaviour against a real repository and working directory.

use std::fs;
use std::path::Path;

use paper_core::snapshot::{
    Change, ChangeKind, SkipReason, Skipped, Snapshot, SnapshotOptions, StatCache, snapshot,
};
use paper_git::{GitRepo, Identity, TreeId};
use tempfile::TempDir;

struct Project {
    dir: TempDir,
    repo: GitRepo,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity {
            name: "Mina Park".to_owned(),
            email: "mina@uni.example".to_owned(),
        };
        let repo = GitRepo::init(dir.path(), &identity).unwrap();
        Self { dir, repo }
    }

    fn write(&self, path: &str, content: &str) {
        let absolute = self.dir.path().join(path);
        fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        fs::write(absolute, content).unwrap();
    }

    fn remove(&self, path: &str) {
        fs::remove_file(self.dir.path().join(path)).unwrap();
    }

    fn empty_tree(&self) -> TreeId {
        self.repo.empty_tree().unwrap()
    }

    fn snap(&self, base: &TreeId, cache: &StatCache) -> Snapshot {
        self.snap_with(base, cache, &SnapshotOptions::default())
    }

    fn snap_with(&self, base: &TreeId, cache: &StatCache, options: &SnapshotOptions) -> Snapshot {
        snapshot(&self.repo, base, cache, options).unwrap()
    }

    fn paths(&self, tree: &TreeId) -> Vec<String> {
        self.repo.tree_files(tree).unwrap().into_keys().collect()
    }

    fn content(&self, tree: &TreeId, path: &str) -> String {
        let files = self.repo.tree_files(tree).unwrap();
        String::from_utf8(self.repo.read_blob(&files[path].blob).unwrap()).unwrap()
    }
}

fn change(path: &str, kind: ChangeKind) -> Change {
    Change {
        path: path.to_owned(),
        kind,
    }
}

#[test]
fn empty_directory_gives_the_empty_tree() {
    let project = Project::new();
    let empty = project.empty_tree();
    let snap = project.snap(&empty, &StatCache::default());
    assert_eq!(snap.tree, empty);
    assert!(snap.changes.is_empty());
}

#[test]
fn new_files_are_tracked_automatically_including_hidden_and_nested() {
    let project = Project::new();
    project.write("main.tex", "\\documentclass{article}");
    project.write("sections/intro.tex", "Intro");
    project.write(".latexmkrc", "$pdf_mode = 1;");
    let snap = project.snap(&project.empty_tree(), &StatCache::default());
    assert_eq!(
        project.paths(&snap.tree),
        [".latexmkrc", "main.tex", "sections/intro.tex"]
    );
    assert_eq!(
        snap.changes,
        [
            change(".latexmkrc", ChangeKind::Added),
            change("main.tex", ChangeKind::Added),
            change("sections/intro.tex", ChangeKind::Added),
        ]
    );
}

#[test]
fn ignored_files_and_the_git_directory_are_left_out() {
    let project = Project::new();
    project.write(".gitignore", "*.aux\nbuild/\n");
    project.write("main.tex", "x");
    project.write("main.aux", "junk");
    project.write("build/main.pdf", "pdf");
    fs::write(project.dir.path().join(".git/info/exclude"), "notes.txt\n").unwrap();
    project.write("notes.txt", "private");
    let snap = project.snap(&project.empty_tree(), &StatCache::default());
    assert_eq!(project.paths(&snap.tree), [".gitignore", "main.tex"]);
}

#[test]
fn modified_and_deleted_files_are_detected_and_empty_folders_disappear() {
    let project = Project::new();
    project.write("main.tex", "v1");
    project.write("sections/old.tex", "old");
    let first = project.snap(&project.empty_tree(), &StatCache::default());

    project.write("main.tex", "version 2");
    project.remove("sections/old.tex");
    let second = project.snap(&first.tree, &first.cache);

    assert_eq!(
        second.changes,
        [
            change("main.tex", ChangeKind::Modified),
            change("sections/old.tex", ChangeKind::Deleted),
        ]
    );
    assert_eq!(project.paths(&second.tree), ["main.tex"]);
    assert_eq!(project.content(&second.tree, "main.tex"), "version 2");
}

#[test]
fn unchanged_files_come_from_the_cache_without_being_read() {
    let project = Project::new();
    project.write("main.tex", "x");
    project.write("refs.bib", "@article{a}");
    let first = project.snap(&project.empty_tree(), &StatCache::default());
    assert_eq!(first.stats.hashed, 2);

    // Pretend the first snapshot ran long ago, so its entries are outside the racy window.
    let aged = age(&first.cache);
    let second = project.snap(&first.tree, &aged);
    assert_eq!(second.tree, first.tree);
    assert!(second.changes.is_empty());
    assert_eq!((second.stats.hashed, second.stats.cached), (0, 2));
}

#[test]
fn a_quick_same_size_edit_right_after_a_snapshot_is_still_seen() {
    let project = Project::new();
    project.write("main.tex", "aaaa");
    let first = project.snap(&project.empty_tree(), &StatCache::default());
    project.write("main.tex", "bbbb");
    let second = project.snap(&first.tree, &first.cache);
    assert_eq!(second.changes, [change("main.tex", ChangeKind::Modified)]);
    assert_eq!(project.content(&second.tree, "main.tex"), "bbbb");
}

#[test]
fn tracked_files_stay_tracked_when_later_ignored() {
    let project = Project::new();
    project.write("draft.aux", "v1");
    let first = project.snap(&project.empty_tree(), &StatCache::default());
    project.write(".gitignore", "*.aux\n");
    project.write("draft.aux", "v2");
    let second = project.snap(&first.tree, &first.cache);
    assert_eq!(project.paths(&second.tree), [".gitignore", "draft.aux"]);
    assert_eq!(project.content(&second.tree, "draft.aux"), "v2");
}

#[test]
fn new_files_over_the_limit_are_skipped_but_tracked_ones_stay() {
    let project = Project::new();
    let options = SnapshotOptions {
        warn_file_bytes: 5,
        max_file_bytes: 10,
    };
    project.write("small.csv", "1234567");
    let first = project.snap_with(&project.empty_tree(), &StatCache::default(), &options);
    assert_eq!(first.large, [("small.csv".to_owned(), 7)]);

    project.write("small.csv", "grew past the limit");
    project.write("huge.h5", "far too big for the limit");
    let second = project.snap_with(&first.tree, &first.cache, &options);
    assert_eq!(project.paths(&second.tree), ["small.csv"]);
    assert_eq!(
        second.skipped,
        [Skipped {
            path: "huge.h5".to_owned(),
            reason: SkipReason::TooLarge { bytes: 25 },
        }]
    );
}

#[test]
fn nested_repositories_are_skipped_and_reported() {
    let project = Project::new();
    project.write("main.tex", "x");
    project.write("vendor/lib/.git/HEAD", "ref: refs/heads/main");
    project.write("vendor/lib/code.py", "print()");
    let snap = project.snap(&project.empty_tree(), &StatCache::default());
    assert_eq!(project.paths(&snap.tree), ["main.tex"]);
    assert_eq!(
        snap.skipped,
        [Skipped {
            path: "vendor/lib".to_owned(),
            reason: SkipReason::NestedRepository,
        }]
    );
}

#[cfg(unix)]
#[test]
fn executable_bit_is_recorded() {
    use std::os::unix::fs::PermissionsExt;
    let project = Project::new();
    project.write("build.sh", "latexmk");
    let first = project.snap(&project.empty_tree(), &StatCache::default());
    let script = project.dir.path().join("build.sh");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let second = project.snap(&first.tree, &first.cache);
    assert_eq!(second.changes, [change("build.sh", ChangeKind::Modified)]);
    let files = project.repo.tree_files(&second.tree).unwrap();
    assert_eq!(files["build.sh"].kind, paper_git::FileKind::Executable);
}

#[cfg(unix)]
#[test]
fn new_symlinks_are_skipped_and_reported() {
    let project = Project::new();
    project.write("main.tex", "x");
    std::os::unix::fs::symlink("/somewhere/data", project.dir.path().join("data")).unwrap();
    let snap = project.snap(&project.empty_tree(), &StatCache::default());
    assert_eq!(project.paths(&snap.tree), ["main.tex"]);
    assert_eq!(
        snap.skipped,
        [Skipped {
            path: "data".to_owned(),
            reason: SkipReason::Symlink,
        }]
    );
}

/// Round-trips the cache through JSON with its start time pushed an hour into the future.
fn age(cache: &StatCache) -> StatCache {
    let dir = tempfile::tempdir().unwrap();
    cache.save(dir.path()).unwrap();
    let path = dir.path().join("wc-state.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let started = json["started_nanos"].as_u64().unwrap();
    json["started_nanos"] = (started + 3_600_000_000_000).into();
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    StatCache::load(Path::new(dir.path()))
}

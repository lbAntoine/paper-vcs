//! Shared helpers: an isolated environment, a runner for the `paper` binary, fixtures built
//! with `paper-core` (so CLI tests don't depend on the CLI's own `init`/`save`), and the
//! redaction used for insta snapshots.
#![allow(dead_code)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use paper_core::project::{PathSelection, Project, SaveOutcome, SaveRequest};
use paper_core::snapshot::SnapshotOptions;
use paper_git::Identity;

const GIT_IDENTITY: &str = "[user]\n\tname = Mina Park\n\temail = mina@uni.example\n";

/// A temp directory with an isolated home, cache and git config, and an empty `work` folder.
pub struct Env {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub work: PathBuf,
    pub home: PathBuf,
    pub xdg_cache: PathBuf,
    pub local_app_data: PathBuf,
    /// `XDG_CONFIG_HOME` for the run (the user config lives under it on Linux and macOS).
    pub xdg_config: PathBuf,
    /// `APPDATA` for the run (the user config lives under it on Windows).
    pub app_data: PathBuf,
    pub global_config: PathBuf,
}

/// What a `paper` run printed.
#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Env {
    /// An environment whose global git config has an identity.
    pub fn new() -> Self {
        Self::with_global_config(GIT_IDENTITY)
    }

    /// An environment whose global git config is empty: no identity anywhere.
    pub fn without_identity() -> Self {
        Self::with_global_config("")
    }

    fn with_global_config(config: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let work = root.join("work");
        let home = root.join("home");
        let xdg_cache = root.join("xdg-cache");
        let local_app_data = root.join("local-app-data");
        let xdg_config = root.join("xdg-config");
        let app_data = root.join("app-data");
        for folder in [
            &work,
            &home,
            &xdg_cache,
            &local_app_data,
            &xdg_config,
            &app_data,
        ] {
            fs::create_dir_all(folder).unwrap();
        }
        let global_config = root.join("gitconfig");
        fs::write(&global_config, config).unwrap();
        Self {
            _dir: dir,
            root,
            work,
            home,
            xdg_cache,
            local_app_data,
            xdg_config,
            app_data,
            global_config,
        }
    }

    /// The crash-report folder `paper` must use in this environment.
    pub fn crash_dir(&self) -> PathBuf {
        if cfg!(windows) {
            self.local_app_data.join("paper")
        } else {
            self.xdg_cache.join("paper")
        }
    }

    /// The user config file `paper` must use in this environment.
    pub fn user_config(&self) -> PathBuf {
        let base = if cfg!(windows) {
            &self.app_data
        } else {
            &self.xdg_config
        };
        base.join("paper").join("config.toml")
    }

    /// The project file `paper config set --project` writes in `work`.
    pub fn paper_toml(&self) -> PathBuf {
        self.work.join("paper.toml")
    }

    fn isolate(&self, command: &mut Command) {
        command.env_clear();
        for key in ["PATH", "SystemRoot", "SYSTEMROOT", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("XDG_CACHE_HOME", &self.xdg_cache)
            .env("LOCALAPPDATA", &self.local_app_data)
            .env("XDG_CONFIG_HOME", &self.xdg_config)
            .env("APPDATA", &self.app_data)
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("NO_COLOR", "1");
    }

    /// Runs `paper` with `args` in the `work` folder.
    pub fn paper(&self, args: &[&str]) -> Run {
        self.paper_in(&self.work, &[], args)
    }

    /// Runs `paper` in `dir` with extra environment variables.
    pub fn paper_in(&self, dir: &Path, extra_env: &[(&str, &str)], args: &[&str]) -> Run {
        let mut command = Command::new(env!("CARGO_BIN_EXE_paper"));
        self.isolate(&mut command);
        command.current_dir(dir).args(args);
        for (key, value) in extra_env {
            command.env(key, value);
        }
        let mut command = assert_cmd::Command::from_std(command);
        let output = command.output().unwrap();
        Run {
            code: output.status.code().unwrap_or(-1),
            stdout: normalise(&output.stdout),
            stderr: normalise(&output.stderr),
        }
    }

    /// Runs `git` in `dir` with the isolated environment; fails the test if git fails.
    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        let mut command = Command::new("git");
        self.isolate(&mut command);
        let output = command
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
        normalise(&output.stdout)
    }

    pub fn git_work(&self, args: &[&str]) -> String {
        self.git(&self.work, args)
    }

    /// `git fsck --strict` must be clean.
    pub fn fsck(&self) {
        self.git_work(&["fsck", "--strict", "--no-progress"]);
    }

    pub fn write(&self, relative: &str, content: &str) {
        let path = self.work.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    /// Writes raw bytes (for binary fixtures).
    pub fn write_bytes(&self, relative: &str, content: &[u8]) {
        let path = self.work.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.work.join(relative)).unwrap()
    }

    /// The subject lines of every commit reachable from HEAD, newest first.
    pub fn subjects(&self) -> Vec<String> {
        self.git_work(&["log", "--format=%s"])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The `change-id` header of `rev`.
    pub fn change_id(&self, rev: &str) -> String {
        self.git_work(&["cat-file", "-p", rev])
            .lines()
            .find_map(|line| line.strip_prefix("change-id "))
            .unwrap_or_else(|| panic!("{rev} has no change-id header"))
            .to_owned()
    }

    fn identity() -> Identity {
        Identity {
            name: "Mina Park".to_owned(),
            email: "mina@uni.example".to_owned(),
        }
    }

    /// A paper project in `work` with three saves, made with `paper-core`:
    /// "Start project" (main.tex, sections/intro.tex), "Add related work" (related.tex)
    /// and "Rewrite intro" (sections/intro.tex). The draft is empty.
    pub fn project_with_three_saves(&self) -> Project {
        self.write("main.tex", "\\documentclass{article}\n");
        self.write("sections/intro.tex", "Intro v1\n");
        let (mut project, _) =
            Project::init(&self.work, Self::identity(), &SnapshotOptions::default()).unwrap();
        self.write("related.tex", "Related work\n");
        save_all(&mut project, "Add related work");
        self.write("sections/intro.tex", "Intro v2\n");
        save_all(&mut project, "Rewrite intro");
        project
    }

    /// A plain git repository in `work` (branch `main`, two commits) plus a `side` branch with
    /// one more commit. No paper state.
    pub fn plain_git_repo_with_side_branch(&self) {
        self.git_work(&["init", "--quiet", "-b", "main"]);
        self.write("a.txt", "one\n");
        self.git_work(&["add", "."]);
        self.git_work(&["commit", "--quiet", "-m", "First"]);
        self.git_work(&["switch", "--quiet", "-c", "side"]);
        self.write("side.txt", "side\n");
        self.git_work(&["add", "."]);
        self.git_work(&["commit", "--quiet", "-m", "Side work"]);
        self.git_work(&["switch", "--quiet", "main"]);
        self.write("b.txt", "two\n");
        self.git_work(&["add", "."]);
        self.git_work(&["commit", "--quiet", "-m", "Second"]);
    }

    /// Replaces ids, hashes and the temp root in `text` so snapshots are stable.
    pub fn redact(&self, text: &str) -> String {
        let out = redact_root(text, &self.root.to_string_lossy());
        redact_ids(&out, &self.known_ids())
    }

    /// `(change id, commit hash)` of every commit git can see from the work folder.
    fn known_ids(&self) -> Vec<(String, String)> {
        let mut command = Command::new("git");
        self.isolate(&mut command);
        let Ok(output) = command
            .arg("-C")
            .arg(&self.work)
            .args(["rev-list", "--all"])
            .output()
        else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|hash| {
                let mut cat = Command::new("git");
                self.isolate(&mut cat);
                let body = cat
                    .arg("-C")
                    .arg(&self.work)
                    .args(["cat-file", "-p", hash])
                    .output()
                    .ok()?;
                let body = String::from_utf8_lossy(&body.stdout).into_owned();
                let change_id = body
                    .lines()
                    .find_map(|line| line.strip_prefix("change-id "))?
                    .to_owned();
                Some((change_id, hash.to_owned()))
            })
            .collect()
    }

    /// Pretty-printed JSON with every time-like value blanked and every string redacted with
    /// [`Env::redact`]. Strings are redacted after parsing, so Windows paths are seen with
    /// single backslashes, not as JSON escapes.
    pub fn redact_json(&self, stdout: &str) -> String {
        let mut value: serde_json::Value = serde_json::from_str(stdout)
            .unwrap_or_else(|err| panic!("stdout is not JSON ({err}): {stdout}"));
        blank_times(&mut value);
        redact_strings(&mut value, &|text| self.redact(text));
        serde_json::to_string_pretty(&value).unwrap()
    }
}

/// Replaces whole tokens of `text` that are a prefix of a known change id (at least four
/// characters, the shortest ID paper shows) or of a known commit hash (at least seven) with
/// `[ID]` / `[HASH]`. A token is a run of alphanumeric characters, so a word that merely
/// contains an id prefix ("yourself" next to the id "yourk...") is left alone.
///
/// `ids` holds `(change id, commit hash)` pairs. The temp root is not touched here: see
/// [`redact_root`].
///
/// Residual risk: a lone word equal to the first four letters of a real id (a k-z word such as
/// "your") is indistinguishable from a real four-letter short id, so it is replaced. The
/// minimum is not raised because real short ids are four letters long. Change ids use 16
/// letters (k-z), so the odds are about one in 65 536 (16^-4) per id per matching four-letter
/// k-z word in the text.
pub fn redact_ids(text: &str, ids: &[(String, String)]) -> String {
    let is_token_char = |c: char| c.is_ascii_alphanumeric();
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if token.is_empty() {
            return;
        }
        let is_id = token.len() >= 4
            && ids
                .iter()
                .any(|(change_id, _)| change_id.starts_with(token.as_str()));
        let is_hash =
            token.len() >= 7 && ids.iter().any(|(_, hash)| hash.starts_with(token.as_str()));
        if is_id {
            out.push_str("[ID]");
        } else if is_hash {
            out.push_str("[HASH]");
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for c in text.chars() {
        if is_token_char(c) {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

/// Backslashes become `/` (so Windows and Unix agree) and the temp root becomes `[TMP]`.
pub fn redact_root(text: &str, root: &str) -> String {
    let root = root.replace('\\', "/");
    text.replace('\\', "/").replace(root.as_str(), "[TMP]")
}

/// Applies `redact` to every string value in `value`.
pub fn redact_strings(value: &mut serde_json::Value, redact: &dyn Fn(&str) -> String) {
    match value {
        serde_json::Value::String(text) => *text = redact(text),
        serde_json::Value::Object(map) => {
            for inner in map.values_mut() {
                redact_strings(inner, redact);
            }
        }
        serde_json::Value::Array(items) => {
            for inner in items {
                redact_strings(inner, redact);
            }
        }
        _ => {}
    }
}

fn blank_times(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                if key.contains("time") || key.contains("date") {
                    *inner = serde_json::Value::String("[TIME]".to_owned());
                } else {
                    blank_times(inner);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(blank_times),
        _ => {}
    }
}

fn normalise(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

pub fn save_all(project: &mut Project, message: &str) {
    let outcome = project
        .save(&SaveRequest {
            message: message.to_owned(),
            paths: PathSelection::All,
        })
        .unwrap();
    assert!(matches!(outcome, SaveOutcome::Saved(_)), "nothing saved");
}

/// insta settings that blank relative times and dates, which change from run to run.
pub fn settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(
        r"(just now|\d+ (?:minute|hour)s? ago|yesterday|\d{4}-\d{2}-\d{2})",
        "[TIME]",
    );
    settings.add_filter(r"\b[0-9a-f]{40}\b", "[HASH]");
    settings
}

impl Run {
    pub fn assert_code(&self, code: i32) -> &Self {
        assert_eq!(self.code, code, "{self:#?}");
        self
    }

    /// Exit code `code`, nothing on stdout, and the three-part error on stderr.
    pub fn assert_three_part_error(&self, code: i32) -> &Self {
        self.assert_code(code);
        assert!(self.stdout.is_empty(), "{self:#?}");
        assert!(self.stderr.starts_with("error:"), "{self:#?}");
        assert!(self.stderr.contains("why:"), "{self:#?}");
        assert!(self.stderr.contains("next:"), "{self:#?}");
        self
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|err| panic!("stdout is not JSON ({err}): {self:#?}"))
    }
}

pub fn os(value: &str) -> OsString {
    OsString::from(value)
}

/// `len` bytes that classify as binary (they hold NULs).
pub fn binary(len: usize, fill: u8) -> Vec<u8> {
    let mut bytes = vec![fill; len];
    bytes[0] = 0;
    bytes
}

/// The object `value` has exactly these keys (order does not matter).
pub fn assert_keys(value: &serde_json::Value, keys: &[&str]) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {value}"));
    let mut found: Vec<&str> = object.keys().map(String::as_str).collect();
    found.sort_unstable();
    let mut expected = keys.to_vec();
    expected.sort_unstable();
    assert_eq!(found, expected, "{value}");
}

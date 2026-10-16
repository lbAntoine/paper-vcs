# PV-003: CLI: clap, human + --json rendering, 3-part errors, exit codes, insta tests

- **Status:** In progress
- **Branch:** `feat/PV-003-cli`
- **Depends on:** PV-001, PV-002 (merged). Runs in parallel with PV-010 (paper-core diff,
  status, restore). PV-011 wires `status`, `diff` and `restore` into the CLI once both are in.
- **Touches:** `crates/paper-cli/**` (sources, `Cargo.toml`, `tests/`, insta snapshots),
  root `Cargo.toml` (workspace deps), `Cargo.lock`,
  `crates/paper-core/src/lib.rs`, `crates/paper-core/src/error.rs` (new),
  `crates/paper-core/Cargo.toml` (only if a dependency is needed),
  `crates/paper-git/src/lib.rs`, `crates/paper-git/src/config.rs` (new),
  `crates/paper-git/tests/config.rs` (new),
  `docs/specs/02-cli.md` (§ Global behaviour, § init, § save, § history, § show only),
  `docs/specs/03-architecture.md` (§ Errors, § Configuration)
- **Must not touch:** anything under `crates/paper-core/src/project/` (PV-010 is changing it),
  the `status`/`diff`/`restore` sections of `02-cli.md`.
- **Specs:** `02-cli.md` § Global behaviour (flags, discovery, exit codes, error format),
  § `paper init`, § `paper save`, § `paper history`, § `paper show`;
  `03-architecture.md` § Errors, § Configuration, § Testing strategy (CLI row);
  `01-concepts.md` § Invariants (I1), § Researcher-facing wording rules

## Goal

A researcher can run `paper init`, `paper save`, `paper history` and `paper show` from a
terminal, with readable output, `--json` for scripts, three-part errors and the documented
exit codes. The CLI is the first real front-end over `paper-core`.

## Context

- `paper-cli/src/main.rs` only prints the version today.
- Commands exist in paper-core: `Project::init`, `Project::open`, `Project::snapshot`,
  `Project::save` (`SaveRequest`, `PathSelection`), `Project::history` (`HistoryRequest`),
  `Project::show`. Points parse with `point::Point` (`FromStr`).
- Discovery: `GitRepo::enclosing_workdir(path)` walks up to the repo; `Project::open` takes that
  exact directory and returns `OpenError::NotAPaperProject` for a plain git repo.
- `Project::open`/`init` need an `Identity`; nothing reads it from config yet.
- Core error enums (`InitError`, `OpenError`, `SaveError`, `ResolveError`, `HistoryError`,
  `ShowError`) are `thiserror` types. `03-architecture.md` § Errors describes the
  `PaperError { kind, message, why, next }` shape front-ends render.
- Planned workspace deps (root `Cargo.toml` comment): clap 4.6, anyhow 1, assert_cmd 2, insta 1.

## Scope

1. **paper-git: identity from git config.** `config.rs` with
   `pub fn configured_identity(path: &Path) -> Result<Option<Identity>, GitError>`: `user.name`
   and `user.email` as git would see them from `path` (the repository's config when `path` is in
   one, plus global and system config), `None` if either is missing. No gix types in the API.
   It must honour `GIT_CONFIG_GLOBAL` / `HOME` so tests can isolate it.
2. **paper-core: `error.rs`** with `PaperError { kind: ErrorKind, message: String, why: String,
   next: Vec<Suggestion> }`, `Suggestion { command: String, explanation: String }`,
   `ErrorKind::{User, NeedsAttention, Project}` (exit codes 1, 2, 3), and
   `impl From<…> for PaperError` for every error enum above. The three parts are written in
   `error.rs` per variant (researcher vocabulary, rule 6), without editing the enums' `Display`
   messages (those files belong to PV-010 right now; aligning them is PV-011). Wrapped git and
   I/O failures are `Project` errors whose `why` names the underlying cause. Also
   `PaperError::no_identity()` and `PaperError::not_a_project(path, is_git_repo)` constructors
   for the CLI's own failures. Exported from `lib.rs`.
3. **paper-cli: `clap` (derive) parsing**, modules split by concern (`cli.rs` args, `run.rs`
   dispatch, `render/` human and JSON, `error.rs` rendering and exit codes):
   - Global flags: `-C <path>`, `--json`, `--no-snapshot`, `-q/--quiet`, `-v/--verbose`,
     `--color auto|always|never` (respecting `NO_COLOR`), `--version`, `--help`.
   - `paper init [--profile <name>] [<path>]`: `--profile` is accepted and validated against
     the six names but has no effect until PV-004 (say so in `--help`). Prints whether the
     project was created or adopted, the tracked file count, skipped/large files and skipped
     branches.
   - `paper save [<message>] [<paths>…] [--all-but <paths>…]`: missing message on a TTY →
     one-line prompt `Describe this save:`; not a TTY → user error. `Nothing to save.` exits 0.
     Prints the short change ID, the message, and what was saved/left in the draft.
   - `paper history [<path>] [--all] [-n <N>]`: the 02-cli layout (id, relative time,
     message, line tips). `--graph` is rejected with a "not yet" user error.
   - `paper show <point>`: metadata (id, author, date, message) and the list of changed files
     with their kind. File diffs come in PV-011.
   - I1: every command except `init`, `--help` and `--version` snapshots first, unless
     `--no-snapshot` (`save` already snapshots inside `Project::save`).
   - Discovery from the current directory (or `-C`): outside any git repo, or in a git repo
     without `.git/paper/`, fail with the 02-cli wording (`… Run: paper init`).
   - Identity: `configured_identity`; if none, a user error whose `next` gives the two
     `git config --global user.…` commands. (Asking interactively and the paper user config are
     PV-005.)
4. **Rendering.** Human output to stdout, errors to stderr in the three-part format of 02-cli
   (`error:` / `why:` / `next:` with aligned commands). `-q` prints only errors; `-v` adds git
   terms in parentheses (e.g. `save (commit 3fa9c1e)`). Colour only on a TTY and when allowed;
   tests run without colour.
   **`--json`**: one JSON object on stdout with `"schema": 1`, using CLI-owned serde DTOs (no
   serde derives added to paper-core); errors as `{"schema":1,"error":{kind,message,why,next}}`
   on stdout with the same exit code.
5. **Exit codes:** 0 success (including friendly no-ops), 1 user error (also clap's argument
   errors, which clap exits with 2 by default: override), 2 needs attention, 3 project error,
   101 panic. A panic hook writes a report to the user cache dir (`$XDG_CACHE_HOME/paper` or
   `~/.cache/paper` on Unix, `%LOCALAPPDATA%\paper` on Windows) as `crash-<timestamp>.txt` and
   prints the issue-tracker URL and the report path. For tests, debug builds panic on purpose
   when `PAPER_DEBUG_PANIC=1` is set (compiled out of release builds).
6. **Relative times** for history (`just now`, `N minutes ago`, `N hours ago`, `yesterday`, then
   the date `YYYY-MM-DD`) as a pure function of (time, now), unit-tested.
7. **Specs:** `02-cli.md` § Global behaviour and the four command sections where behaviour was
   settled here (clap exit-code override, `--profile` not yet effective, `--graph` not yet,
   JSON error shape); `03-architecture.md` § Errors (`ErrorKind` ↔ exit codes, where the parts
   are written) and § Configuration (identity read from git config until PV-005).

## Out of scope

- `status`, `diff`, `restore` commands and file diffs in `show` (PV-011).
- Profiles and `.gitignore` merging (PV-004); `config`, the user config file and the identity
  prompt (PV-005); cloud-sync warning and `--yes`; the lock; `--graph`; milestones.
- Rewording core `Display` messages (PV-011).

## Acceptance criteria

- [ ] `configured_identity` reads name and email from a repo's config and from an isolated global config file, and returns `None` when either is missing.
- [ ] Every core error variant converts to a `PaperError` with a non-empty message and why, the right kind, and at least one `next` suggestion for user errors (unit tests in paper-core).
- [ ] `paper --version` prints the version; `paper --help` lists init, save, history, show.
- [ ] `paper init` in an empty temp dir creates a project (exit 0), and `git log` shows the first save; in an existing git repo it adopts it and says so.
- [ ] `paper save "msg"` saves (exit 0, `git log -1 --format=%s` is `msg`); with paths it saves only those; `--all-but` excludes; with no changes it prints `Nothing to save.` and exits 0; no message and no TTY → exit 1 with a three-part error.
- [ ] A path with no unsaved changes → exit 1, error lists the path.
- [ ] `paper history` lists saves newest first in the 02-cli layout; `-n`, `<path>` and `--all` work; `--graph` → exit 1 "not yet".
- [ ] `paper show last` and `paper show <change-id prefix>` print metadata and changed files; an unknown point → exit 1 with `why` and `next`.
- [ ] Outside a git repo, and in a plain git repo, a command fails with exit 1 and the `paper init` hint; `-C <path>` runs elsewhere.
- [ ] No identity configured → exit 1 with the `git config --global` suggestions.
- [ ] `--json` output for init, save, history, show and an error has `"schema": 1` and is snapshot-tested; human output for each command and for each error path is insta-snapshot-tested (ids, hashes, times and temp paths redacted).
- [ ] An unknown flag or missing argument exits 1, not clap's 2.
- [ ] `PAPER_DEBUG_PANIC=1` → exit 101, a crash report file exists in the (test-isolated) cache dir, and stderr names it.
- [ ] Commands other than `init` snapshot first: an edit made with no paper command in between appears in `paper show @`; with `--no-snapshot` it doesn't.
- [ ] After every writing command, `git fsck --strict` is clean and `git status` shows the expected state.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass on Linux, macOS and Windows.

## Test plan

- `crates/paper-git/tests/config.rs`: temp repos with `git config` written by the git binary;
  global config isolated with `GIT_CONFIG_GLOBAL` pointing at a temp file (and
  `GIT_CONFIG_NOSYSTEM=1`).
- `crates/paper-core/src/error.rs` unit tests for the conversions.
- `crates/paper-cli/tests/*.rs` with `assert_cmd` + `insta`: every test runs the binary in a
  temp dir with an isolated environment (`HOME`, `XDG_CACHE_HOME`, `LOCALAPPDATA`,
  `GIT_CONFIG_GLOBAL` with a test identity, `GIT_CONFIG_NOSYSTEM=1`, `NO_COLOR=1`), so results
  don't depend on the developer's or CI's git config. Use insta filters for change IDs,
  hashes, relative times and temp paths. Snapshots must be identical on Windows (path
  separators, line endings).
- Relative-time and rendering helpers: unit tests next to the code.

## Summary

## Handoff log

### RED · test-author · 2026-10-16

- Tests (all fail at RED; 93 failing, every pre-existing test still passes):
  - `crates/paper-git/tests/config.rs`: 7 (6 real tests + `child_prints_identity`, the helper body
    the tests re-run in a child process with isolated `GIT_CONFIG_GLOBAL`/`HOME`; it passes
    vacuously). Fail on `todo!()`.
  - `crates/paper-core/src/error.rs` unit tests: 15 (every enum's variants: kind, non-empty
    message/why, `next` for user errors, io/git cause in `why`, `no_identity`, `not_a_project`).
  - `crates/paper-cli/src/render/time.rs` (10) and `src/error.rs` (1) unit tests, `todo!()`.
  - `crates/paper-cli/tests/{basics,init,save,history,show}.rs`: 16 + 10 + 12 + 9 + 8 = 55,
    helpers in `tests/common/mod.rs`. They fail because `run::run()` is `todo!()` (wrong exit
    code, 101). Snapshot tests (`insta::assert_snapshot!` with explicit names, 17 of them) have
    no `.snap` files yet, so they fail on the missing snapshot as well; GREEN reviews and accepts
    them (run with `INSTA_UPDATE=no` at RED to avoid stray `.snap.new` files).
- Environment: every run is `env_clear`ed, then PATH (+ Windows basics), `HOME`, `USERPROFILE`,
  `XDG_CACHE_HOME`, `LOCALAPPDATA`, `GIT_CONFIG_GLOBAL` (temp file with a test identity, or empty
  for `Env::without_identity`), `GIT_CONFIG_NOSYSTEM=1`, `NO_COLOR=1`. Fixtures are built with
  `paper-core` (not the CLI). Snapshots redact the temp root, change IDs (all prefixes >= 4),
  commit hashes, relative times and dates; JSON times are blanked by key name (`*time*`, `*date*`).
- Stubs: `paper-git::config::configured_identity` (re-exported), `paper-core::error`
  (`PaperError`, `Suggestion`, `ErrorKind`, six `From` impls, `no_identity`, `not_a_project`;
  re-exported from `lib.rs`), CLI skeleton (`cli.rs` clap derive, `run::run`, `error::exit_code`,
  `render::time::relative_time`). New workspace deps: assert_cmd 2, clap 4.6 (derive), insta 1
  (filters, json); paper-cli also gets paper-git, serde, serde_json, jiff.
- Decisions the tests pin (change them in GREEN only with a spec/ticket reason):
  - Kinds: `OutOfSyncWithGit`, `HeadOnAnotherBranch`, `NoChangesAt`, `MissingMessage`,
    `UnknownPoint`, `Ambiguous`, `NoSaveBefore`, `NotEnoughHistory`, `NotAPaperProject` and the
    five user-fixable `InitError`s are `User`; every Git/Io/Snapshot/View wrapper,
    `UnsupportedFormat` and `Incomplete` are `Project`. No variant maps to `NeedsAttention` yet.
  - Relative time: <1 min or future `just now`; `1 minute ago`/`N minutes ago`; `1 hour ago`/
    `N hours ago`; 24h to <48h `yesterday`; else UTC `YYYY-MM-DD`. Signature
    `relative_time(then: i64, now: i64) -> String` (epoch seconds) in `render/time.rs`.
  - Crash report: `$XDG_CACHE_HOME/paper/crash-*.txt` (Windows `%LOCALAPPDATA%\paper`);
    `PAPER_DEBUG_PANIC=1` panics early so `paper history` in a project exits 101; stderr names
    the file name and contains `https://github.com/lbAntoine/paper-vcs/issues`.
  - JSON field names: init `adopted`, `tracked_files`; save `saved` (bool), `message`,
    `saved_files[{path,kind}]`, `remaining[{path,kind}]`; history `saves[{message,..}]`; show
    `save{message,..}`, `changes[{path,kind}]` with kind `"modified"`. Every object has
    `"schema": 1`.
  - Human: init says `created`/`adopted` (case-insensitive) and the tracked count; save shows the
    first 4 chars of the change ID and message, lists files left in the draft; `-v save` contains
    `(commit <7-hex>)`; `-q` prints nothing on success; history lines start with the short ID and
    the tip line is named on the newest line; `show` prints the author name, `added` for the first
    save's files. A subfolder of a project is discovered by walking up.
- Not covered (needs a pty): the `Describe this save:` prompt on a TTY. Colour on a TTY is only
  checked negatively (piped and `--color never` have no escapes).
- Questions: none blocking. Note `Project::open` takes the exact workdir, so GREEN must use
  `GitRepo::enclosing_workdir` for discovery from `sections/`.

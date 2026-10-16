# PV-005: config get/set/list and identity

- **Status:** In progress
- **Branch:** `feat/PV-005-config`
- **Depends on:** PV-003, PV-011 (merged). Runs alone.
- **Touches:** `crates/paper-core/src/config/` (new), `crates/paper-core/src/lib.rs`,
  `crates/paper-core/src/error.rs`, `crates/paper-core/src/project/mod.rs`,
  `crates/paper-core/src/project/init.rs`, `crates/paper-core/tests/config.rs` (new),
  `crates/paper-core/Cargo.toml`, `crates/paper-cli/**`, root `Cargo.toml`, `Cargo.lock`,
  `docs/specs/02-cli.md` (§ Global behaviour, § `paper init`, § `paper config`),
  `docs/specs/03-architecture.md` (§ Configuration)
- **Specs:** `03-architecture.md` § Configuration (precedence, the two files);
  `02-cli.md` § `paper config`, § `paper init` (identity prompt), § Global behaviour (`--color`);
  `04-content.md` § Large files; CLAUDE.md rule 8 (no personal data in the repo)

## Goal

A researcher can tell paper who they are and change its settings with
`paper config get|set|list [--project]`, without touching git config. Settings take effect:
identity in saves, the colour default, and the large-file thresholds.

## Context

- Identity today: `paper_git::configured_identity(path)` (git config only), called from
  `crates/paper-cli/src/commands.rs` (`identity()`); `PaperError::no_identity` suggests
  `git config --global …`.
- `Project` holds `SnapshotOptions` (`warn_file_bytes`, `max_file_bytes`), always the default
  (`Project::open`, `Project::init`). Nothing reads `paper.toml` yet, and `init` doesn't write it.
- `--color` has a clap default (`default_value_t = Auto`), so the CLI can't tell whether the user
  passed it.
- `toml` is in the planned dependency list. Use `toml_edit` for writing, so `set` keeps the user's
  comments and layout.

## Decisions (coordinator, 2026-10-16)

- **Keys in v0.1.** Each key has a type and a home:

  | Key | Values | Home | Effect in v0.1 |
  |---|---|---|---|
  | `user.name` | non-empty text | user only | author of saves |
  | `user.email` | text containing `@` | user only | author of saves |
  | `ui.color` | `auto` / `always` / `never` | user only | default for `--color` |
  | `large_files.warn_mb` | integer ≥ 1 | project or user | snapshot warning threshold |
  | `large_files.max_mb` | integer ≥ 1, ≥ `warn_mb` | project or user | snapshot skip threshold |
  | `diff.default` | `lines` (`words` rejected "not yet, v0.3") | project or user | none yet beyond validation |
  | `profile` | the six profile names | project only | stored; used by PV-004 |

  `ui.editor` and any other key: a user error naming the known keys (`editor` arrives later).
- **Where `set` writes.** User config by default; `--project` writes `paper.toml` at the project
  root. `user.*` and `ui.*` with `--project` are refused: identity must never be committed to a
  shared (often public) repository. `profile` without `--project` is refused (it describes the
  project).
- **Precedence** (03-architecture): CLI flag > `PAPER_*` env > `paper.toml` > user config > git
  config (identity only) > default. Env names: `PAPER_` + key upper-cased with `.` → `_`
  (`PAPER_USER_NAME`, `PAPER_LARGE_FILES_MAX_MB`).
- **User config path:** `$XDG_CONFIG_HOME/paper/config.toml`, else `~/.config/paper/config.toml`
  (Linux and macOS); `%APPDATA%\paper\config.toml` on Windows. Created on the first `set`.
- **`paper.toml`** is created by the first `config set --project`. It is an ordinary project file,
  so it shows in the draft and gets saved like any other file.
- **Identity prompt** (02-cli § init): when no identity is found and stdin is a terminal, `init`
  (and any command that needs an identity) asks `Your name:` and `Your email:` once, stores them in
  the user config, says where, and continues. Not a terminal → the existing three-part error, now
  suggesting `paper config set user.name …` first and the git config commands second.

## Scope

1. **paper-core `config` module** (pure where possible; never prints or prompts):
   - `Key` (the table above) with parse, type, home and env name; `Value` typed per key; validation
     errors that teach (rule 6).
   - `ConfigFile::load(path)` / `with(key, value)` / `write(path)` on top of `toml_edit`, preserving
     comments and unknown tables; a missing file loads as empty; a malformed file is an error
     naming the file and line.
   - `Settings::resolve(sources)` where `Sources { env: impl Fn(&str) -> Option<String>,
     project: ConfigFile, user: ConfigFile, git_identity: Option<Identity> }` → every key's
     effective value and its source (`Env`, `Project`, `User`, `Git`, `Default`), plus
     `Settings::identity()` and `Settings::snapshot_options()`.
   - `ConfigError` with `PaperError` conversions.
2. **Project uses the settings:** `Project::open` and `Project::init` take the `SnapshotOptions`
   from the caller (or the resolved settings) instead of always the default.
3. **CLI `paper config`:**
   - `get <key>`: prints the effective value (exit 0); a key with no value (identity unset) →
     exit 1 with a three-part error.
   - `set <key> <value> [--project]`: validates, writes, prints `Set <key> = <value> in <file>`.
   - `list`: every key, its value and its source, in a table; `--json` with `"schema": 1`.
   - Works outside a project for user keys; `--project` or project keys need a project
     (the not-a-project error otherwise).
4. **CLI uses the settings:** identity from settings (env, user config, then git); `ui.color`
   when `--color` isn't given (make the flag optional); large-file options passed to `Project`.
   The identity prompt above, with injected input like `prompt.rs`.
5. **Specs:** `02-cli.md` § `paper config` (keys table, homes, `--project`, sources),
   § `paper init` (prompt), § Global behaviour (`--color` falls back to `ui.color`);
   `03-architecture.md` § Configuration (remove the "*Today:*" note; precedence with env names;
   paths per OS).
6. **PV-014 item absorbed:** rewrite the CLI's large-file `status` test to lower `max_mb` through
   config (env or `paper.toml`) with a 2 MB file instead of the 10 MB default.

## Out of scope

- `ui.editor` and `$EDITOR` for save messages; `config unset`; profiles' effect (PV-004);
  `diff.default = words` (v0.3); a GUI.

## Acceptance criteria

- [ ] `config set user.name/user.email` writes the user config at the OS path (honouring `XDG_CONFIG_HOME` / `APPDATA` in tests), and the next `paper save` is authored with them (checked with `git log --format=%an/%ae`).
- [ ] Identity precedence: env > user config > git config; each step tested.
- [ ] `config set` keeps existing comments and other keys in the file byte-for-byte.
- [ ] `config set --project large_files.max_mb 1` writes `paper.toml`; a 2 MB file is then skipped by `status`/`save`; `PAPER_LARGE_FILES_MAX_MB` overrides it; user config applies when the project doesn't set it.
- [ ] Invalid values (`ui.color purple`, `large_files.max_mb 0`, `max_mb` < `warn_mb`, `user.email nobody`, `diff.default words`, an unknown key, `ui.editor`) → exit 1, three-part error, nothing written.
- [ ] `user.name --project` and `profile` without `--project` are refused with a reason.
- [ ] `config get` prints the effective value; an unset identity → exit 1.
- [ ] `config list` shows every key with value and source; `--json` has `"schema": 1` and a fixed key set; snapshot-tested.
- [ ] A malformed `paper.toml` or user config → exit 1 naming the file and line.
- [ ] `ui.color always` colours output without `--color`; `--color never` still wins.
- [ ] No identity and no terminal → three-part error suggesting `paper config set user.name …`; the prompt path is unit-tested with injected input and stores both values.
- [ ] The large-file status test uses config and a 2 MB file.
- [ ] Specs updated.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/config.rs` and unit tests in `src/config/`: parsing, validation,
  precedence with an injected env function, comment-preserving writes on temp files.
- `crates/paper-cli/tests/config.rs` plus additions to `status.rs`, `save.rs`, `basics.rs`, using
  the isolated env from `tests/common/mod.rs`; add `XDG_CONFIG_HOME` and `APPDATA` to the
  isolation so the developer's real config is never read or written.
- Cross-check identity with `git log`, and that `paper.toml` is an ordinary tracked file
  (`git status --porcelain` after a save).

## Summary

## Handoff log

### RED · test-author · 2026-10-16
- Tests: `crates/paper-core/tests/config.rs` (37, all fail on `todo!()`), `crates/paper-cli/tests/config.rs` (29; 28 fail because `paper config` does not exist, 1 passes on today's git-config identity), `crates/paper-cli/src/prompt.rs` unit tests (5, `todo!()`), `crates/paper-cli/tests/status.rs` (3 new, 1 rewritten; all 4 fail).
- Run with `INSTA_UPDATE=no`. New snapshots are for GREEN to accept after review: `config_list`, `config_list_json`, and `status_large` (rewritten, see below).
- Stubs: `paper_core::config` (`Key`, `Home`, `Scope`, `Value`, `Source`, `ConfigFile`, `Sources<E>`, `Entry`, `Settings`, `ConfigError`, `user_config_path`, `user_config_path_for`), `From<ConfigError> for PaperError`, `Project::with_options(self, SnapshotOptions) -> Self`, `paper-cli` `prompt::ask_for_identity(is_tty, input, output, user_config) -> Result<Option<Identity>, PaperError>`. Dependency: `toml_edit 0.25` (workspace, used by paper-core).
- Test support: `tests/common/mod.rs` isolation gains `XDG_CONFIG_HOME` and `APPDATA` in the temp dir, plus `Env::user_config()` and `Env::paper_toml()`.
- Pre-existing tests changed: only `status_lists_large_files_that_are_tracked` (scope item 6). It now writes a 2 MB file and lowers `PAPER_LARGE_FILES_WARN_MB` to 1; the `status_large` snapshot must change from `10.0 MB` to `2.0 MB`. Nothing weakened; every other pre-existing test passes. Existing `basics.rs` tests still assert the two `git config --global` suggestions (kept true); the `no_identity` snapshot will change at GREEN because `paper config set …` comes first.
- Decisions I made where the ticket was silent (GREEN may push back):
  - `Project::open`/`init` signatures stay; `Project::with_options` is the injection point (`init` already takes options).
  - Core API is `Settings::resolve(Sources { env, project, user, git_identity })`; `Key::parse_value`, `Key::check_scope(Scope)` and `ConfigFile::with` do the validation. `ConfigFile::with` checks `max_mb >= warn_mb` only against values in the same file; `max_mb 1` alone must be accepted (the ticket's own example, with the default warn of 10), so `Settings::snapshot_options` clamps warn to at most max.
  - CLI output: `get` prints the bare value; `list --json` is `{"schema":1,"settings":[{"key","value","source"}]}` in `Key::ALL` order, value is a string, an integer (MB) or null, source is `env|project|user|git|default`.
  - `ask_for_identity` returns `None` without a terminal and writes nothing; it stores via `ConfigFile` and prints the file path.
  - Errors: malformed file is `ErrorKind::User` with the file and `line N` in the text; unreadable file is `Project`.
- Questions: none blocking.

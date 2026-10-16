# PV-011: CLI for status, diff and restore; file diffs in show; error wording

- **Status:** Done
- **Branch:** `feat/PV-011-cli-diff-status-restore`
- **Depends on:** PV-003 (CLI), PV-010 (core diff, status, restore). Both merged.
  Runs in parallel with PV-013 (test-only, owns `crates/paper-core/tests/resolve.rs`,
  `tests/history.rs`, `ci.yml` and `NOTES.md`).
- **Touches:** `crates/paper-cli/**` (sources, tests, snapshots, `Cargo.toml`),
  `crates/paper-core/src/error.rs`,
  `crates/paper-core/src/project/*.rs` (error `Display` texts only),
  `crates/paper-git/src/config.rs`, `crates/paper-git/tests/config.rs`,
  root `Cargo.toml` and `Cargo.lock` (only if `paper-cli` needs `paper-diff` as a direct dependency),
  `docs/specs/02-cli.md`, `docs/specs/03-architecture.md` (§ Errors)
- **Must not touch:** `crates/paper-core/tests/resolve.rs`, `crates/paper-core/tests/history.rs`,
  `.github/workflows/ci.yml`, `docs/tickets/NOTES.md` (PV-013).
- **Specs:** `02-cli.md` § Global behaviour, § `paper status`, § `paper show`, § `paper diff`,
  § `paper restore`; `04-content.md` § Line diff, § Binary summary; `03-architecture.md` § Errors;
  `01-concepts.md` § Researcher-facing wording rules

## Goal

The v0.1 command set is complete in the terminal: a researcher can run `paper status`,
`paper diff`, `paper restore` and see real file diffs in `paper show`. Error texts read
well everywhere, and the minor issues left by PV-003's review are closed.

## Context

- Core API: `Project::status(&mut self, &StatusOptions)`, `Project::diff(&self, &DiffRequest)`,
  `Project::restore(&mut self, &RestoreRequest)` (`crates/paper-core/src/project/{status,diff,restore}.rs`);
  `paper_diff::to_unified` renders a `TextDiff` as git's unified format.
- CLI layout from PV-003: `cli.rs` (clap), `run.rs`, `commands.rs`, `paths.rs` (path resolution
  from the start folder), `render/{human,json,style,time}.rs`, `error.rs`. Follow its patterns,
  test helpers (`tests/common/mod.rs`, redaction) and DTO style.
- `PaperError` conversions live in `crates/paper-core/src/error.rs`; `DiffError`, `StatusError`
  and `RestoreError` have none yet.
- The PV-003 ticket's Summary lists the review-round-2 follow-ups folded in here (item 6).

## Scope

1. **`paper status`**: the 02-cli layout, with line counts until v0.3 (`modified  sections/intro.tex
   +12 −3 lines`; binary: sizes like `(318 KB)` or `318 KB → 331 KB`), a skipped/large section,
   `Nothing unsaved.` for an empty draft. Header `On <line> · last save <id> "<message>" (<time>)`.
   `--no-snapshot` maps to `StatusOptions { snapshot: false }`. Exit 0.
2. **`paper diff [<from>] [<to>] [<paths>…] [--lines|--words] [--stat] [--since <point>]`**:
   - Positionals: up to two leading arguments are points if they resolve as points; the first one
     that doesn't, and everything after it, are paths (resolved with `paths.rs`). `--` forces
     everything after it to be paths. A name that is both a point and a file is a point; the
     spec says so and shows `--`.
   - `--since X` = `diff X @`; combining it with positional points is a user error.
   - Output: unified diff per file via `to_unified` (labels `a/<path>`, `b/<path>`, `/dev/null`),
     binary files as the 04-content one-line summary. Colour (red/green) only on a colour TTY.
   - `--stat`: one line per file with `+N −M` (or the binary sizes), then a total.
   - `--lines` is the default and accepted; `--words` is a user error "not yet (v0.3)".
   - No differences: prints nothing, exit 0.
3. **`paper restore <paths>… [--from <point>]`**: paths through `paths.rs`; prints what was
   restored or deleted and what was skipped (with why); `RestoreError::UnknownPaths` as a
   three-part user error.
4. **`paper show <point>`**: after the metadata, the file diffs (same renderer as `diff`);
   `--stat` gives the summary instead. `show @` shows the draft's diff against `last`.
5. **JSON** for status, diff, restore and show (`"schema": 1`, every key always present, `null`
   when absent): diff files carry `path`, `kind`, `binary`, `added`, `removed`, `old_bytes`,
   `new_bytes` and `hunks` (`old_start`, `old_len`, `new_start`, `new_len`, `lines` with `kind`
   and `text`, `missing_newline`). Document the shapes in 02-cli § Global behaviour or a short
   JSON subsection.
6. **PV-003 review follow-ups:**
   - the no-op save JSON test asserts every key is present and `null`/`[]`;
   - `PaperError::project_failure`: `next[].command` holds the issue URL, the explanation the prose;
     document that `command` may be a URL;
   - `configured_identity` falls back to global config only on gix's "no repository found"
     discover error; a stray or broken `.git` above an `init` target no longer blocks `init`
     of a new folder; the `# Errors` doc matches; a test covers it;
   - the raw `--json` scan stops at `--`;
   - Windows root-relative (`\a`) and drive-relative (`C:a`) paths are a user error
     ("use a path relative to this folder or a full path"); unit-tested under `cfg(windows)`
     plus a pure-logic test on all platforms if the logic allows;
   - the `history .` from a subfolder test also asserts the saves that should be listed.
7. **Errors and wording:**
   - `PaperError` conversions for `DiffError`, `StatusError`, `RestoreError` (unit tests).
   - Core `Display` texts: none suggests a command that doesn't exist yet (`paper lines` is
     v0.4); each reads as one clear sentence about what happened, without duplicating the
     `next` hints the CLI shows. Keep every substring asserted by
     `crates/paper-core/tests/resolve.rs` (lines 207 and 495) and `tests/save.rs` unchanged,
     since those files aren't yours to edit (resolve.rs) or shouldn't need edits (save.rs).
8. **TTY prompt:** the save-message prompt reads from an injected `BufRead` and an `is_tty`
   flag so it is unit-tested (prompt text, empty answer → error, trimmed answer).
9. **Specs:** `02-cli.md` sections above (remove the "*Today:*" note in `show`; diff positional
   rules and `--`; `--words` not yet; JSON shapes); `03-architecture.md` § Errors if conversions
   or `Suggestion` semantics change.

## Out of scope

- Word diff (v0.3), Office diffs, conflicts in `status` (v0.4), activity/undo (v0.2).
- Restore on case-insensitive filesystems and blocked flips (PV-012).
- Profiles (PV-004), config (PV-005).

## Acceptance criteria

- [ ] `paper status` after edits shows the header, each change with kind and line counts or sizes (sorted), skipped/large files; empty draft → `Nothing unsaved.`; `--no-snapshot` doesn't see an edit made since the last command. Snapshot-tested, human and JSON.
- [ ] `paper diff` shows unsaved changes as a unified diff; `paper diff A`, `paper diff A B`, `--since A` and `-- <path>` compare the right points; paths from a subfolder work; an unknown path or point → exit 1 three-part error.
- [ ] A file named like a point is diffed as a path after `--`, and as a point without it.
- [ ] `--stat` prints per-file counts and a total; a binary file prints the size summary; `--words` → exit 1 "not yet".
- [ ] `paper diff` output for edited, added and deleted text files passes `git apply --check -R` in the project (cross-check with the `git` binary).
- [ ] `paper restore <file>` and `--from last~1 <dir>` restore content (checked on disk and with `git status --porcelain`); skipped files are listed; unknown path → exit 1, nothing changed on disk.
- [ ] `paper show last` prints metadata then the diff; `show --stat`; `show @`; the first save shows every file as added.
- [ ] JSON for status, diff, restore and show has `"schema": 1` and a fixed key set, snapshot-tested.
- [ ] Every PV-003 follow-up in scope item 6 has a test.
- [ ] `DiffError`, `StatusError`, `RestoreError` convert to three-part `PaperError`s with the right kind; no core message mentions `paper lines`.
- [ ] The save prompt is unit-tested.
- [ ] Specs updated.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-cli/tests/{status,diff,restore}.rs` and additions to `show.rs`, `save.rs`,
  `paths.rs`, `json_errors.rs`, using `tests/common/mod.rs` (isolated env, redaction on parsed
  JSON values). Build fixtures with paper-core or the CLI itself.
- Snapshots must be identical on Windows: no absolute paths in output except redacted roots;
  file content in fixtures written with `\n`.
- Git cross-check for `paper diff` (`last` → `@`): write the output to a file and run
  `git apply --check -R <file>` in the project. The working files are the `@` side, so the
  reversed patch must apply cleanly.
- Core: unit tests in `crates/paper-core/src/error.rs`. paper-git: `tests/config.rs`.

## Summary

`paper status`, `paper diff` and `paper restore` in the CLI, file diffs in `paper show`, and
error wording across paper-core.

- `status`: header with the last save, per-file line counts or sizes, skipped and large files,
  `Nothing unsaved.`
- `diff`: up to two leading points while they resolve, then paths; `--` forces paths;
  `--since`, `--stat`; `--words` "not yet". Output is git's unified format, checked with
  `git apply --check -R`. Mistyped points (`lsat`, `last~x`, `HEAD~1`) get point errors.
- `restore`: paths from the current folder; skipped files listed with a reason.
- `show`: metadata then the diff (`--stat` for the summary).
- JSON with `"schema": 1` and fixed keys for all four. `DiffError`, `StatusError` and
  `RestoreError` convert to three-part errors, and core messages no longer name commands
  that don't exist yet.
- PV-003 follow-ups closed: identity fallback for a stray `.git` above the target, `--json`
  scan stopping at `--`, Windows root/drive-relative paths rejected, the issue URL in
  `next[].command`, the no-op save JSON keys, `history .` tests, and the save prompt unit-tested.

Deviations: `Project::show_diff` in paper-core (shares `parent_tree` with history);
`matches("")` means every file (for `restore .`); `Restored.skipped` carries a `RestoreSkip`
reason; `paper-cli` depends on `paper-diff` directly (shown in 03-architecture). Review
round 2 found a one-line Windows compile error, so this was escalated to the maintainer: fixed
with no third round, and windows-latest CI is the check.

Follow-ups in PV-014: the file-typo wording in `diff`, a broken `.git` at the `init` target as a
user error, long doc-comment lines in `config.rs`, and a large-file test through config once
PV-005 exists.

## Handoff log

### RED · test-author · 2026-10-16

**New tests (all RED).**
- `crates/paper-cli/tests/status.rs` (10), `diff.rs` (27), `restore.rs` (10): CLI behaviour. They fail
  because the new subcommands are `todo!()` stubs (panic) or on assertions.
- `show.rs` +2 (`show_stat_…`, `show_the_draft_shows_its_diff_against_last`), `json_errors.rs` +3
  (issue URL in `command`; `--json` scan stops at `--`; diff/restore errors are JSON), unit tests in
  `src/prompt.rs` (4) and `src/paths.rs` (3 all-platform + 1 `cfg(windows)`), `paper-core/src/error.rs`
  (+10: conversions, `project_failure` shape, no `paper lines`, no `` `paper … ` `` command in
  Diff/Restore/Resolve `Display`), `paper-git/tests/config.rs` (+2: broken `.git` file and empty
  `.git` folder above the target; the existing damaged-repository-at-the-path test is untouched).
- Fail modes: `todo!()` panics (CLI subcommands, conversions, prompt, path helpers); assertions
  (error texts, `--json` scan, `configured_identity`, snapshot mismatches).
- Snapshot calls without a `.snap` file (GREEN reviews and accepts them): `status_human`,
  `status_nothing_unsaved`, `status_json`, `diff_draft`, `diff_unknown_path`, `diff_binary`,
  `diff_stat`, `diff_json`, `restore_human`, `restore_unknown_path`, `restore_json`, `show_stat`.

**Stubs.** `cli.rs`: `Status`, `Diff { args, lines, words, stat, since }`, `Restore { paths, from }`,
`Show { point, stat }`. `commands.rs`: `status`, `diff`, `restore` (`todo!()`); `show` ignores `--stat`.
`src/prompt.rs::ask_for_message(is_tty, &mut dyn BufRead, &mut dyn Write)` (not tty: nothing printed
or read, `Ok("")`; empty answer: user error); `paths::is_root_or_drive_relative`,
`paths::rooted_path_error`; `From<DiffError|StatusError|RestoreError> for PaperError`. The two
helpers and `ask_for_message` carry `#[allow(dead_code)]` until GREEN wires them in (remove it then).
`diff`'s `args` can't see `--` through clap: GREEN must read the raw args (or use clap's
`last`/`trailing_var_arg`) to tell "forced paths" from points.

**Pre-existing tests/snapshots changed (none weakened; all assert the new behaviour).**
- `show__show_human.snap`: the diff of `sections/intro.tex` now follows the unchanged
  "Changed 1 file:" list after one blank line. `show__show_json.snap`: new `files` array
  (diff-file shape, below); `changes` kept.
- `json_errors__json_error_project.snap`, `json_errors__project_error_human.snap`: `next[0].command`
  is `https://github.com/lbAntoine/paper-vcs/issues`, explanation `report it, including what you
  ran and the message above`.
- `show.rs`: `show_last_…` and `show_the_first_save_…` gain diff assertions; the JSON test gains key
  assertions. `save.rs`: the no-op save JSON test asserts all keys (`null`/`[]`). `paths.rs`:
  `history_dot_from_a_subfolder…` also asserts "Rewrite intro" and "Start project" are listed.
  `tests/common/mod.rs`: helpers `write_bytes`, `read`, `binary`, `assert_keys`.

**Formats the tests pin (decide against the spec if you disagree, and tell the coordinator).**
- Diff labels `a/<path>`, `b/<path>`, `/dev/null`; files concatenated in path order, no extra
  header. Binary: one line with `changed`/`new`/`deleted` and `2 KB → 4 KB` (U+2192).
- `--stat`: a line per file with `+N −M` (U+2212) or binary sizes, then a total containing `N files`.
- Status: header `On main · last save <id> "<msg>" (<time>)`, `Draft (unsaved):`, kinds
  `modified`/`new`/`deleted`, `+2 −1 lines`, `(2 KB)`; `Skipped` section; `Nothing unsaved.`.
- Restore human: file lines with kind (`modified`/`deleted`…), a `Skipped` section.
- Colour: `\x1b[31m` removed lines, `\x1b[32m` added lines, only with `--color always` here.
- JSON: status `{schema,line,last,changes,skipped,large}`; change/diff file
  `{path,kind,binary,added,removed,old_bytes,new_bytes[,hunks]}` (text: bytes `null`; binary:
  added/removed `null`, `hunks: []`); line `{kind: context|added|removed,text,missing_newline}`;
  diff `{schema,files}`; restore `{schema,from (commit hash),changes[{path,kind}],skipped[path]}`;
  show gains `files`.

**Questions.** (1) `missing_newline` is per line (ticket wording was ambiguous). (2) The new
`configured_identity` tests (stray `.git` above an existing target folder) must coexist with the old
"damaged repository at the path itself is an error" test; if gix cannot tell them apart, say so.
(3) `Env::git` fails on non-zero exit, so `git apply --check -R` failures show up as panics.

### GREEN · rust-engineer · 2026-10-16

**Implemented.** `paper status`, `diff`, `restore` and diffs in `show` (`src/inspect.rs`; renderers in
`render/{diff,status,json_diff}.rs`), `DiffError`/`StatusError`/`RestoreError` conversions, core `Display`
rewording (no commands, no `paper lines`; substrings asserted by resolve.rs/save.rs kept), save prompt
via `prompt::ask_for_message`, Windows root/drive-relative path error, `--json` scan stops at `--`,
`configured_identity` ignores a broken `.git` above the target (decided by whether `start/.git` exists,
since gix's error doesn't separate the cases), `project_failure` puts the issue URL in `command`.
Snapshots accepted after reading each: status/diff/restore/show human and JSON.

**Deviations.**
- Added `Project::show_diff(&Point)` in paper-core (`project/diff.rs`): `show` needs the diff of a save against
  its first parent, and the first save against the empty tree, which `DiffRequest` can't express. `diff()` and
  it share a private `entries`.
- `matches()` in `project/save.rs` treats the empty path (`.` at the project root) as every file, so
  `paper restore .` works; `save` already mapped it to "all".
- `paper-cli` now depends on `paper-diff` directly (Cargo.toml, Cargo.lock).
- `--` in `diff` uses clap's `#[arg(last = true)] paths`; leading args are points while they resolve
  (`UnknownPoint`/unparseable means path; other resolve errors and foreign syntax like `HEAD~1` are errors).
- Status shows line counts for deleted text files too (`+0 −1 lines`).

**Specs.** `02-cli.md` (paths, JSON shapes, `command` may be a URL, show/diff/restore/status), `03-architecture.md` § Errors.

**Follow-ups.** Restore skipped files are paths only; a reason per file needs a core change.

### Review · reviewer · 2026-10-16
- Verdict: CHANGES. Checks: tests, clippy and fmt pass on macOS (isolated target).
- MAJOR: `configured_identity` checks `.git` at the nearest existing folder, not the target; `paper init newproj` under a folder with a stray `.git` exits 3 with a "report a bug" error. Tests created the target first.
- MINOR: a mistyped point (`lsat`, `last~x`) becomes a path and the error only talks about files.
- MINOR: no test that `HEAD~1` or an ambiguous point is an error rather than a path.
- MINOR: `Project::show_diff` (deviation, justified) has no core test and repeats the first-parent/empty-tree choice of `history.rs`.
- MINOR: `matches("")` changes core semantics (`Only([""])`, `AllBut([""])`) without a unit test or docs; no CLI test for `paper restore .`.
- MINOR: restore's skipped list has no reasons and its header claims "paper does not track these", which isn't always true.
- MINOR: deleted text files show `−N lines` but the 02-cli example doesn't.
- MINOR: skipped/large output in `status` is only tested on Unix (symlink) and not at all for large files.
- NITs: `diff . nope` skips the unknown-path check; a needless clone; a placeholder `next` command; a terminal read failure is `User` instead of a project failure; the direct `paper-diff` edge isn't in 03-architecture.
- Coordinator decisions: fix everything in this round. Restore carries a reason per skipped path from core (`restore.rs` may change for that), rendered in the CLI and JSON. Deleted text files keep `−N lines`; update the spec. Keep the direct `paper-diff` dependency and show it in 03-architecture. Adding tests is authorised; don't weaken any.

### GREEN (review round 1) · rust-engineer · 2026-10-16

All findings fixed.
- `configured_identity` errors only when the target itself has `.git`; new test with a not-yet-created target under an empty `.git` folder.
- diff: a `last~`/`date:` word that fails to parse, and git syntax like `HEAD~1`, are point errors; a first word in point position that matches nothing reads "'lsat' is not a save, line, date or file" (next: `paper history`, `paper status`). Resolved paths pass through unchanged, so `diff -- . nope` reports `nope`. Tests added (HEAD~1, `last~x`, ambiguous short ID, `lsat`, `. nope`). This changed the existing `diff__diff_unknown_path.snap` (it now uses the new wording for `diff mian.tex`, whose assertion `contains("mian.tex")` still holds).
- core: `parent_tree` helper shared by `history.rs` and `show_diff`; new `tests/show_diff.rs` (4 tests); `matches("")` unit tests and `PathSelection` doc; CLI test for `paper restore .`.
- restore: `Restored.skipped` is now `Vec<SkippedPath { path, reason: RestoreSkip }>` (reasons: saved symlink, symlink, symlinked folder, too large, nested repository, non-UTF-8 name, blocked). tests/restore.rs assertions moved to the new shape with the same paths plus the expected reason. Human: `path  (why)` under "Skipped (left as they are):"; JSON: `[{path, reason, bytes}]`. New CLI snapshot and JSON test.
- status tests for a nested repo and for a large file (a real 10 MB file: the threshold is a core default the CLI can't change), with snapshots.
- spec: deleted text files keep `−N lines` (02-cli example and text), restore skipped reasons and JSON, direct `paper-diff` edge in 03-architecture.
- nits: real example in the rooted-path `next`, read failure is a project failure, no needless clone.

### Review round 2 · reviewer · 2026-10-16
- Verdict: CHANGES. Tests, clippy and fmt pass on macOS. Every round-1 finding verified fixed, including the `paper init newproj` reproduction (now exit 0).
- BLOCKER: `tests/restore.rs`'s `skipped` helper is `#[cfg(unix)]` but a test on every platform calls it → Windows can't compile the file. One-line fix.
- MINOR, deferred: the mistyped-point wording also applies to words that can't be points (`diff sections/nope.tex`); keep the file wording unless the word parsed as a point.
- MINOR, deferred: `paper init` in a folder whose own `.git` is broken is a project error (exit 3), should be a user error.
- NIT, deferred: long doc-comment lines in `paper-git/src/config.rs`.
- The 10 MB large-file test is acceptable on CI; lower the threshold through config once PV-005 exists.
- Escalated to the maintainer (two rounds without approval). Decision: the same engineer removes the `cfg`, no third review round; the coordinator checks the diff, and windows-latest CI verifies. Deferred items go to PV-014.

### GREEN (review round 2) · rust-engineer · 2026-10-16

Removed `#[cfg(unix)]` from the `skipped` helper in `crates/paper-core/tests/restore.rs` (an ungated test calls it); the other gated items in the changed files are self-contained.

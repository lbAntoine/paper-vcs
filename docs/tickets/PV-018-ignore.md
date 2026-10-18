# PV-018: `paper ignore`: own .gitignore block, untrack saved matches, list rules

- **Status:** In progress
- **Issue:** #36 (design and maintainer decisions there; PV-032 / #59 is the second half)
- **Branch:** `feat/PV-018-ignore`
- **Depends on:** PV-021 (activity entries), PV-023 (undo). Runs in parallel with PV-026.
- **Touches:**
  - `crates/paper-core/src/profile/gitignore.rs`: generalised to two blocks.
  - `crates/paper-core/src/project/ignore.rs`: new.
  - `crates/paper-core/src/project/mod.rs`: `mod`/`pub use` lines only.
  - `crates/paper-core/src/activity/op.rs`: one new `OpKind`, plus any place that must name it.
  - `crates/paper-core/src/error.rs`: additions only.
  - `crates/paper-core/tests/ignore.rs`: new.
  - `crates/paper-core/tests/profile.rs`: two-block cases.
  - `crates/paper-cli/src/{cli.rs,commands.rs}`: the `ignore` command.
  - `crates/paper-cli/src/render/ignore.rs`: new.
  - `crates/paper-cli/src/render/mod.rs`.
  - `crates/paper-cli/tests/ignore.rs`: new, with snapshots.
  - `docs/specs/02-cli.md`: new § `paper ignore`, plus the command list.
  - `docs/specs/04-content.md`: § Profiles (the second block) and § Large files (the "add it to `.gitignore`" line).
  - `docs/specs/01-concepts.md`: I7, one sentence.
- **Must not touch:**
  - `Cargo.toml` and `Cargo.lock` (no new dependency; the `ignore` crate is already one).
  - `crates/paper-core/src/project/{undo,checkout,recover,gc,save}.rs`.
  - `crates/paper-core/tests/session*.rs` (PV-026).
- **Specs:** `02-cli.md`; `04-content.md` § Profiles, § Large files; `01-concepts.md` I7; ADR-0002 (plain `.gitignore` only)

## Goal

A researcher can stop tracking a file or folder, even one that's already saved, with one
command, and see every rule that applies, without learning gitignore syntax. Hand-editing
`.gitignore` stays fully supported.

## Context

- Design study and maintainer decisions are on #36. This ticket is the first half. PV-032
  (#59) adds `unignore`, `--local` and `--why`.
- `profile/gitignore.rs` already writes one marked block (`# >>> paper-vcs profile: <name> >>>`)
  atomically. It keeps every other byte, keeps line endings, and reports UTF-8, permission and
  unterminated or duplicate block errors.
  - **Trap:** today any second `# >>> paper-vcs` start marker is a `DuplicateBlock` error. The
    new block must not trip it.
  - **Trap:** re-applying a profile (`paper config set project.profile …`) must keep paper's
    ignore block.
- The snapshot (`snapshot/walk.rs`) walks the way git does with the `ignore` crate. A tracked
  file stays tracked even if it is ignored, because the snapshot compares against the draft's
  own tree. Removing a path from the draft is what stops tracking it. No history rewrite is
  needed.
- Activity entries and undo (PV-021/023): a command that records an op through `record_at` is
  undoable for free. Undo restores the old draft and writes the old `.gitignore` back through the
  checkout, so the files are tracked again.

## Decisions (coordinator, 2026-10-18)

### Writing rules

- **Paper's block:**
  ```
  # >>> paper-vcs: ignored >>>
  /data/raw/
  *.aux
  # <<< paper-vcs: ignored <<<
  ```
  It goes in the root `.gitignore`, after the profile block when there is one, otherwise at the
  end (after a blank line). Each rule appears once, in the order added. An empty block is never
  written.
  - `gitignore.rs` tells the two block kinds apart by their marker text. A duplicate of the same
    kind is still an error.
  - Applying a profile keeps paper's ignore block and its position after the profile block.
- **Turning arguments into rules.** Paths are resolved from where paper runs, like `save`.
  1. If the argument names an existing file or folder (on disk or in the draft), it becomes an
     anchored path rule: `/data/raw/` for a folder, `/notes/a.log` for a file. Gitignore
     metacharacters in the name (`*`, `?`, `[`, `\`, a leading `!` or `#`, trailing spaces) are
     escaped with `\`.
  2. Otherwise, an argument containing `*`, `?` or `[` is a pattern, written exactly as typed.
     It applies to the whole project, wherever paper runs, and the output says so.
  3. Otherwise it is a path to something that doesn't exist yet: anchored, with a trailing `/`
     only if typed with one.
- **Refused** with a three-part user error, writing nothing:
  - an empty argument;
  - a leading `!` (unignoring is PV-032: suggest `paper unignore`, coming in v0.2);
  - `.gitignore` or `paper.toml` themselves;
  - anything under `.git`;
  - a path outside the project.
- **A rule already in paper's block** is not added again. It is reported as already ignored, and
  its matches are still untracked (see below).

### Untracking

- **What stops being tracked:** every file in the draft (after the I1 snapshot) that git would
  now ignore *because of one of the rules named by this command*. The deciding rule is computed
  over the whole rule set, so a later negation in the user's own lines still wins and keeps the
  file.
  - These files leave the draft and stay on disk byte-identical.
  - The output lists them and says the next save records their removal while earlier saves keep
    their versions.
  - Running the same `paper ignore` again finishes an interrupted run, because already-present
    rules still untrack their matches.
- **Threshold:** untracking more than 20 files needs `--yes`. Without it, the command is a
  three-part user error: it gives the count and the first 5 paths, suggests the same command
  with `--yes`, and writes nothing.
- **Nothing to do:** when no rule is added and nothing is untracked, the command prints
  `Nothing to do: already ignored.`, records no entry and exits 0.

### Order, activity and crash safety

- **Order:** I1 snapshot, then decide everything (new text, untrack set, threshold) before
  writing, then write `.gitignore` atomically, then record one op with the new draft (`.gitignore`
  updated and the matches removed) through `record_at`, then refresh the stat cache.
- **Activity entry:** `OpKind::Ignore`, described as `ignore /data/raw/, *.aux`. One entry per
  command, undoable like any other: undo restores the old `.gitignore` and tracks the files
  again. `paper activity` lists it.
- **Crash safety:** a crash after the `.gitignore` write and before the op leaves the new rule on
  disk with the files still tracked. The next command records the `.gitignore` edit, and
  re-running `paper ignore` untracks them. Nothing is lost.
- **Plain git moved the line:** `ignore` writes only paper's draft, not the line, so it doesn't
  refuse.

### Listing

- **`paper ignore` with no arguments** lists every ignore rule that applies, grouped by source,
  each with its file and line. Comments and blank lines are skipped. The sources are:
  - `paper` (paper's block);
  - `profile` (the profile block);
  - `yours` (other lines of the root `.gitignore`, and nested `.gitignore` files, named by path);
  - `local` (`.git/info/exclude`);
  - `global` (git's `core.excludesFile`, or the XDG default `git/ignore` when unset).

  Unreadable sources are listed with a note, not an error. With no rules at all, it prints
  `No ignore rules.`
- **JSON:**
  - `paper ignore … --json`: `{"schema":1,"added":[…],"already":[…],"untracked":[…],"entry":<activity entry or null>}`
  - `paper ignore --json`: `{"schema":1,"rules":[{"rule","source","file","line"}]}`

  Every key is always present.

## Scope

1. `gitignore.rs`: two blocks, plus the ignore block API (read rules, add rules), values only.
2. `project/ignore.rs`: `Project::ignore(args, yes) -> IgnoreOutcome` and
   `Project::ignore_rules() -> Vec<IgnoreRule>`.
3. `OpKind::Ignore` and its description.
4. CLI: `paper ignore [<path|pattern>…] [--yes] [--json]`, in human and JSON form.
5. Specs:
   - 02-cli § `paper ignore`;
   - 04-content § Profiles (paper's second block) and § Large files ("run `paper ignore <path>`" instead of "add it to `.gitignore`");
   - 01-concepts I7 (one sentence: `paper ignore` stops tracking saved files too).

## Out of scope

- `unignore`, `--local` and `--why` (PV-032).
- Writing nested `.gitignore` files.
- `.paperignore`.
- Removing files from history.
- `paper help large-files` itself.

## Acceptance criteria

- [ ] `paper ignore data/raw` with saved files under it writes `/data/raw/` in paper's block,
  untracks them (gone from the draft, still on disk byte-identical), lists them, and records one
  `ignore …` activity entry. The next `paper save` records their removal, and `paper history`
  still shows them in earlier saves. Cross-check with `git ls-files` after the save and `git
  check-ignore -v`.
- [ ] `paper undo` after it restores `.gitignore` and tracks the files again.
- [ ] `paper ignore '*.aux'` is written as typed. An existing file with glob characters in its
  name gets an escaped anchored rule, and `git check-ignore` agrees.
- [ ] With a profile block present, paper's block goes after it. `paper config set
  project.profile …` afterwards keeps paper's block. Hand-written lines are byte-identical.
  CRLF files stay CRLF.
- [ ] A user negation after the rule keeps that file tracked.
- [ ] More than 20 files to untrack without `--yes`: a three-part error, nothing written. With
  `--yes`: done.
- [ ] Refusals (`!x`, `.gitignore`, `paper.toml`, `.git/...`, outside, empty): three-part errors,
  nothing written.
- [ ] Re-running finishes the job. A no-op says `Nothing to do: already ignored.` and records no
  entry.
- [ ] `paper ignore` lists rules from all five sources with file and line, and the JSON key sets
  are fixed. Snapshot-tested.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- **`crates/paper-core/tests/profile.rs`:** two-block parsing and writing, profile re-apply
  keeping the ignore block, duplicate same-kind block still an error, CRLF.
- **`crates/paper-core/tests/ignore.rs`:**
  - argument classification and escaping, checked against `git check-ignore`;
  - the untrack set, including the negation case, plus files on disk byte-identical;
  - the activity entry and undo;
  - threshold, refusals, re-run and no-op;
  - listing from each source. Point `core.excludesFile` at a temp file through the test's
    isolated `GIT_CONFIG_GLOBAL`.
- **`crates/paper-cli/tests/ignore.rs`:** human and JSON output for add, untrack, already,
  no-op, the threshold error and listing, through the binary.
- Write no snapshot by hand. GREEN writes and reviews them.
- Every new shared helper gets a sanity test that passes on main.

## Summary

## Handoff log

### RED · test-author · 2026-10-18

**Tests** (all run with an empty `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM=1`; `cargo fmt` and `clippy -D warnings` clean):

- `crates/paper-core/tests/profile.rs`: 18 new tests, two-block API (`add_rules`, `paper_rules`), profile re-apply keeping paper's block, duplicate/unterminated same-kind block, CRLF. Of the 32 tests in the file, 16 fail on `todo!()` (`add_rules`/`paper_rules` and everything that calls them). Two pass on main by design because the current `apply` already ignores the new marker text: `applying_a_profile_replaces_only_the_profile_block_and_keeps_paper_block` and `applying_a_profile_to_a_crlf_file_with_paper_block_keeps_crlf`. They guard the "Trap" once `find_block` is generalised.
- `crates/paper-core/tests/ignore.rs`: 41 tests. 40 fail on `todo!("PV-018")`. One passes on main by design: `the_test_helpers_see_files_ignore_rules_and_the_draft` (helper sanity for `files`, `check_ignore`, `draft_files`, `op_count`, `head_op`, `paper_block`). Covers: folder untrack with files byte-identical, next save + `git ls-files` + history, later snapshots, activity entry and `ignore /data/raw/, *.aux` description, undo/redo, classification and escaping against `git check-ignore --no-index`, untrack set (only rules named, negation after/before), threshold 20/21/`yes`, refusals (three-part), re-run finishing, no-op, plain git moved the line, CRLF, listing (five sources, order, file, line, CRLF, no rules).
- `crates/paper-cli/tests/ignore.rs`: 19 tests, all fail on the `todo!()` in `commands::ignore`. Human output on key phrases only, JSON parsed, no insta snapshots (GREEN writes and reviews them). No new shared helper (`tests/common` untouched).

**Probe on origin/main** (detached worktree `probe-018`, tests and stubs copied in, removed afterwards): same result as here, since the API does not exist on main. The 3 tests above pass on main (the 2 profile ones and the helper sanity test); every other new test fails on `todo!()`, none on an unrelated reason.

**Stubs added** (all `todo!("PV-018")`): `profile::gitignore::{paper_rules, add_rules}`; `project/ignore.rs` with `IgnoreRequest`, `IgnoreOutcome`, `IgnoreRule`, `IgnoreSource` (+ `name()`), `IgnoreError`, `Project::{ignore, ignore_rules}`; `impl From<IgnoreError> for PaperError`; `OpKind::Ignore` (a variant, no logic) and `"ignore"` in `render/json.rs::op_kind_name`; `Command::Ignore { paths, yes }` in `cli.rs` and `commands::ignore`. `render/ignore.rs` is not created (GREEN adds it).

**Design choices for GREEN to confirm or change (the tests encode them):**

1. Signature: `Project::ignore(&mut self, &IgnoreRequest { args, from, yes })`, not `(args, yes)`. `from` is the folder paper runs in, relative to the project (`""` at the top, `/`-separated). Core needs it to resolve paths, while patterns stay as typed and are not moved into the subfolder. The CLI computes `from` (it has `Paths`' base) and may also turn absolute paths into project-relative ones; core tests only use relative args. The CLI test passes an absolute outside path.
2. `IgnoreOutcome { added, already, patterns, untracked, entry: Option<ActivityEntry> }`. `patterns` (an addition to the ticket) = rules of `added`+`already` that came from a pattern, so the front-end can say "applies to the whole project". `entry: None` iff nothing to do. `untracked` is sorted; `added` is in command-line order, deduped, and `already` holds rules already in paper's block (including repeats within one command of a rule in the block).
3. Rule is recorded as an `entry` also when only the `.gitignore` text changed (new rule, nothing untracked), and when only untracking happened (rule already there).
4. `Project::ignore_rules(&self) -> Vec<IgnoreRule>` (infallible, `&self`, no snapshot). The "unreadable source listed with a note" behaviour has no representation in `Vec<IgnoreRule>` and is not tested; GREEN picks the shape (suggestion: a `note` on the human output only, JSON keys stay the four fixed).
5. `IgnoreRule.file`: `.gitignore`, `docs/.gitignore`, `.git/info/exclude`, and for `global` the path of the excludes file as git reads it (tests only check `ends_with`). `line` is 1-based. Listing is sorted by source in the order paper, profile, yours, local, global; the within-group order is not tested.
6. Core tests cannot change the process environment, so the `global` source is exercised with `git config core.excludesFile` in the repo's own config. The CLI tests use the isolated `GIT_CONFIG_GLOBAL` for `core.excludesFile` and `XDG_CONFIG_HOME/git/ignore` for the unset default.
7. Block position: tests require paper's block after the profile block (when the profile block is last, the result starts with the old text and ends with paper's block), every other line kept in order, and, if there is only paper's block and a profile is applied, the profile block lands before it. They do not fix whether a blank line separates the two blocks when the profile block is last (the exact-bytes tests use `ends_with(block)` / `starts_with(original)` only). With no profile block: appended after a blank line, as for profiles.
8. A rule "already" in the block is matched on exact text. Comment lines inside the block are kept and not treated as rules (`paper_rules` skips them).
9. Refusals tested: `""`, `!x`, `!/data/raw/keep.csv`, `.gitignore`, `./.gitignore`, `paper.toml`, `.git`, `.git/`, `.git/hooks`, `.git/info/exclude`, `../outside`, `data/../../outside`, and `../../elsewhere` from `data`. One refused arg among valid ones refuses the whole command. A damaged (unterminated) paper block surfaces as `IgnoreError::Gitignore(_)` and writes nothing. The test only matches that variant plus `PaperError::from(..)` kind `User` and non-empty three parts; other variants (`Empty`, `Unignore`, `Protected`, `InsideGit`, `Outside`, `TooMany`) are only checked through `PaperError`, so GREEN may rename or reshape them. Threshold errors must put the count and the first five sorted paths in message/why/next and `--yes` in a suggestion command.
10. Classification of a name that exists "in the draft" but no longer on disk is not tested (the I1 snapshot removes it from the draft first, so the ticket wording is ambiguous).
11. Phrases the CLI tests expect: `Nothing to do: already ignored.`, `No ignore rules.` (the whole stdout), "next save" and "earlier saves" (in the untrack message), "whole project" (for patterns), "already" for a rule already present, "unignore" in the `!` refusal, `--yes` in the threshold error, source words `paper/profile/yours/local/global` and the `file` of each rule in the listing. JSON: `{"schema":1,"added","already","untracked","entry"}`; `entry` has the same keys as an `activity --json` entry and `kind: "ignore"`; listing `{"schema":1,"rules":[{rule,source,file,line}]}`.
12. `config set --project profile <name>` after `ignore` is tested through the binary (paper's block kept after the profile block).

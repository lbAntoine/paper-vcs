# PV-018: `paper ignore`: own .gitignore block, untrack saved matches, list rules

- **Status:** Done
- **Issue:** #36 (design and maintainer decisions there; PV-032 / #59 is the second half)
- **Branch:** `feat/PV-018-ignore`
- **Depends on:** PV-021 (activity entries), PV-023 (undo). Runs in parallel with PV-026.
- **Touches:** (plus `crates/paper-core/src/project/checkout.rs`, `crates/paper-git/src/repo.rs`, `crates/paper-core/src/activity/describe.rs`; see the decisions after GREEN)
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

### Coordinator decisions after GREEN (2026-10-18)

- **Checkout leaves ignored files alone (data-loss fix).**
  - The problem: redo of an `ignore` entry plans a checkout from the tracked set to a draft
    without the files, so it would delete them from disk.
  - The rule, in `plan_checkout` (`checkout.rs`, now in this ticket's Touches): a path the
    wanted state doesn't track, and which the wanted state's `.gitignore` rules ignore, is left
    on disk. It is not listed as skipped, because it is correctly untracked there.
  - It lives in the shared planner so that PV-025's recovery (`finish_checkout`) gets it too.
  - Tests: redo after undo of an `ignore`, and a crash at `mid-checkout` during that redo
    followed by the next command. In both, the files stay on disk byte-identical.
- **A name that doesn't exist yet, typed without a slash, is written as typed** and matches
  anywhere (`.DS_Store`, `b.scratch`). With a slash it is anchored. Such names count as patterns
  in the output ("applies to the whole project").
- **`.gitignore` files and `paper.toml` are never untracked**, even when a rule matches them.
- `paper-git` gains `GitRepo::global_excludes_file()` (no gix types), and
  `activity/describe.rs` gains `ignored()`.

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

- [x] `paper ignore data/raw` with saved files under it writes `/data/raw/` in paper's block,
  untracks them (gone from the draft, still on disk byte-identical), lists them, and records one
  `ignore …` activity entry. The next `paper save` records their removal, and `paper history`
  still shows them in earlier saves. Cross-check with `git ls-files` after the save and `git
  check-ignore -v`.
- [x] `paper undo` after it restores `.gitignore` and tracks the files again.
- [x] `paper ignore '*.aux'` is written as typed. An existing file with glob characters in its
  name gets an escaped anchored rule, and `git check-ignore` agrees.
- [x] With a profile block present, paper's block goes after it. `paper config set
  project.profile …` afterwards keeps paper's block. Hand-written lines are byte-identical.
  CRLF files stay CRLF.
- [x] A user negation after the rule keeps that file tracked.
- [x] More than 20 files to untrack without `--yes`: a three-part error, nothing written. With
  `--yes`: done.
- [x] Refusals (`!x`, `.gitignore`, `paper.toml`, `.git/...`, outside, empty): three-part errors,
  nothing written.
- [x] Re-running finishes the job. A no-op says `Nothing to do: already ignored.` and records no
  entry.
- [x] `paper ignore` lists rules from all five sources with file and line, and the JSON key sets
  are fixed. Snapshot-tested.
- [x] Specs updated (only the listed sections).
- [x] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows). Local run passes; CI is left to the coordinator.

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

`paper ignore` adds rules to a second marked block of the root `.gitignore` (after the profile
block), takes the saved files they match out of the draft while leaving them on disk, records
one undoable `ignore` entry, and with no arguments lists every rule that applies by source.
`gitignore.rs` handles two block kinds; `project/ignore.rs` holds the command and the git-order
matcher; the shared checkout planner (`plan_checkout_to_draft`) leaves ignored, untracked files
alone on undo, redo and recovery, so redoing an `ignore` never deletes data. Restore is
unchanged. Follow-ups: PV-034 (repo-local `core.excludesFile`, #66) and PV-035 (Unicode
normalisation-only renames, #67).

## Known gaps

- A repository-local `core.excludesFile` is shown by the `paper ignore` listing (git honours it) but not applied by the snapshot walker or the untrack decision (the `ignore` crate reads only the user's global config). Predates this ticket; PV-034, GitHub issue #66.

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

### GREEN · rust-engineer · 2026-10-18

**Status: two tests are red and cannot be fixed inside the Touches list (BLOCKED, details below).** Everything else is done: `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean, and `cargo test --workspace --all-targets --no-fail-fast` passes except for the two tests named here (core `redo_ignores_and_untracks_again`, CLI `a_pattern_is_written_as_typed_and_the_output_says_it_applies_to_the_whole_project`).

**Implemented**

- `profile/gitignore.rs`: two block kinds (`Kind::{Profile, Paper}`), told apart by marker text, at most one of each. `paper_rules`, `add_rules`, `rule_lines` (lenient, for the listing). A new paper block goes at the end of the file after a blank line (so it follows the profile block and any user lines; the core test `a_negation_before_the_new_rule_loses...` needs that). `apply` keeps paper's block and, when there is no profile block yet, puts the new one right before it.
- `project/ignore.rs`: `Project::ignore` (check args, I1 snapshot, `plan_ignore`, `write_ignore`), `ignore_rules`, and `ignore_listing` (`IgnoreListing { rules, unreadable }`). `ignore_rules` stays a `Vec` so the RED tests are unchanged; the CLI uses `ignore_listing` and prints a `Note:` line for unreadable sources. The untrack decision rebuilds git's precedence with the `ignore` crate (nested `.gitignore` deepest first, root with the new text, `.git/info/exclude`, global), takes the deciding glob's text and keeps the file when it is not one of the rules named by the command. No new dependency.
- `OpKind::Ignore` (already in RED), `describe::ignored` in `activity/describe.rs` (not in Touches; it is where the other descriptions live), `From<IgnoreError> for PaperError`, one wording change in `from_gitignore` (the damaged-block text now names both marker pairs).
- `paper-git/src/repo.rs`: new `GitRepo::global_excludes_file()` (not in Touches). Core cannot read `core.excludesFile` without a gix type, and rule 2 keeps gix in `paper-git`. It re-opens the repository to read the configuration fresh, because an open `gix::Repository` keeps the config it had at open (the core listing test runs `git config core.excludesFile` after the project is open). Falls back to `$XDG_CONFIG_HOME/git/ignore` or `~/.config/git/ignore`.
- CLI: `commands::ignore` (absolute non-pattern args are turned into `from`-relative ones), `render/ignore.rs` (human and JSON), `render/mod.rs`. `render/json.rs::op_kind_name` already had `OpKind::Ignore` from RED. No other exhaustive match needed it.
- Snapshots (read by hand before accepting): `ignore__ignore_{add_human,add_json,untrack_already_human,no_op_human,no_op_json,threshold_error,unignore_error,listing_human,listing_json}.snap`, six new tests appended to `crates/paper-cli/tests/ignore.rs` (no existing assertion touched).
- Specs: 02-cli § `paper ignore` (new; activity descriptions and kinds updated), 04-content § Profiles (second block) and § Large files, 01-concepts I7, and one line in 03-architecture (op kinds).
- Checked by hand: `paper config set --project profile markdown` after `paper ignore out` keeps paper's block after the new profile block.

**Interpretation answers**: the extra `patterns` field, `from` in the request, `entry: None` only for the no-op, and a draft-only name treated as a path were all kept as the coordinator accepted. The `unreadable` sources got the extra `ignore_listing` instead of changing `ignore_rules`. Files named `.gitignore` (any folder) and `paper.toml` are never untracked even if a rule matches them (not in the ticket; without it `paper ignore '*.toml'` would drop paper's settings from the draft).

**Deviation to decide: a bare name that does not exist yet.** The ticket says a path to something that doesn't exist yet is anchored. The core test `rules_appear_once_in_the_order_they_were_added` expects `paper ignore b.scratch` (no such file, no slash) to be written as `b.scratch`, while `a_path_that_does_not_exist_yet_is_anchored...` expects `build/out` and `cache/` to be `/build/out` and `/cache/`. I implemented the only rule that satisfies both: a single-component name typed without any slash is written as typed (matches at any depth, like gitignore itself); anything with a slash is anchored. The spec says so. If the ticket's wording is what is wanted, the test line is wrong (`"/b.scratch"`); say which.

**BLOCKED 1: redo of an `ignore` deletes the files.** `redo_ignores_and_untracks_again` fails: after `undo`, the files are tracked again; `redo` then plans the checkout from the tracked set to the wanted draft, which no longer has them, so `plan_checkout` deletes them from disk (data loss, not only a failed test). The fix is in `project/undo.rs::go` (off limits): leave alone the files of `current` that the wanted state does not track and that the wanted state's `.gitignore` ignores. I wrote and tested this (with it, all 41 core ignore tests, `undo`, `undo_skipped` and `activity` pass), then took it out. The patch is two hunks.

In `undo.rs` (replace the `targets` line in `go`):

```rust
        // Files `paper ignore` took out of the draft stay on disk when the wanted state's
        // .gitignore ignores them, instead of being deleted as "no longer tracked".
        let ignored = self.ignored_in(&current, &wanted_files)?;
        let targets: BTreeSet<&String> = current
            .keys()
            .chain(wanted_files.keys())
            .filter(|path| !ignored.contains(*path))
            .collect();
```

In `project/ignore.rs`, inside `impl Project` (uses the private `Layers`):

```rust
    /// The files of `current` that `wanted` does not track and that the `.gitignore` in
    /// `wanted` ignores: undo and redo leave these on disk.
    pub(super) fn ignored_in(
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
        let text = match wanted.get(GITIGNORE) {
            Some(file) => String::from_utf8_lossy(&self.repo.read_blob(&file.blob)?).into_owned(),
            None => String::new(),
        };
        let layers = Layers::build(&self.repo, &text);
        let mut memo = HashMap::new();
        Ok(gone
            .into_iter()
            .filter(|path| layers.decide(path, false, &mut memo).is_some())
            .cloned()
            .collect())
    }
```

**BLOCKED 2: a CLI test whose premise is false.** `a_pattern_is_written_as_typed_and_the_output_says_it_applies_to_the_whole_project` writes `sections/x.aux` and expects it in the output. The test project has `main.tex`, so it gets the latex profile, which already ignores `*.aux`: the file is never tracked, nothing is untracked and nothing can be listed. Use a pattern the profile doesn't ignore (for example `*.scratch` and `sections/x.scratch`). The assertion is untouched.

**Follow-ups**: `paper help large-files` still doesn't exist (out of scope). `Layers` is built per command; fine for now. Matching is case-sensitive (`core.ignorecase` is not read).

### GREEN (unblocked) · rust-engineer · 2026-10-18

- **Data-loss fix** in the shared planner: `plan_checkout` (`checkout.rs`) now calls `Project::ignored_in(current, wanted)` (the helper from the earlier handoff, kept in `project/ignore.rs`) and skips those paths, so they are neither deleted nor listed as skipped. `undo.rs` and `recover.rs` are unchanged; recovery and `restore` get the rule through the planner. Existing undo, restore and recover suites pass unchanged (ordinary deletes and writes are as before).
- **Tests**: core `redo_ignores_and_untracks_again` now passes. New in `paper-cli/tests/ignore.rs`: `redo_after_undoing_an_ignore_leaves_the_files_on_disk`, and `a_crash_while_redoing_an_ignore_keeps_the_files_and_the_project_consistent` (`PAPER_DEBUG_CRASH_AT=mid-checkout` on `redo`, then `paper status`, files byte-identical, `git fsck --strict`).
- **Fixture change**: in `a_pattern_is_written_as_typed_...` only the file name and pattern changed, `x.aux`/`*.aux` to `x.scratch`/`*.scratch` (the latex profile already ignores `*.aux`). The assertions are otherwise untouched.
- Bare names that don't exist yet (typed without a slash) now count as patterns in `IgnoreOutcome::patterns`, so the output says the rule applies to the whole project.
- 02-cli § `paper ignore` already says `.gitignore` files and `paper.toml` are never untracked.
- fmt, clippy and the full `cargo test --workspace --all-targets --no-fail-fast` pass.

### GREEN (review round 1) · rust-engineer · 2026-10-18

1. **Restore unchanged.** `plan_checkout` is back to its old behaviour (restore.rs untouched); the new `plan_checkout_to_draft` adds the "leave ignored files" rule. One-call edits only: `undo.rs::go` and `recover.rs::finish_checkout` now call it. Regression test `restore_of_a_file_the_older_save_did_not_have_still_removes_it_even_if_ignored_there` (single path and `.`).
2. **Nested `.gitignore`**: `Layers::from_tree` builds root and nested matchers from the wanted tree's blobs; `.git/info/exclude` and the global excludes stay on disk. `plan_ignore` uses it too (draft tree, root replaced by the new text). Test `undo_to_a_state_whose_nested_gitignore_ignores_a_file_keeps_it_on_disk`.
3. **Specs**: 03-architecture (§ Checkout, new paragraph), 02-cli § `paper undo` (bullet) and § `paper ignore` (refusals, spelling, `#`). Restore section untouched.
4. **Already-matching targets**: in the planner, a target not in `current` whose on-disk bytes and exec bit equal the wanted blob is neither written nor skipped (every caller). `undo_restores_the_old_gitignore...` now asserts `undone.skipped.is_empty()` (an assertion added, none changed). All existing undo/restore/recover/crash suites pass unchanged.
5. A bare name starting with `#` is written `\#name` (test added).
6. Patterns starting above the project or inside `.git` are refused with three-part errors (test added). Gitignore-style absolute patterns (`/data/*.csv`) are left alone.
7. Names use the spelling found on disk (`on_disk_spelling`); the test detects a case-insensitive filesystem at runtime and returns early otherwise.
8. The decision layers now use the `ignore` crate's `Gitignore::global()`, as the walker does. `GitRepo::global_excludes_file` is kept for the listing only: the core listing test sets `core.excludesFile` in the repository's own config, which the crate's global lookup doesn't read. Added the `USERPROFILE` fallback (Windows only).
9. An existing file named `!bang` is ignored, not refused (test added); 02-cli refusal wording updated.

### GREEN (review round 2) · rust-engineer · 2026-10-18

- **Case-only rename (blocker).** The "already there" rule now runs only in `plan_checkout_to_draft` (undo, redo, `undo --to`, recovery); `plan_checkout` (restore) is as on main. Even there it is skipped when the spelling listed on disk isn't exactly the path (`spelled_exactly`) or when another path in `current` differs only in case (`has_case_twin`). Tests (file and folder, `restore` and `undo`) rename through a temporary name and assert that exactly one spelling is left with the content, `git fsck --strict` clean. They are filesystem-agnostic, so they run everywhere and bite on case-insensitive ones; with the guards removed the two `undo` tests fail on macOS.
- **Restore spec.** Test `restoring_an_ignored_untracked_file_with_the_saved_bytes_is_still_listed_as_blocked`. 03-architecture "In every checkout" now names undo, redo, `undo --to` and recovery only; 02-cli § undo bullet updated; § restore already says ignored files are never overwritten and listed as skipped, which matches the code.
- Doc comment of `on_disk_symlink` moved back above it.
- Known gap (global excludes mismatch) recorded in the ticket under "Known gaps".

### GREEN (post-approval) · rust-engineer · 2026-10-18

- `has_case_twin` now takes a lowercase-path count map built once per plan (`lowercase_counts`), so the cost no longer grows with added files times tracked files. The exists-on-disk check runs before `spelled_exactly`.
- 02-cli wording and the 03-architecture paragraph rewrapped as asked.
- `tasks.json`: PV-034 (#66, v0.3, low) and PV-035 (#67, v0.2-b, high) added as todo. Ticket: Summary filled in, acceptance criteria ticked except the CI part, PV-034 marked under Known gaps.

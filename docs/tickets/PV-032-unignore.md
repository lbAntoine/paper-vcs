# PV-032: `paper unignore`, `--local` and `--why`

- **Status:** In progress
- **Issue:** #59. The design is in #36. PV-018 (#68) shipped the first half.
- **Branch:** `feat/PV-032-unignore`
- **Depends on:** PV-018 (merged). Runs in parallel with PV-030 (undo wording).
- **Touches:**
  - `crates/paper-core/src/project/ignore.rs` becomes the folder `crates/paper-core/src/project/ignore/` (`mod.rs`, `args.rs`, `listing.rs`, `unignore.rs`, `why.rs`, or a similar split). The move comes first, in its own `refactor:` commit with no behaviour change.
  - `crates/paper-core/src/profile/gitignore.rs`: removing a rule; the block in `.git/info/exclude`.
  - `crates/paper-core/src/activity/{op.rs,describe.rs}`: `OpKind::Unignore` and its description.
  - `crates/paper-core/src/error.rs`: additions only.
  - `crates/paper-core/tests/ignore.rs`, `crates/paper-core/tests/unignore.rs` (new).
  - `crates/paper-cli/src/{cli.rs,commands.rs}`: `unignore`, `--local`, `--why`. In `commands.rs`, only the ignore and unignore functions.
  - `crates/paper-cli/src/render/{ignore.rs,json.rs}`: the op kind name.
  - `crates/paper-cli/tests/ignore.rs` and its snapshots.
  - `docs/specs/02-cli.md`: § `paper ignore`, plus the command list.
  - `docs/specs/01-concepts.md`: I4 (one sentence) and I7.
- **Must not touch:**
  - `crates/paper-core/src/project/{undo,checkout,recover,save,restore,gc}.rs`.
  - `crates/paper-cli/src/render/undo.rs` until PV-030 lands. After that, only the `--local` note (see Decisions).
  - 02-cli § `paper undo` and § `paper restore`, and 03-architecture. These belong to PV-030.
  - `Cargo.toml` / `Cargo.lock`.
- **Specs:** `02-cli.md` § `paper ignore`; `01-concepts.md` I4, I7

## Goal

A researcher can undo an ignore rule (`paper unignore`), keep a rule private to their copy
(`--local`), and find out why a file isn't tracked (`--why`), without learning gitignore
precedence.

## Context

- PV-018 code:
  - `Project::ignore(&IgnoreRequest)`, argument classification (`Arg::parse`), paper's block in
    the root `.gitignore`, `ignored_in`, `ignore_listing` with five sources, `OpKind::Ignore`.
  - The untrack rule (deciding rule among the named rules), the 20-file `--yes` threshold, and
    the escaping and refusal rules.
- Gitignore precedence, lowest to highest: global excludes, then `.git/info/exclude`, then
  `.gitignore` files (deeper and later lines win).
  - A negation in `.git/info/exclude` can't override a `.gitignore` rule.
  - Git can't re-include a file whose parent folder is excluded.

## Decisions (coordinator and maintainer, 2026-10-18)

### `paper unignore <path|pattern>…`
- Arguments are classified like `ignore`'s.
- For each argument:
  - If paper's block (in the chosen file) has that exact rule, remove it.
  - Otherwise, if the path or pattern is still ignored by another rule (profile block, your own
    lines, a nested `.gitignore`, info/exclude, global), add a negation to paper's block:
    `!/notes/keep.log`. For a pattern argument, `!<pattern>`. The negation goes last in the
    block, so it wins over everything earlier in the same file.
  - Then **simulate**: if any named path would still be ignored, refuse with a three-part error
    and write nothing. This covers a parent folder that is excluded, or a rule in a
    higher-precedence file. The error names the rule, file and line that keep it ignored (the
    `--why` result) and says what to do: unignore the folder, or edit that file.
  - Not ignored at all: `Nothing to do: not ignored.`, exit 0, no entry.
- A rule and its negation never both stay in paper's block. Adding a negation for a rule
  paper's block also has removes that rule instead. `ignore` after `unignore` removes the
  negation, and doesn't add a rule if that alone makes the path ignored again.
- **Tracking again:**
  - Order: I1 snapshot, write the file atomically, then snapshot again so the newly visible
    files enter the draft.
  - It records **one** `OpKind::Unignore` entry, `unignore /notes/keep.log`, whose draft holds
    the new `.gitignore` and the files.
  - The output lists the files now tracked. Undo removes them from the draft again (they stay
    on disk) and restores the old `.gitignore`.
- Same refusals as `ignore`: empty argument, `.git/...`, outside the project, `.gitignore`, and
  `paper.toml`. A leading `!` is refused, because `unignore !x` is meaningless.

### `--local` (for `ignore` and `unignore`)
- Rules go to a paper block in `.git/info/exclude`, with the same markers as paper's block, so
  they aren't shared with co-authors. The file and the folder `.git/info` are created if
  missing.
- Untracking and tracking work exactly as without `--local`, and one entry is recorded.
- **Maintainer decision: local rules are outside history.**
  - Undo and redo of a `--local` entry take back or bring back the tracking change only.
    `.git/info/exclude` is left as it is.
  - Undo's output for such an entry adds one note line: ``The rule in .git/info/exclude is
    unchanged; change it with `paper ignore --local` or `paper unignore --local`.``
    - Sequencing: PV-030 owns `render/undo.rs` and is expected to merge first. Then the
      coordinator merges main into this branch, and PV-032 adds the note in `render/undo.rs`
      (the only edit to that file), plus the data it needs on `Undone`, for example
      `local_rules: bool` from the op's kind and description. Before that merge, don't edit
      `render/undo.rs`. If PV-030 hasn't landed by GREEN, expose the data, write the note's
      test as `#[ignore]`d with a reason, and tell the coordinator.
  - 01-concepts I4 gets one sentence: rules written with `--local` live in
    `.git/info/exclude`, outside history, like git config.
- `unignore --local` for a path that the shared `.gitignore` ignores is refused by the
  simulation, with the hint to run it without `--local`. Info/exclude can't override
  `.gitignore`.

### `paper ignore --why <path>`
- Read-only. It records nothing and exits 0 whether or not the path is ignored.
- Ignored: ``data/raw/a.csv is ignored by `/data/raw/` (paper's block, .gitignore line 12).``
  Tracked as well: add `It is still tracked, because it was saved before the rule; paper ignore
  <path> stops tracking it.`
- Not ignored: `data/raw/a.csv is not ignored.` If a negation decides it, say so: ``... is
  not ignored: `!/notes/keep.log` (paper's block, .gitignore line 14) brings it back.``
- Uses the same layers as the snapshot walker and `ignored_in`. That means global excludes
  through the `ignore` crate, as PV-018 does. PV-034 (#66) tracks the repo-local
  `core.excludesFile` gap; don't fix it here.
- JSON:
  `{"schema":1,"path","ignored":bool,"tracked":bool,"rule":{"rule","source","file","line","negated":bool}|null}`,
  with every key always present.
- `--why` takes exactly one path and can't be combined with other arguments, `--yes` or
  `--local`. Any other combination is a usage error.

### JSON for `unignore`
- `{"schema":1,"removed":[…],"negated":[…],"tracked":[…],"entry":<entry or null>,"local":bool}`.
- `ignore --json` gains `"local":bool`, the only change to its key set.

### Coordinator decisions after GREEN (2026-10-18)

- **Undo or redo of an ignore/unignore entry changes tracking, never files (data-loss fix).**
  Undoing `paper unignore --local` used to delete the newly tracked files from disk. The usual
  undo rule keeps a file only when the target's own `.gitignore` layers ignore it, and a local
  rule lives outside history.
  - The rule: when a single `undo` or `redo` step takes back or brings back an `Ignore` or
    `Unignore` entry, the checkout writes or deletes only `.gitignore` files. Every other file
    stays on disk as it is.
  - After undoing a `--local` unignore, the files stay, and the next command records them
    again. That follows from the maintainer's decision that local rules are outside history, and
    the undo note tells the user how to put the rule back.
  - `undo --to` across such entries keeps the normal rules. The files are in history either
    way.
  - The change goes in `project/undo.rs`. It lands **after PV-030 merges**: the coordinator
    merges main into this branch, then PV-032 makes three small edits:
    - the rule above, in `undo.rs`;
    - the `--local` note, in `render/undo.rs`;
    - `unignore` added to 02-cli's activity kind list, in PV-030's section.
  - Tests:
    - undo and redo of a `--local` unignore and of a `--local` ignore: files byte-identical on disk;
    - undo and redo of a shared ignore and unignore: unchanged behaviour;
    - the `#[ignore]`d note test, enabled.
- RED fixes for 6 tests are routed to the test-author. Details are in the GREEN entry.

## Scope

1. Split `project/ignore.rs` into a module folder (refactor commit, no behaviour change).
2. `Project::unignore`, `Project::why_ignored`, `--local` for both commands, `OpKind::Unignore`.
3. CLI: `paper unignore <args…> [--local] [--json]`, `paper ignore … --local`, and
   `paper ignore --why <path> [--json]`.
4. Specs: 02-cli § `paper ignore` (all three, plus the undo behaviour of local rules); 01-concepts
   I4 and I7.

## Out of scope

- Nested `.gitignore` writing.
- PV-034 (repo-local excludesFile) and PV-035 (NFC/NFD).

## Acceptance criteria

- [ ] `paper unignore data/raw` after `paper ignore data/raw` removes the rule and tracks the files again in one entry. `git check-ignore` agrees, and undo takes it back.
- [ ] `paper unignore notes/keep.log`, where your own `*.log` line ignores it, adds `!/notes/keep.log` to paper's block, and the file is tracked.
- [ ] Unignoring a file whose parent folder is excluded by another rule is refused with a three-part error naming that rule, and nothing is written.
- [ ] A path that isn't ignored gives `Nothing to do: not ignored.` and no entry.
- [ ] `ignore` and `unignore` with `--local` write paper's block in `.git/info/exclude`. Co-authors' `.gitignore` is unchanged, and tracking changes as without `--local`.
- [ ] Undo of a `--local` entry reverts tracking and leaves `.git/info/exclude` unchanged. The data for the undo note is exposed and specified.
- [ ] `unignore --local` of a path ignored by `.gitignore` is refused with the "run it without `--local`" hint.
- [ ] `--why` reports the rule, source, file and line, or "not ignored" (including a deciding negation), plus the tracked-anyway case, and matches `git check-ignore -v`.
- [ ] JSON key sets are fixed, and snapshot-tested.
- [ ] `project/ignore/` files are each under 800 lines.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/unignore.rs`:
  - remove, negate and refuse (parent folder excluded, higher-precedence file);
  - no-op and re-ignore after unignore;
  - the entry, undo and redo;
  - `--local` for both commands, plus undo leaving `.git/info/exclude` alone;
  - `why` for each source, for negation and for tracked-anyway.
  - Cross-check with `git check-ignore -v` and `git ls-files`.
- `crates/paper-cli/tests/ignore.rs`: human and JSON output for `unignore`, `--local` and `--why`, plus usage errors.
- Tests must fail on `origin/main`. No hand-written snapshots.

## Summary

## Handoff log

### RED · test-author · 2026-10-18
- Tests (new): `crates/paper-core/tests/unignore.rs` (42: 2 helper sanity, 40 behaviour) and a new "PV-032" section in `crates/paper-cli/tests/ignore.rs` (26 new: 1 helper sanity, 24 behaviour, 1 `#[ignore]`d undo-note test). No snapshots written by hand: key sets are checked with `assert_keys` or parsed JSON, wording with `contains`.
- Tests (changed, pre-existing): `json_for_an_add_has_every_key` and `json_for_a_no_op_has_a_null_entry` in the CLI ignore tests now expect `"local"` (the ticket's only change to `ignore --json`), so they fail until GREEN. `request()` in `crates/paper-core/tests/ignore.rs` gained `local: false` (compile fix only). The existing insta snapshots `ignore_add_json`, `ignore_no_op_json` and `ignore_unignore_error` (it says `paper unignore` "comes in v0.2") will change in GREEN; GREEN accepts them after reading.
- Failing on: `todo!()` in `Project::unignore` / `why_ignored` / `Undone::local_rules` / `From<UnignoreError> for PaperError` and in `Project::ignore` when `local` is set (core: 40 of 42); the `paper unignore` and `ignore --why` command bodies (CLI: 23 of 26 new, plus the 2 changed). Assertion failures only: the two changed JSON tests (missing `local`). Passing on main-plus-stubs: the 3 helper sanity tests, `unignore_needs_at_least_one_argument` (clap's required argument), every pre-existing test (full workspace run: every failure is in `tests/unignore.rs` or the CLI ignore tests above).
- Probe: a detached worktree of origin/main with the crates diff applied (stubs plus tests) gave the same counts as the branch (core unignore 2 pass / 40 fail, CLI ignore 27 pass / 25 fail / 1 ignored, core ignore 52 pass). On plain origin/main the new tests do not compile (no API), which is the expected reason. Probe removed. Also run with `GIT_CONFIG_GLOBAL` = empty file and `GIT_CONFIG_NOSYSTEM=1`.
- Stubs: `crates/paper-core/src/project/unignore_stub.rs` (delete when the module folder exists): `UnignoreRequest { args, from, local }`, `UnignoreOutcome { removed, negated, tracked, entry, local }`, `IgnoreMatch { rule, source, file, line, negated }`, `IgnoreWhy { path, ignored, tracked, rule }`, `UnignoreError { Refused(IgnoreError), StillIgnored { path, rule, local } }` plus its `From` into `PaperError`, `Project::unignore(&mut self, &UnignoreRequest)`, `Project::why_ignored(&self, from, path) -> Result<IgnoreWhy, IgnoreError>`, `Undone::local_rules()`. Edited in place (minimal): `IgnoreRequest.local`, `IgnoreOutcome.local` (stubbed `false`), `todo!()` at the top of `Project::ignore` when `local`, `OpKind::Unignore` (+ `"unignore"` in `render/json.rs` so the match compiles), clap flags `ignore --local`, `ignore --why <PATH>` and the `unignore <PATHS>... [--local]` subcommand, with `todo!()` command bodies. The `--why` conflicts (`conflicts_with_all` paths, `--yes`, `--local`) are left for GREEN.
- Interpretations for GREEN:
  - `Undone::local_rules()` is a method in the stub file, not a field, because `project/undo.rs` is off limits. It must be derived from the entry's kind and description, so a `--local` entry's description has to say so (any wording; tests don't pin it, only that it starts with `ignore /data/raw/` for `ignore --local`). It is true for both `Ignore` and `Unignore` entries, for undo and for redo.
  - Names in `removed`/`negated` are rule texts (`/data/raw/`, `!/notes/keep.log`, pattern `!*.scratch`); entry descriptions are `unignore /data/raw/` and `unignore /notes/keep.log` (no `!`). `tracked` is sorted.
  - Unignore with nothing ignored returns all-empty vectors and `entry: None`; so does a second `ignore --local` (with `already` filled), as for `ignore`.
  - Refusal text (core and CLI): names the deciding rule, its file and line, and the way out: for a parent folder `paper unignore archive` (tests look for `unignore archive`), for a nested `.gitignore` the file name, for `--local` against a `.gitignore` rule the words `without` and `--local`. A later line of your own in the same file as paper's block also refuses (the negation sits before it).
  - `--why` reports `rule: None` when no rule matches, and the deciding negation (`negated: true`, `ignored: false`) when one decides, as `git check-ignore -v --no-index` does. Note: with `-v` git exits 0 even for a negation, so the tests read "ignored" from the pattern, not the exit code. `tracked` means "in the current draft" (no snapshot is taken); a folder given as itself (`data/raw`) is judged as a directory. The `path` field is project-relative with `/`.
  - `unignore` with a `--local` request edits only `.git/info/exclude`; a negation for a rule in `.git/info/exclude` goes to the same file under `--local`, and to `.gitignore` without it (it outranks the exclude file). The global-excludes source is tested only through the CLI (`core.excludesFile` in the isolated global config), because core tests can't set the environment safely.
  - Usage errors from clap print `error:` only (not three parts) with exit 1; the `--why` combination tests accept that, and each has a passing-control `--why` run so they can't pass vacuously.
- Open for the coordinator: the undo-note test `undoing_a_local_entry_adds_the_note_about_info_exclude` is `#[ignore = "PV-032: needs PV-030's render/undo.rs, enabled after the merge"]`. Undo of a `--local` entry is otherwise tested (core and CLI). Questions: none blocking.

### GREEN · rust-engineer · 2026-10-18
- Commits: `refactor:` split of `project/ignore.rs` into `project/ignore/` (`mod.rs`, `args.rs`, `listing.rs`, `layers.rs`, `plan.rs`; the RED stub became `unignore.rs` and `why.rs`), then the implementation. Every file is under 400 lines.
- Implemented: `Project::unignore` (remove, negate, simulate, one `unignore` entry via the second snapshot walk), `Project::why_ignored` (reads rules from disk; `Layers::explain` gives rule, file and line per source, with the parent-folder rule and deciding negation as `git check-ignore -v` reports them), `--local` for both commands (`store.rs`: paper's block in `.git/info/exclude`, `.git/info` created if missing), `OpKind::Unignore` descriptions, `Undone::local_rules()` (kind Ignore or Unignore and description ends with ` (in .git/info/exclude)`), `ignore` after `unignore` (negation removed, no rule added if that alone re-ignores the path), CLI `unignore`, `ignore --local`, `ignore --why` (clap `conflicts_with_all`), renderers and JSON (`local` key on `ignore --json`).
- `gitignore.rs`: `edit_rules` (remove and append in paper's block, drops an emptied block with its blank line) and `read_path`. `error.rs`: additions, plus the `IgnoreError::Unignore` text now points to `paper unignore` (three snapshots re-accepted after reading: `ignore_add_json`, `ignore_no_op_json` gain `"local": false`; `ignore_unignore_error` text). New `UnignoreError::LeadingBang` and `StillIgnored { folder }` (fields added to the RED stub).
- New snapshots (insta-written, read by hand): `unignore_*` (remove, negate, nothing, refused folder, refused nested, bang, local human/json/refused) and `why_*` (ignored, free, yours, negated, tracked, human and JSON), plus `ignore_local_human`.
- Specs: 02-cli § paper ignore (unignore, `--local`, `--why`, undo of local entries, JSON), 01-concepts I4 and I7. The spec describes the undo note line, which is not implemented yet (waits for PV-030 and `render/undo.rs`; the `#[ignore]`d CLI test stays). The activity list of kinds in 02-cli § undo/activity still lacks `unignore`: PV-030 owns that section.
- **Test defects (BLOCKED on these; assertions untouched, 6 tests fail):**
  1. `crates/paper-core/tests/unignore.rs::unignoring_the_excluded_folder_is_the_way_out` asserts `check_verbose(dir, "archive/old.txt")` is `Some(not ignored)`. Real git (2.54) prints nothing for a file under a folder that a later `!/archive/` re-includes (`git check-ignore -v --no-index archive/old.txt` exits 1; the negation is reported only for `archive`). Paper's behaviour is correct (`git status` lists the file). Fix: check `"archive"` instead of `"archive/old.txt"`, or assert `None`.
  2. Four CLI tests (`unignore_refuses_what_ignore_refuses_and_a_leading_bang`, `unignoring_a_path_that_is_not_ignored_says_so_and_records_nothing`, `why_cannot_be_combined_with_other_arguments_or_flags`, `why_resolves_the_path_from_where_paper_runs_and_changes_nothing`) call `assert_unchanged(&env, &gitignore, "", ops)`, but `Project::init` (gix) writes the default commented `.git/info/exclude`, so `exclude(env)` is never `""`. Fix: read `exclude(&env)` before the command and pass it.
  3. `why_json_has_fixed_keys_and_agrees_with_git_for_every_local_source` expects `tracked == false` for `local.tmp`, but the file is written before `paper ignore data/raw`, whose I1 snapshot records it in the draft, so it is correctly tracked-anyway. Fix: write `local.tmp` after that command, or expect `true`.
  With exactly those edits in a scratch copy, the CLI ignore tests (55 pass, 1 ignored) and core `unignore` (42) pass. The rest of the workspace is green.
- Known limitation to decide: undo of a `--local` **unignore** removes the tracked files from the draft, and `ignored_in` (which keeps files on disk after undo) judges by the target's `.gitignore` plus the *current* `.git/info/exclude`, which no longer holds the rule, so those files are deleted from disk by the undo (redo restores them). Fixing it needs `checkout.rs`/`undo.rs` (off limits here), or remembering the local rule.
- Cosmetic: a damaged block in `.git/info/exclude` reports "the paper block in .gitignore is damaged" (shared `from_gitignore` text).

### RED (fix) · test-author · 2026-10-18
- `unignoring_the_excluded_folder_is_the_way_out` (core): now checks `archive` for the `!/archive/` negation (git prints no rule for the file under it, so `None` there) and that the file is in the draft.
- Four CLI tests (`unignore_refuses_what_ignore_refuses_and_a_leading_bang`, `unignoring_a_path_that_is_not_ignored_says_so_and_records_nothing`, `why_cannot_be_combined_with_other_arguments_or_flags`, `why_resolves_the_path_from_where_paper_runs_and_changes_nothing`): compare against `.git/info/exclude` as read before the command, not `""` (init writes a default commented file).
- `why_json_has_fixed_keys_and_agrees_with_git_for_every_local_source`: `local.tmp` is written after `paper ignore data/raw`, so it is untracked and locally ignored.
- Against GREEN at HEAD: core unignore 42 pass, core ignore 52 pass, CLI ignore 55 pass + 1 ignored (the undo note). Clippy clean.

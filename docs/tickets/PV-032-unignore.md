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

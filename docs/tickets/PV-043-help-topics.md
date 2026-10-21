# PV-043: `paper help large-files` (help topics)

- **Status:** In progress
- **Issue:** #82
- **Branch:** `feat/PV-043-help-topics`
- **Depends on:** none. Runs in parallel with PV-041 (`paper-diff` only).
- **Touches:**
  - `crates/paper-cli/src/cli.rs` (the `help` subcommand)
  - `crates/paper-cli/src/commands.rs` (help dispatch only)
  - `crates/paper-cli/src/help/` (new: topic texts and lookup)
  - `crates/paper-cli/src/main.rs` or `run.rs`, if the help subcommand needs wiring there
  - `crates/paper-cli/tests/help.rs` (new) and its snapshots
  - `docs/specs/02-cli.md` (the `paper help` bullet and its "Planned" note)
- **Must not touch:** `paper-core`, `paper-diff`, or any other command's behaviour or output.
  `paper status` already says "see `paper help large-files`"; leave that line unchanged.
- **Specs:**
  - `02-cli.md`: the `paper help …` bullet (around line 778).
  - `04-content.md` § Large files: the content of the topic.
  - ADR-0006 (large files deferred).

## Goal

The "see `paper help large-files`" pointer in `paper status` leads somewhere. A researcher
whose file was skipped as too large reads, in plain words, why it happened and what their
options are.

## Context

- clap provides a built-in `help` subcommand, so today `paper help large-files` is an
  "unrecognized subcommand" error. `paper help save` prints `save`'s help.
- `04-content.md` § Large files sets two limits:
  - **`warn_mb`** (10 MB): files above it are tracked, with a warning.
  - **`max_mb`** (100 MB): files above it are not tracked, and are listed under "Skipped" in
    `status`.

  A file that is already tracked and then grows past `max_mb` stays tracked, with a loud
  warning. The options for a skipped file are `paper ignore <path>`, storing it elsewhere
  (Zenodo, OSF, DVC), or raising the limit knowingly. The config keys are
  `large_files.warn_mb` and `large_files.max_mb` (`paper config`).

## Decisions (coordinator, 2026-10-21)

- **Replace clap's help subcommand** (`disable_help_subcommand`) with our own
  `paper help [<topic|command>]`:
  - `paper help`: the same output as `paper --help`, followed by a short "Help topics:" list
    (`large-files  What happens to big files, and what to do about them`).
  - `paper help <command>`: exactly what `paper <command> --help` prints. Every command keeps
    working.
  - `paper help large-files`: the topic text.
  - Anything else: a three-part user error that names the unknown word and lists the topics and
    the commands. Exit code 1.
  - `--help` and `-h` still work everywhere.
- **Topic framework.** Topics are a small table of `(name, one-line summary, text)`, with the
  texts in `help/*.md`-style string constants or `include_str!` files. `concepts` and `git` can
  be added later without any other change. In v0.3, only `large-files` exists.
- **The `large-files` text** is plain words, about 25–40 lines, wrapped at 80 columns:
  - what the two limits are, with their defaults and the config keys;
  - what "Skipped" in `status` means: the file stays on disk, it isn't in your saves, and so it
    isn't shared;
  - what happens when a tracked file grows past the limit;
  - the three options, with the exact commands: `paper ignore <path>`, an example of where to
    store large data, and `paper config set --project large_files.max_mb <N>` with its
    trade-off (a slower project to share, and hosting limits such as GitHub's 100 MB);
  - Git LFS support is planned after 1.0. Say so in one line, with no date.

  Check every command and default in the text against the real binary and the specs.
- **JSON.** `paper help <topic> --json` gives `{"schema":1,"topic","summary","text"}`.
  `paper help --json` lists the topics: `{"schema":1,"topics":[{"name","summary"}]}`.
  `paper help <command> --json` is a user error saying that command help is text only.
- **Spec.** In 02-cli, the `paper help` bullet describes `help large-files` as shipped in v0.3
  and keeps `concepts` and `git` as planned.

## Acceptance criteria

- [ ] `paper help large-files` prints the topic, exit 0. Every command and default in it is
  correct against the binary.
- [ ] `paper help` prints the general help plus the topic list. `paper help save` (and every
  other command) matches `paper save --help` exactly.
- [ ] `paper help nonsense` is a three-part error listing the topics and commands, exit 1.
- [ ] JSON shapes as decided, with fixed keys.
- [ ] The `status` pointer text is unchanged, and now resolves.
- [ ] Snapshot-tested: topic text, `paper help` output, the error.
- [ ] Spec updated (only the listed bullet).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS and Windows).

## Test plan

- `crates/paper-cli/tests/help.rs`, through the binary:
  - `help large-files` and its JSON;
  - `help`, and `help --json`;
  - `help <cmd>` against `<cmd> --help`, looping over every subcommand;
  - an unknown topic;
  - `-h` and `--help` still work.
- Use `contains` checks in RED. GREEN writes and reviews the snapshots.
- The tests must fail on `origin/main`: there is no `help large-files` there.

## Summary

## Handoff log

### RED · test-author · 2026-10-21

- **Tests:** `crates/paper-cli/tests/help.rs`, 14 tests, through the binary, `contains` checks
  only (no snapshots). 10 fail on `origin/main` (probe worktree): the topic text and its
  facts (limits, defaults 10/100 MB, config keys, `paper ignore <path>`, Zenodo, `paper config
  set --project large_files.max_mb <N>`, "Skipped", one "Git LFS" line with "1.0"), 25-40 lines
  at <= 80 columns, `help` + "Help topics:" list, unknown topic three-part error (names the
  word, lists `large-files` and commands), topic JSON keys, `help --json` keys,
  `help <cmd> --json` user error. They fail with "unrecognized subcommand".
- **Pass on main by design (4):** `help <cmd>` equals `<cmd> --help` for every command (clap's
  built-in already does this; guards the replacement), the same inside a project, `-h` and
  `--help`, and the unknown-topic JSON error (clap's error is already a JSON user error naming
  the word).
- **Command list:** discovered from the `Commands:` block of `paper --help` (minus `help`),
  and checked to include a fixed list of the 14 current commands.
- **Stubs:** none needed (tests go through the binary).
- **QUESTION (ticket premise is wrong):** `paper status` does NOT print "see `paper help
  large-files`" today. The heading is "Skipped (paper leaves these out of the draft):" and no
  source or snapshot contains the pointer (only the spec, 02-cli.md line 139). So there is
  nothing to pin, and adding the pointer changes `status` output and snapshots, which the
  ticket's Touches/Must-not-touch lists forbid. No status test was written. The coordinator
  must decide: add the pointer (and widen Touches to `render/status.rs`/`human.rs` and the
  `status_skipped` snapshot) or drop that acceptance criterion.

# Notes and decisions

Dated process lessons and project decisions that aren't architecture (those are ADRs).
Newest first.

## 2026-10-25: sprint v0.3-a retrospective

Sprint v0.3-a met its goal: researchers read their changes as prose, and the v0.2.0 data-loss
known issue (#67) is fixed. Five tickets shipped in PRs #86, #88, #90, #91 and #94: word
tokenisers (PV-040), the same-file rule for Unicode-form and case renames (PV-035), `paper help
large-files` (PV-043), the word diff (PV-041), and word diff as the default in `diff`, `show`
and `status` (PV-042). The roadmap's v0.3 acceptance is met: a PI can run `paper diff --since`
on a co-author's changes and read it as prose. 1424 tests. The goal verification passed 7/8;
the failing truth was two unlisted gaps (restoring one spelling after a Unicode-form rename,
#95; a 5 MB fallback the spec promises, #96), carried into v0.3-b.

- **What slowed us down: rendering and diff logic fixed one example at a time.** PV-041 and
  PV-042 took the most rounds of any tickets so far. Each review found inputs the example tables
  hadn't covered: paragraphs that start with the same word, heavy rewrites, swapped paragraphs,
  a comment's line break at the end of a run, then at its start. Two fixes were the
  coordinator's own designs and made things worse (a paragraph-first alignment that turned edits
  into whole rewrites; narrow comment patches that each missed the next edge). What finally held
  in PV-042 was one rule plus a property test over random `.tex` edits, which checks the
  structure of the output for any input.
- **What worked: checks by someone other than the author, run against the real binary.** Every
  regression above was caught before main: diff-only checks with the reviewer's own probes,
  fuzzing in scratch crates, the RED test-author questioning the ticket (the missing status
  hint, `layout_only`), and the goal verification. The maintainer made the scope calls when a
  ticket passed two rounds. One reviewer probe wrote the maintainer's real
  `~/.config/paper/config.toml` because `XDG_CONFIG_HOME` leaked; briefs now isolate every
  config root.
- **Process change:** a ticket whose output comes from an algorithm over arbitrary input (a diff,
  a tokeniser, a renderer, wrapping) gets a property test in RED, not only example tables. The
  test generates random inputs and checks invariants of the output (round trip, balanced markers,
  nothing lost, no line wider than the width), so review starts from a test that already
  explores the input space (docs/workflow.md, section 3).
- **Open for v0.3-b:** #95 (restore one spelling), #96 (5 MB fallback), #97 (word-diff
  readability notes), #87, #89, #92, #93, #72, #75 and the low-priority items carried from
  earlier.

## 2026-10-19: sprint v0.2-b retrospective

Sprint v0.2-b met its goal: nothing is ever lost, proven, and released as v0.2.0 (a GitHub
pre-release, tag at 20c178c). Ten tickets shipped in sixteen pull requests (#62 to #77): crash
recovery (PV-025), `paper gc` (PV-027), `status` noticing plain-git moves (PV-029), undo past
skipped files (PV-031), `paper ignore` (PV-018), the 200-step session proptest (PV-026), undo
wording and the `git reset --soft` way out (PV-030), `unignore`/`--local`/`--why` (PV-032), a
data-loss fix found by the goal verification (PV-038), and the release (PV-033). 1205 tests.

- **What slowed us down: "narrow" fixes to the undo and checkout code that broke something
  else.** Four times a fix in that code, three of them decided by the coordinator, introduced a
  new data-loss path that only a later check caught. In PV-018, an "already on disk" rule for
  every checkout deleted a file renamed only in letter case. In PV-032, a tracking-only rule
  dropped PV-031's leftovers, and the "narrow" fix for a file edited while ignored let a second
  undo overwrite it. In PV-038, a lookup turned an undo after `gc` into an internal error. The
  goal verification found two more `--local` deletion paths before the tag (one fixed, one a
  known issue). The pattern: each fix was stated as a local patch for one reported scenario,
  not checked against the invariant that keeps work safe (a file leaves the disk only if its
  content is already in history and the target state does not keep it). Three tickets needed
  the maintainer's decision after two review rounds.
- **What worked: checks that try to break the result, run by someone other than the author.**
  Diff-only reviews with the reviewer's own probe scripts on a fresh build caught every one of
  the regressions above. Mutation-testing the PV-026 proptest (planting nine undo bugs) showed
  its first oracle missed three and drove an independent model that catches all nine. The goal
  verification before the tag, run by hand with the real binary, found what no ticket test did.
- **Process change:** tickets that touch undo, checkout, recovery or snapshot code name the
  data-safety invariant they must keep, and every fix in that code after a review, including
  the coordinator's own decisions, is checked by the reviewer against that invariant with its
  probe scripts on a fresh build in its own target directory (docs/workflow.md, sections 2
  and 5).
- **Open for v0.3:** the known issues #67 (macOS Unicode rename), #72 (edit while ignored ends
  redo), #75 (`--local` and `undo --to` paths, plus ignore steps in the session proptest), #71,
  #66; and #15, #33, #50 carried from earlier.

## 2026-10-18: sprint v0.2-a retrospective

Sprint v0.2-a met its goal: every paper command can be undone and redone, working files
included (PV-017, PV-019 to PV-024; PRs #47 to #56). A goal verification passed with a hand-run
session: five undos back to the start and five redos forward, a v0.1.0 project upgraded, and
`git gc` keeping everything undo needs.

- **What slowed us down: RED expectations that couldn't be checked until code existed.** In three
  of seven tickets GREEN stopped (BLOCKED, correctly) on a test bug rather than a product bug: a
  hand-written snapshot with the wrong padding, an exit code that contradicted the rest of the CLI,
  and a state-capturing helper that crashed on a detached HEAD and compared a ref that moves by
  design. A test helper that redacted hex op ids in every snapshot would also have added random
  failures across the suite. The RED check now forbids hand-written snapshots of output that
  doesn't exist yet and requires a sanity test for every new shared helper.
- **What worked: review that probes the real binary, plus agents that stop instead of guessing.**
  Round 1 found a blocker or major in six of seven tickets, all real: a migration that would have
  reset a v0.1 user's plain-git commit, a crash window that would silently reverse an undo, a
  missing-head error that looped back to `paper init`, and power-cut gaps before `version`. Agents
  reported BLOCKED instead of editing assertions every time a test was wrong.
- **Open from verification:** undo can get stuck behind a file its checkout had to leave alone
  (PV-031, #57); spec sentences that describe `paper gc` as existing (#55).

## 2026-10-17: v0.1 retrospective

v0.1.0 shipped as a GitHub pre-release: 17 pull requests (#1–#18), 636 tests on three operating
systems, and a goal verification that passed with the acceptance scenario run by hand.

- **What slowed us down: tests that pass locally but fail on CI, or catch nothing.** Four CI
  failures were platform assumptions in tests (Windows paths inside JSON, a `#[cfg(unix)]`
  helper called from a portable test, the POSIX `unset` suggestion on Windows, and git's default
  branch, which is `master` on CI). In PV-012 the corrected RED tests passed against `main` and
  caught nothing. Each cost a CI or review round; 12 of 14 tickets needed two review rounds.
  The RED check now proves new tests fail on `main` and runs them with an empty git config.
- **What worked: review that runs the real binary, and reproducing before fixing.** Review round
  1 found a blocker or major in 12 of 14 tickets, all real (a write through a symlinked folder,
  invalid TOML from `config set`, a 75× `status` slowdown, a "report a bug" error for a stray
  `.git`). Reproducing PV-012 on `main` with the CLI showed the reported bug was already gone
  and found the real one in snapshot. Mutation-testing the PV-008 refactor (14 of 14 caught)
  is worth repeating for every refactor.
- **Tracking:** every ticket now has a GitHub issue with labels and a milestone (v0.1 closed,
  v0.2 open). Done tasks were archived from `tasks.json`; their tickets and issues keep the record.

## 2026-10-16: flaky prefix tests and hidden CI failures

Two lessons from PV-003 and PV-010:

- **A test that assumes random IDs avoid a fixed prefix is flaky.** Change IDs are random
  (16 values per letter), so fixed IDs with short (2 to 4 letter) prefixes clashed with the
  draft's random ID now and then (0.5% of runs for the 2-letter one). Give fixed IDs long
  shared prefixes (8 or more letters, odds below 1 in 10^9) and look up the long prefix. To
  test short prefixes on purpose, rebuild the project, bounded, while a random ID clashes.
- **CI stopped at the first failing test binary**, which hid whether later CLI test binaries
  passed on Windows. CI now runs `cargo test --workspace --all-targets --no-fail-fast`.
  Redact JSON snapshots on parsed values, not on escaped text (Windows paths are escaped in
  JSON).

## 2026-10-16: one build directory per worktree

The first parallel run (PV-002 and PV-009) shared the main checkout's `target/` across both
worktrees. The crates have the same names in both, so cargo reused the other branch's
artifacts: a test run in one worktree executed the other branch's test binaries (`ancestors`
tests showing up in PV-009, PV-009's `paper-diff` tests failing in PV-002). Two agents and the
reviewer hit it independently. Results were re-verified in isolated builds before trusting
them. Rule since then: each worktree builds in its own `target/`.

## 2026-10-16: agent pipeline adopted

v0.1 steps 1–6 were written by hand from guided handoffs. From here on, agents write the
code and the coordinator merges through pull requests ([workflow](../workflow.md)).

The pipeline is modelled on an earlier project's agent workflow, and fixes the problems that
project recorded:

- **Shared working directories leaked changes between parallel agents.** Every ticket now
  gets its own worktree from the RED phase onward, never just a branch in a shared checkout.
- **An agent ended up on an auto-named branch instead of its feature branch.** The
  coordinator now creates each worktree with an explicit branch name and checks it before
  dispatch; agents never create or switch branches.
- **Two tickets defining the same behaviour both got through planning.** Grooming now checks
  each new ticket's `Touches` list and acceptance criteria against all open tickets.
- **No review step existed.** A read-only `reviewer` agent now gates every pull request.
- **Each ticket cost a separate "close" commit on main.** Tickets are now closed on their
  branch, before the pull request.

## 2026-10-16: v0.1 progress before the pipeline

Done by hand: workspace and CI, change IDs, points, line names, the view model, the gix
storage layer (`GitRepo`), snapshot and init. Remaining v0.1 work is
tracked as `PV-` tickets in `tasks.json`.

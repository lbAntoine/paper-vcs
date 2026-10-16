# Notes and decisions

Dated process lessons and project decisions that aren't architecture (those are ADRs).
Newest first.

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

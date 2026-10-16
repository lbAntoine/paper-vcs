# Workflow: the agent pipeline

Agents write the code. The main session (the **coordinator**) plans, isolates, checks and
merges. Every ticket goes through the same six phases, and agents communicate only through
the ticket file and short reports, never through conversation transcripts.

```
isolate → groom → RED (test-author) → GREEN (rust-engineer) → review (reviewer) → PR → merge
```

## 1. Isolate

The coordinator creates the worktree itself, with an explicit branch name, from fresh `origin/main`:

```console
T=PV-001 SLUG=save
git fetch origin
git worktree add "../paper-vcs.wt/$T" -b "feat/$T-$SLUG" origin/main
test "$(git -C "../paper-vcs.wt/$T" branch --show-current)" = "feat/$T-$SLUG"
```

The branch check happens **before** any agent starts. Worktrees live outside the repository
(`../paper-vcs.wt/`).

**Each worktree builds in its own `target/`**, passed to agents as
`CARGO_TARGET_DIR=<worktree>/target`. Never share one build directory between worktrees on
different branches: their crates have the same names, so cargo reuses the other branch's
artifacts and tests run against the wrong code. The cost is one full build per ticket.

**Parallelism:** at most two tickets at once, only with disjoint `Touches` lists, each in its
own worktree.

## 2. Groom

In the worktree, the coordinator writes `docs/tickets/PV-NNN-<slug>.md` from
[the template](tickets/TEMPLATE.md), sets the task to in progress in `tasks.json`, and commits
both as the branch's first commit: `docs: groom PV-NNN`. A pull request then contains the
whole story (ticket, RED, GREEN, review fixes, close) and `main` gets no grooming commits.

- **Context is pointers, not copies:** link spec sections and existing code instead of pasting them.
- **Conflict check:** compare the new ticket's `Touches` list and acceptance criteria with every
  open ticket branch. Two tickets touching the same file are sequenced (`Depends on`), never
  parallel. Two tickets defining the same behaviour are merged or one is rewritten before dispatch.
- A ticket is `Ready` only when every acceptance criterion is testable.
- Tickets are groomed when their work starts, not in advance: tickets rot when the code under
  them moves.

## 3. RED: test-author

Dispatch prompt: the ticket path, the absolute worktree path, and `CARGO_TARGET_DIR`.

The agent writes failing tests for every acceptance criterion, plus `todo!()` stubs so they
compile, appends to the handoff log, and commits `test: RED tests for PV-NNN …`.

**Coordinator check:** in the worktree, `cargo test --workspace` compiles; the new tests
fail on `todo!()` or assertions; every pre-existing test passes. A test that fails to
compile, or fails for an unrelated reason, goes back to the agent.

## 4. GREEN: rust-engineer

Same inputs. The agent implements until every test passes with clean fmt and clippy, keeps
the specs true, appends to the handoff log, and commits `feat|fix|refactor: … (PV-NNN)`.
It must not weaken tests: if a test is wrong, it reports `BLOCKED` with the reason.

**Coordinator check:** the full check suite passes in the worktree.

## 5. Review: reviewer

Read-only. The reviewer inspects `git diff origin/main...HEAD` against the ticket, the specs,
the ADRs and the hard rules in `CLAUDE.md`, and returns findings ranked `BLOCKER`, `MAJOR`,
`MINOR` or `NIT`, with a verdict:

- **APPROVE** (no blocker or major findings): the coordinator copies the verdict into the
  handoff log and moves on.
- **CHANGES**: the coordinator sends the findings to the *same* rust-engineer agent with
  `SendMessage`, so it keeps its context, then re-runs the review. After two rounds without
  approval, the coordinator escalates to the maintainer.

## 6. Pull request and merge

The coordinator:

1. Runs the full check suite once more.
2. Sets the ticket's status to `Done`, fills in its summary, marks the task done in
   `tasks.json`, and commits `chore: close PV-NNN` on the branch, so `main` gets no extra commit.
3. Pushes and opens the pull request:
   `gh pr create --title "PV-NNN: <title>" --body <summary + acceptance checklist + ticket link>`.
4. Waits for CI on Linux, macOS and Windows: `gh pr checks --watch`. A failure goes back to the
   rust-engineer (via `SendMessage` if the agent is still available) with the failing log.
5. Merges with a merge commit, so the RED and GREEN commits stay visible as proof of TDD:
   `gh pr merge --merge --delete-branch`.
6. Cleans up: `gh pr merge --delete-branch` also removes the local branch and its worktree.
   Then `git worktree prune` and `git pull --ff-only` in the main checkout.

## Communication

**Handoff log.** Each ticket ends with an append-only log. Every phase adds one entry:

```markdown
### RED · test-author · 2026-10-16
- Tests: crates/paper-core/tests/save.rs (7 tests)
- Failing on: todo!() in Project::save (6), assertion in save_with_paths_keeps_the_rest (1)
- Stubs: Project::save, SaveRequest, Saved
- Questions: none
```

GREEN entries list what was implemented, spec changes and any deviations. Review entries
(written by the coordinator) give the verdict and the findings.

**Reports.** Each agent ends with a report of 15 lines or fewer: `DONE` or `BLOCKED`, commit
hashes and subjects, the checks it ran with results, and open questions.

**Blocked.** An agent that finds the ticket ambiguous or contradicting the specs writes the
question in the handoff log and reports `BLOCKED` without guessing. The coordinator answers
(asking the maintainer if it's a product decision), records the answer in the ticket, and
resumes the same agent with `SendMessage`.

## Notes and decisions

Process lessons and decisions that aren't architecture go in
[`docs/tickets/NOTES.md`](tickets/NOTES.md), dated. Architecture decisions are ADRs in
`docs/specs/adr/`.

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

Every ticket has a GitHub issue (see [Tracking on GitHub](#tracking-on-github)). Grooming
reuses the ticket's issue if one exists and creates it otherwise, and writes its number in
the ticket's `Issue` field.

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
compile, or fails for an unrelated reason, goes back to the agent. Then two more checks, which
catch the tests that pass locally but fail on CI or catch nothing:

- **The new tests fail against `main` for the stated reason.** When the stubs allow it (the
  feature's API already exists, as for fixes), copy the new test files into a detached probe
  worktree of `origin/main` (under the ticket worktree's `target/`), run them there, and remove
  the probe afterwards. A new test that passes on `main` proves nothing and goes back.
- **The suite doesn't depend on the machine.** Run the new tests once with
  `GIT_CONFIG_GLOBAL` pointing at an empty file and `GIT_CONFIG_NOSYSTEM=1` (CI's git has no
  identity and defaults to `master`), and grep the new tests for Unix-only APIs outside
  `#[cfg(unix)]` and for platform-specific expected output without a `cfg!` branch.
- **Expectations that can't be checked yet aren't written yet.** The test-author doesn't
  hand-write snapshots of output that doesn't exist; GREEN accepts them after reading them.
  Every new or changed shared test helper (one that captures repository state, redacts output
  or builds fixtures) gets a sanity test that passes on `main` today, so a helper bug shows up
  in RED instead of blocking GREEN.

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
   The body says `Closes #NN` (the ticket's issue), so merging closes it.
4. Waits for CI on Linux, macOS and Windows: `gh pr checks --watch`. A failure goes back to the
   rust-engineer (via `SendMessage` if the agent is still available) with the failing log.
5. Merges with a merge commit, so the RED and GREEN commits stay visible as proof of TDD:
   `gh pr merge --merge --delete-branch`.
6. Cleans up: `gh pr merge --delete-branch` also removes the local branch and its worktree.
   Then `git worktree prune` and `git pull --ff-only` in the main checkout.

## Tracking on GitHub

Every ticket has an issue titled `PV-NNN: <title>`, with a link to the ticket file, labels and
a milestone. The ticket file stays the source of truth (scope, criteria, handoff log); the
issue is the public index.

- **Labels:** one `type:` (`feature`, `fix`, `perf`, `test`, `docs`, `chore`), one or more
  `area:` (`core`, `cli`, `git`, `diff`, `release`) and one `priority:` (`high`, `medium`, `low`).
- **Milestones:** one per roadmap version (`v0.1`, `v0.2`, …). A milestone is closed when its
  version is released and its issues are closed; open leftovers move to the next one.
- Review follow-ups that become a new ticket get their issue when the ticket is created in
  `tasks.json`, even before it is groomed.

## Releasing

Releases are built by [cargo-dist](https://axodotdev.github.io/cargo-dist/) (`dist`), pinned
in `dist-workspace.toml` (`cargo-dist-version`). `.github/workflows/v-release.yml` is generated
by `dist generate`: never edit it by hand. (It is `v-release.yml` because
`tag-namespace = "v"` is set; a stale `release.yml` must not exist, or both would run.) A
release publishes a GitHub Release with archives for five targets (macOS Intel and Apple
silicon, Linux x86_64 and aarch64, Windows x86_64), the shell and PowerShell installers and
SHA-256 checksums. There is no source tarball, Homebrew tap, MSI or crates.io publish yet.

The workflow runs on pull requests (it never publishes there) and on tag pushes. It starts a
release only for tags matching `v**[0-9]+.[0-9]+.[0-9]+*`: a `v` followed by anything, then a
semver such as `v0.1.0` or `v0.2.0-rc.1`. Only the maintainer approves a release.

1. Bump `version` in the root `Cargo.toml`, run `cargo build` to update `Cargo.lock`, bump the
   version-pinned installer URLs in the README's Install section, and add a `## X.Y.Z`
   section to `CHANGELOG.md` (cargo-dist uses it as the release body).
2. Run `dist plan` (and `dist generate --check` if `dist-workspace.toml` changed). Merge the
   pull request once CI is green.
3. Get the maintainer's approval to publish.
4. Tag the merged, CI-green commit: `git switch main && git pull --ff-only`, check that
   `git rev-parse HEAD` equals `git rev-parse origin/main`, then
   `git tag vX.Y.Z && git push origin vX.Y.Z`.
5. Watch the Release workflow without prompts:
   `gh run list --workflow=v-release.yml --event push --branch vX.Y.Z --limit 1 --json databaseId --jq '.[0].databaseId'`, then
   `gh run watch <id> --exit-status`. It builds the five targets, then creates the release.
6. Pre-releases only: cargo-dist marks a release as a pre-release only when the version has
   a suffix such as `-rc.1`. v0.1.0 is a pre-release without one, so right after publishing
   run `gh release edit v0.1.0 --prerelease`.
7. Smoke-test an installer on a clean machine or folder: run the README one-liner, open a new
   terminal, then `paper --version`, `paper init`, `paper save "x"`, `paper history`.

**If a build fails,** nothing is published: the release is created only after every build
succeeds. Delete the tag on the remote and locally (`git push origin --delete vX.Y.Z`,
`git tag -d vX.Y.Z`), fix the problem in a pull request, merge it, and tag again.

To upgrade cargo-dist, install the new version into the worktree
(`cargo install cargo-dist --locked --version X --root <worktree>/target/tools`), run
`dist init --yes` and review the regenerated workflow. Use a version at least two weeks old.

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

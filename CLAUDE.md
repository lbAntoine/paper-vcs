# paper-vcs

Version control for researchers: plain git underneath, a jj-inspired model on top.
A Rust workspace, pre-alpha.

## Read first

- [`docs/specs/README.md`](docs/specs/README.md): the design (concepts, CLI, architecture,
  ADRs). The specs are the source of truth. A change that deviates from them updates them
  in the same pull request.
- [`docs/workflow.md`](docs/workflow.md): how work is planned, isolated, implemented,
  reviewed and merged by the agent pipeline.
- [`docs/tickets/`](docs/tickets/): one file per ticket. `tasks.json` is the status index.

## Commands

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Tests need the `git` binary.

## Crates

Dependencies point one way: `paper-cli → paper-core → paper-git / paper-diff`.

| Crate | Role |
|---|---|
| `paper-core` | Domain model and commands (`Project`, `View`, `snapshot`, points, names) |
| `paper-git` | Everything that talks to gitoxide (`GitRepo`, ids, change IDs) |
| `paper-diff` | Tokenisers and diffs; pure functions, no git |
| `paper-cli` | The `paper` binary: argument parsing, prompts, rendering |

## Hard rules (every agent, every change)

1. `paper-core` never prints or prompts. It returns data, and front-ends render it.
2. No gix types in `paper-git`'s public API.
3. Plain git is a promise (ADR-0002). Code that writes to a repository is tested against a
   real temp repo and cross-checked with the `git` binary (`fsck --strict`, `log`, `status`).
4. Tests first. No mocks of git: use temp directories and real repositories.
5. Values over mutation: state transitions return new values (`View::with_line`).
6. Errors teach: what happened, why, what to run next, in researcher vocabulary
   (save, line, draft), not git jargon.
7. Clippy pedantic is clean. Public functions document `# Errors` and `# Panics`.
8. This is a public repository: no secrets, no personal data, no business or strategy content.
9. Conventional Commits (`feat`, `fix`, `test`, `refactor`, `docs`, `chore`, `perf`, `ci`),
   with a `Co-Authored-By:` trailer naming the model.

## Roles

The main session is the **coordinator**: it writes tickets, creates worktrees, dispatches
agents, checks each phase, opens pull requests and merges them. Project agents live in
`.claude/agents/`:

| Agent | Phase | Writes code? |
|---|---|---|
| `test-author` | RED: failing tests + `todo!()` stubs | Tests and stubs only |
| `rust-engineer` | GREEN: implementation + spec updates | Yes |
| `reviewer` | Review of the branch diff before the pull request | No (read-only) |

Agents only work inside the worktree they're given, and never merge, push, rebase or
switch branches.

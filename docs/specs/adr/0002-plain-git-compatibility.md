# ADR-0002: Every paper project is a plain, colocated git repo

- **Status:** Accepted · 2026-09-25

## Context

Researchers collaborate with people who won't install our tool, use hosts we don't control
(GitHub, GitLab, the Overleaf git bridge), and fear lock-in. Our business model (ADR-0007)
depends on the CLI being free and fully usable with any git host.

## Decision

- The project *is* a git repo. Paper's metadata lives in `.git/paper/` (never committed)
  and in `refs/paper/*` (never pushed). The only committed file is `paper.toml`.
- After every command, the git view is coherent: HEAD is attached to the current line's
  branch, the index matches the last save, and `git status` shows the draft as unstaged
  changes. New, untracked files show as untracked in git even though paper tracks them,
  and this is documented.
- Paper imports external git changes on its next run instead of refusing them
  ([03-architecture § Git interop](../03-architecture.md#git-interop)).
- Deleting `.git/paper/` leaves a perfectly normal git repo, and the only thing lost is
  undo history.

## Consequences

- We can't use storage tricks that git can't represent, such as jj-style first-class
  conflict trees. Conflicts are markers plus metadata.
- There is a permanent class of interop tests against the real `git` binary.

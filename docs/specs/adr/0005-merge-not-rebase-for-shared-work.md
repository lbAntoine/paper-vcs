# ADR-0005: `keep` and `sync` merge; only local drafts are rebased

- **Status:** Accepted · 2026-09-25

## Context

Rebasing gives a clean, linear history, but it rewrites commits. Once a save has been
pushed, rewriting it creates duplicate histories for co-authors, which is the worst git
experience a researcher can have.

## Decision

- `keep`: fast-forward if the origin line hasn't moved, merge otherwise.
- `sync`: fast-forward if only one side moved, merge if both did.
- **Drafts** are always rebased when their base moves, because they are never shared (I5).
- Paper never force-pushes.

## Consequences

- History shows merge commits (`Keep try/intro`, `Sync with origin/main`), which is honest
  and understandable. `history` shows only the first-parent line by default, so it still
  reads linearly.
- A "tidy history" feature (squashing a try into one save on keep) can be an opt-in flag later: `keep --as-one`.

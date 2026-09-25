# ADR-0001: Own engine on gitoxide, with jj as the mental model

- **Status:** Accepted · 2026-09-25

## Context

We want jj's user model: the working copy is a commit, there is no dirty state, every
operation can be undone, and conflicts never block. We also want full control over
behaviour and vocabulary.

## Options

| Option | Pros | Cons |
|---|---|---|
| **A. Build on `jj-lib`** | Model comes for free, battle-tested | API is unstable across releases. jj's model leaks through (bookmarks don't advance, evolog, revsets). Hard to diverge from it, e.g. with per-line drafts (ADR-0004) |
| **B. Shell out to the `git` CLI** | Simplest, full git feature set | Parsing text output is fragile, forks are slow (status under 100 ms is hard), and the git model fights ours |
| **C. Own engine on `gix` (gitoxide)** ✅ | Full control, pure Rust, fast, good fit for a library used by a GUI | We re-implement snapshot, the operation log and undo ourselves. Some gix features are still maturing |

## Decision

**Option C.** Implement our own model (draft, lines, activity log) on top of `gix` for
objects, refs, trees, status and merge. We read jj's design docs and source as the
reference for the concepts. We don't depend on its code.

**Network operations (`fetch`, `push`, `clone`) shell out to the `git` binary.** We get
credential helpers, SSH configuration and every hosting quirk for free, and this is where
gix has historically been least complete. Every git access goes through the `Store`
trait, so any gix gap (e.g. tree merge or checkout edge cases) can temporarily fall back
to `git` without touching `paper-core`.

## Consequences

- More work up front, especially in v0.2 (operation log). This is accepted because it is
  the most interesting part of the engine.
- Before each milestone, check gix's feature status and record any fallbacks here.
- Users need `git` installed to sync. Researchers who sync already have it.

## Implementation notes

- **2026-09-28, v0.1:** `paper-git` exposes a concrete `GitRepo` struct rather than a
  `Store` trait. There is only one implementation, so a trait would add generics without
  benefit. Extract one when a second implementation (a `git` subprocess fallback or a
  test fake) actually appears. No gix types appear in the public API.
- **gix 0.88 gaps found so far:**
  - The empty tree is virtual in gix, so it must be written explicitly or `git fsck` reports it missing.
  - Updating any ref fails when no committer is configured, so paper installs its own
    identity as the fallback (`committer_or_set_fallback`).

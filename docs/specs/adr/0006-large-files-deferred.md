# ADR-0006: Guard rails for large files in v0.x; LFS-compatible storage after 1.0

- **Status:** Accepted · 2026-09-25

## Context

Research projects contain datasets, checkpoints and figures. Git handles large blobs badly,
and GitHub rejects files over 100 MB. A proper solution (a content-addressed side store
with pointer files) is a sizeable project of its own.

## Options

1. Our own pointer-file store. Full control, but no host understands it.
2. **Git LFS compatible** ✅ (later). The LFS pointer format and batch HTTP API are simple
   and documented, and GitHub, GitLab and most other hosts already serve LFS.
3. Delegate to DVC or DataLad. Good tools, but adding a second tool breaks "one simple tool".

## Decision

- **v0.x:** guard rails only. Warn above `warn_mb` (10), and don't track above `max_mb`
  (100), which appears under "Skipped" in `status`. See [04-content § Large files](../04-content.md#large-files).
- **Post-1.0:** implement an LFS-compatible client in `paper-git` (pointer files, smudge
  and clean equivalents inside snapshot and checkout, batch API over HTTPS).

## Consequences

Until then, datasets live outside the repo, and the docs point to Zenodo, OSF or DVC.
Milestone PDF attachments are blocked on this decision (README open question 2).

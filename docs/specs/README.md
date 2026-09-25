# paper-vcs — Specifications

Status: **Draft v0.1** · Last updated: 2026-09-25 · Scope: the `paper` CLI and its libraries

paper-vcs is a version control system for researchers. It stores everything as plain git,
and its day-to-day model is borrowed from Jujutsu (jj): there is no dirty state, every
action can be undone, and there is no staging area.

## Documents

| # | Document | What it settles |
|---|---|---|
| 00 | [Vision](00-vision.md) | Problem, personas, scope, principles, non-goals |
| 01 | [Concepts & glossary](01-concepts.md) | The user-facing model: draft, save, line, try, milestone, activity |
| 02 | [CLI reference](02-cli.md) | Every command, flag, output and exit code for v0.x |
| 03 | [Architecture](03-architecture.md) | Crates, engine, on-disk format, operation log, git interop, crash safety |
| 04 | [Content handling](04-content.md) | Any file type, prose (word-level) diff, profiles, large files |
| 05 | [Roadmap](05-roadmap.md) | Small shippable versions with acceptance criteria |
| ADR | [Decision records](adr/) | Why we chose what we chose |

## Decisions so far

| ADR | Decision |
|---|---|
| [0001](adr/0001-own-engine-on-gitoxide.md) | Own engine on gitoxide (`gix`), with the jj model re-implemented; network ops via the `git` binary |
| [0002](adr/0002-plain-git-compatibility.md) | Every paper repo is a plain, colocated git repo. No lock-in, ever |
| [0003](adr/0003-change-ids-in-commit-headers.md) | Stable change IDs stored as an extra git commit header |
| [0004](adr/0004-per-line-drafts.md) | Each line has its own draft; lines auto-advance on save (deliberate deviation from jj) |
| [0005](adr/0005-merge-not-rebase-for-shared-work.md) | `keep` and `sync` merge (never rewrite shared history) |
| [0006](adr/0006-large-files-deferred.md) | Large files: guard rails in v0.x, Git-LFS–compatible storage post-1.0 |
| [0007](adr/0007-licensing-and-repos.md) | Apache-2.0 OR MIT; `paper-core` is published as a reusable library |
| [0008](adr/0008-office-documents-diff-not-merge.md) | Office documents (docx, xlsx, xls, ods, csv) are versioned and diffed, never merged automatically |

## Open questions

Tracked here until they become ADRs.

1. **Binary name.** Is `paper` taken or confusing on Homebrew, conda-forge or winget? The fallback is `ppr`.
2. **Milestone attachments** (for example, the compiled PDF at submission). Where should the blob live so it gets pushed? This depends on ADR-0006.
3. **Jupyter notebooks.** Should outputs be stripped by default in the `jupyter` profile, the way nbstripout does?
4. **Co-authorship.** Should `paper save --with <name>` add `Co-authored-by:` trailers? This is cheap and fits papers well.
5. **jj interop.** Could we write the change-id header in exactly jj's format, so a repo works with both tools? We need to check this against jj's current source first.
6. **Commit signing.** Is this out of scope for v1?

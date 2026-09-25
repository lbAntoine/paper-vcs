# 00 — Vision

## Problem

Researchers keep papers, code and analysis in files that change constantly. Their options
today are poor:

- **Git** is powerful but hostile. The staging area, detached HEAD and rebase all cause
  trouble, the index is invisible state, and it is easy to feel that work has been lost.
  Many researchers use git as nothing more than "a way to upload to GitHub".
- **Line diffs are useless for prose.** A paragraph on one line shows up as "1 line changed"
  even when a single word moved.
- **Research milestones are not modelled.** "Submitted to NeurIPS", "reviewer response"
  and "camera-ready" end up as ad-hoc tags, copies of folders or `paper_final_v3_REAL.tex`.
- **Nothing is safe to try.** "Let me restructure section 3 and see if it reads better"
  requires knowing how to branch.

## Scope

This specification covers the `paper` CLI. From day one, the CLI is a thin shell over a
library crate (`paper-core`) that other front-ends can reuse.
See [03-architecture](03-architecture.md).

## Personas

**Mina, ML PhD student.** Writes a LaTeX paper next to Python experiments and uses git
through VS Code buttons. She is scared of `git rebase` and has lost work to
`git checkout -- .`. She wants to be unable to lose work and to see word-level changes
from her co-author.

**Tomás, computational biology postdoc.** Works in Quarto or Markdown with R scripts and
some large data files. His repo exploded once when he committed a 2 GB file. He wants
guard rails on big files and clear "submission" snapshots to point reviewers at.

**Prof. Okafor, PI (secondary).** Reads rather than writes, and sends comments back as a
`.docx`. Wants "what changed since the version I reviewed?" in one command, including for
the Word file.

## Principles

1. **You cannot lose work.** Every command first snapshots the working files, and every
   operation can be undone.
2. **There is no dirty state.** No staging area, no stash and no "commit before you switch".
3. **Say it in researcher words.** Commands use the vocabulary in
   [01-concepts](01-concepts.md). Git words appear only in `--help` for people who want them.
4. **Plain git underneath.** Any git tool, host or collaborator still works.
5. **Prose is a first-class citizen.** Word-level diff is the default for text.
6. **Opinionated over configurable.** One good workflow beats five options. Add config
   only when real users ask for it.
7. **Errors teach.** Every error says what happened, why, and the exact command to run next.

## Non-goals

- Replacing git for software engineers. Monorepo scale, submodules and rebase-heavy workflows are out.
- Merging Word or Excel files. `.docx`, `.xlsx`, `.xls`, `.ods` and `.csv` are versioned
  and get a readable diff (text-level for documents, cell-level for spreadsheets). Parallel
  edits to the same file are never merged automatically. See [ADR-0008](adr/0008-office-documents-diff-not-merge.md).
- Google Docs, Sheets or other cloud-only documents. There is no file to version.
- Hosting, a GUI or a web UI. This project is the CLI and its libraries.
- Multiple workspaces per repo (jj's `workspace`).
- Word-level merging. Diffs are word-level, but merges are line-level and git-compatible.
- Telemetry. We never phone home.

## Quality bar

- CI on macOS, Linux and Windows.
- 80%+ test coverage on `paper-core`.
- The design is documented (this folder) and kept in sync with the code.

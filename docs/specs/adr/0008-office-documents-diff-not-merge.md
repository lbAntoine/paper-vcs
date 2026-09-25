# ADR-0008: Office documents are versioned and diffed, never merged automatically

- **Status:** Accepted · 2026-09-25

## Context

Even computational researchers receive `.docx` files from supervisors and journals, and
`.xlsx`/`.csv` files with results, and many researchers work in Word and Excel daily. Git
treats all of these as opaque binaries: you can't see what changed, and parallel edits
conflict with no help. Supporting them well is something no git host offers.

Office files are zip archives of XML (OOXML), except for legacy `.xls`, which is BIFF binary.
Mature Rust crates read them: `zip`, `quick-xml` and `calamine` (xlsx, xlsm, xls, ods).

## Options

| Option | Verdict |
|---|---|
| A. Treat as binary (the status quo in the first draft) | Too weak. Loses the differentiator |
| **B. Version and diff, but don't merge** ✅ | High value, moderate effort (~3–4 weeks), no risk of producing broken files |
| C. Also merge automatically (3-way merge of the XML) | High risk: a wrong merge produces a file Word can't open, or silently loses edits. Word's own Compare/Combine already does this job well |
| D. Convert via pandoc or LibreOffice | External dependency, slow, and a lossy round trip. Rejected for the core. Could be an optional plugin later |

## Decision

**Option B.**

- **Store:** unchanged. Office files are ordinary blobs. Saving, restoring, undo and milestones already work.
- **Diff:** `.docx` gets a text-level word diff, reusing `paper-diff`'s tokenisers with
  paragraphs and headings as context. Spreadsheets and csv/tsv get a cell-level diff
  (rows lined up first, then cells). Things out of scope (formatting, tracked changes,
  comments, equations, images) are *reported briefly, never silently dropped*.
- **Merge:** never. Conflicts in any non-text file produce a `.theirs` copy next to yours,
  plus a tip about the file type (Word: Review → Compare → Combine Documents). They are
  resolved by deleting the copy or with `paper resolve`.
- **Safety:** Office files must pass a completeness check before being snapshotted, lock
  files are ignored everywhere, and `init` warns about cloud-sync folders.

Full specification: [04-content § Office documents](../04-content.md#office-documents).

## Consequences

- `paper-diff` gains `zip`, `quick-xml`, `calamine` and `csv`. They are pure Rust, so
  cross-compilation is unaffected.
- A permanent test set of real files saved by Word, Excel, LibreOffice and Google Docs export.
- Storage grows by roughly a full copy per save for Office files, because zips don't delta well.
  Mitigated by a warning now, and a post-1.0 rezip filter (with ADR-0006).
- Office conflicts are prevented better than resolved. File locking ("X is editing
  manuscript.docx") would need a server and is out of scope for the CLI.
- Scheduled as roadmap v0.7. It may move earlier if user feedback shows most researchers
  exchange Office files.

# 05 — Roadmap

Each version is small, shippable and demo-able. Each ships with a GitHub Release and a
short changelog entry written in researcher words. The target is **one version every
2–3 weeks**. Plain git is always available underneath, so every version is usable on its
own. Starting at v0.3, users can already share work by running `git push`.

## v0.1: Walking skeleton (local, single line)

`init`, `status`, `save` (with paths), `history`, `show`, `diff --lines`, `restore`, `config`.

- [x] Cargo workspace with 4 crates, CI on 3 OSes, `cargo-dist` release pipeline (GitHub Release, shell and PowerShell installers)
- [x] `Store` trait with the gix implementation: read and write objects, trees, refs, commits with the `change-id` header
- [x] Draft model: `refs/paper/draft/main`, snapshot with stat cache, automatic tracking of new files
- [x] Git mirror (I6): HEAD, branch and index kept in sync. `git log` and `git status` look sensible
- [x] Adoption of an existing git repo
- **Accept** (automated in `crates/paper-cli/tests/acceptance.rs`): a researcher can init a LaTeX paper, edit, save 10 times, view history, restore
  a file from 3 saves ago, and push with plain `git push`.

## v0.2: Nothing is ever lost

`undo`, `redo`, `activity`, `undo --to`, `gc`.

- [ ] Operation log, view model, head pointer with atomic rename
- [ ] Keep-alive chain, crash recovery (steps 2 and 8 of the lifecycle), fault-injection tests
- [ ] Proptest for I4: undo restores the exact view and the exact working files
- **Accept:** a random 200-step session can be fully undone step by step, and redone.

## v0.3: Reads like prose (**first public alpha**)

Word diff, profiles, large-file guard rails, `--json` everywhere.

- [ ] `paper-diff`: generic, CJK and LaTeX tokenisers, paragraph grouping, colour and plain rendering
- [ ] Six built-in profiles, idempotent `.gitignore` block, auto-detection
- [ ] `warn_mb` / `max_mb`, "Skipped" in `status`, `help large-files`
- [ ] README, a 2-minute asciinema demo, install docs
- **Accept:** a PI can run `paper diff --since <point>` on a co-author's changes and read it as prose.

## v0.4: Safe experiments

`try`, `keep`, `drop`, `switch`, `lines`, `line new`.

- [ ] Per-line drafts (I2, I3), with the draft moving on `try`
- [ ] Fast-forward or merge on keep, in-progress keep with conflicts, `keep --abort`
- [ ] Conflicts in non-text files: `.theirs` copies (never tracked), `paper resolve`. These are needed as soon as images or PDFs can conflict
- [ ] Interop: `git checkout` and branch creation imported correctly
- **Accept:** a user can have 3 tries open with unsaved work in each, switch freely, keep one, drop one, and lose nothing.

## v0.5: Milestones

`milestone`, `milestones`, `milestone --delete`, `diff --since <milestone>`, milestones shown in `history`.

- [ ] Annotated tags with the `Paper-Milestone: 1`, `Venue:` and `Description:` trailers
- **Accept:** "what changed since the NeurIPS submission" is a single command.

## v0.6: Collaboration (**public beta**)

`clone`, `remote`, `sync`, `share`.

- [ ] Fetch and push via the `git` binary, shared and local lines, pushing milestones
- [ ] Sync merge flow with conflicts in progress, exit code 2, never pushing an unresolved merge
- [ ] Works with GitHub, GitLab, Codeberg and the Overleaf git bridge (manual test matrix)
- [ ] conda-forge package
- **Accept:** two co-authors on different machines (one using plain git) collaborate on a paper for a week without anyone touching `git` commands.

## v0.7: Office documents

Text diff for `.docx`, cell diff for `.xlsx`/`.xlsm`/`.xls`/`.ods`/`.csv`/`.tsv`, and safe snapshots. Decided in [ADR-0008](adr/0008-office-documents-diff-not-merge.md).

- [ ] `paper-diff::office`: paragraph extraction from `.docx` (including footnotes and endnotes), with the nearest heading as context
- [ ] Table diff (rows lined up first, then cells), shared by csv/tsv and spreadsheets via `calamine`
- [ ] Short reports for things out of scope: formatting only, tracked changes, comments, equations, images
- [ ] Completeness check for zips and `.xls` at snapshot time, and handling of files Windows has locked
- [ ] Office lock files in every profile's ignores, and the cloud-sync folder warning at `init`/`clone`
- [ ] Storage warning when one Office file's history exceeds 100 MB
- [ ] Word-specific tips for non-text conflicts (Review → Compare → Combine)
- **Accept:** a supervisor's edited `manuscript.docx` shows a readable word diff against the
  submitted milestone. A results `.xlsx` shows exactly the cells that changed. Saving while
  Word is still writing never records a corrupt file.

## v0.8: Hardening

- [ ] Every row of the git-interop table covered by tests
- [ ] Windows parity (paths, locking, long paths), performance targets met on the reference repo
- [ ] `paper help` topics complete, and every error has a `next:` suggestion

## v1.0: Stable

- On-disk format `version = 1`, with a migration path promised
- The CLI surface and the `--json` schema are frozen under semver

## Later backlog (post-1.0, unordered)

Storage compatible with Git LFS · `.bib`-aware merge · notebook output stripping ·
paragraph-move detection · `save --with <coauthor>` trailers · milestone PDF attachments ·
jj interop for change IDs ·
repacking Office zips uncompressed inside git for cheaper storage · `.odt`/`.pptx` text diff ·
showing Word's tracked changes individually.

# 04 — Content Handling

**Stance:** paper-vcs accepts *any* file. There is no extra engine work per file type,
because git stores bytes. The only per-format work is optional polish (nicer diffs, sensible
ignores), and it is added through small, declarative profiles. Nothing is restricted. We
prioritise the formats researchers actually use.

## File classes

| Class | Detection | Stored | Diff |
|---|---|---|---|
| **Text / prose** | Valid UTF-8 and no NUL byte in the first 8 KB, or a known text extension | git blob | Word diff (default) or line diff |
| **Office document** | `.docx`, `.xlsx`, `.xlsm`, `.xls`, `.ods`. Also `.odt` post-1.0 | git blob | Text diff (documents) or cell diff (spreadsheets). See [Office documents](#office-documents) |
| **Table** | `.csv`, `.tsv` (text, but tabular) | git blob | Cell diff (default) or line diff |
| **Binary** | Everything else, or a known binary extension (`.pdf`, `.png`, `.h5`, …) | git blob | One-line summary |
| **Oversized** | Size > `large_files.max_mb` | **Not tracked** (I7) | Listed under "Skipped" in `status` |

## Diff

### Line diff (`--lines`)

`paper diff --lines` compares files line by line with the patience algorithm, which gives
more readable hunks than Myers. The output is git's unified format with 3 lines of context
(the library takes the context as an option):

- Header `--- a/<path>` and `+++ b/<path>`; an added file uses `/dev/null` as the old label,
  a deleted file as the new one.
- Hunks start with `@@ -<line>,<count> +<line>,<count> @@`. Line numbers are 1-based, a count
  of 1 is omitted, and an empty side is `0,0`. Edits closer than twice the context merge into
  one hunk. As in git, the header ends with the nearest earlier old line that starts with a
  letter, `_` or `$` (at most 80 bytes), when there is one.
- A last line without a final newline is followed by `\ No newline at end of file`.
- A file is text when it is valid UTF-8 with no NUL byte in its first 8 KiB; otherwise it is
  binary. If either side is binary, no hunks are produced: the result is a one-line summary
  with the old and new sizes in bytes.

### Word diff (default for text from v0.3)

1. **Tokenise** each version. The tokeniser is chosen by file extension:
   - **Generic** (all text): Unicode word boundaries (UAX #29) via `unicode-segmentation`.
     Whitespace runs and punctuation are separate tokens.
   - **CJK**: Han, Hiragana, Katakana and Hangul runs without spaces are split per grapheme,
     so a Korean or Japanese edit shows only the changed characters.
   - **LaTeX** (`.tex`, `.bib`, `.sty`, `.cls`): `\command` and `\command*`
     are single tokens, and `%` comments are one token.
   - **Markdown and Typst**: generic in v0.3. Format-aware tokenisers come later if users ask.
2. **Diff** the token streams with the patience algorithm (`similar`), and group
   the hunks by paragraph (a blank line separates paragraphs).
3. **Render.** On a colour TTY, deletions are red with strike-through and insertions green,
   inline. Without colour, use `[-old-]{+new+}`. The header shows `+N words −M words`.

```
sections/intro.tex  +12 words −4 words
  ¶ 3
  Recent work has [-mostly-]{+largely+} focused on supervised settings
  {+, although self-supervised approaches are gaining traction+}.
```

`--lines` gives a classic unified diff, which is useful for code and for piping to other
tools. `--stat` gives a per-file summary only.

### Binary summary

```
figures/ablation.pdf   changed   318 KB → 331 KB
data/results.csv       too large for word diff (48 MB) — use --lines
```

For text files over 5 MB, word diff falls back to a line summary and the output says so.

### Table diff (`.csv`, `.tsv`, and spreadsheets via the Office extractor)

1. Parse into rows (`csv` crate; delimiter sniffed for `.csv`). If the first row looks
   like a header, use it as the column names.
2. **Line up the rows** by diffing them as whole units, the way a line diff lines up
   lines. Inserted or deleted rows then don't show up as "every cell below changed".
3. For rows that changed, compare cell by cell. Numbers are compared as numbers, so
   `0.850` versus `0.85` counts as unchanged. If the header row changed, report
   column additions, removals and renames.
4. Render:

```
results/metrics.csv  +3 rows −1 row, 4 cells changed
  row 7  (model=resnet50)   accuracy: 0.83 → 0.85
  row 12 (model=vit-b)      f1:       0.71 → 0.74
  + row 18  (model=convnext, accuracy=0.86, f1=0.77)
  − row 9   (model=baseline-old, …)
```

The row label uses the value in the first column, when that column's values are unique.
Above 50,000 rows, the output falls back to a summary (`+N rows −M rows, K cells changed`),
and `--lines` gives the raw line diff.

### Future diff work (post-1.0, not specified here)

Detecting moved paragraphs, cleaning `.ipynb` output before diffing, extracting PDF text
for diffs, and visual image diffs (better suited to a graphical front-end).

## Office documents

Decided in [ADR-0008](adr/0008-office-documents-diff-not-merge.md): **Office files are
versioned and diffed, but never merged automatically.**

### Text diff for `.docx`

A `.docx` file is a zip of XML files. The extractor, in `paper-diff/office`, works like this:

1. Open the zip (`zip` crate) and stream `word/document.xml` (`quick-xml`).
2. Rebuild the plain text paragraph by paragraph: join the text runs inside each
   paragraph, turn tabs and line breaks inside the document into whitespace, and flatten
   table cells into `cell | cell | cell` paragraphs.
3. Also extract footnotes and endnotes (`word/footnotes.xml`, `word/endnotes.xml`) as
   extra sections labelled `[footnote 3]`.
4. Put the paragraphs through the **same word diff** as prose (generic or CJK tokeniser),
   grouped by paragraph and headed by the nearest heading (`Heading1`–`Heading3` styles):

```
manuscript.docx  +42 words −17 words
  § 2 Related Work, ¶ 3
  Recent work has [-mostly-]{+largely+} focused on supervised settings.
```

**Out of scope for v1** (each is reported briefly rather than silently dropped):

| Content | What `diff` says |
|---|---|
| Formatting-only changes (bold, fonts, styles) | `formatting changed (no text changes)` |
| Metadata/ID churn after re-saving in Word | `no text changes` (exit 0, nothing to show) |
| Word's own tracked changes (the edits Word marks as inserted or deleted) | Text is shown **as if all tracked changes were accepted**, followed by one line: `contains 12 tracked changes` |
| Comments | `+2 comments −1 comment` (count only) |
| Equations (Word's equation format), embedded images and charts | `equation changed in ¶ 14`, `image replaced: media/image3.png` |

### Cell diff for `.xlsx`, `.xlsm`, `.xls`, `.ods`

1. Read each sheet with `calamine`, which handles all four formats, including the old binary `.xls`.
2. Sheets are matched by name, and added, removed or renamed sheets are reported.
3. Each sheet goes through the **table diff** above, labelled `Sheet!A1`-style when
   there's no header:

```
results.xlsx  2 sheets changed
  Summary!B7     0.83 → 0.85
  Summary!B8     =AVERAGE(B2:B6) → =MEDIAN(B2:B6)     (value 0.79 → 0.80)
  + sheet "Ablation" (14 rows × 6 cols)
```

Formulas are compared when present, and values otherwise. When a formula changes, the
output shows both the formula and its value. Charts, cell formatting and macros are out of
scope and reported as a count.

### Snapshotting Office files safely

- **Half-written files.** Word and Excel write in several steps. Before snapshotting an
  Office file, paper checks it's a complete zip (the end-of-central-directory record
  exists and the entry count matches). For `.xls`, it checks the file's internal header.
  If the check fails, paper keeps the previous version in the draft and reports
  `manuscript.docx is being written — kept previous version`. The next command takes the
  snapshot.
- **Locked files (Windows).** If the file can't be read because another program has it
  open, paper does the same as above and suggests closing the document.
- **Lock files** created by Office and LibreOffice are ignored in every profile (see below).

### Storage cost

Office files are compressed zips, so git's delta compression barely helps: every save
stores roughly a full copy. `status` shows the running total when a single Office file's
history exceeds 100 MB:
`manuscript.docx history uses 240 MB (120 saves). Images inside the document are the usual cause.`
Post-1.0, repacking these zips uncompressed inside git (a "rezip" filter) would help, and
that goes with the storage work in [ADR-0006](adr/0006-large-files-deferred.md).

### Cloud-sync folders

A `.git` folder inside OneDrive, Dropbox, iCloud Drive or Google Drive can be corrupted
by the sync client. Researchers who use Word often work in exactly these folders. At
`init` and `clone`, paper detects the known sync roots (by path and by marker files). It
then warns once and asks for confirmation: `This folder is synced by OneDrive. Syncing a
paper project this way can corrupt its history. Use paper sync instead, or move the
project outside OneDrive.` The warning is also shown by `paper status --verbose`.

## Merging

Merging is **line-based and git-compatible**, because merges must stay understandable to
plain git and to co-authors on other tools. Conflicts are written as standard markers
with the labels given in researcher words:

```
<<<<<<< yours (main)
Recent work has largely focused on supervised settings.
||||||| before
Recent work has mostly focused on supervised settings.
=======
Recent work has mostly focused on supervised and weakly-supervised settings.
>>>>>>> theirs (origin/main)
```

### Conflicts in anything that isn't plain text

This covers Office documents, tables in Office formats, images and PDFs. Paper never merges
inside these files. When both sides changed the same one:

1. The working file stays **yours**, unchanged.
2. **Their** version is written next to it as `<stem>.theirs.<ext>`, e.g.
   `manuscript.theirs.docx`. The copy is never tracked (I7), and paper records it in the
   line's pending-conflict metadata.
3. `status` lists the conflict with a hint for the file type:

```
Conflicts (1):
  manuscript.docx   both changed — theirs is in manuscript.theirs.docx
    Tip: in Word, use Review → Compare → Combine Documents, save the result
         as manuscript.docx, then delete manuscript.theirs.docx
```

4. The conflict counts as resolved when the `.theirs` copy has been deleted, or when you
   run `paper resolve <path> --ours|--theirs`. `save` refuses while any `.theirs` copy
   exists, unless given `--with-conflicts`.

`.csv`/`.tsv` are text, so they use line-based merging like any other text file.

**Nudge rather than enforce.** Prose written one sentence per line merges far better.
Profiles for `latex`, `markdown` and `typst` add one tip to `paper help concepts`, and when
a conflict involves a line over 400 characters, the output adds a single hint. There is no linter.

## Profiles

A profile is a built-in TOML file. It is cheap to add, and community pull requests are welcome.

```toml
# profiles/latex.toml
name = "latex"
detect = ["*.tex"]
ignore = [
  "*.aux", "*.log", "*.out", "*.toc", "*.lof", "*.lot", "*.fls", "*.fdb_latexmk",
  "*.synctex.gz", "*.bbl", "*.blg", "*.bcf", "*.run.xml", "_minted-*/", "build/",
]
text_extensions   = ["tex", "bib", "sty", "cls", "bst"]
binary_extensions = ["pdf", "png", "jpg", "eps"]
tokenizer = { "tex" = "latex", "bib" = "latex" }
tips = ["Write one sentence per line: diffs and merges become much cleaner."]
```

| Profile | Detect | Notable ignores |
|---|---|---|
| `latex` | `*.tex` | aux files, `build/`. The compiled PDF is **not** ignored by default, so the user decides |
| `markdown` | `*.md` without other signals | `_site/`, `.cache/` |
| `typst` | `*.typ` | none (Typst is clean), `*.pdf` optional |
| `quarto` | `_quarto.yml` | `_freeze/`, `_site/`, `*_files/`, `.quarto/` |
| `jupyter` | `*.ipynb` | `.ipynb_checkpoints/`. Output stripping is an open question |
| `generic` | fallback | `.DS_Store`, `Thumbs.db`, editor swap files |

Every profile also includes the generic ignores, plus:
- common Python and R environment folders (`.venv/`, `__pycache__/`, `.Rproj.user/`, `renv/library/`);
- Office and LibreOffice lock and temporary files (`~$*`, `.~lock.*#`, `~WRL*.tmp`, `*.tmp` next to Office files).

There's no `office` profile. Office files work in every profile, because a LaTeX project
often includes a supervisor's `.docx` or a results `.xlsx`.

Profiles only ever **add** lines to `.gitignore`, inside a marked block, so re-running is
idempotent and user lines are never touched:

```
# >>> paper-vcs profile: latex >>>
...
# <<< paper-vcs profile: latex <<<
```

## Large files

**v0.x (guard rails)**, per [ADR-0006](adr/0006-large-files-deferred.md):

- Over `warn_mb` (10 MB): tracked, with a warning on the first snapshot:
  `data/model.ckpt (48 MB) is now tracked. Large files make the project slow to share.`
- Over `max_mb` (100 MB): **not tracked**. It is shown under "Skipped" in `status` with
  its own entry in `paper help large-files`, which explains the options: add it to
  `.gitignore`, store it elsewhere (Zenodo, OSF, DVC), or raise the limit knowingly.
- A file that is already tracked and then grows past `max_mb` stays tracked, and it gets
  a loud warning instead of silently disappearing from the draft.

**Post-1.0:** storage compatible with Git LFS, so that GitHub, GitLab and other
LFS-capable hosts all serve the same large files.

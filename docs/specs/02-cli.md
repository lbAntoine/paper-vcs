# 02 — CLI Reference

The binary is `paper` (the name is still an open question, see README). The **Since**
column maps each command to the [roadmap](05-roadmap.md).

## Global behaviour

| Flag | Meaning |
|---|---|
| `-C <path>` | Run as if started in `<path>` |
| `--json` | Machine-readable output. Every command supports it, and the schema is versioned (`"schema": 1`) |
| `--no-snapshot` | Skip the automatic snapshot (I1). For scripts and editors only |
| `-q, --quiet` / `-v, --verbose` | Less output / more output. `-q` prints only errors (and `--json` output). Verbose adds git terms in parentheses, e.g. `Saved kmrw: Tighten intro (commit 3fa9c1e)` |
| `--color <auto\|always\|never>` | Respects `NO_COLOR` under `auto`. Without the flag, `PAPER_UI_COLOR` or `ui.color` in the user config decides (default `auto`) |

**Project discovery:** walk up from the current directory until a `.git` is found. If the
repo has no `.git/paper/` yet, commands other than `init` and `clone` fail with:
`This is a git repo but not a paper project yet. Run: paper init`.

Every command except `init`, `--help` and `--version` records the working files first
(I1), unless `--no-snapshot`. `save` does so as part of saving.

**Paths** on the command line (`save`, `history`, `diff`, `restore`)
are relative to where you run paper (or to `-C`), so `paper history intro.tex` works from
`sections/`. `./` and `\` (on Windows) are accepted, `.` means that folder, and a path
outside the project is a user error. On Windows, a root-relative path (`\a`) or a
drive-relative one (`C:a`) is a user error too ("use a path relative to this folder or a full
path"), because it names no folder paper can place.

The identity (`user.name`, `user.email`) comes from `PAPER_USER_NAME` / `PAPER_USER_EMAIL`,
then the paper user config, then git config. If it is missing and stdin is a terminal, the
command asks once (`Your name:`, `Your email:`), stores both in the user config, says where,
and continues. Without a terminal it fails with `paper config set user.name …` and
`user.email …` first, then the two `git config --global user.…` commands.

**Exit codes**

| Code | Meaning |
|---|---|
| 0 | Success, including a friendly no-op such as "nothing to save" |
| 1 | User error: bad arguments (including unknown flags and missing arguments, where clap would exit with 2), unknown point, etc. |
| 2 | Needs attention: unresolved conflicts, or a remote that has diverged and needs sync |
| 3 | Project error: corrupted state, another paper command held the project for the whole 5 s wait ("Another paper command is running in this project"), git binary missing for network ops |
| 101 | Bug (panic). Prints the issue-tracker URL and a report file path |

A panic writes `crash-<timestamp>.txt` to the user cache folder (`$XDG_CACHE_HOME/paper` or
`~/.cache/paper` on Unix, `%LOCALAPPDATA%\paper` on Windows). Debug builds panic on purpose
when `PAPER_DEBUG_PANIC=1` is set, so tests can exercise this path.

**Error format** (always three parts):

```
error: nothing to keep — you are on "main", which is not a try
  why:  keep folds a try back into the line it came from
  next: paper lines            # see your tries
        paper keep try/intro   # keep a specific one
```

Errors go to stderr. With `--json`, an error is one object on stdout with the same exit
code: `{"schema":1,"error":{"kind":"user","message":…,"why":…,"next":[{"command":…,"explanation":…}]}}`,
where `kind` is `user`, `needs_attention` or `project`. Argument errors under `--json` use the
same object (exit 1); the `--json` flag is looked for only before a `--` argument. Every error
has all three parts: errors of kind `project` suggest reporting the problem at the issue
tracker, except the busy-project error (another paper command is running), which is not a bug.
A suggestion's `command` is usually a command to run, but it may be a URL (the issue tracker)
or a short instruction when no single command fits, and the `explanation` then carries the prose ("report it, including what you ran and
the message above").

**JSON output:** one object per command on stdout, with `"schema": 1`. Times are RFC 3339
in the author's UTC offset. Change kinds are `added`, `modified` or `deleted`. Every key is always present, with `null`
when a value doesn't apply (e.g. the ID of a save that wasn't made).

*File diffs* (`diff`, and `files` in `show`) share one shape, an array of
`{path, kind, binary, added, removed, old_bytes, new_bytes, hunks}`. For a text file `added`
and `removed` are line counts, the byte sizes are `null` and `hunks` lists
`{old_start, old_len, new_start, new_len, lines}`, each line being
`{kind: "context"|"added"|"removed", text, missing_newline}` (`missing_newline` marks the
file's last line when it has no trailing newline). For a binary file `added` and `removed`
are `null`, the byte sizes are set (`null` for an absent side) and `hunks` is `[]`.
`paper diff --json` prints `{schema, files}` (also with `--stat`), `paper show --json`
adds `files` beside `save` and `changes`, and `paper status --json` prints
`{schema, line, last, changes, skipped, large, outside_paper}`, where `last` has the shape of a save in
`history` and each change has the file-diff keys without `hunks`.
`paper restore --json` prints `{schema, from, changes: [{path, kind}], skipped: [{path, reason, bytes}]}`,
with `from` the commit the files were restored from.

---

## Commands

### `paper init [--profile <name>] [<path>]` (since v0.1)

- **Empty folder, or files without git:** create a git repo, write the profile's block to
  `.gitignore` (merging, never overwriting), write `paper.toml` (`profile = "<name>"`), create
  line `main`, and make an initial save `"Start project"` containing the existing files
  (subject to large-file rules) plus those two files, and none of the ignored ones.
- **Existing git repo:** adopt it. Import all local branches as lines. The current git branch
  becomes the current line. Uncommitted git changes become that line's draft. History is never
  rewritten: the profile block and `paper.toml` are written to the working files, so they show
  up in the draft as unsaved changes, and files already saved stay tracked even when the new
  ignores match them. A `profile` already in `paper.toml` wins over detection, and a
  `--profile` that disagrees with it is a user error (exit 1) pointing to
  `paper config set --project profile <name>`.
- **Detached HEAD in an existing repo:** refuse, and suggest `git switch <branch>` first.
- `--profile` accepts `latex | markdown | typst | quarto | jupyter | generic`. The default is
  auto-detected from the files the snapshot would see (`_quarto.yml` → quarto, `*.tex` →
  latex, `*.typ` → typst, `*.ipynb` → jupyter, `*.md` → markdown, in that order), falling back
  to generic. A damaged paper block in `.gitignore` (a start marker without its end, or two
  blocks) is a user error and nothing is written.
- It reports whether the project was created or adopted, how many files are tracked, the
  profile (`Profile: latex (detected from *.tex)`, `Profile: markdown (chosen)` or
  `Profile: latex (from paper.toml)`; with nothing to detect,
  `Profile: generic (nothing more specific found)`), the files it wrote (`.gitignore`,
  `paper.toml`), the profile's tips, the files left out (too large, symlinks, nested
  repositories) and any branches skipped. `--json` adds `profile`, `detected`, `written` and
  `tips`.
- If no identity is found in the environment, paper config or git config, ask for name and
  email once on a terminal and store them in the user config (see Global behaviour).
  Without a terminal `init` fails with the `paper config set user.…` commands (then the
  `git config --global user.…` ones) and writes nothing.
- If the folder is inside a cloud-sync root (OneDrive, Dropbox, iCloud Drive, Google Drive),
  warn and ask for confirmation (`--yes` skips the prompt). See
  [04-content](04-content.md#cloud-sync-folders). *Planned for v0.7 (roadmap § v0.7: Office
  documents).* *Today:* `init` doesn't check for cloud-sync folders and has no `--yes` flag.
- A `.git` in the target folder that git can't open (empty, damaged, or a file pointing at a
  missing folder) is a user error ("This folder has a damaged .git") that says to inspect it
  or move it away; nothing is written. This holds inside another repository too.

### `paper status` (since v0.1)

```
On main · last save kmrwxq "Rewrite related work" (2 hours ago)

Draft (unsaved):
  modified  sections/intro.tex        +42 words −17 words
  new       figures/ablation.pdf      (318 KB)
  deleted   notes/old-outline.md      +0 −85 lines

Skipped (too large to track, see `paper help large-files`):
  data/raw_scans.h5                   (2.1 GB)

Remote: 3 new saves on origin/main — run `paper sync`       # v0.6+
```

Other states it reports:
- conflicts in progress (exit code 2), including `.theirs` copies for non-text files;
- a keep in progress;
- Office files kept at their previous version because they were being written;
- an empty draft (`Nothing unsaved.`);
- plain git having moved things since paper last wrote them (PV-029). A block follows the
  header, worded like `save`'s refusal, with the suggestion formatted like an error's `next:`
  line, and the exit code stays 0:

  ```
  The line 'main' was changed outside paper: commits were added to it, or it was moved, with plain git. Paper can't save on top of that yet.
    next: git log main  # see what changed
  ```

  or `Git is on the branch 'other', not on the line 'main'.` (`Git is on no branch (detached
  HEAD), …` for a detached HEAD) with `next: git switch main  # go back to the line`. When both
  apply, only the another-branch block shows. The draft is still compared with paper's last
  save, and nothing is written to git. Importing those commits is v0.4.

Until word diffs arrive (v0.3), each change shows line counts (`+12 −3 lines`, also for a deleted
text file: `+0 −85 lines`) for text and
sizes for binary files (`(318 KB)` for a new or deleted one, `318 KB → 331 KB` for a changed
one), taken from the line diff. `paper status` snapshots first (I1);
`--no-snapshot` reads the stored draft instead and lists no skipped or large files.

`paper status --json` always has an `outside_paper` key: `null` when nothing moved,
`{"kind":"line_moved","line":"main"}`, or
`{"kind":"on_another_branch","line":"main","found":"other"}` (`"found": null` for a detached
HEAD). Every other key is unchanged.

### `paper save [<message>] [<paths>…] [--all-but <paths>…]` (since v0.1)

- With no paths, saves the whole draft. With paths, saves only those files, and the rest
  stays in the new draft. This replaces the staging area.
- If `<message>` is missing and stdin is a TTY, prompt with one line (`Describe this save:`).
  Otherwise it is a user error (exit 1). No `$EDITOR` by default. `paper config set editor …` opts in.
- A path selects the file with exactly that path, or every file under it when it names a
  directory (`sections` and `sections/` are the same). Only whole path components match:
  `sec` does not select `sections/intro.tex`. `--all-but` (one path per flag, repeatable: `--all-but a --all-but b`) saves everything except
  the matches.
- A path with no unsaved changes is an error that lists every such path
  (`None of these paths has unsaved changes: …`), and nothing is written. This catches
  typos, in `--all-but` too.
- A file that becomes a directory (or the reverse) can't be saved in halves, but the rule is
  one-way. Selecting an *added or modified* file also saves the deletions a valid tree needs:
  `notes/a.tex` added takes the deletion of the file `notes`; `notes` added takes the
  deletion of every file under `notes/`. Selecting a deletion forces nothing, and the
  additions stay in the draft. Files excluded with `--all-but` that are forced this way are
  saved too, and the result lists them.
- If the line changed with plain git since paper last saw it (new commits, a `git reset`, or
  a deleted branch), `save` refuses and writes nothing: saving on top of that isn't
  supported yet. Paper starts importing git changes in v0.4 (`git checkout` and
  branch creation, roadmap § v0.4: Safe experiments); every row of the interop table is
  covered by v0.8; co-authors' work arrives with `sync` in v0.6. The error names the line
  and points to `git log <line>` to see what changed; it never suggests a command that could drop git
  commits. If git is on another branch (or detached), it refuses too and says to run
  `git switch <line>`.
- An empty draft means a no-op: `Nothing to save.` (exit 0).
- It prints the short change ID and message, the files saved, and the files left in the draft.
  `--all-but` can't be combined with paths.
- If conflict markers are still present in files being saved: warn, and require `--with-conflicts`.
  *Planned for v0.4, when conflicts arrive with `keep` (roadmap § v0.4: Safe experiments).*
  *Today:* there is no `--with-conflicts` flag and `save` doesn't look for conflict markers.
- **Effect:** the draft becomes a save on the current line, the line advances, a new empty
  draft is created, and git HEAD and the index are updated to match (I6).

### `paper history [<path>] [--all] [-n <N>] [--graph]` (since v0.1)

Saves on the current line, newest first, with milestones inline:

```
kmrwxq  2 hours ago   Rewrite related work
tqlpvn  yesterday     Add ablation figure         ◆ arxiv-v1
zzvoko  2026-09-10    Start project
```

The tip of a line is marked with the line's name in parentheses, e.g. `(main)`. Times read
`just now`, `N minutes ago`, `N hours ago`, `yesterday`, then the UTC date `YYYY-MM-DD`.

`<path>` restricts the list to saves touching that file or anything under that directory
(whole path components; a trailing `/` is optional). `-n` limits the number of saves. `--all` shows
every line's saves, each once. `--graph` (draw the lines) is not yet available and fails with
a user error. The draft is not a save and is
not listed. IDs are the shortest unique change-ID prefix (min 4 letters), or 7 hash characters
for saves made with plain git.

### `paper show <point>` (since v0.1)

Metadata for the save (author, date, change ID, message, milestones), the list of changed
files with their kind (`added`, `modified`, `deleted`), then the diff of each file against the
save's first parent in the same format as `paper diff` (the first save shows every file as
added). `--stat` prints the per-file counts, as `paper diff --stat` does, instead of the list
and the diffs. The date reads like `history` times; `--json` has the exact timestamp.
`paper show @` shows the draft's unsaved changes against `last`. `<point>` follows the
[points](01-concepts.md#points-how-you-name-a-state) rules.

### `paper diff [<from>] [<to>] [<paths>…] [--lines|--words] [--stat] [--since <point>]` (since v0.1, words in v0.3)

| Invocation | Compares |
|---|---|
| `paper diff` | `last` → `@` (what's unsaved) |
| `paper diff <A>` | `A` → `@` |
| `paper diff <A> <B>` | `A` → `B` |
| `paper diff --since neurips-submission` | Alias for `paper diff neurips-submission @`. This is the PI command |

**Points or paths?** Up to two leading arguments are points if they resolve as points; the
first one that doesn't, and everything after it, are paths. A word that is clearly a mistyped
point (`last~x`, `HEAD~1`) is an error, and a first word that is neither a point nor a file
is reported as "not a save, line, date or file". A word containing `/` or `.` is treated as a
file (line names may contain them, but such a typo is far more likely a file), so a mistyped
`sections/nope.tex`, or a line name with `/` or `.`, gets the file wording ("No file
matches", with `paper status` and `paper show <point>`). A name that is both a point and a
file is a point; `--` forces everything after it to be paths (`paper diff -- last` diffs the
file called `last`). `--since <point>` can't be combined with points.

Text defaults to `--words` from v0.3 on. Until then the comparison is by line (`--lines`, the
default) and `--words` fails with a user error saying it is not yet available. The output
is git's unified diff with `a/<path>` and `b/<path>` labels (`/dev/null` for an added or
deleted file), in path order, coloured red and green only on a colour terminal; it applies
with `git apply`. A binary file is one line: `figures/plot.bin  changed  318 KB → 331 KB`
(`new` or `deleted` with `(318 KB)`). No differences print nothing (exit 0). From v0.7, `.docx` gets a text diff, and
`.xlsx`/`.xls`/`.ods`/`.csv`/`.tsv` get a cell diff. Other binary files show a one-line
summary (see [04-content](04-content.md#diff)). `--lines` on an Office file shows the line
diff of the extracted text, which is handy for piping.

Paths restrict the output to those files, or to everything under a directory (whole path
components). A path that names no file in either save is an error listing every such path,
so a typo never looks like "no differences"; a path that exists but is unchanged prints
nothing. `--stat` prints one line per file, `+12 −3` or the binary sizes, then a total
(`2 files changed, +13 −3`). The front-end
snapshots before comparing against `@`.

### `paper restore <paths>… [--from <point>]` (since v0.1)

Replaces those paths in the draft with their content at `<point>` (default `last`). Paths
that don't exist at `<point>` are deleted. Paths follow the rule of `paper save` (a file, or
everything under a directory), and at least one is required. Undoable from v0.2
(`paper undo`). There is no "are you sure?": undo covers it, and until v0.2 the draft snapshot below is the safety net.

The draft is snapshotted before anything is overwritten, so the previous content always
exists as a commit; the activity log that `undo` reads arrives in v0.2. A path that matches
no file in the draft or at `<point>` is an error listing every such path. The draft snapshot
stays (your unsaved edits are recorded), but no working file is changed. Files that already match are left alone, the executable bit is kept, empty
directories are removed, and a file can be restored over a directory of the same name (and
back). Files `paper` never records (symlinks, over-size files, nested repositories, ignored files)
are never overwritten or deleted, nor is anything reached through a symlinked folder; they
are listed as skipped.

**Letter case.** On a filesystem that ignores case (the default on macOS and Windows), a
file or folder that differs from the save only in case ends with the save's spelling when
both spellings are named (`paper restore Notes.tex notes.tex`). Naming only the saved
spelling leaves the draft's file alone and lists the saved one as skipped (`case_alias`,
"differs only in letter case from notes.tex; run `paper restore Notes.tex notes.tex`").
On a filesystem that tells the spellings apart (Linux), both can exist, so the saved
spelling is simply written. A case-renamed folder that still holds something paper doesn't
track (an ignored file, say) is kept as it is and listed as skipped (`blocked`). Case is
compared by lowercasing whole names, so a few letters (`ß`, Turkish `İ`) aren't treated as
the same name in two cases; paper leaves such files alone rather than overwrite them. The
draft records the spelling that the folder lists, never an alias of it.

**All or nothing.** If something paper doesn't track blocks a write, restore also leaves
alone the deletions that were only there to make room for it, so a blocked file/folder
swap changes neither side. The write is listed as skipped (`blocked`) and the files kept
for it as `kept_for_swap` ("kept because chap can't be restored"). Unrelated deletions in
the same restore still happen.

Git's index and HEAD are untouched, so plain `git status` shows the restored files as
unstaged changes. It prints the files restored or deleted, then the skipped ones with the
reason for each (a symbolic link, a folder that is a symbolic link, too large to track,
inside a separate git repository, a file paper doesn't track in the way, …). In `--json`,
`skipped` is a list of `{path, reason, bytes}` like `init`'s, with the reasons
`saved_symlink`, `symlink`, `symlinked_folder`, `too_large`, `nested_repository`,
`non_utf8_path`, `blocked`, `case_alias` (plus `other`, the draft's spelling) and
`kept_for_swap` (plus `target`, the path that couldn't be restored).

### `paper ignore [<path|pattern>…] [--yes]` (since v0.2)

Stops tracking files and folders, including ones already saved, without learning gitignore
syntax. Hand-editing `.gitignore` stays fully supported. With no arguments it lists the ignore
rules instead (below). `unignore`, `--local` and `--why` are planned (v0.2, PV-032).

**Rules.** Paper keeps what it adds in its own block of the root `.gitignore`:

```
# >>> paper-vcs: ignored >>>
/data/raw/
*.aux
# <<< paper-vcs: ignored <<<
```

The block goes at the end of the file after a blank line, so it comes after the profile block
(and after your own lines: later lines win in gitignore). Each rule appears once, in the order
added, and an empty block is never written. Your own lines stay byte-identical and the file
keeps its line endings (CRLF if the first line ends in CRLF). A damaged paper block (a start
marker without its end, or two blocks) is a user error, and nothing is written.

**Arguments** are resolved from where paper runs, like `save`:

1. A name that is an existing file or folder (on disk or in the draft) becomes an anchored
   rule: `/data/raw/` for a folder, `/notes/a.log` for a file, typed with or without a
   trailing slash. Gitignore characters in the name (`\`, `*`, `?`, `[`, `]`, trailing spaces)
   are escaped with `\`, so `a[1].log` is written `/a\[1\].log`.
2. Otherwise an argument with `*`, `?` or `[` is a pattern, written exactly as typed. It
   applies to the whole project wherever paper runs, and the output says so.
3. Otherwise it is a name for something that doesn't exist yet. With a slash in it (`build/out`,
   `cache/`) it is anchored (`/build/out`, `/cache/`, a trailing slash only if typed). A bare
   name (`scratch.txt`) is written as typed and matches at any depth, as it does in
   `.gitignore`.

Refused with a user error, writing nothing (one refused argument stops the whole command): an
empty argument; a leading `!` unless it names an existing file (taking a file back off the
list is `paper unignore`, coming in
v0.2; until then edit `.gitignore`); `.gitignore` and `paper.toml` themselves; anything under
`.git`; the whole project folder; a path outside the project; a pattern that starts above the
project (`../*.aux`) or inside `.git`, which could never match. A name is written with the
spelling found on disk, so `DATA/raw` on a filesystem that ignores case gives `/data/raw/`. A
bare name starting with `#` is written `\#name`, so it isn't a comment.

A rule already in paper's block is not added again: it is reported as already ignored, and its
matches are still untracked.

**Untracking.** Every file in the draft (after the snapshot of invariant I1) that git would
ignore *because of one of the rules this command names* stops being tracked. The deciding rule
is computed over the whole rule set (nested `.gitignore` files, `.git/info/exclude` and git's
global excludes included), so a later negation in your own lines still wins and keeps the
file. `.gitignore` files and `paper.toml` are never untracked, whatever a rule matches. The
files leave the draft and stay on disk byte-identical; earlier saves keep their versions, and
the next `paper save` records the removal. No history is rewritten. Running the same command
again finishes an interrupted run, because rules already present still untrack their matches.

More than 20 files to untrack need `--yes`. Without it the command is a user error that gives
the count and the first five paths, suggests the same command with `--yes`, and writes nothing.

**Order.** Arguments are checked; the snapshot records unsaved edits as their own entry; then
everything is decided (rules, untrack set, threshold) before anything is written; then
`.gitignore` is written atomically; then one `ignore` activity entry records the new draft
(`.gitignore` updated, matches removed). A crash after the `.gitignore` write and before the
entry leaves the new rule on disk with the files still tracked: the next command records the
`.gitignore` edit, and running `paper ignore` again untracks them. `ignore` writes only
paper's draft, never the line, so it works after plain git moved the line.

**Activity and undo.** The entry is described `ignore /data/raw/, *.aux` (at most three rules,
then `and N more`) and is undoable like any other: `undo` writes the old `.gitignore` back and
tracks the files again.

**Output.** `Now ignoring:` and `Already ignored:` list the rules (a pattern is marked `(a
pattern: applies to the whole project)`); then `Stopped tracking N files; they are still on
your disk:` lists them (at most 20, then `and N more`) and says the next save records their
removal while earlier saves keep their versions. When no rule is added and nothing is
untracked, the output is `Nothing to do: already ignored.`, no entry is recorded, and the exit
code is 0.

`--json`: `{"schema":1,"added","already","untracked","entry"}`, every key always present.
`entry` is an activity entry (`{"id","short_id","time","kind","description"}`) or `null` for the
no-op.

**Listing.** `paper ignore` with no arguments lists every ignore rule that applies, grouped by
source, each with its file and line. It doesn't snapshot or record anything. Comments, blank
lines and marker lines are skipped. The sources, in this order:

| Source | Where |
|---|---|
| `paper` | paper's block in the root `.gitignore` |
| `profile` | the profile block in the root `.gitignore` |
| `yours` | the other lines of the root `.gitignore`, and nested `.gitignore` files (named by path, e.g. `docs/.gitignore`) |
| `local` | `.git/info/exclude` |
| `global` | git's `core.excludesFile`, or `$XDG_CONFIG_HOME/git/ignore` (`~/.config/git/ignore`) when unset |

A source that exists but can't be read (not UTF-8 text, no permission) is not an error: the
human output ends with a `Note:` line naming it, and its rules are left out. With no rules at
all the output is `No ignore rules.` `--json`: `{"schema":1,"rules":[{"rule","source","file","line"}]}`,
every key always present; `source` is one of the five words above and `line` is 1-based. An
unreadable source has no place in this fixed shape, so only the human output names it.

### `paper undo` / `paper redo` / `paper activity [-n <N>]` (since v0.2)

- `undo` takes back the last thing you or paper did. Unsaved edits go first (`Undid: edited
  sections/intro.tex`); then each `undo` steps one state further back, through every earlier
  state, including states that were themselves undone. The files follow: the command rewrites
  or deletes the files that differ. Before undoing, it snapshots (even with `--no-snapshot`), so edits made since the last
  command are an entry of their own and can be redone.
- `redo` brings back what you just undid (`Redid: save "Rewrite intro"`). Any other change
  that adds to the activity log, including the edits a later command records, clears what you
  can redo.
- `paper undo --to <activity-id>` goes back to the state right after that entry, taking the
  short id from `paper activity` (`Went back to a41c: save "Add related work"`). It can also go
  to an entry that was undone. An empty, unknown or ambiguous id is a user error (exit 1) and
  changes nothing; an ambiguous one lists the candidates.
- Output: the first line says what happened (`Undid: <description>`, `Redid: <description>`,
  `Went back to <id>: <description>`), then `Changed N files:` with the files the command
  rewrote (listed as `restore` lists them), then `Skipped (left as they are):` with one
  `path  (reason)` line per file paper does not record that it left alone (the reasons of
  `restore`). If the working files don't match the state undo went to (skipped files keep
  different content), a last line warns: "2 files were left as they are (see
  above). `paper undo` and `paper redo` go on from here; any other paper command that looks at
  your files (`paper status`, `paper save`, ...) records them as edits, which clears what you
  can redo." ("1 file was left as it is ... records it as an edit ..." for a single file.)
  JSON is unchanged. Repeated `undo` (and `redo`) is not held back by such files: the
  leftovers are remembered, and the next undo-family command does not record them as an edit,
  so it walks one entry further. Any other paper command (`status`, `save`, ...) records them
  as an edits entry, and a leftover or any other file edited again is a real edit. When
  there is nothing to take back or bring back, the output is `Nothing to undo.` or `Nothing to
  redo.` and the exit code is 0.
- Files that the state `undo`, `redo` or `undo --to` goes to doesn't track, and whose
  `.gitignore` rules ignore them (what `paper ignore` leaves behind), stay on disk: they are
  not deleted and not listed as skipped. A file that is not in the draft but already holds
  exactly the wanted bytes (and executable bit) is not "in the way" either: nothing is written
  and nothing is listed (unless a path differing only in letter case is involved). `restore` is
  unchanged: such a file stays listed as `blocked`.
- After a plain `git commit` or `git switch`, `undo`, `redo` and `undo --to` refuse with the
  errors of `save` and change nothing.
- `--json`: `{"schema":1,"outcome","entry","changed","skipped","files_match"}`, every key
  always present. `outcome` is `undone`, `redone`, `went_back`, `nothing_to_undo` or
  `nothing_to_redo`. `entry` is `{"id","short_id","time","kind","description"}` (what was
  undone, redone or gone back to) or `null`; `changed` is `[{"path","kind"}]` and `skipped` is
  `[{"path","reason",...}]`, as in `restore --json`; `files_match` is `false` when skipped files
  keep their own content, `true` otherwise, and `null` when nothing happened.
- `activity [-n <N>] [--json]` lists the entries from the current head back, newest first
  (default 20). It doesn't snapshot first, so listing the log never adds to it, but it takes
  the lock like every command. Each line is `<id>  <time>  <description>`: the shortest
  prefix of the entry's id that no other operation in the log shares, listed or not (at least 4 hex
  characters, sized from the file names in `ops/`, so `undo --to <id>` can take it), the
  time in the same relative style as `history` (padded to 14 characters), and the description.

```
a41c  just now        save "Rewrite related work"
b7e0  9 minutes ago   edited sections/intro.tex, main.tex
9e02  10 minutes ago  restore sections/intro.tex from last~1
77d1  1 hour ago      start the project
```

  Descriptions: `edited a.tex, b.tex, c.tex and 2 more` (sorted, at most three paths),
  `save "<first line of the message>"`, `restore <paths> from <point in paper's spelling, or last>`, `start the project`,
  `adopt the git repository`, `upgrade to paper format 2`, `ignore <rules>`, `undo: <description of what was undone>`,
  `redo: <description>` and `go back to <id>: <description>`. Several restored paths are listed
  in the order given, separated by `, `, at most three then `and N more`; restoring the whole
  project is `restore everything from <point>`. A point is written in paper's spelling, so
  `last~0` is `last`.
  `-n` must be at least 1. The listing reads the operations on the chain from `head`; if one of
  them is damaged or missing, the command stops with a project error and lists nothing.
  `--json`: `{"schema":1,"entries":[{"id","short_id","time","kind","description"}]}`, every key
  always present; `time` is RFC 3339, `kind` is `init`, `migrate`, `snapshot`, `save`,
  `restore`, `ignore`, `undo` or `redo` (`go back to` entries have kind `undo`).

### `paper gc [--older-than <duration>] [--dry-run]` (since v0.2)

Trims the undo history to a recent window so the activity log stops growing and git can
reclaim old objects. Nothing runs it automatically.

- `--older-than` takes `<n>d` (days) or `<n>w` (weeks), at least `1d`; the default is `90d`.
  `0d`, a missing or unknown unit, or anything else is a user error (exit 1) and changes nothing.
- Kept: every activity entry newer than the cutoff, the current entry, the entry your next
  `undo` starts from, and every entry you can still `redo`, plus all entries between the
  oldest of those and now. Older entries are removed, and so are crash leftovers (`.tmp*`
  files, orphan operation files) and the `view.json.v1` file of a format 1 upgrade. The oldest kept entry is the *horizon*: `paper
  activity` ends there, and `undo` reports `Nothing to undo.` once it gets there.
- Afterwards `paper activity` ends with, and `Nothing to undo.` is followed by, the line
  ``Older history was cleaned up by `paper gc`, so there is nothing further back to go to.``
  `paper activity` shows it only when the listing reaches the horizon: with more kept entries
  than `-n` allows, the line is left out, since the list stops short of the horizon.
- `gc` is not undoable and records no activity entry. `--dry-run` lists what would be removed
  and changes nothing. When no entry is old enough, nothing is trimmed (leftovers are still
  swept). Running it twice is safe; the second run removes nothing new.
- Paper doesn't run `git gc`: the output says to run it to reclaim the old drafts and undone
  work paper no longer needs. Saves on lines are always kept.
- `--older-than` followed by an option (`paper gc --older-than --dry-run`) is the same user
  error, and its message says a duration such as `90d` is needed after `--older-than`.
- Output: `Removed 3 entries from the activity list; kept 12 entries, back to 2026-07-01.` (a
  UTC date), then `Swept N leftover files:` with one path per line (relative to `.git/paper/`) when there
  were any, then the `git gc` line. With `--dry-run` the output starts with `Dry run: nothing
  was changed.` and says `Would remove` / `would keep` / `Would sweep`.
- `--json`: `{"schema":1,"dry_run","removed_entries","kept_entries","horizon","swept"}`, every key
  always present. The counts are entries on the activity chain; `horizon` is the date
  (`YYYY-MM-DD`, UTC) of the oldest kept entry; `swept` lists the leftovers (not the removed
  entries' files) relative to `.git/paper/`, sorted.

### Lines: `paper try`, `keep`, `drop`, `switch`, `lines` (since v0.4)

**`paper try <name> [--clean]`**
- Creates line `try/<name>` whose origin is the current line, and switches to it.
- By default the current draft *moves* into the try ("this is getting experimental"), and
  the origin line's draft becomes empty. `--clean` starts the try with an empty draft and
  leaves the origin's draft where it is.

**`paper keep [<try>]`** (the default is the current line if it is a try)
1. Snapshot, then save the try's draft if it is non-empty, asking for a message
   (`--message` or a prompt).
2. If the origin hasn't moved since the try started, **fast-forward** it.
   Otherwise **merge** with the message `Keep try/<name>`.
3. **Clean:** delete the try line, switch to the origin, done.
4. **Conflicts:** switch to the origin. Its draft becomes an *in-progress keep*: a draft with
   two parents and conflict markers in the files. `status` lists the files (exit 2), and the
   user edits them and then runs `paper save`. `paper keep --abort` returns to the state
   before the keep (an alias for undo of that entry).

**`paper resolve <paths>… [--ours | --theirs]`** marks conflicts as resolved. `--ours`
keeps the working file, `--theirs` replaces it with the other version. With neither flag,
the file is accepted as it is now, after checking it has no text markers and no `.theirs`
copy left. For non-text files, deleting the `.theirs` copy also counts as resolving.
See [04-content](04-content.md#conflicts-in-anything-that-isnt-plain-text).

**`paper drop <try>`** deletes the line and its draft. Undoable. `drop` never applies to `main`.

**`paper switch <line>`** stores the current draft and restores the target line's draft (I3).
Cheap and silent.

**`paper lines`** lists lines with origin, last save, draft size and ahead/behind of origin.

Lines that don't start with `try/` can be created with `paper line new <name>` (for
long-lived variants, such as a journal version and a thesis chapter). `keep` refuses them unless given `--into <line>`.

### Milestones: `paper milestone`, `milestones` (since v0.5)

- `paper milestone <name> [-m <description>] [--at <point>] [--venue <text>]` labels a save
  (default `last`). If the draft is non-empty, it warns:
  `Your unsaved changes are NOT part of this milestone. Save first? [y/N]`.
- Names are slugified (`"NeurIPS submission"` → `neurips-submission`) and must be unique.
- `paper milestones` lists them with date, venue and save.
- `paper milestone --delete <name>` removes one. Undoable. Once a milestone has been synced,
  deleting it warns that the remote copy will be removed on the next sync.

### Remotes: `paper clone`, `remote`, `sync` (since v0.6)

**`paper clone <url> [<path>]`**: git clone, then `paper init` adoption.

**`paper remote add <name> <url>` / `remote list` / `remote remove <name>`**. The first remote is named `origin` unless told otherwise.

**`paper sync [--line <line> | --all]`**
1. Snapshot. Fetch from the tracked remote using the `git` binary, which brings credentials,
   SSH and helpers along for free.
2. For each shared line (the current one by default):
   - Only the remote moved → fast-forward, then rebase the draft onto the new save (the draft is
     local, so rewriting it is fine). Any conflicts go into the draft as markers.
   - Only local moved → push.
   - Both moved → merge (`Sync with origin/main`), then push. Conflicts give an in-progress
     merge in the draft and exit 2. Nothing is pushed until that is resolved and saved.
3. Push milestones (tags). Never push drafts, tries or `refs/paper/*` (I5).

**Shared vs local lines.** `main` and any line created by clone or fetch are shared. Tries
are local until `paper share <line>`. This means experiments are private by default.

### `paper config get|set|list [--project]` (since v0.1)

- `paper config get <key>` prints the effective value (nothing else) and exits 0. A key with
  no value (an unset identity or `profile`) is a user error (exit 1) that says how to set it.
- `paper config set <key> <value> [--project]` validates, writes, and prints
  `Set <key> = <value> in <file>`. It writes the user config by default and the project's
  `paper.toml` (at the project root, created on first use) with `--project`. Comments, layout
  and other keys in the file are kept. `set --project profile <name>` also replaces the
  profile block in `.gitignore` (created if missing, your own lines untouched) and says so; a
  damaged block stops it before anything is written. Invalid values, unknown keys, a section
  that is a plain value and a key in the wrong file are user errors and nothing is written.
  A `.gitignore` paper can't read or write is a user error when permission is denied (it
  names the file) and a project error otherwise.
- `paper config list` prints every key with its value and its source (`env`, `project`,
  `user`, `git` or `default`). With `--json`:
  `{"schema":1,"settings":[{"key","value","source"}],"notes":[…]}`, in the table's order;
  `value` is a string, an integer (megabytes) or `null` when unset (human output shows
  `(not set)` with source `-`). `get --json` prints
  `{"schema":1,"key","value","source","notes":[…]}` and `set --json`
  `{"schema":1,"key","value","file"}`.
- Personal keys (`user.*`, `ui.*`) found in `paper.toml` are ignored, never read; `get` and
  `list` print a note on stderr saying so and to remove them, because `paper.toml` is saved
  with the project and may be shared. `diff.default = words` gets a note too: it "has no
  effect until v0.3; comparisons stay line by line". With `--json` the notes are not
  printed; they are the strings in the `notes` array of the output (always present, `[]` when
  there are none, and not affected by `-q`), so scripts need not read stderr.
- A bad value found in the environment or a file is a user error that names its origin (the
  variable, or the file) and suggests what fixes it (`unset` the variable, `paper config set
  --project …` or edit `paper.toml`, or the plain `paper config set …` for the user config).
  On Windows the suggestion for a variable is PowerShell's `Remove-Item Env:NAME` (naming
  `set NAME=` for `cmd`). A `max_mb` below `warn_mb` in one file reads "large_files.max_mb
  (1) can't be below large_files.warn_mb (10) in <file>" and offers both fixes: raise `max_mb`
  to at least the warning level, or lower `warn_mb` to at most `max_mb`.
- Editing keeps the file's bytes: line endings of existing lines stay as they are, and new
  lines use the ending of the file's first line.
- User keys work outside a project. `--project` and project keys need one.

User config: `$XDG_CONFIG_HOME/paper/config.toml`, else `~/.config/paper/config.toml`
(Linux and macOS); `%APPDATA%\paper\config.toml` on Windows. Created on the first `set`.
A malformed file is a user error naming the file and line. See
[03-architecture](03-architecture.md#configuration).

| Key | Values | Home | Effect in v0.1 |
|---|---|---|---|
| `user.name` | non-empty text | user only | author of saves |
| `user.email` | text containing `@` | user only | author of saves |
| `ui.color` | `auto` / `always` / `never` | user only | default for `--color` |
| `large_files.warn_mb` | integer ≥ 1 | project or user | snapshot warning threshold |
| `large_files.max_mb` | integer ≥ 1, not below `warn_mb` in the same file | project or user | snapshot skip threshold |
| `diff.default` | `lines` or `words` | project or user | none until v0.3 (comparisons stay by line; `get` and `list` say so) |
| `profile` | the six profile names | project only | the project's profile; setting it also replaces the block in `.gitignore` |

Identity and looks are never written to `paper.toml`: it is saved with the project, which
may be shared or public. `profile` describes the project, so it needs `--project`. When
`max_mb` is set below the default `warn_mb` without a `warn_mb`, the warning threshold is
capped at `max_mb`. `paper.toml` is an ordinary project file: it shows in the draft and is
saved like any other. `ui.editor` is not a key yet.

### Escape hatches

- Every paper repo is a git repo, so `git …` always works. Paper importing external
  changes on its next run (see [03-architecture](03-architecture.md#git-interop)) starts in
  v0.4 with `git checkout` and branch creation (roadmap § v0.4: Safe experiments, Interop);
  bringing in co-authors' work arrives with `sync` in v0.6. *Today (v0.1 to v0.3):* paper
  doesn't import them. After `git commit` or `git reset`, `save` refuses and names
  `git log <line>`; if git is on another branch, it says to run `git switch <line>`.
- `git push` works today. `git pull` (or `git merge`) isn't supported yet: after a pull
  brings new commits, `status` shows their edits as your unsaved changes, `history` doesn't
  show their saves, and `paper save` refuses. Importing starts in v0.4 (see above);
  syncing with co-authors (`paper sync`) is v0.6 (roadmap § v0.6: Collaboration).
- `paper help concepts`, `paper help git` (the vocabulary mapping table from 01) and
  `paper help large-files` give built-in explanations. *Planned:* `help large-files` ships
  with v0.3 (roadmap § v0.3); the roadmap gives no version for the other two (the complete
  set of help topics is v0.8 hardening). *Today:* all three are unknown subcommands. Read
  [01-concepts](01-concepts.md) and [04-content](04-content.md) instead, and `paper --help`
  for the commands.

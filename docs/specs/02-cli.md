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
| 3 | Project error: corrupted state, lock held too long, git binary missing for network ops |
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
tracker. A suggestion's `command` is usually a command to run, but it may be a URL (the issue
tracker), and the `explanation` then carries the prose ("report it, including what you ran and
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
`{schema, line, last, changes, skipped, large}`, where `last` has the shape of a save in
`history` and each change has the file-diff keys without `hunks`.
`paper restore --json` prints `{schema, from, changes: [{path, kind}], skipped: [{path, reason, bytes}]}`,
with `from` the commit the files were restored from.

---

## Commands

### `paper init [--profile <name>] [<path>]` (since v0.1)

- **Empty folder, or files without git:** create a git repo, write the profile's ignores to
  `.gitignore` (merging, never overwriting), write `paper.toml`, create line `main`, and
  make an initial save `"Start project"` containing the existing files (subject to large-file rules).
- **Existing git repo:** adopt it. Import all local branches as lines. The current git branch
  becomes the current line. Uncommitted git changes become that line's draft. Nothing is rewritten.
- **Detached HEAD in an existing repo:** refuse, and suggest `git switch <branch>` first.
- `--profile` accepts `latex | markdown | typst | quarto | jupyter | generic`. The default is
  auto-detected from the files present (`*.tex` → latex, …), falling back to generic.
  *Not yet effective:* the name is validated, but profiles (ignores, `paper.toml`) arrive
  later, and `--help` says so.
- It reports whether the project was created or adopted, how many files are tracked, the
  files left out (too large, symlinks, nested repositories) and any branches skipped.
- If no identity is found in the environment, paper config or git config, ask for name and
  email once on a terminal and store them in the user config (see Global behaviour).
  Without a terminal `init` fails with the `paper config set user.…` commands (then the
  `git config --global user.…` ones) and writes nothing.
- If the folder is inside a cloud-sync root (OneDrive, Dropbox, iCloud Drive, Google Drive),
  warn and ask for confirmation (`--yes` skips the prompt). See [04-content](04-content.md#cloud-sync-folders).

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
- an empty draft (`Nothing unsaved.`).

Until word diffs arrive (v0.3), each change shows line counts (`+12 −3 lines`, also for a deleted
text file: `+0 −85 lines`) for text and
sizes for binary files (`(318 KB)` for a new or deleted one, `318 KB → 331 KB` for a changed
one), taken from the line diff. `paper status` snapshots first (I1);
`--no-snapshot` reads the stored draft instead and lists no skipped or large files.

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
  supported yet, and a later version will. The error names the line and points to
  `git log <line>` to see what changed; it never suggests a command that could drop git
  commits. If git is on another branch (or detached), it refuses too and says to run
  `git switch <line>`.
- An empty draft means a no-op: `Nothing to save.` (exit 0).
- It prints the short change ID and message, the files saved, and the files left in the draft.
  `--all-but` can't be combined with paths.
- If conflict markers are still present in files being saved: warn, and require `--with-conflicts`.
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
is reported as "not a save, line, date or file". A name that is both a point and a
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
everything under a directory), and at least one is required. Undoable. There is no "are you
sure?", because undo exists.

The draft is snapshotted before anything is overwritten, so the previous content always
exists as a commit; the activity log that `undo` reads arrives in v0.2. A path that matches
no file in the draft or at `<point>` is an error listing every such path. The draft snapshot
stays (your unsaved edits are recorded), but no working file is changed. Files that already match are left alone, the executable bit is kept, empty
directories are removed, and a file can be restored over a directory of the same name (and
back). Files `paper` never records (symlinks, over-size files, nested repositories, ignored files)
are never overwritten or deleted, nor is anything reached through a symlinked folder; they
are listed as skipped. Git's index and HEAD are untouched, so
plain `git status` shows the restored files as unstaged changes. It prints the files
restored or deleted, then the skipped ones with the reason for each (a symbolic link, a
folder that is a symbolic link, too large to track, inside a separate git repository, a file
paper doesn't track in the way, …). In `--json`, `skipped` is a list of `{path, reason, bytes}`
like `init`'s, with the reasons `saved_symlink`, `symlink`, `symlinked_folder`, `too_large`,
`nested_repository`, `non_utf8_path` and `blocked`.

### `paper undo` / `paper redo` / `paper activity [-n <N>]` (since v0.2)

- `undo` reverts the most recent activity entry that hasn't been undone. Repeated `undo`
  goes further back. Before undoing, it snapshots, so edits made since then are themselves
  an entry that can be redone.
- `redo` re-applies the most recently undone entry. A new state-changing command clears
  the redo stack.
- `paper undo --to <activity-id>` restores the state right after that entry.
- `activity`:

```
a41c  just now      save "Rewrite related work"
9e02  10 min ago    restore sections/intro.tex from last~1
77d1  1 hour ago    try restructure-intro
```

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
  and other keys in the file are kept. Invalid values, unknown keys and a key in the wrong
  file are user errors and nothing is written.
- `paper config list` prints every key with its value and its source (`env`, `project`,
  `user`, `git` or `default`). With `--json`:
  `{"schema":1,"settings":[{"key","value","source"}]}`, in the table's order; `value` is a
  string, an integer (megabytes) or `null` when unset.
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
| `diff.default` | `lines` (`words` arrives in v0.3) | project or user | validated only |
| `profile` | the six profile names | project only | stored; used by profiles later |

Identity and looks are never written to `paper.toml`: it is saved with the project, which
may be shared or public. `profile` describes the project, so it needs `--project`. When
`max_mb` is set below the default `warn_mb` without a `warn_mb`, the warning threshold is
capped at `max_mb`. `paper.toml` is an ordinary project file: it shows in the draft and is
saved like any other. `ui.editor` is not a key yet.

### Escape hatches

- Every paper repo is a git repo, so `git …` always works and paper imports external
  changes on its next run (see [03-architecture](03-architecture.md#git-interop)).
- `paper help concepts`, `paper help git` (the vocabulary mapping table from 01) and
  `paper help large-files` give built-in explanations.

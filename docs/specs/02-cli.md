# 02 — CLI Reference

The binary is `paper` (the name is still an open question, see README). The **Since**
column maps each command to the [roadmap](05-roadmap.md).

## Global behaviour

| Flag | Meaning |
|---|---|
| `-C <path>` | Run as if started in `<path>` |
| `--json` | Machine-readable output. Every command supports it, and the schema is versioned (`"schema": 1`) |
| `--no-snapshot` | Skip the automatic snapshot (I1). For scripts and editors only |
| `-q, --quiet` / `-v, --verbose` | Less output / more output. Verbose adds git terms in parentheses |
| `--color <auto\|always\|never>` | Respects `NO_COLOR` |

**Project discovery:** walk up from the current directory until a `.git` is found. If the
repo has no `.git/paper/` yet, commands other than `init` and `clone` fail with:
`This is a git repo but not a paper project yet. Run: paper init`.

**Exit codes**

| Code | Meaning |
|---|---|
| 0 | Success, including a friendly no-op such as "nothing to save" |
| 1 | User error: bad arguments, unknown point, etc. |
| 2 | Needs attention: unresolved conflicts, or a remote that has diverged and needs sync |
| 3 | Project error: corrupted state, lock held too long, git binary missing for network ops |
| 101 | Bug (panic). Prints the issue-tracker URL and a report file path |

**Error format** (always three parts):

```
error: nothing to keep — you are on "main", which is not a try
  why:  keep folds a try back into the line it came from
  next: paper lines            # see your tries
        paper keep try/intro   # keep a specific one
```

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
- If no identity is found in git config or paper config, ask for name and email once and
  store them in the user config.
- If the folder is inside a cloud-sync root (OneDrive, Dropbox, iCloud Drive, Google Drive),
  warn and ask for confirmation (`--yes` skips the prompt). See [04-content](04-content.md#cloud-sync-folders).

### `paper status` (since v0.1)

```
On main · last save kmrwxq "Rewrite related work" (2 hours ago)

Draft (unsaved):
  modified  sections/intro.tex        +42 words −17 words
  new       figures/ablation.pdf      (318 KB)
  deleted   notes/old-outline.md

Skipped (too large to track, see `paper help large-files`):
  data/raw_scans.h5                   (2.1 GB)

Remote: 3 new saves on origin/main — run `paper sync`       # v0.6+
```

Other states it reports:
- conflicts in progress (exit code 2), including `.theirs` copies for non-text files;
- a keep in progress;
- Office files kept at their previous version because they were being written;
- an empty draft (`Nothing unsaved.`).

### `paper save [<message>] [<paths>…] [--all-but <paths>…]` (since v0.1)

- With no paths, saves the whole draft. With paths, saves only those files, and the rest
  stays in the new draft. This replaces the staging area.
- If `<message>` is missing and stdin is a TTY, prompt with one line (`Describe this save:`).
  Otherwise it is an error. No `$EDITOR` by default. `paper config set editor …` opts in.
- A path selects the file with exactly that path, or every file under it when it names a
  directory (`sections` and `sections/` are the same). Only whole path components match:
  `sec` does not select `sections/intro.tex`. `--all-but` saves everything except the
  matches.
- A path with no unsaved changes is an error that lists every such path
  (`None of these paths has unsaved changes: …`), and nothing is written. This catches
  typos, in `--all-but` too.
- A file that becomes a directory (or the reverse) can't be saved in halves: selecting one
  side also saves the other (`notes` deleted along with `notes/a.tex` added).
- If a plain `git commit` added saves to the line since paper last saw it, `save` refuses and
  writes nothing: saving on top of commits made with plain git isn't supported yet. If git
  is on another branch (or detached), it refuses too and says to run `git switch <line>`.
- An empty draft means a no-op: `Nothing to save.` (exit 0).
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

`<path>` restricts the list to saves touching that path. `--all` shows every line, and `--graph` draws the lines.

### `paper show <point>` (since v0.1)

Metadata for the save (author, date, change ID, message, milestones) followed by its diff against its first parent.

### `paper diff [<from>] [<to>] [<paths>…] [--lines|--words] [--stat]` (since v0.1, words in v0.3)

| Invocation | Compares |
|---|---|
| `paper diff` | `last` → `@` (what's unsaved) |
| `paper diff <A>` | `A` → `@` |
| `paper diff <A> <B>` | `A` → `B` |
| `paper diff --since neurips-submission` | Alias for `paper diff neurips-submission @`. This is the PI command |

Text defaults to `--words` from v0.3 on. From v0.7, `.docx` gets a text diff, and
`.xlsx`/`.xls`/`.ods`/`.csv`/`.tsv` get a cell diff. Other binary files show a one-line
summary (see [04-content](04-content.md#diff)). `--lines` on an Office file shows the line
diff of the extracted text, which is handy for piping.

### `paper restore <paths>… [--from <point>]` (since v0.1)

Replaces those paths in the draft with their content at `<point>` (default `last`). Paths
that don't exist at `<point>` are deleted. Undoable. There is no "are you sure?", because
undo exists.

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

User config lives in `~/.config/paper/config.toml` (or the XDG/OS equivalent). Project
config lives in `paper.toml`. See [03-architecture](03-architecture.md#configuration).

### Escape hatches

- Every paper repo is a git repo, so `git …` always works and paper imports external
  changes on its next run (see [03-architecture](03-architecture.md#git-interop)).
- `paper help concepts`, `paper help git` (the vocabulary mapping table from 01) and
  `paper help large-files` give built-in explanations.

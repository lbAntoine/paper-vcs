# 01 — Concepts & Glossary

This is the model users learn. Every command in [02-cli](02-cli.md) is defined in these
terms. The git and jj equivalents are listed for implementers and power users only.

## Glossary

| Term | Meaning for the user | Underneath (git / jj) |
|---|---|---|
| **Project** | A folder under paper-vcs | A colocated git repo plus `.git/paper/` |
| **Draft** | Your unsaved work on the current line. It is always recorded, so it can never be lost | jj working-copy commit, stored at `refs/paper/draft/<line>` |
| **Save** | A named point in history that you chose to keep | A git commit on a branch |
| **Line** | A named line of work. Every project starts with `main` | A git branch (local `refs/heads/<line>`) |
| **Try** | A line created to experiment ("restructure intro"), later kept or dropped | A branch with a recorded origin line |
| **Keep / Drop** | Fold a try back into its origin line, or discard it | Merge (or fast-forward) / delete branch |
| **Milestone** | A labelled save that marks an event: submission, preprint, camera-ready | Annotated git tag with metadata trailers |
| **Activity** | Everything you have done to the project, each entry undoable | jj operation log |
| **Point** | Anything that names a state: a save, a milestone, a line, a date | jj revset (a small subset) |
| **Remote** | Somewhere the project is shared: GitHub, GitLab, the Overleaf git bridge, or any git host | git remote |
| **Sync** | Get others' saves and share yours, in one step | fetch + merge + push |
| **Conflict** | Two edits to the same place. For text, both versions are shown inside the file. For anything else (Word, Excel, images), you get both copies side by side. It never blocks you | Conflict markers, or a `.theirs` copy, plus paper metadata |
| **Change ID** | The stable identity of a save, which stays the same if the save is rewritten (e.g. amended) | jj change id, stored in a commit header |

## The model in one picture

```
 line: main   ●───●───●───●  ← last save ("main")
                          │
                          ○  draft (always exists, auto-updated by every command)

 line: try/restructure-intro   (origin: main)
              ●───●  ← last save
                  │
                  ○  its own draft — switching lines never loses or mixes work
```

## Invariants

These are the promises the implementation must keep. Tests assert them (see [03-architecture](03-architecture.md#testing-strategy)).

1. **I1: snapshot first.** Every command, even a read-only one, starts by recording the working
   files into the current line's draft. The only exceptions are `--help`, `--version` and
   commands run with `--no-snapshot`.
2. **I2: every line has exactly one draft.** The draft's parent is the line's last save,
   or the two merge parents while a keep is in progress.
3. **I3: switching is always safe.** Switching lines stores the current draft and restores the
   target line's draft. No prompts and no "please commit first".
4. **I4: everything is undoable.** Every command that changes state records at most two
   activity entries: an `edits` entry for working-file changes it found first (only if there
   were any), then one entry for the command itself. `undo` steps back one entry at a time and
   restores the state before it, including the working files. Rules written with `--local`
   (`paper ignore --local`) live in `.git/info/exclude`, outside history, like git config: undo
   changes what is tracked, not that file.
5. **I5: only saves leave the machine.** Drafts, activity and `refs/paper/*` are never pushed.
6. **I6: it is always git.** After any paper command, the repo is a valid git repo. `git log`
   shows the saves, `git status` shows the draft as unstaged changes, and the checked-out
   git branch is the current line.
7. **I7: new files are tracked automatically** unless ignored. `paper ignore` adds the
   rule, and also stops tracking matching files that are already saved (earlier saves keep
   them); `paper unignore` takes the rule back (or adds an exception) and tracks the files
   again, and `paper ignore --why <path>` says which rule keeps a path out. There are three exceptions,
   all reported in `status`:
   - oversized files are skipped (see [04-content](04-content.md#large-files));
   - `.theirs` copies made for a conflict are never tracked (see [04-content](04-content.md#merging));
   - Office files that are incomplete while being written keep their previous version until
     the next snapshot (see [04-content](04-content.md#office-documents)).

## Points (how you name a state)

The grammar is small on purpose. Anything not listed is an error that suggests the closest valid form.

| Form | Example | Resolves to |
|---|---|---|
| `@` | `@` | The current draft |
| `last` | `last` | Last save on the current line (the draft's parent) |
| `last~N` | `last~2` | N saves before `last`, following the first parent |
| line name | `main`, `try/intro` | That line's last save |
| milestone name | `neurips-submission` | The save the milestone labels |
| change ID prefix | `kmrw` | That save (letters `k`–`z`, so it can't be confused with a hash) |
| commit hash prefix | `3fa9c1` | That save |
| `date:YYYY-MM-DD[THH:MM]` | `date:2026-09-01` | The last save on the current line at or before that time |

If one name matches several kinds, the order of precedence is `@`/`last`, then milestone,
then line, then change ID, then hash. When an ambiguity is resolved this way, the CLI
prints which kind it matched.

Details of the lookup:

- **Change ID prefix:** one or more letters `k`–`z`, searched over every save and draft
  reachable from any line (so `@`'s change ID resolves too). Several commits sharing one
  change ID resolve to the newest.
- **Commit hash prefix:** at least 4 hex characters (shorter is never a hash), searched over
  the same set.
- **Ambiguity:** several *different* matches are an error that lists the candidates (short ID
  and message); nothing matching is an "unknown point" error.
- **`date:`** compares the save's *committer* time with the end of the given day or minute,
  read in the computer's local time zone, following the current line's first parents.
- **Short IDs** shown by `history` and `show` are the shortest change-ID prefix that is
  unique in the project (at least 4 letters). Saves made by plain git have no change ID and
  show the first 7 characters of their hash.
- Milestones join the precedence order in v0.5; their slot (after `@`/`last`, before lines)
  is reserved until then.

## Researcher-facing wording rules

- Never show "HEAD", "index", "stage", "commit", "rebase", "detached" or "ref" in normal
  output. `--verbose` and `--help` may mention them in parentheses.
- Show saves as `kmrwxq  2026-09-24  "Rewrite related work"`: short change ID, date, message.
- Show times as relative for the last 7 days ("2 hours ago") and as dates after that.

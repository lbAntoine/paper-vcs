# Changelog

Each version has a section written in researcher words. The release workflow uses the section
for the version being released as the body of its GitHub Release.

## 0.2.0

**Nothing is ever lost.** A pre-release for early testers. paper still works on one line of
work, on your own machine, and everything it writes is still a normal git repository. What's
new is that you can take back anything you or paper did except `paper gc`, files included.

What's new since 0.1.0:

- `paper undo` takes back the last thing you or paper did: for example a save, a restore, an
  ignore rule or your unsaved edits. Your files follow. `paper redo` brings it back, and
  `paper undo --to <id>` goes straight back to an earlier point.
- `paper activity` lists everything that happened to the project, newest first, with the ids
  `undo --to` takes. You can go back to any of them with `paper undo --to <id>`.
- If paper is stopped in the middle of a command (a crash, a power cut, a closed terminal),
  the next command repairs the project, and no work is lost.
- `paper gc` trims the undo history to a recent window (90 days by default; `--dry-run`
  shows what it would do). Nothing runs it for you.
- `paper ignore` stops tracking files or folders, even ones you've already saved, and
  `paper unignore` takes the rule back. With no arguments, `paper ignore` lists the rules.
  `--local` keeps a rule private to your machine, and `paper ignore --why <path>` tells you
  which rule ignores a file.
- `paper status` tells you when the line was changed with plain git (a commit, a reset).
  When `paper save` or `paper undo` then refuse, they name a way back: `git reset --soft <id>`
  keeps your files and shows anything different from paper's last save as unsaved edits.

Upgrading from 0.1.0: your project upgrades itself the first time you run a 0.2.0 command.
History from before the upgrade can't be undone. After the upgrade, paper 0.1.0 can no
longer open the project.

Known issues:

- On macOS, if you rename a file by changing only how an accent in its name is encoded (the
  name looks the same), `paper undo` or `paper restore` can delete the file
  (#67). Avoid such renames.
- If you edit a file while it is ignored, then undo or redo that `ignore` or `unignore`, your
  next command records the edit and you can no longer redo (#72). Nothing is lost.
- Rules added with `--local` live in `.git/info/exclude`, outside history. `paper undo`
  changes which files are tracked, but the rule itself stays.
- `paper undo --to` an entry before a `paper unignore --local` removes the files that unignore
  started tracking from your folder (#75). Their content is kept: `paper undo --to` the
  unignore entry brings them back.
- A few small ignore gaps remain: an exception (`!name`) in a nested `.gitignore` can
  silently override `paper ignore` (#71), and the rule listing can show rules from a
  repository-level `core.excludesFile` that paper doesn't apply (#66).

What doesn't work yet:

- No lines (`try`, `keep`) and no milestones.
- No sync with collaborators. You can `git push` your saves, but bringing in other people's
  work with `git pull` isn't supported yet: after a pull brings in commits, `paper save`
  stops until a later version.
- Diffs are by line, not by word (planned for v0.3).

## 0.1.0

**A pre-release walking skeleton for early testers.** paper works on one line of work, on your
own machine. Everything it writes is a normal git repository, so `git push`, `git log` and
GitHub keep working.

What works:

- `paper init` turns a folder into a project (or adopts an existing git repository) and
  detects a LaTeX project, ignoring junk such as `main.aux` and `main.log`.
- `paper save "message"` saves the draft, or only the files you name.
- `paper history` lists your saves, newest first (`-n 3` for the last three).
- `paper show` shows one save: who, when, which files changed and how.
- `paper status` lists the unsaved changes; `paper diff` compares two points, or the last
  save and the draft.
- `paper restore <file> --from last~3` puts a file back the way it was three saves ago.
- `paper config` reads and changes paper's settings.
- Plain git underneath: add a remote and `git push -u origin main`. paper's own bookkeeping
  never leaves your machine.

What doesn't work yet:

- No undo or redo.
- No lines (`try`, `keep`) and no milestones.
- No sync with collaborators. You can `git push` your saves, but bringing in other people's
  work with `git pull` isn't supported yet: after a pull brings in commits, `paper save`
  stops until a later version.
- Diffs are by line, not by word.

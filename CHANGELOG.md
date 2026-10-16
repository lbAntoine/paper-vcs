# Changelog

Each version has a section written in researcher words. The release workflow uses the section
for the version being released as the body of its GitHub Release.

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
- No sync with collaborators beyond plain `git push` and `git pull`.
- Diffs are by line, not by word.

Install it from the [release page](https://github.com/lbAntoine/paper-vcs/releases/tag/v0.1.0).

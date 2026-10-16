# PV-004: profiles: .gitignore block merge, auto-detect, paper.toml

- **Status:** In progress
- **Branch:** `feat/PV-004-profiles`
- **Depends on:** PV-005 (the `profile` key in `paper.toml`), merged. Runs alone.
- **Touches:** `crates/paper-core/profiles/*.toml` (new), `crates/paper-core/src/profile/` (new),
  `crates/paper-core/src/lib.rs`, `crates/paper-core/src/error.rs`,
  `crates/paper-core/src/project/init.rs`, `crates/paper-core/src/project/mod.rs`,
  `crates/paper-core/src/config/keys.rs` (profile names from the profile list),
  `crates/paper-core/tests/profile.rs` (new), `crates/paper-core/tests/init.rs`,
  `crates/paper-core/Cargo.toml`, `crates/paper-cli/**`, root `Cargo.toml` / `Cargo.lock` (only if
  a TOML parser is added), `docs/specs/02-cli.md` (§ `paper init`, § `paper config`),
  `docs/specs/04-content.md` (§ Profiles)
- **Specs:** `04-content.md` § Profiles; `02-cli.md` § `paper init`; `01-concepts.md` I6, I7;
  ADR-0002 (plain git: `.gitignore` is git's own file)

## Goal

`paper init` picks the right profile for a LaTeX, Markdown, Typst, Quarto or Jupyter project
(or takes `--profile`), keeps build junk out of saves through a marked `.gitignore` block that
never touches the user's own lines, and records the choice in `paper.toml`.

## Context

- `paper init --profile` is parsed and validated but has no effect (`crates/paper-cli/src/cli.rs`).
- `Key::Profile` (`crates/paper-core/src/config/keys.rs`) is a project-only key with its own list of
  the six names; `ConfigFile` (`config/file.rs`) writes `paper.toml` keeping comments.
- `Project::init` (`project/init.rs`) creates or adopts, then makes the first save (new project) or
  imports the draft (adoption). The snapshot walk (`snapshot/walk.rs`) honours `.gitignore`, and
  files already tracked stay tracked even when a rule matches them later, like git.
- `fs_util::write_atomic` exists.

## Decisions (coordinator, 2026-10-16)

- **Profile files** are built in: `crates/paper-core/profiles/<name>.toml`, embedded with
  `include_str!`, parsed once. Fields in v0.1: `name`, `detect` (glob patterns or file names),
  `ignore` (lines for `.gitignore`), `tips` (strings). `text_extensions`, `binary_extensions` and
  `tokenizer` are left out until a feature reads them (v0.3). The ignore lists follow the
  04-content table, and every profile adds the shared lines (generic, Python/R environments,
  Office lock files) listed there. The compiled PDF is **not** ignored.
- **Detection** (no `--profile`): scan the files the snapshot walk would see (so existing
  ignores apply) and pick the first match in this order: `quarto` (`_quarto.yml`), `latex`
  (`*.tex`), `typst` (`*.typ`), `jupyter` (`*.ipynb`), `markdown` (`*.md`), else `generic`.
  The result says whether the profile was detected or chosen.
- **The block:** profiles only ever add a marked block:
  ```
  # >>> paper-vcs profile: <name> >>>
  …
  # <<< paper-vcs profile: <name> <<<
  ```
  - No `.gitignore`: create it with the block.
  - A `.gitignore` without a block: append the block (after a blank line), leaving every
    existing byte as it was.
  - An existing paper-vcs block (any profile name): replace it in place. Re-applying the same
    profile changes nothing (byte-identical).
  - A start marker without its end marker, or two blocks: an error that teaches (rule 6), and
    nothing is written.
  - Line endings follow the file: CRLF if its first line ends in CRLF, else LF.
- **New project** (`init` in a folder without git): write the block and `paper.toml`
  (`profile = "<name>"`) before the first save, so the save contains both and none of the ignored
  files.
- **Adoption** (existing git repo): history is never rewritten. The block and `paper.toml` (if it
  has no `profile` yet) are written to the working files, so they show up in the draft as unsaved
  changes; the init result lists them. An existing `profile` in `paper.toml` wins over detection
  and `--profile` disagreeing with it is a user error (change it with `paper config set --project`).
- **Changing profile later:** `paper config set --project profile <name>` also replaces the block
  in `.gitignore`, and says so.
- **Tips:** `init` prints the chosen profile's tips (human output) and includes them in JSON.

## Scope

1. paper-core `profile` module: `Profile` (name, detect, ignore, tips), `Profile::builtin(name)`,
   `Profile::all()`, `detect(files) -> Profile`, and `gitignore::apply(existing: Option<&str>,
   profile) -> Result<Option<String>, GitignoreError>` (pure; `None` when nothing changes). No
   printing.
2. `config::keys` takes its profile names from `Profile::all()` (one source of truth).
3. `Project::init` takes an optional profile choice, detects otherwise, applies the block and
   `paper.toml` as decided, and reports `profile`, `detected: bool`, the files it wrote, and tips.
4. `paper config set --project profile` applies the block too.
5. CLI: `paper init --profile` becomes effective (remove the "no effect yet" help text); human
   output says `Profile: latex (detected from *.tex)` or `(chosen)`, the files written, then the
   tips; JSON has `profile`, `detected`, `written`, `tips`.
6. Specs: 04-content § Profiles (fields in v0.1, detection order, block rules, line endings),
   02-cli § `paper init` (remove the "*Not yet effective*" note; adoption behaviour) and
   § `paper config` (`profile` updates the block).

## Out of scope

- `text_extensions`, `binary_extensions`, `tokenizer` (v0.3); user-defined profiles; removing the
  block; Jupyter output stripping; the cloud-sync warning.

## Acceptance criteria

- [ ] Each of the six profiles parses, includes the shared ignore lines, and `latex` doesn't ignore `*.pdf`.
- [ ] Detection picks quarto > latex > typst > jupyter > markdown > generic, using only files the walk sees.
- [ ] Block merge: created when there's no file; appended keeping every existing byte; replaced in place (other profile); re-applying is byte-identical; CRLF kept; an unterminated or doubled block → error, nothing written. Unit-tested.
- [ ] `init` in a new LaTeX folder writes `.gitignore` and `paper.toml`, and the first save contains them and none of `*.aux`/`*.log` (checked with `git ls-tree` and `git status --porcelain` clean).
- [ ] `init --profile markdown` in a LaTeX folder uses markdown and reports it as chosen.
- [ ] Adoption: the block and `paper.toml` appear as unsaved changes in the draft, history is unchanged (`git log` identical), already-committed files matching the new ignores stay tracked, and `git fsck --strict` is clean.
- [ ] Adoption with an existing `profile` in `paper.toml` keeps it; a conflicting `--profile` → exit 1 three-part error.
- [ ] `paper config set --project profile typst` replaces the block and says so.
- [ ] `init` prints the profile's tips; JSON has `profile`, `detected`, `written`, `tips`; snapshot-tested.
- [ ] Specs updated.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- Unit tests in `src/profile/` for parsing, detection and every block case (pure strings).
- `crates/paper-core/tests/profile.rs` and additions to `tests/init.rs`: real temp folders and
  repositories, cross-checked with `git ls-tree -r`, `git status --porcelain`, `git log`,
  `git check-ignore`, `git fsck --strict`.
- `crates/paper-cli/tests/init.rs` and `config.rs`: human and JSON snapshots, with the isolated env.

## Summary

## Handoff log

### RED · test-author · 2026-10-16
- Tests: `crates/paper-core/src/profile/mod.rs` (9 unit: parsing, shared lines, detection order),
  `crates/paper-core/src/profile/gitignore.rs` (14 unit: block merge, CRLF, errors, `apply_to_file`),
  `crates/paper-core/tests/profile.rs` (12: new project, chosen, walk-only detection, malformed block,
  adoption, conflicts, `apply_to_file` vs git), `crates/paper-cli/tests/init.rs` (+7),
  `crates/paper-cli/tests/config.rs` (+3). 46 new tests (+ 1 new snapshot `init_json_latex`).
- Failing on: `todo!()` in `Profile::all/builtin`, `detect`, `gitignore::apply/apply_to_file`,
  `Project::init_with_profile` (all profile unit and core integration tests); assertions in CLI
  tests (no profile line, no `.gitignore`/`paper.toml`, `--help` text, no block update on
  `config set --project profile`). All other pre-existing tests pass.
- Stubs: `paper_core::profile::{Profile{name,detect,ignore,tips}, Profile::all, Profile::builtin(&str)
  -> Option<&'static Profile>, detect(&[impl AsRef<str>]) -> &'static Profile}`,
  `profile::gitignore::{apply(Option<&str>, &Profile) -> Result<Option<String>, GitignoreError>,
  apply_to_file(&Path, &Profile) -> Result<bool, GitignoreError>, GitignoreError{UnterminatedBlock,
  DuplicateBlock, Io}}`, `Project::init_with_profile(path, identity, options, Option<&Profile>)`,
  `Initialized{profile, detected, written, tips}` (placeholders in the old `init`),
  `InitError::{ProfileConflict{requested,existing}, Gitignore, Config}` (mapped to a project error
  for now; GREEN must make ProfileConflict and Gitignore user errors, exit 1, three parts).
  GREEN should make `Project::init` delegate to `init_with_profile(.., None)`.
- Binding choices of mine (the ticket was silent): detection matches by file name in any folder;
  `apply_to_file` is the core entry point for `config set --project profile`; `written` is
  `[".gitignore", "paper.toml"]` in that order; adoption with `profile` in paper.toml reports it
  as not detected is NOT asserted. CLI prints `Profile: latex (detected from *.tex)` /
  `Profile: markdown (chosen)`.
- Existing tests changed (all because init now saves/writes `.gitignore` and `paper.toml`):
  core `tests/init.rs`: `init_in_an_empty_folder_creates_a_first_save_and_an_empty_draft`
  (tracked_files 0 -> 2), `init_with_existing_files_saves_them_and_leaves_git_clean`
  (tracked 3 -> 4, ls-tree gains `paper.toml`), `adopting_keeps_history_and_turns_uncommitted_work_into_the_draft`
  (draft files gain `.gitignore`, `paper.toml`). These fail until GREEN.
- Existing CLI snapshots that GREEN must re-accept (still pass today, will change): `init__init_created`,
  `init__init_adopted`, `init__init_json`. `init__init_twice` should not change. New snapshot to accept:
  `init__init_json_latex`. Other tests that run init (CLI `Env::project_with_three_saves`, core tests) now
  also get `.gitignore`/`paper.toml`; GREEN to check they still pass.
- Questions: none.

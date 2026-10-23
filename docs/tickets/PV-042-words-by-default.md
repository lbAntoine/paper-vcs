# PV-042: word diff is the default for text in diff, show and status

- **Status:** In progress
- **Issue:** #81
- **Branch:** `feat/PV-042-words-by-default`
- **Depends on:** PV-040 (tokenisers) and PV-041 (word diff), both merged. Wave 3 of v0.3-a, alone.
- **Touches:**
  - **paper-core**
    - `crates/paper-core/src/project/{diff,status}.rs`: the word mode and `ChangeStat::Words`.
    - `crates/paper-core/src/config/{keys,settings}.rs`: `diff.default` defaults to `words`, and its description loses "no effect until v0.3".
    - `crates/paper-core/tests/{diff,status,show_diff,config}.rs`.
  - **paper-cli**
    - `crates/paper-cli/src/{inspect.rs,cli.rs,commands.rs}`: diff, show and status wiring, and the flag docs.
    - `crates/paper-cli/src/render/{diff,json_diff,status,human,style}.rs`.
    - `crates/paper-cli/tests/{diff,show,status,acceptance}.rs`, plus their snapshots.
  - **Specs**
    - `docs/specs/02-cli.md` § `paper diff`, § `paper show`, § `paper status` and the JSON output section.
    - `docs/specs/04-content.md` § Word diff, step 3 (rendering only).
    - `docs/specs/05-roadmap.md` § v0.3: the `paper-diff` line, and the Accept line if met.
- **Must not touch:** `paper-diff`. If the word diff needs a change, report it first. Also off limits: undo/checkout/restore/recover code and `paper ignore`.
- **Specs:** `02-cli.md` § `paper diff`, § `paper show`, § `paper status`, § JSON output; `04-content.md` § Word diff; `05-roadmap.md` § v0.3.

## Goal

A researcher reads changes as prose. `paper diff`, `paper diff --since <point>`, `paper show`
and `paper status` compare text word by word by default. `--lines` and `diff.default = lines`
keep the unified line diff for code and piping.

This ticket carries the roadmap's v0.3 acceptance: a PI can run `paper diff --since <point>`
on a co-author's changes and read it as prose.

## Context

- `paper-diff` provides:
  - `tokeniser_for(path)`;
  - `diff_words(old, new, tokeniser) -> WordDiff { paragraphs, added, removed, layout_only }`;
  - `Segment::{Same, Removed, Added, Cut}` and `Segment::is_break()`;
  - `to_word_markup`.

  Only `Word` tokens are counted. Spacing-only differences are `layout_only`. The 2 s deadline is a documented limit.
- Today:
  - `paper diff --words` is a user error ("not yet available").
  - `diff.default` defaults to `lines`.
  - `status` shows line counts (`+12 −3 lines`).
  - The JSON `added`/`removed` fields count lines.
  - The diff JSON has `hunks`.
- The 02-cli examples already show words: `status` prints `+42 words −17 words`, and 04-content's word diff example shows `¶ 3` with inline markup.

## Decisions (coordinator, 2026-10-23)

### Choosing words or lines

- **Mode:** `diff.default` now defaults to `words`.
  - `paper diff`/`show` use the config value. `--lines` and `--words` override it. They are mutually exclusive, as today.
  - `status` always uses words for text files. It is a summary, so `--lines` does not apply to it.
  - Binary files (and Office files, until v0.7) are unchanged.
- **Core:**
  - `DiffRequest` gains `mode: DiffMode { Lines, Words }`.
  - `FileDiffEntry` carries either the line diff or a word diff: an enum, or a new variant next to `FileDiff`. Use `tokeniser_for(path)`.
  - An added or deleted text file is a word diff against the empty text.
  - `status`'s `ChangeStat` gains `Words { added, removed }`.
  - Core never prints.

### Human output

- **Word mode, per file:**
  - A header line: `sections/intro.tex  +12 words −4 words`. Use `1 word` for the singular.
  - Then, for each changed paragraph, `  ¶ N` followed by its text indented two spaces.
  - Wrap the text at the terminal width (80 when not a terminal). Never break inside a token, and never break inside a `[-`/`{+` marker pair's delimiters.
  - Colour on a terminal: removed text red with strike-through, added text green, unchanged text plain. A `Cut` shows as a dim ` … `.
  - Without colour, use the plain markup `[-…-]{+…+}`, `{+¶+}`, `[-¶-]` and ` … `, exactly as `to_word_markup` writes it.
  - A file with `layout_only`: `path  spacing and line breaks only`, with no paragraphs.
  - Files are in path order, with one blank line between files.
- **`--stat` in word mode:** `path  +12 words −4 words` per text file, the binary sizes line as today, and a total `2 files changed, +13 words −3 words`.
- **`paper status`:** text changes show `+N words −M words` (`1 word` singular). A layout-only change shows `spacing only`. The rest of the layout is unchanged.
- **Line mode** (`--lines`, or `diff.default = lines`): byte-identical to today's output.
- **`paper show <point>`:** the same diff rendering as `paper diff` for that save, honouring `diff.default` and `--lines`.

### JSON (schema 1, additive only)

- **Diff and show files:** each file object keeps every existing key and gains:
  - `"unit": "words"|"lines"|null`, which is `null` for binary files;
  - `"layout_only": bool`;
  - `"paragraphs"`: `null` in line mode, or `[{"number", "segments": [{"kind": "same"|"removed"|"added"|"cut", "text": string|null}]}]` in word mode. `text` is `null` only for `cut`.

  In word mode, `hunks` is `null`. In line mode, `paragraphs` is `null`. `added` and `removed` count in `unit`.
- **Status changes:** each change gains `"unit"`. The existing `added`/`removed` fields count in that unit.
- **Spec:** 02-cli's JSON output section lists the new keys.

### Snapshots and acceptance

- **Snapshots:** existing diff, show and status snapshots change on purpose. GREEN re-accepts each one after reading it, and keeps line-mode snapshots byte-identical by running them with `--lines` where they test the line format.
- **Acceptance test:** add an end-to-end CLI test in `tests/acceptance.rs`.
  - Build a LaTeX project with a milestone-like save point (a save is enough). Then make a "co-author" change: re-wrap one paragraph, edit a few words in another, add a sentence, and delete a paragraph.
  - `paper diff --since <that save>`, without colour, shows only the real edits as word markup with `¶` numbers.
  - It shows nothing for the re-wrapped paragraph.
  - Its header counts match.
  - Tick the roadmap Accept line when it passes.
- **Performance:** `paper diff` on a 300 KB `.tex` file with scattered edits stays under 0.5 s in a release build. Measure it and record the result in the handoff.

## Acceptance criteria

- [ ] `paper diff`, `diff <A> <B>`, `diff --since` and `paper show` use words for text by default. `--lines` and `diff.default = lines` give today's unified diff byte for byte, and `--words` works.
- [ ] Human word output as decided: header counts, `¶ N`, wrapping, colour, plain markup, layout-only, cuts. `--stat` uses word counts and totals.
- [ ] `paper status` shows word counts for text changes, and `spacing only` for layout changes.
- [ ] The JSON keys are added as decided, with every existing key still present, and are snapshot-tested.
- [ ] Binary and Office files are unchanged.
- [ ] The acceptance test (PI reads a co-author's changes as prose) passes, and the roadmap line is ticked.
- [ ] Specs are updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`, exit code checked) and the full suite pass (CI on Linux, macOS and Windows).

## Test plan

- **Core:** word mode in `diff` and `show_diff`:
  - the counts;
  - added and deleted files;
  - `layout_only`;
  - binary files unchanged;
  - `ChangeStat::Words` in `status`.
- **CLI:** words by default, `--lines`, `--words`, `diff.default = lines`, and `--stat`, plus the status and show outputs.
  - Use `contains` checks in RED. GREEN writes the snapshots.
  - Check the JSON key sets.
  - Add a wrapping test with a fixed width (`COLUMNS`, or the not-a-terminal default of 80).
- **Acceptance:** the test described in the Decisions.
- **RED proof:** the tests must fail on `origin/main` for the stated reason.

## Summary

## Handoff log

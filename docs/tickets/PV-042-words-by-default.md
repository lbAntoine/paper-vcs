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

### RED · test-author · 2026-10-23

- **State:** fmt and clippy (`-D warnings`) clean. `cargo test --workspace --all-targets`: 1332 pass, 62 fail. Every failing test is a new test or a deliberately changed one (lists below). Every other pre-existing test passes.
- **Stubs added (no logic, current behaviour kept):**
  - core `project/diff.rs`: `DiffMode { Lines, Words }` (`Default` is `Words`); `DiffRequest.mode`; `enum EntryDiff { Lines(FileDiff), Words(WordDiff) }` and `FileDiffEntry.diff: EntryDiff` (binary files are `Lines(FileDiff::Binary{..})` in both modes); `Project::show_diff(&point, mode)` (signature change). The stub ignores `mode` and always returns `Lines`.
  - core `project/status.rs`: `ChangeStat::Words { added, removed, layout_only }`. **Deviation from the ticket's `Words { added, removed }`:** `layout_only` is needed for `spacing only` (a spacing-only change and a punctuation/command-only change both count 0 words). The stub never produces it.
  - CLI: `paper show` gains `--lines`/`--words` (mutually exclusive, accepted and ignored); `Words` arms in `render/{diff,json_diff,status}.rs` are `todo!()`; `inspect.rs` passes `DiffMode::Lines`. `paper diff --words` still gives the old "not yet available" error (GREEN removes it).
- **New core tests (22):** `diff.rs` (8: default mode is words, counts for modified/added/deleted, paragraph numbers and segments, rewrap is `layout_only`, LaTeX tokeniser only for `.tex`, cuts, binary unchanged, paths and unknown paths), `show_diff.rs` (7: counts, first save vs nothing, draft with deleted file, layout-only, binary, cut, lines still has hunks), `status.rs` (3: spacing only, commands not counted, other text files), `config.rs` (2). Binary and "lines still has hunks" tests are guards and pass already. They fail on `expected a word diff` or on count assertions.
- **New CLI tests (about 55):**
  - `diff.rs`: default words with the exact single-file output, singular/plural, file order and blank line, `¶ N`, added/deleted files, `[-¶-]`, layout-only line, cut, `--lines` byte-for-byte, `--words`, `--lines --words` refused, `diff.default = lines` and the flag overrides both ways, `--since`, binary unchanged, colour (red, green, strike-through `\x1b[9m` or `;9m`, no markers; dim cut), wrapping at 80 columns (12 filler widths, no line over 80, no token or marker delimiter split, long token never broken), `--stat` totals and binary line and `--stat --lines`, JSON word mode (`unit`, `layout_only`, `paragraphs`, `hunks` null, cut `text` null, binary `unit` null), JSON line mode, `--help` text. Sanity tests for the `minus` and `paragraph_lines` helpers.
  - `show.rs` (7), `status.rs` (5: singular, `spacing only`, ignores `diff.default`, `--lines` is not a flag, JSON layout-only), `config.rs` (3 new), `acceptance.rs` (2: the PI scenario and a pure re-wrap).
- **Existing tests changed deliberately:**
  - `cli/tests/diff.rs`: now pass `--lines` (unified format kept byte for byte, snapshots untouched): `diff_shows_the_unsaved_changes_as_a_unified_diff`, `diff_from_a_point_compares_it_with_the_draft`, `diff_between_two_points_compares_them_and_ignores_the_draft`, `paths_after_the_points_restrict_the_diff`, `a_path_with_no_points_before_it_is_a_path`, `a_folder_path_selects_everything_under_it`, `double_dash_makes_everything_after_it_a_path`, `a_name_that_is_both_a_point_and_a_file_is_a_point_without_double_dash`, `paths_are_relative_to_where_paper_runs`, `diff_shows_added_and_deleted_files_against_dev_null`, `diff_output_applies_in_reverse_with_git`, `diff_between_saves_applies_in_reverse_on_that_tree`, `stat_prints_one_line_per_file_and_a_total`, `colour_is_used_only_when_asked_for`, `diff_snapshots_first_and_with_no_snapshot_does_not`, `diff_json_has_a_fixed_key_set_for_text_and_binary_files` (also gains the `unit`, `layout_only`, `paragraphs` keys; snapshot `diff_json` will change).
  - `cli/tests/diff.rs` removed: `words_is_not_yet_available` and `lines_is_accepted_and_is_the_default` (replaced by `words_flag_is_accepted_and_is_the_default` and the `--lines` tests).
  - `cli/tests/show.rs`: `show_last_prints_metadata_and_changed_files`, `show_the_first_save_lists_every_file_as_added`, `show_stat_prints_the_counts_instead_of_the_diff`, `show_the_draft_shows_its_diff_against_last` now pass `--lines`; `show_json_has_schema_1_the_save_and_its_changes` passes `--lines` and gains the three keys. Snapshots `show_human`, `show_stat`, `show_json` kept in line mode (`show_json` gains keys).
  - `cli/tests/status.rs`: `status_shows_the_header_and_each_change_with_counts_or_sizes_sorted` now expects `+3 words −1 word` (snapshot `status_human` will change); `status_json_has_a_fixed_key_set` gains `unit` and counts `3`/`1` (snapshot `status_json` will change).
  - `cli/tests/acceptance.rs`: the v0.1 scenario runs `diff --lines`.
  - `cli/tests/config.rs`: `diff_default_words_is_accepted_and_says_it_has_no_effect_yet` (now: no note), `get_prints_the_effective_value` and `list_shows_every_key_with_its_value_and_source` (default is `words`), `the_words_note_is_one_clear_sentence` (replaced by `diff_default_words_prints_no_note_on_get_or_list`), `get_json_carries_the_words_note` (now `get_json_has_no_words_note`), `list_json_carries_every_note_and_keeps_stderr_empty` (2 notes). Snapshot `config_list` will change. The `--help` snapshots that mention `diff.default` or the words note may need re-accepting.
  - `cli/tests/json_errors.rs`: the `diff --words` error case became `diff --since nope`.
  - `core/tests/diff.rs`: struct literals get `mode: DiffMode::Lines`; the `diff` helper uses `Lines`, matches go through `EntryDiff::Lines`. `core/tests/show_diff.rs`: existing calls pass `DiffMode::Lines`. `core/tests/status.rs`: `status_reports_line_last_save_and_sorted_changes_with_counts` renamed `..._with_word_counts` and expects `ChangeStat::Words`. `core/tests/config.rs`: `defaults_apply_when_nothing_is_set` expects `words`; `words_for_diff_default_is_accepted_and_has_no_effect_yet` renamed.
- **RED proof on origin/main** (b6ee9ef, detached probe worktree, own target dir, removed after): the new and changed CLI test files run against the old binary. 55 tests fail in `diff`, `show`, `status`, `config` and `acceptance`. Reasons: the word format is absent (output is unified, status says `lines`), `diff --words` returns "not yet available", `show --lines/--words` is an unknown flag, and the JSON key sets lack `unit`/`layout_only`/`paragraphs`. Core tests don't compile on origin/main (no `DiffMode`, `EntryDiff`, `ChangeStat::Words`), so their failure there is the missing API; against the stubs they fail on `expected a word diff` and count assertions.
- **Open for GREEN:**
  - `Words { added, removed, layout_only }` instead of the ticket's two fields (tell the coordinator).
  - Tests don't pin the exact segment split of a paragraph add or delete (`[-¶-]` plus `Gamma delta`), only that the marker and text appear; the PI test expects paragraph numbers 2, 3, 4 (the deleted paragraph takes the number of the next new one).
  - The strike-through code is asserted loosely (`\x1b[9m` or `;9m`); the red and green codes as `\x1b[31`/`\x1b[32`.
  - `paper show --stat` in word mode: `1 file changed, +1 word −1 word`.
  - No insta snapshots were written by hand. GREEN re-accepts `diff_json`, `show_json`, `status_human`, `status_json`, `config_list` (and any help snapshot) after reading them.
  - Performance (300 KB `.tex`, under 0.5 s release) is GREEN's to measure.

# PV-042: word diff is the default for text in diff, show and status

- **Status:** In progress
- **Issue:** #81
- **Branch:** `feat/PV-042-words-by-default`
- **Depends on:** PV-040 (tokenisers) and PV-041 (word diff), both merged. Wave 3 of v0.3-a, alone.
- **Touches:** (widened 2026-10-23 by the coordinator to `crates/paper-diff/src/word.rs`, `src/word/align.rs`, `tests/word*.rs` (new tests only) and 04-content § Word diff, for the semantic cleanup)
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
  - `status`'s `ChangeStat` gains `Words { added, removed, layout_only }`. `layout_only` lets status say `spacing only` for a spacing change, while a punctuation- or command-only change counts 0 words and shows `+0 words −0 words`.
  - `FileDiffEntry.diff` is `EntryDiff { Lines(FileDiff), Words(WordDiff) }` (binary files are `Lines(FileDiff::Binary)`), and `show_diff(&point, mode)`.
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

- [x] `paper diff`, `diff <A> <B>`, `diff --since` and `paper show` use words for text by default. `--lines` and `diff.default = lines` give today's unified diff byte for byte, and `--words` works.
- [x] Human word output as decided: header counts, `¶ N`, wrapping, colour, plain markup, layout-only, cuts. `--stat` uses word counts and totals.
- [x] `paper status` shows word counts for text changes, and `spacing only` for layout changes.
- [x] The JSON keys are added as decided, with every existing key still present, and are snapshot-tested.
- [x] Binary and Office files are unchanged.
- [x] The acceptance test (PI reads a co-author's changes as prose) passes, and the roadmap line is ticked.
- [x] Specs are updated (only the listed sections).
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

## Known gaps

- A change of file mode only (for example the executable bit) is listed as `modified` with `+0 words −0 words` in `status` and `diff`, and prints nothing with `--lines`. Left as is.
- In a `.bib` file a `%` inside a value (for example `%20` in a URL) starts a comment for the renderer's line-break rule, so the line break after it is kept. Harmless but not exact.
- A line can start with CJK closing punctuation (no kinsoku rules).
- Removed text carries the new text's spacing (PV-049, #93), so on the old side a comment can still run into the next line. Two cases: `[-l3w0 % l3w1 l4w0-]` (two source lines shown as one removed run) and `Begin [-% note here-]{+added+} alpha[- beta-] gamma`. An ignored property test (`removed_text_never_runs_a_comment_into_another_line`) records it.
- At narrow widths a long comment soft-wraps, and its continuation lines read as uncommented.
- A fully rewritten paragraph can still split into pieces (PV-048, #92).

## Summary

Word diff is the default for text in `paper diff`, `paper show` and `paper status`. Core returns `EntryDiff::{Lines, Words}` per file (`DiffMode`, `show_diff(&point, mode)`) and `ChangeStat::Words`. The CLI renders a header with counts, numbered paragraphs wrapped by display width (terminal on stdout, `COLUMNS`, else 80), colour or plain markup, layout-only files, `--stat` word counts, and the additive JSON keys (`unit`, `layout_only`, `paragraphs`). `--lines` and `diff.default = lines` keep the unified diff. paper-diff gained a cleanup pass so a replaced stretch reads as one removed and one added run. New dependencies: `terminal_size` 0.4.4 and `unicode-width` 0.2.2. The PI acceptance test passes; 300 KB `.tex` diff takes 0.03 s in release.

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

### GREEN · rust-engineer · 2026-10-23

- **State: BLOCKED on three RED tests** (details below). Everything else is implemented; fmt and clippy (`-D warnings`) exit 0; the full suite passes except those three.
- **Implemented**
  - core: `entry_diff()` in `project/diff.rs` (word diff via `tokeniser_for(path)`, absent side = empty text, binary stays `Lines(Binary)`); `status` always compares in words and builds `ChangeStat::Words`; `diff.default` defaults to `words` and its description lost "no effect until v0.3".
  - CLI: mode from `--lines`/`--words`, else `diff.default` (`inspect::diff_mode`); the "words not available" error and the config words note are gone; `render/words.rs` (header, `¶ N`, wrap, colour or plain markup, layout-only, singular); `diff`/`show`/`--stat`/`status` use it; JSON gains `unit`, `layout_only`, `paragraphs` (and `unit` in status), `hunks` null in word mode.
  - Deviation from Touches: a new dependency, `terminal_size` 0.4 (workspace `Cargo.toml`, `paper-cli/Cargo.toml`, `Cargo.lock`), to read the terminal width. Off a terminal the width is 80. `docs/specs/03-architecture.md` config sample line updated too.
  - Wrapping: white space runs in a paragraph become one space or a line break (so wrapped output is the plain markup with re-flowed white space). A removed/added segment's edge white space stays glued to its delimiter, so a delimiter never starts or ends a line; a paragraph-break marker is a zero-width break point.
  - `--stat`: a spacing-only file shows `spacing and line breaks only`. Total uses word counts when any entry is a word diff (a stat of only binary files prints `+0 −0`).
- **Snapshots re-accepted** (each diff read): `config_list` and `config_list_json` (`diff.default` value `lines` to `words`); `diff_json` and `show_json` (each text file gains `layout_only: false`, `paragraphs: null`, `unit: "lines"`; the binary file gains `unit: null`; hunks unchanged; both run in `--lines`); `status_human` (`+0 −1 lines`, `+2 −1 lines` became `+0 words −2 words`, `+3 words −1 word`); `status_json` (`unit` added; counts now words: related.tex removed 2, intro added 3). Help snapshots did not change.
- **Performance:** release build, 300 KB `.tex` (about 300 paragraphs, scattered single-word edits in every 7th): `paper diff` 0.033 s, `diff --stat` 0.027 s, `status` 0.026 s.
- **Specs updated:** 02-cli (status, show, diff, JSON output, config table and notes), 04-content (Word diff step 3), 05-roadmap (`paper-diff` line ticked; the Accept line is NOT ticked, see below), 03-architecture (config sample).
- **Failing RED tests (not touched):**
  1. `acceptance::a_pi_reads_a_co_authors_changes_as_prose`. Cause is the word diff's alignment in `paper-diff` (off limits). Old `...next section.¶This paragraph will be deleted by the reviewer entirely.¶Closing`, new `...next section. They are stable across runs.¶Closing`. The `Space` tokens and the final `.` align as unchanged, so the segments of paragraph 3 interleave: `Same("Results ... section.") Removed("¶") Removed("This paragraph") Same(" ") Removed("will") Added("They") Same(" ") Removed("be") Added("are") ...`. Neither "This paragraph will be deleted by the reviewer entirely" nor "They are stable across runs." is contiguous (to_word_markup gives the same interleaving), and there is no `¶ 4`. The test expects the replaced region as one removed run and one added run. Fix options: change paper-diff alignment so a changed run is not split by equal single spaces/punctuation (preferred), or the coordinator relaxes the test. I did not touch either.
  2. `diff::stat_keeps_the_sizes_line_of_a_binary_file` expects `-3 words` in the total. `main.tex` was `\documentclass{article}`: the LaTeX tokeniser counts one word (`article`), so the total is `+3 words −1 word` (consistent with the status test that says commands are not counted). The assertion's constant looks wrong.
  3. `diff::diff_json_in_word_mode_has_unit_paragraphs_and_null_hunks` expects `layout.txt` to be `layout_only: true` in `diff last~1`, but `layout.txt` is created in the "Add layout" save, so against `last~1` it is an added file (`+3 words`). Either the diff must be `last` (and the `intro`/`plot` expectations moved) or the file saved earlier.

### RED (fix) · test-author · 2026-10-23

- `cli/tests/diff.rs`, two wrong expectations fixed (test files only, no src):
  - `stat_keeps_the_sizes_line_of_a_binary_file`: the total is `+3 words −1 word` (the old `main.tex` holds only a LaTeX command, no words). The binary sizes assertion is unchanged.
  - `diff_json_in_word_mode_has_unit_paragraphs_and_null_hunks`: `layout.txt` is now saved first, then re-spaced in the draft together with the intro edit and the binary file, and the test runs `diff` (last against the draft). So it still checks a real layout-only file.
- Both pass against GREEN d69b360 (the whole `diff` test file: 61 pass). `a_pi_reads_a_co_authors_changes_as_prose` is untouched.

### GREEN (unblocked) · rust-engineer · 2026-10-23

- **Cleanup pass** (`cleanup` in `paper-diff/src/word.rs`, between alignment and grouping): a `Same` run between two changes with no `Word` and no paragraph break is turned into a removed and an added copy; each changed stretch that absorbed something is reordered to removed-then-added between paragraph breaks. Counts unchanged. All PV-041 tests pass unchanged; new table tests in `tests/word_cleanup.rs`. Spec: 04-content § Word diff, new Cleanup bullet.
- **terminal_size:** 0.4.4, published 2026-03-23 (about 7 months). `Cargo.lock` changed only by the `terminal_size` package, its `paper-cli` dependency line (its own dependencies `rustix` and `windows-sys` were already locked).
- **Test-author fixes** (25d96b8) pulled in as they sit in this branch.
- **Acceptance test still fails, STOPPED as instructed.** Actual output of `paper diff --since`:

  ```
  sections/intro.tex  +7 words −11 words
    ¶ 2
    Our method relies on [-careful-]{+precise+} measurements of the sample under
    [-three-]{+four+} different conditions.
    ¶ 3
    Results are summarised in the next section.[-¶-][-This paragraph will be
    deleted by the reviewer entirely-]{+ They are stable across runs+}.
  ```

  It reads as prose, the counts match and paragraph 1 is silent, but the test wants (a) `They are stable across runs.` with the final period, which is unchanged text after the added run and before the unchanged paragraph break (so not "between two changes"), (b) `¶ 4`, and (c) the removed phrase unsplit (the 80-column wrap breaks it after "will be"). The roadmap Accept line is not ticked.

### RED (fix 2) · test-author · 2026-10-23

- `cli/tests/acceptance.rs::a_pi_reads_a_co_authors_changes_as_prose` re-expressed around readability (test file only). It keeps the header counts, silence for the re-wrapped paragraph and the `[-careful-]{+precise+}` / `[-three-]{+four+}` edits. It drops `¶ 3`/`¶ 4`, the period in the added run and any line position. New helper `assert_deleted_paragraph_and_added_sentence_read_as_prose` reads `--json diff --since` and checks: (a) the deleted paragraph's text is one removed run; (b) "They are stable across runs" is one added run; (c) that stretch holds at most one removed and one added non-break run; (d) a removed paragraph-break segment exists. The plain markup with the wrap undone (whitespace joined) is checked for `[-¶-]` and both phrases.
- Passes on c2e9294. On a scratch copy of d69b360 (own target dir, removed after) it fails at (a): no single removed run holds the deleted paragraph. clippy and fmt clean.

### GREEN (review round 1) · rust-engineer · 2026-10-23

1. Roadmap Accept line ticked with the PV-048 (#92) limit.
2. Width: `terminal_size_of(stdout)` (so only stdout counts), then numeric `COLUMNS` first; documented in 02-cli. Test `columns_sets_the_wrap_width` (a pty is not available, so the stderr/stdin case is covered by design: stdout is a pipe in tests and wraps at 80).
3. Comments: the CLI renderer recognises a LaTeX `%` comment (not `\%`) and keeps the line break that ends it as a hard break. Deviation: it is done in `render/words.rs` from the segment text, not by a new marker in paper-diff (no API change); 04-content says so. Tests for a comment and for `100% sure` in a .txt file.
4. Cleanup absorbs only when both neighbours are non-break changes; test `"、\n\nab"` to `"ab\n\n、"`.
5. `Command` and `Comment` are never absorbed (`solid`); tests for an unchanged comment and command between edits; 04-content updated.
6. Display width with `unicode-width` 0.2.2: **new dependency** (it was not in Cargo.lock; Cargo.lock changed only for it). Lines may break between CJK characters (a Joint between two CJK chars), never inside delimiters. Korean and Japanese wrapping tests.
7. Paragraphs with the same number share one `¶ N`.
8. Status JSON changes gain `layout_only` (`null` for binary files); `status_json` snapshot re-accepted (only the new key); `status_json_has_a_fixed_key_set` key list gained `layout_only` (needed by this decision).
9. A cut at a line end loses its trailing space (test over widths 30 to 59).
10. A removed or added run is painted whole, spaces included (test).
11. 02-cli documents the binary-only `--stat` total; test added.
12. Snapshots `diff_words_human` (plain markup: modified paragraph with an added clause, a spacing-only file, a new file) and `diff_words_json` (modified file with segments, layout-only file); both read before accepting.
13. Mode-only change noted under Known gaps.

### GREEN (review round 2) · rust-engineer · 2026-10-23

1. A comment's line break that ends an added or removed run now closes the marker and is kept as a hard break (`close_last_piece`), so `{+% TODO check with Ana+}` is followed by a new line. Tests for an added and a removed comment line, plain and colour (line structure checked after stripping colour codes). 02-cli and 04-content say so.
2. CJK: a Joint is pushed between a piece and the next piece when both edge characters are CJK, so edits break away from neighbours. Tests `다.연[-구-]{+漢+}자`-style (COLUMNS=20) and `す[-。-]{+x+}段[-落-]{+ん+}は`-style (COLUMNS=27): no line exceeds the width.
3. Hangul is not cut inside a run; it still breaks at piece boundaries and spaces. The Korean wrapping test now uses spaced words. 04-content and 02-cli updated.
4. Known gaps added: `.bib` `%20`, CJK closing punctuation at line start.
5. Summary filled; the acceptance criteria that hold are ticked; the last one (CI on three platforms) is left to the coordinator.

### GREEN (comment rule) · rust-engineer · 2026-10-23

- **One rule** in `render/words.rs`: the text of every segment is cut into elements (words, spaces, CJK break points and comment-ending line breaks); a line break is any whitespace run containing `\n` while a `%` comment is open. The comment state is kept per side (old, new), because unchanged text is spelled as in the new text. A segment is then split into spans at those breaks, and each span gets its own marker pair (`{+…+}` newline `{+…+}`). The edge branches of the earlier patches are gone. A paragraph-break marker and a cut that follow an open comment also start a new line.
- **Repros** (plain, 80 columns): `Intro. % n` / `{+Added sentence.+}`; `A % n` / `[-B.-]`; `A % n` / `{+B.+}`, each on separate lines.
- **Property test** (`render/words.rs`, proptest, 400 cases; also run at 20 000 cases) over random `.tex` pairs built from words, `%` comments, trailing comments, `\%`, blank lines, CRLF, and added, removed and changed lines, rendered at width 10 000. Asserts markers are balanced per line, every old and new word is shown, and no `%` comment is followed on its line by text of another source line, judged on the new side (unchanged text takes the new spelling, so the old side can't be judged). Verified in a scratch copy of 32ecbd3 with its own target dir: the property test and both repro tests fail there.
- **Edge-space glue:** a `Joint` after the closing-edge space piece; tests for `{+라구연 +}바자구바.` at COLUMNS=20 and a Latin case at 30.
- **CJK boundary test** now pads the text so the edit falls across the wrap point.
- **Specs:** 02-cli and 04-content reworded (Hangul sentence, exact comment claim).

### GREEN (final) · rust-engineer · 2026-10-23

- **Spec honesty:** the comment claim in 02-cli and 04-content is limited to the new text, with a pointer to #93. Both reviewer cases and the soft-wrap gap are in Known gaps. The Hangul wording is now "Hangul breaks only at spaces and at the edges of an edit".
- **Old-side check:** `removed_text_never_runs_a_comment_into_another_line` (per `[-…-]` span, `comment_is_contained`), `#[ignore = "PV-049 (#93): ..."]`. Run with `--ignored` it fails today (see the report), as intended.
- **Opening-edge glue:** a `Joint` before an opening-edge space piece; tests `supercalifragilistic{+ extraordinarily+}` at 30 and `바자구바.{+ 라구연+}` at 20 columns, no overflow.
- **Generator:** comments fixed (kinds, edit codes, kind 7 = no insertion); added `\\%` (escaped backslash then a comment), `word%c` and a file without a final newline; the `ids` doc fixed.
- **Bookkeeping:** PV-048 (#92) and PV-049 (#93) added to tasks.json (v0.3-b, todo, medium). Summary filled in and acceptance criteria ticked earlier; CI left to the coordinator.

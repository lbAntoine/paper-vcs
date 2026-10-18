# PV-031: undo gets stuck behind a file it had to leave alone

- **Status:** Done
- **Issue:** #57
- **Branch:** `feat/PV-031-undo-skipped`
- **Depends on:** PV-023/024/025 (merged). Runs in parallel with PV-029 (status).
- **Touches:** `crates/paper-core/src/project/undo.rs`, `crates/paper-core/src/project/save.rs`
  (a snapshot variant for the undo family only), `crates/paper-core/src/project/checkout_left.rs`
  (new: the remembered leftovers), `crates/paper-core/src/project/mod.rs` (one `mod` line only),
  `crates/paper-core/src/error.rs` (additions only), `crates/paper-core/tests/undo_skipped.rs`
  (new), `crates/paper-cli/src/render/undo.rs` (the warning), `crates/paper-cli/tests/undo.rs` and
  its snapshots, `docs/specs/02-cli.md` (§ `paper undo` only), `docs/specs/03-architecture.md`
  (§ On-disk layout: one entry for the new file; § Undo / redo algorithm: the Skipped paths and
  Snapshot first bullets)
- **Must not touch:** `crates/paper-core/src/project/status.rs`, `crates/paper-cli/src/render/status.rs`,
  `crates/paper-cli/src/render/json_diff.rs`, `crates/paper-cli/src/commands.rs`, 02-cli
  § `paper status` (PV-029); `project/recover.rs`, `project/checkout.rs`, `project/gc.rs`.
- **Specs:** `03-architecture.md` § Undo / redo algorithm, § Command lifecycle (I1); `02-cli.md`
  § `paper undo`; `01-concepts.md` I1

## Goal

When undo's checkout has to leave a file alone (a symlink or an ignored file in the way), plain
`paper undo` keeps walking back, and `redo` keeps working, instead of taking back the same
leftover over and over. Maintainer decision on #57: option (a).

## Context

- Today: `go()` in `undo.rs` runs `self.snapshot()` (I1) first. The leftover difference at the
  skipped paths becomes an edits entry, the undo takes back only that entry, the checkout skips
  the same file again, and so on forever. Redo is cleared every time. `undo --to` still works.
- `plan_checkout` (`checkout.rs`) returns `skipped: Vec<SkippedPath>`; `refresh_stat_cache` in
  `undo.rs` already snapshots the working files after the checkout and knows `files_match`.
- The crash marker (`checkout-pending`) and the recovery rules (PV-025) are unchanged.

## Decisions (coordinator, 2026-10-18)

- **Remember the leftovers.** After an undo-family checkout whose working files don't match the
  new draft, write `.git/paper/checkout-left.json` atomically:
  `{"schema":1,"op":"<id of the op this command recorded>","paths":{"<path>": <what a snapshot
  sees there now>}}`. A path's value is its blob id and mode as the post-checkout snapshot
  records it, or `null` when the snapshot sees nothing there. Write it after the checkout and
  the stat-cache refresh, before the marker is removed. When the files match, remove any
  existing file instead.
- **Ignore them in the undo family's own snapshot only.** In `undo`, `redo` and `undo --to`,
  the I1 snapshot computes the working tree as usual. If `checkout-left.json` names the current
  head op, and the snapshot differs from the stored draft **only** at remembered paths, each
  still exactly as remembered, then no edits entry is recorded and the command proceeds from
  the stored draft. Any other difference, including a remembered path that changed again, is a
  real edit: the normal I1 entry is recorded, covering every difference.
- **Every other command is unchanged.** `status`, `save`, `restore` and the rest still record
  the leftovers as an edits entry (the files really differ). After that, one `undo` takes back
  that entry, its checkout skips the file again and remembers it, and the next `undo` walks on.
  Undo is never stuck.
- **Stale or unreadable file** (another op id, bad JSON, unknown schema): ignored, never an
  error, and replaced or removed by the next undo-family checkout.
- **Crash safety.** A crash before the file is written leaves today's behaviour: the next
  command records the leftovers as edits, and nothing is lost. Recovery doesn't write the file.
- **Warning.** The CLI warning for `files_match == false` says what now happens:
  `2 files were left as they are (see above). paper undo and paper redo go on from here; any
  other paper command records them as edits, which clears what you can redo.` The singular is
  `1 file was left as it is … records it as an edit …`. Backticks go around the commands as in
  the current text. JSON is unchanged.

## Scope

1. `checkout_left.rs`: read, write and remove the file, values only (no printing).
2. `undo.rs` / `save.rs`: the undo-family snapshot that ignores remembered leftovers; writing
   the file after the checkout.
3. CLI: the new warning text (human output only).
4. Specs: 02-cli § `paper undo` (warning text, and what repeated undo does with leftovers);
   03-architecture § Undo / redo algorithm (Skipped paths and Snapshot first bullets) and
   § On-disk layout (`checkout-left.json`).

## Out of scope

- Changing what `status` or `save` record. Recording leftovers in the op format. Recovery
  writing the file. Undo wording beyond this warning (PV-030).

## Acceptance criteria

- [x] A symlink in the way of a tracked file (unix), then three `paper undo`: each walks one
  entry further back (checked against `paper activity` positions), the symlink is never
  touched, and no `edited …` entry is recorded.
- [x] An ignored file in the way of a file undo wants back: the same, three undos walk back.
- [x] After those undos, `paper redo` brings the entries back in order (the redo stack wasn't
  cleared by leftovers).
- [x] Editing a remembered path again, or any other file, before `undo`: the normal edits entry
  is recorded (I1 holds), and nothing is lost.
- [x] `paper status` between undos records the leftovers as an edits entry (unchanged), and
  plain `undo` still gets past them (one extra step, never stuck).
- [x] A stale, unreadable or foreign `checkout-left.json` changes nothing (today's behaviour) and
  is not an error.
- [x] The file is written atomically after the checkout and removed when the files match;
  `git fsck --strict` clean; plain `git status` unaffected by it.
- [x] The new warning wording (both counts) is snapshot-tested.
- [x] Specs updated (only the listed sections).
- [x] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/undo_skipped.rs`: the symlink scenario (`#[cfg(unix)]`) and the
  ignored-file scenario, both on real temp repos. Each runs three undos, then redos, and checks
  the activity positions, the file bytes and the symlink target. Also cover: an edit before
  undo, a `status` in between, and stale or bad JSON. These must fail on `origin/main` (stuck
  undo).
- `crates/paper-cli/tests/undo.rs`: the warning lines (one file, two files) through the binary,
  plus one end-to-end "three undos walk back" with the ignored file, so it runs on Windows too.
- Reuse the existing undo test helpers. Any new shared helper gets a sanity test that passes on
  main.

## Summary

`paper undo`, `paper redo` and `paper undo --to` no longer get stuck behind a file the checkout
had to leave alone (a symlink or an ignored file in the way). After such a checkout, paper
remembers the leftovers in `.git/paper/checkout-left.json` (written atomically, removed when
the files match). The undo family's own I1 snapshot ignores remembered paths that are still
exactly as remembered, so no edits entry is recorded and each undo walks one entry further;
redo keeps working. Every other command still records the leftovers as an edits entry. The
warning says what happens next. A stale or unreadable file is ignored. The checkout is planned
against the working files, so skipped paths show again at each step.

## Handoff log

### RED · test-author · 2026-10-18

Files: `crates/paper-core/tests/undo_skipped.rs` (new, 18 tests) and
`crates/paper-cli/tests/undo.rs` (3 changed or new tests). **No stubs**: the tests need no new
API (they read `.git/paper/checkout-left.json` as plain JSON and use `Project::{undo, redo,
undo_to, status, activity}`), so GREEN adds `checkout_left.rs` from scratch. No shared helper
changed (`tests/common/mod.rs` untouched); the local scenario builders have a sanity test
(`the_ignored_scenario_leaves_chap_and_its_file_behind_at_the_second_undo`, passes on main).

Core tests (`undo_skipped.rs`), position = `position` field of the head op:

- Fail on main (undo stuck, redo stack cleared, or no `checkout-left.json`):
  `an_ignored_file_in_the_way_does_not_stop_undo_from_walking_back_and_records_no_edit`,
  `a_symlink_in_the_way_does_not_stop_undo_from_walking_back_and_never_gets_touched` (unix),
  `redo_brings_every_entry_back_in_order_after_undos_that_left_a_file_alone`,
  `redo_right_after_a_checkout_that_left_a_file_alone_records_no_edit`,
  `undo_to_goes_on_from_a_checkout_that_left_a_file_alone_without_recording_an_edit`,
  `the_checkout_remembers_what_it_left_alone_in_a_file_named_after_the_op_it_recorded`,
  `the_remembered_file_is_removed_when_a_later_checkout_leaves_nothing_alone`,
  `status_between_undos_records_the_leftovers_and_the_next_undo_still_gets_past_them`
  (fails at the last step, "never stuck"), `an_edit_to_a_remembered_path_taken_back_by_undo_can_be_redone`,
  `an_edit_to_another_file_taken_back_by_undo_can_be_redone`, and five stale/unreadable file
  variants (`not json`, `empty`, `schema 2`, `foreign op`, `paths is a list`): each asserts
  today's behaviour (no error, the leftovers recorded as one edits entry) and then that the
  file was replaced by a valid one naming the head op.
- Pass on main by design: the scenario sanity test, `editing_a_remembered_path_again_records_the_normal_edits_entry`
  and `editing_another_file_before_undo_records_every_difference_and_loses_nothing` (I1 still
  records a real edit; the redo halves are the separate tests above, which need the fix).
  The `status` recording half of the status test also holds today.

CLI tests (`paper-cli/tests/undo.rs`):

- `three_undos_walk_back_past_an_ignored_file_and_redo_brings_the_entries_back` (all OS): fails
  on main (the 3rd and 4th undo repeat `edited chap`).
- `a_checkout_that_skips_files_prints_each_with_its_reason_and_the_warning` and
  `a_single_skipped_file_gets_the_singular_warning` (unix): the old `WARNING_TAIL` assertions
  were replaced by `assert_warning` (key phrases via `contains`, backticks stripped before
  matching, old wording asserted gone). They fail on main by design: this is the wording change.
- **GREEN must accept** `snapshots/undo__undo_blocked_human.snap`: it contains the old warning
  and still passes on main; it will go stale with the new text. Nothing else in the existing
  snapshots changes (JSON unchanged).

Design choices GREEN should know:

- Op chain semantics (found while writing): the "position" after taking back an edits entry
  E recorded on top of head H is H's id, not an older entry. The status and edit tests assert
  that (`position == head before the command`) and that the next undo moves to H's parent.
- The ignored scenario has ops 0 start, 1 `edited keep.tex`, 2 save "one", 3 `edited chap,
  chap/one.tex`, 4 save "folder"; undo 2 is the one that can't bring `chap` back, leaving the
  remembered paths `chap` (null) and `chap/one.tex`. The test expects the value of a present
  path to be non-null (its exact shape, blob id + mode, is not asserted).
- The symlink scenario's first undo legitimately records one edits entry (the symlink is an
  edit when the undo starts); the tests assert no further one.
- Bad-file variants use full hand-written JSON (`remembered_for`) so they work on main too.
- Snapshot differences of the new warning are asserted with `contains` only; no hand-written
  snapshot of output that doesn't exist.

Questions: none. Open point for GREEN: the 0-skipped warning ("Some files were left as they
are ...") isn't specified by the ticket; the tests don't cover it.

### GREEN · rust-engineer · 2026-10-18

- `project/checkout_left.rs` (new): `CheckoutLeft` reads (`load_for(head)`: None when absent,
  bad JSON, unknown schema, other op), writes atomically (`write_atomic`), removes, and
  `explains(draft, now)` (differences confined to remembered paths, each still as remembered).
  Value of a path is `{"id","mode"}` or `null`.
- `save.rs`: `snapshot_ignoring` (the old `snapshot_as` delegates with `None`); no edits
  entry when the leftovers explain the whole difference. The stat cache is saved either way.
- `undo.rs`: loads the file for the head op before the I1 snapshot; after checkout and
  stat-cache refresh (now returns the working tree) writes the file for the op just recorded
  (paths = every path where the working files differ from the new draft, not only the
  `skipped` list, so it stays stable) or removes it, then clears the marker.
- CLI: new warning in `render/undo.rs` (no backticks, as the existing text had none; the 0
  skipped variant gets the same new tail). Re-accepted `undo__undo_blocked_human.snap` after
  reading the diff (only the last line changed).
- Specs: 02-cli § `paper undo`, 03-architecture § On-disk layout and § Undo / redo algorithm.
- No test assertions changed. Not covered: `gc` and the new file (gc.rs off limits; it
  ignores unknown files in `.git/paper/` as far as the tests show).

### GREEN (review round 1) · rust-engineer · 2026-10-18

- Finding 1: `go()` plans the checkout against the working files (`DraftState::working`, a
  new `pub(super)` tree id of the I1 snapshot) instead of the stored draft. Skipped paths now
  show again at each step where they are still in the way (the plan's own obstacle rules), so
  the "(see above)" list is real. Core test `four_undos_past_a_directory_swapped_for_a_file_...`
  and CLI twin carry the reviewer's sequence, but that exact sequence did NOT fail on the old
  planning for me (macOS, in-process and with the binary: undo 4 skipped `chap` as blocked);
  they pass and stay as regression tests. The PV-025 crash sequence
  (`a_crash_after_the_checkout_of_a_blocked_undo_does_not_stop_the_undos_that_follow`) does
  fail on the old planning and passes now.
- **Correction (post-approval):** the claim above that the reviewer's sequence did not fail on
  the old planning is wrong. Built from bc57e1d, `four_undos_past_a_directory_swapped_for_a_file_...`
  (core) fails with `Io { action: "remove", path: "chap", PermissionDenied }`, its CLI twin fails
  with exit 3, and `a_crash_after_the_checkout_...` fails too. They are genuine failing-first tests.
- Finding 2: `leftovers_to_remember` (pure, unit-tested in `undo.rs`): remembered only if the
  path differs from the new draft and holds what the I1 snapshot saw. Since a real race
  can't be timed with the binary, the rule is tested at that level; the consequence (a
  changed remembered path is recorded by the next I1) is the existing
  `editing_a_remembered_path_again_records_the_normal_edits_entry`.
- Finding 3: added `Undone::left_over` (a warning naming leftovers when `skipped` is empty);
  removed again in the post-approval cleanup. When both are empty the text is "Some files were
  left as they are." (no "see above").
- Findings 4-7: warning and 02-cli reworded (backticks, "looks at your files"), `files_match`
  doc updated, new snapshot `undo__undo_blocked_single_human.snap` (read), `undo_blocked_human`
  re-accepted (only the last line changed). 03-architecture updated for the plan and the
  remember rule. `checkout.rs` untouched.

### GREEN (post-approval) · rust-engineer · 2026-10-18

- Dropped `Undone::left_over`: every remembered path is a planned target (skipped, rewritten or
  unchanged), so it was reachable only through an exec-bit filesystem quirk. Removed the field,
  the two `left_over` branches of `warning()`, the three render unit tests that built it, and
  the matching 02-cli sentence. `remember_leftovers` now returns `()`. "Some files were left as
  they are." stays for `skipped == 0 && !files_match`. Removed the PV-030 follow-up about
  exposing `left_over` in JSON (round 1 entry).
- Rewrapped the 02-cli paragraph. Corrected the round 1 entry (see its correction line).
  Filled in the Summary and ticked the acceptance criteria.

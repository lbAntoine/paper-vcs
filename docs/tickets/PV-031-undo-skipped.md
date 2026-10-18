# PV-031: undo gets stuck behind a file it had to leave alone

- **Status:** In progress
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

- [ ] A symlink in the way of a tracked file (unix), then three `paper undo`: each walks one
  entry further back (checked against `paper activity` positions), the symlink is never
  touched, and no `edited …` entry is recorded.
- [ ] An ignored file in the way of a file undo wants back: the same, three undos walk back.
- [ ] After those undos, `paper redo` brings the entries back in order (the redo stack wasn't
  cleared by leftovers).
- [ ] Editing a remembered path again, or any other file, before `undo`: the normal edits entry
  is recorded (I1 holds), and nothing is lost.
- [ ] `paper status` between undos records the leftovers as an edits entry (unchanged), and
  plain `undo` still gets past them (one extra step, never stuck).
- [ ] A stale, unreadable or foreign `checkout-left.json` changes nothing (today's behaviour) and
  is not an error.
- [ ] The file is written atomically after the checkout and removed when the files match;
  `git fsck --strict` clean; plain `git status` unaffected by it.
- [ ] The new warning wording (both counts) is snapshot-tested.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

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

## Handoff log

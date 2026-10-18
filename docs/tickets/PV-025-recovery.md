# PV-025: crash recovery on open, and fault-injection tests

- **Status:** In progress
- **Issue:** #43
- **Branch:** `feat/PV-025-recovery`
- **Depends on:** PV-023/024 (merged). Runs in parallel with PV-027 (gc).
- **Touches:** `crates/paper-core/src/project/{mod,checkout,export,undo,save,init}.rs` (the open path,
  recovery, and debug-only crash points), `crates/paper-core/src/project/recover.rs` (new),
  `crates/paper-core/src/fault.rs` (new, debug-only), `crates/paper-core/src/lib.rs`,
  `crates/paper-core/src/error.rs` (additions only), `crates/paper-core/tests/recover.rs` (new),
  `crates/paper-cli/tests/crash.rs` (new), `docs/specs/03-architecture.md` (§ Command lifecycle,
  § Crash safety)
- **Must not touch:** `crates/paper-core/src/activity/`, `crates/paper-core/src/project/activity.rs`,
  `crates/paper-cli/src/**` (PV-027).
- **Specs:** `03-architecture.md` § Command lifecycle (step 2 and the repair rule written in
  PV-023), § Crash safety; `01-concepts.md` I4, I6

## Goal

Killing paper at any moment (power cut, `kill -9`, a crash) never loses work and never leaves
git and paper disagreeing: the next command repairs what was half done, silently and safely.

## Context

- Command order (PV-022/023): snapshot → `checkout-pending` marker (undo family only) → keep-alive →
  op (head moves) → export (index, refs, HEAD, `exported.json`) → checkout → stat cache → marker
  removed.
- The exact repair rule is already written in 03-architecture § Command lifecycle ("Repair
  (PV-025), on open …"): head == `from` → drop the marker; head.parent == `from` and head's draft
  tree == `tree` → finish, rewriting a file only if its content still equals its version in one of
  the two trees; otherwise drop.
- `PendingCheckout` (`checkout.rs`) reads and writes the marker; `export` already follows
  "leave alone what plain git moved".
- Two checks were deferred here: the marker exists during the checkout (PV-023), and the index is
  written before the refs and HEAD (PV-022). Notes on #43.

## Decisions (coordinator, 2026-10-18)

- **Recovery runs in `Project::open_with`**, under the lock, after the migration and the head read,
  before anything else the command does: (1) repair a `checkout-pending` exactly as the spec rule
  says; (2) if the refs or HEAD paper should have written differ from the head op's view as
  recorded in `exported.json` (or `exported.json` lags the head op), run the export (normal
  leave-alone rules). Both steps are idempotent. Recovery records no activity entry and prints
  nothing (paper-core never prints).
- **Finishing a checkout** reuses the shared checkout, with the extra rule that a file is rewritten
  or deleted only if its current content equals its version in `from`'s draft tree or in `tree`;
  other files are left alone (as skipped). Then refresh the stat cache and remove the marker.
- **Fault injection:** a debug-only hook, `fault::crash_point("<name>")`, compiled out of release
  builds (`#[cfg(debug_assertions)]`), that aborts the process (`std::process::abort`) when the
  environment variable `PAPER_DEBUG_CRASH_AT` equals the name. Points: `after-snapshot`,
  `after-marker`, `after-keep`, `after-op`, `after-index`, `after-refs`, `after-export`,
  `mid-checkout` (after the first file), `after-checkout`. Same pattern as `PAPER_DEBUG_PANIC`.
- **Tests drive the real binary** (`paper save` / `restore` / `undo` / `redo` with
  `PAPER_DEBUG_CRASH_AT`), then run the next command and check the repaired state.

## Scope

1. `recover.rs`: marker repair and re-export on open.
2. `fault.rs` and the crash points in save, restore, undo/redo and export.
3. Tests for every crash point with `save`, `restore`, `undo` and `redo`, including the two
   deferred checks.
4. Specs: § Command lifecycle (recovery runs in open; the crash points exist in debug builds),
   § Crash safety (now true and tested; what "repaired" means for each point).

## Out of scope

- gc (PV-027); a `paper doctor` command; reporting repairs to the user (paper-core can't print;
  the CLI may add a note later).

## Acceptance criteria

- [ ] For every crash point during `paper save`, the next `paper status` shows a consistent project: refs, HEAD, index and `exported.json` match the head op's view (or the old state, if the crash came before the op), no work is lost, and `git fsck --strict` is clean.
- [ ] For every crash point during `paper undo` and `paper redo`, the next command leaves the files, the draft and the refs consistent with the head op (the undo either happened completely or not at all), and the marker is gone.
- [ ] A crash at `mid-checkout` leaves `checkout-pending` on disk (the deferred PV-023 check); the next command finishes the checkout.
- [ ] A crash at `after-index` leaves the new index with the old refs; the next command's export completes the refs and HEAD (the deferred PV-022 ordering check).
- [ ] An edit made after a crash and before the next command is never overwritten by the repair: it ends up as an edits entry.
- [ ] A stale marker whose op never committed is dropped, and the working files are untouched.
- [ ] Release builds ignore `PAPER_DEBUG_CRASH_AT` (the hook is compiled out).
- [ ] Recovery records no activity entry; an open with nothing to repair does no extra writes.
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-cli/tests/crash.rs`: drive the debug `paper` binary with `PAPER_DEBUG_CRASH_AT`,
  expect an abnormal exit, then run `paper status` (or the next command) and check files (bytes),
  `git rev-parse`, `symbolic-ref HEAD`, `ls-files -s`, `fsck --strict`, `exported.json`, and
  `paper activity`. Reuse `tests/common/mod.rs`.
- `crates/paper-core/tests/recover.rs`: the repair rule on hand-made marker states (all three
  branches, and the "edited after the crash" case).
- Aborting a process on Windows: `std::process::abort` exits with a nonzero status; assert
  "not success" rather than a specific code.

## Summary

## Handoff log

### RED · test-author · 2026-10-18

Tests (run with an empty `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM=1`, `INSTA_UPDATE=no`):

- `crates/paper-core/tests/recover.rs` (12 tests, 10 fail RED, 2 pass today): the repair rule on
  hand-made marker states (head == from dropped; parent == from and tree matches finished, also
  with the first file already rewritten; unrelated `from` dropped; tree mismatch dropped), the
  edited-after-the-crash cases (edited file and would-be-deleted file kept, then recorded as an
  edits entry), no activity entry from recovery, re-export when refs or `exported.json` lag the
  head op, idempotence. Pass today by design: "nothing to repair writes nothing" and "recovery
  records no entry" (they guard the invariant; they will fail if recovery misbehaves).
- `crates/paper-cli/tests/crash.rs` (40 tests, 33 fail RED, 7 pass today): save x6 points,
  restore x8, undo x9, redo x9, edit-after-crash x1; plus a release-only test (compiled out in
  debug) that the variable is ignored. Crash tests fail because the process does not abort
  ("should abort at <point>"). Deferred checks live in the same scenarios: marker exists after
  every undo-family point from `after-marker`, exactly one file rewritten at `mid-checkout`;
  index ahead of refs at `after-index` (save).
- Helper sanity tests (pass on main today): runner ignores an unknown point, `crash` demands a
  non-success exit, `assert_consistent` accepts healthy projects and rejects a lagging ref, a
  wrong index and a left-over marker, state readers describe a project.
- `crates/paper-core/src/fault.rs` unit test (passes today).

Design choices the implementer should know:
- Setup for undo/redo is "three saves, edit intro.tex and related.tex, `paper status`" so the
  undo of the edits entry rewrites two files (a save's undo changes no files). Save tests run
  `status` first, so crashes hit the save's own op, not an edits entry.
- Restore has no marker; its tests assert consistency, nothing lost (the unsaved edit stays
  reachable from refs), earlier activity kept as a suffix, and settling (a second `status`
  records nothing). After `mid-checkout`/`after-checkout`/`after-keep` the half-restored files
  become an `edited ...` entry on the next command; from `after-op` on the `restore ...` entry
  is the newest.
- `exported.json`'s keep ref is not compared (after-keep moves it without an op).

Stubs added (all no-ops, no logic): `paper_core::fault::crash_point(&str)`;
`project/recover.rs::recover(&GitRepo, &Path, &OpId)` called from `open_with` after the head
read; the nine `crash_point` calls placed in `record_at` (after-keep, after-op), `export`
(after-index, after-refs, after-export), `save`/`restore`/`undo` (after-snapshot), `undo`
(after-marker, after-checkout), `restore` (after-checkout) and `apply_checkout` (mid-checkout,
after the first deletion or write). `recover` carries an `allow(clippy::unnecessary_wraps)`
to remove when it gets a body.

Questions: none.

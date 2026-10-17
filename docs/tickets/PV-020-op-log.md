# PV-020: operation log and head pointer (on-disk format 2)

- **Status:** In progress
- **Issue:** #38
- **Branch:** `feat/PV-020-op-log`
- **Depends on:** PV-019 (lock, merged). Runs alone (PV-021 to PV-024 build on it).
- **Touches:** `crates/paper-core/src/activity/` (new: ops, head, store), `crates/paper-core/src/lib.rs`,
  `crates/paper-core/src/view.rs`, `crates/paper-core/src/project/{mod,init,save,restore}.rs`
  (only where the view is read or written), `crates/paper-core/src/fs_util.rs`,
  `crates/paper-core/src/error.rs`, `crates/paper-core/Cargo.toml` (only if a dependency is
  needed), `crates/paper-core/tests/activity.rs` (new), `crates/paper-core/tests/{save,restore}.rs`
  (only the helpers that read `view.json`), `docs/specs/03-architecture.md` (§ On-disk layout,
  § Undo / redo algorithm)
- **Specs:** `03-architecture.md` § On-disk layout, § Command lifecycle (step 6), § Undo / redo
  algorithm; `01-concepts.md` I4; ADR-0002

## Goal

paper keeps every state of the project as an immutable operation, and the current state is the
operation `activity/head` points at. This is the record undo (PV-023) walks; nothing is undoable
yet, but nothing is overwritten any more either.

## Context

- Today the whole state is the `View` (`view.rs`: current line, each line's tip and draft),
  written to `.git/paper/view.json` from three places: `init` (`init.rs`), the snapshot step and
  `save` (`save.rs`). `Project::open` reads it (`project/mod.rs`).
- `FORMAT_VERSION = 1` in `project/mod.rs`; `.git/paper/version` is written last by `init`.
- `fs_util::write_atomic` writes a temp file, fsyncs it and renames it, but doesn't fsync the
  directory.
- The project lock (PV-019) is held for the whole life of a `Project`, so op writes are already
  serialised.
- Issue #38 carries a note from the PV-019 review: a first `init` (adoption) reads `paper.toml`
  before it locks, so a concurrent `config set --project` can be overwritten.

## Decisions (maintainer and coordinator, 2026-10-17)

- **Op file:** `.git/paper/activity/ops/<id>.json`, immutable, written once (temp + fsync + rename +
  directory fsync on Unix). Fields: `id`, `parent` (`null` for the first op), `kind`
  (`init`, `migrate`, `snapshot`, `save`, `restore`; more kinds later), `time` (RFC 3339),
  `view` (the full `View`), `position` (the op whose state is current; for a normal op, itself),
  `redo` (the redo stack, a list of op ids; empty for normal ops), and a short `description`. The
  undo fields are written now so the format doesn't change in PV-023. Ids: 32 lowercase hex
  characters of randomness (the same source as change IDs), so concurrent clones never collide.
- **Head:** `.git/paper/activity/head` holds one op id, replaced atomically (temp + fsync + rename).
  The head op's `view` is the project's view. The head is written after the op file, so a crash
  leaves either the old head or a complete new op.
- **One op per view write for now:** every place that wrote `view.json` appends an op instead (a
  snapshot inside `save` may give two ops). Folding to exactly one op per command, and
  `paper activity`, are PV-021.
- **Format 2 and migration (maintainer):** `FORMAT_VERSION = 2`. `init` writes format 2 with an
  `init` op. Opening a format-1 project (v0.1.0) under the lock migrates it: a `migrate` op from
  `view.json` (parent `null`), then `head`, then `version` = 2, then `view.json` renamed to
  `view.json.v1` (removed by `gc` in PV-027). Each step is safe to repeat, so a crash during
  migration is finished by the next open. Nothing in git changes. A format above 2 is
  `UnsupportedFormat`, as today.
- **No `view.json` writes** remain after this ticket; `View::save` goes away (`load` stays only
  for the migration).
- **Init/config race (note on #38):** after taking the lock, adoption re-reads `paper.toml` (re-runs
  `ProfilePlan::prepare`) before writing, so a concurrent `config set --project` isn't lost.

## Scope

1. `activity` module: `OpId`, `Op`, `OpKind`, `ActivityLog::{open, head, read, append}` (append =
   write op file, then move head), with errors in the style of `ViewError`.
2. `Project` reads its view from the head op; `init`, snapshot, `save` and `restore` append ops.
3. Format 2, migration from format 1, `version` handling.
4. `fs_util`: directory fsync after rename (no-op where unsupported, e.g. Windows).
5. Adoption re-reads `paper.toml` under the lock.
6. Specs: 03-architecture § On-disk layout (op fields, ids, head, format 2, migration), § Undo /
   redo algorithm (field names match the op format).

## Out of scope

- Exactly one op per command, snapshot folding, op descriptions for the CLI, `paper activity`
  (PV-021); `exported.json` and the keep-alive chain (PV-022); undo/redo (PV-023/024); crash
  recovery of half-done commands (PV-025); gc (PV-027).

## Acceptance criteria

- [ ] `init` (new and adopted) creates `activity/ops/<id>.json` (an `init` op, parent `null`) and `activity/head`; no `view.json`; `version` is 2.
- [ ] `save` and `restore` append ops whose `parent` chain leads back to the first op; the head op's view equals `Project::view()`, and reopening the project gives the same view.
- [ ] Op files are never rewritten: an op's bytes are unchanged after later commands.
- [ ] A format-1 project (built in the test by writing v0.1's `view.json` + `version` 1) opens, migrates (one `migrate` op, `view.json.v1`, version 2), and keeps its tips and drafts; git refs, `git log`, `git status` and `git fsck --strict` are unchanged.
- [ ] Migration interrupted at each step (simulated by leaving the files as each step would) completes on the next open.
- [ ] A format-3 project is `UnsupportedFormat`; a missing or damaged head or op file is a clear project error (rule 6), not a panic.
- [ ] Adoption with a concurrent change to `paper.toml` between the first read and the lock keeps that change (or, if that can't be tested deterministically, a unit test of the re-read).
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- `crates/paper-core/tests/activity.rs` with real temp repos: op chain after init/save/restore,
  immutability (hash op files before and after), head = view, reopen, migration and its interrupted
  states, unsupported format, damaged head/op.
- Existing tests in `tests/save.rs` and `tests/restore.rs` read `view.json` through helpers: change
  only those helpers to read the head op's view (authorised), keeping every assertion.
- Cross-check with git (`fsck --strict`, `log`, `status`, refs) after migration.

## Summary

## Handoff log

### RED · test-author · 2026-10-17

**New tests (31):** `crates/paper-core/tests/activity.rs` (29 tests, 28 fail, 1 passes by design:
`plain_git_still_agrees_after_a_session_of_commands`, a regression guard) and 2 unit tests in
`crates/paper-core/src/project/init_profile.rs` (`reprepare_*`, both fail).
Existing tests now failing for the intended reason (13): `tests/save.rs` 10, `tests/restore.rs` 2,
`tests/init.rs` 1. Everything else passes (paper-cli included).

**How each group fails on main**
- Project-level tests (init/save/restore/migration/damage): `.git/paper/activity/head` does not
  exist (init still writes `view.json`), so the `head_id` helper's `unwrap` panics; `format_version_is_2`
  fails on the assertion (`FORMAT_VERSION` is deliberately NOT bumped in the stubs).
- `ActivityLog` / `OpId` tests and `reprepare` tests: `todo!()` panics in the stubs.
- Damage tests call `head_id` first (to prove the project really is format 2), so they cannot pass
  by accident through `UnsupportedFormat`.

**Stubs added (all `todo!()`)**: `crates/paper-core/src/activity/mod.rs` (`OpId` with `FromStr`,
`Display`, `random`; `OpKind`; `Op` (derive serde, fields as in the ticket, `time` is a `String`);
`ActivityError {Io, Corrupt, MissingOp}`; `ActivityLog::{open, head, read, append}`);
`ProfilePlan::reprepare` in `init_profile.rs`; `Activity(#[from] ActivityError)` variants on
`OpenError`, `InitError`, `SaveError`, mapped to project errors in `error.rs` (plain plumbing, same
arms as `View(_)`). Two `#[allow(dead_code)]` (field `root`, `reprepare`): drop them in GREEN.

**Changes to existing tests (all listed)**
- `tests/save.rs`: new helper `head_view_json`; `stored()` reads `view_json` through it instead of
  `view.json`. No assertion changed.
- `tests/restore.rs`: new helper `head_view_json`; `stored()` uses it. In
  `restore_never_touches_symlinks_and_reports_symlinks_in_the_from_tree` (a test body, not a helper)
  the rewrite of `view.json` became "read the head view, replace the tip, `ActivityLog::append` an
  `OpKind::Snapshot` op with the edited view"; same assertions.
- `tests/init.rs`: `init_in_an_empty_folder_...` asserted `version` == `"1\n"`; now `"2\n"` (the
  binding format change).

**Decisions for GREEN worth knowing**
- Migration tests do not constrain the op count after an interrupted migration (a repeated step may
  leave an orphan op), only that the head chain's root is a `migrate` op with parent `null`, the view
  is the original, `view.json` is gone and `view.json.v1` exists. The clean path must give exactly
  one op.
- Op/head damage tests only require a project error with all three parts (rule 6), via `PaperError`.
- Adoption re-read: not deterministically testable end to end, so unit tests call
  `ProfilePlan::reprepare(workdir, choice)` (to be called after taking the lock in `adopt` and the
  fresh-init path). GREEN may restructure but must keep equivalent unit tests.
- The existing `an_interrupted_init_can_be_run_again` (version removed, then `init` again) must keep
  passing with ops/head already on disk.
- `View::save` still exists (its unit tests are untouched); GREEN removes it together with its tests.

**Checks:** ran `activity` tests with `GIT_CONFIG_GLOBAL` pointing to an empty file and
`GIT_CONFIG_NOSYSTEM=1`: same results (no dependence on a global identity). No Unix-only APIs, no
`/tmp` paths in the new tests. fmt and clippy `-D warnings` clean. Questions: none.

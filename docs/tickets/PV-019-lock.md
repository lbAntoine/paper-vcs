# PV-019: project lock with a 5 s wait

- **Status:** In progress
- **Issue:** #37
- **Branch:** `feat/PV-019-lock`
- **Depends on:** none. Runs in parallel with PV-017 (docs only; it edits other sections of
  `02-cli.md` and `03-architecture.md`).
- **Touches:** `crates/paper-core/src/project/mod.rs`, `crates/paper-core/src/project/init.rs`,
  `crates/paper-core/src/project/lock.rs` (new), `crates/paper-core/src/error.rs`,
  `crates/paper-core/tests/lock.rs` (new), `crates/paper-cli/src/**` (only to pass the wait
  through and render the error), `crates/paper-cli/tests/lock.rs` (new),
  `docs/specs/03-architecture.md` (§ Crates table row for paper-core, § On-disk layout, § Command
  lifecycle only), `docs/specs/02-cli.md` (§ Global behaviour only)
- **Must not touch:** the sections PV-017 edits: 02-cli § `paper init`, § `paper save`,
  § `paper restore`, § Escape hatches; 03-architecture § Git interop; `README.md`.
- **Specs:** `03-architecture.md` § Command lifecycle (steps 1 and 9), § On-disk layout (`lock`);
  `02-cli.md` § Global behaviour (exit code 3: "lock held too long"); CLAUDE.md rule 6

## Goal

Two paper commands never run at the same time in one project: the second waits up to 5 s for the
first, then stops with a clear error. This is the first piece of the v0.2 command lifecycle; the
operation log (PV-020) relies on it.

## Context

- `Project::open` (`project/mod.rs`) and `Project::init`/`init_with_profile` (`project/init.rs`)
  are the only ways to get a `Project`. The CLI opens one project per process, runs one command,
  and exits.
- `.git/paper/` is created by `init`; `version` is written last so an interrupted init can rerun.
- `PAPER_DEBUG_PANIC` (`crates/paper-cli/src/panic.rs`) is the precedent for debug-only test hooks.

## Decisions (maintainer and coordinator, 2026-10-17)

- **Mechanism (maintainer):** std `File::lock` / `File::try_lock` (stable since Rust 1.89; the
  toolchain is 1.97), not fd-lock. Exclusive lock on `.git/paper/lock`, created if missing. The
  file's content doesn't matter; the OS lock is the lock, so a crashed process never leaves a stale
  lock (the OS releases it). Write the holder's pid into the file after locking, as a hint for the
  error message only.
- **Scope of holding:** a `Project` holds the lock for its whole lifetime: acquired in `open` and
  `init` (after `.git/paper/` exists), released when the `Project` is dropped. One CLI command =
  one `Project` = lifecycle steps 1 to 9. Every command that opens a project takes it, read-only
  ones included (they snapshot). Commands that don't open a project (`config` on user keys,
  `--help`, `--version`) don't.
- **Wait:** poll `try_lock` with a short sleep (e.g. 50 ms) until 5 s have passed. The wait is a
  parameter in paper-core (a `LockWait`/`Duration` in an options value, default 5 s), so tests can
  shorten it; the CLI uses the default, and in debug builds honours `PAPER_DEBUG_LOCK_WAIT_MS` so
  CLI tests don't sleep 5 s. Compiled out of release builds.
- **Error:** `OpenError::Busy` (and the init equivalent), a three-part `PaperError` of kind `Project`
  (exit 3): what ("Another paper command is running in this project"), why (one command at a time
  keeps the history consistent; it waited 5 s), next (wait for it to finish and run the command
  again; mention the pid from the lock file if readable). No "delete the lock file" advice: it is
  never needed with OS locks.
- **Not in this ticket:** the rest of the lifecycle (recover, import, op log, export, checkout).

## Scope

1. `project/lock.rs`: `ProjectLock::acquire(paper_dir, wait) -> Result<ProjectLock, LockError>`,
   holding the `File`; released on drop.
2. `Project` holds a `ProjectLock`; `open` and `init*` acquire it; a new options entry for the wait.
3. `PaperError` conversion and CLI rendering; the debug-only `PAPER_DEBUG_LOCK_WAIT_MS`.
4. Specs: 03-architecture (drop fd-lock from the paper-core dependency list and the `lock` line;
   describe std file locking, the pid hint, lock lifetime = `Project` lifetime), 02-cli § Global
   behaviour (exit 3 wording for a busy project).

## Acceptance criteria

- [ ] While one `Project` is open, opening the same project again fails with `Busy` after the configured wait, and succeeds once the first is dropped (core test, short wait).
- [ ] `init` on a project another process holds is also refused with `Busy`.
- [ ] The lock is released when the holder exits or crashes: a child process that takes the lock and is killed leaves a project the next command can open at once.
- [ ] The second command waits: a holder that releases after ~200 ms lets a waiting `open` succeed within the wait.
- [ ] CLI: with the project busy, `paper status` exits 3 with a three-part error (human and `--json`) that names the pid when known; `PAPER_DEBUG_LOCK_WAIT_MS` shortens the wait in tests and is absent from release builds.
- [ ] Commands that don't open a project (`paper --version`, `paper config get user.name` outside a project) don't take the lock.
- [ ] Works on Linux, macOS and Windows (CI).
- [ ] Specs updated (only the listed sections).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass.

## Test plan

- `crates/paper-core/tests/lock.rs`: two `Project`s in one test process (OS file locks conflict
  between separate open handles on Unix and Windows), a thread that holds then releases, and a
  child process (`std::process::Command` running the test binary or `paper`) that holds the lock
  and is killed.
- `crates/paper-cli/tests/lock.rs`: hold the lock from the test process (open the project through
  paper-core or lock the file directly), run `paper status` with `PAPER_DEBUG_LOCK_WAIT_MS=200`,
  assert exit 3 and the error shape. Reuse `tests/common/mod.rs`.
- No timing assertions beyond "it eventually succeeds / fails"; keep waits short.

## Summary

## Handoff log

### RED · test-author · 2026-10-17

**Tests** (all pass nothing today; pre-existing suite fully green, also with an empty `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_NOSYSTEM=1`):

- `crates/paper-core/tests/lock.rs`: 10 tests + 1 helper (`lock_holder_helper`, the child-process body, a no-op unless `PAPER_TEST_LOCK_HOLDER` is set). 10 fail: default wait is 5 s (`Default` is `todo!()`); busy-after-wait then free after drop; Busy names the holder pid; `PaperError` is kind Project, message "Another paper command is running in this project", pid in message/why/next, no "delete" advice; `init_with` on a held project is `InitError::Busy` (not `AlreadyInitialized`, so the lock is taken before that check); init's busy conversion; init's returned project keeps the lock; a waiting open succeeds after a 100 ms holder (asserts the holder released first, no timing bound); killed child process leaves no stale lock (asserts the child holds it while alive, then kill, then open succeeds); lock file exists inside `.git` and `fsck --strict`/`log`/`status` stay clean. Fail today because `open` does not lock (no `Busy`), the conversions are `todo!()`, and no lock file is created.
- `crates/paper-cli/tests/lock.rs`: 5 tests. 3 fail (`status` human and `--json` on a busy project want exit 3 + three-part error naming the test process's pid; usable again after drop: first run must be 3); today `status` exits 0. 2 are regression guards that pass today and must stay green (`--version` and `config get user.name` do not take the lock).

**Not testable here:** that `PAPER_DEBUG_LOCK_WAIT_MS` is absent from release builds (rust-engineer: use `#[cfg(debug_assertions)]`, like `PAPER_DEBUG_PANIC`).

**Stubs / API added (for GREEN to fill in):**
- `project/lock.rs`: `LockOptions { wait }` (`Default` = `todo!()`), `DEFAULT_LOCK_WAIT`, `LockError { Busy { holder }, Io }`, `ProjectLock::acquire` (`todo!()`, not called by existing code), re-exported from `paper_core::project`.
- `Project::open_with(path, identity, &LockOptions)`; `open` delegates with `DEFAULT_LOCK_WAIT`. `Project::init_with(path, identity, &SnapshotOptions, Option<&Profile>, &LockOptions)`; `init_with_profile` delegates. Both currently ignore the lock.
- `OpenError::Busy { holder: Option<u32> }`, `InitError::Busy { holder }`; their `PaperError` arms are `todo!()`.

**Notes for GREEN:** `Env::project_with_three_saves()` returns a `Project`; once `Project` holds the lock, any existing CLI test that keeps that value alive while running `paper` will see Busy and needs the value dropped (that is a test fix, report it if it happens). `tempfile`'s `dunce` is not a core dependency, so core tests use plain temp paths.

**Questions:** none.

### GREEN · rust-engineer · 2026-10-17

**Implemented.** `ProjectLock::acquire` (std `File::try_lock` polled every 50 ms up to the wait; pid written after locking; pid read back for `Busy`). `Project` holds the lock in a last-declared field. `open_with` takes it once `.git/paper/` exists (a plain git repo with no `.git/paper/` is `NotAPaperProject` without creating a lock file). `init_with` takes it right after creating `.git/paper/`, before the `AlreadyInitialized` check. `PaperError::busy` (kind Project, exit 3, no "delete" advice). CLI: `lock_options()` in `commands.rs`, with `PAPER_DEBUG_LOCK_WAIT_MS` under `#[cfg(debug_assertions)]`.

**Deviation to note.** Adopting a repo creates `.git/paper/` before it can lock; when adopt then fails before writing anything (e.g. conflicting profile), the lock file and the empty folder are removed again, because `adopting_with_a_conflicting_profile_is_a_user_error_and_writes_nothing` asserts `.git/paper` is absent. A process queued on that lock file in that failure window could end up holding an unlinked file; very narrow, noted for the op-log work.

**Specs.** 03-architecture: paper-core row (fd-lock dropped), on-disk `lock` line, Command lifecycle intro. 02-cli: exit code 3 row. No other sections.

**Pre-existing tests touched** (all only drop the first `Project` before a second `Project::open`, assertions unchanged; comparisons use a cloned `View` taken before the drop): `paper-core/tests/init.rs::open_reads_back_the_project_init_created`, `tests/restore.rs::restore_never_touches_symlinks_and_reports_symlinks_in_the_from_tree`, `tests/save.rs::{snapshot_persists_the_new_draft_in_the_ref_and_the_view, opening_the_project_after_a_save_sees_the_new_tip_and_draft, a_reopened_project_can_save_again}`. No CLI test or helper needed changes. The `fd-lock` mention in the workspace `Cargo.toml` comment is left (not a dependency).

**Checks.** fmt, clippy `-D warnings`, full suite green with `INSTA_UPDATE=no`; the new lock tests also pass with `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1`.

### Review · reviewer · 2026-10-17
- Verdict: CHANGES. Tests, clippy and fmt pass on macOS. Polling, the debug-only wait override, read-only commands locking, the crash test and the five adapted tests (drop only) all checked.
- BLOCKER: on Windows the lock is mandatory, so the pid can't be read from the locked file by anyone else; `holder` is always `None` and four tests assert the pid unconditionally (from std/Win32 semantics; Windows not run locally).
- MAJOR: `adopt` unlinks `.git/paper/lock` on failure, which breaks mutual exclusion (a waiter can end up locking an unlinked file while a third process locks a new one); it also runs after writes, contrary to its comment.
- MINOR: `why` hardcodes "5 seconds" though the wait is a parameter; `next` suggests `paper status`, which blocks the same way, and the pid isn't in `next`.
- MINOR: the spec should state the Windows pid behaviour; project-level `config set` writes `paper.toml` without the lock.
- MINOR: the workspace `Cargo.toml` comment still plans fd-lock.
- NITs: `LockOptions::default()`; missing `# Panics` on `ProjectLock::acquire` and `Project::open_with`.
- Coordinator decisions:
  - The pid hint goes to a sidecar `.git/paper/lock-holder` that is never locked (written after taking the lock, readable on every OS); update the on-disk layout.
  - Adoption runs every read-only check before creating `.git/paper/`; when `.git/paper/` already exists, lock first (Busy before AlreadyInitialized); re-check `version` under the lock; the lock file is never unlinked (record it as an invariant in the spec). Existing tests unchanged.
  - Project-level `config set` takes the project lock.
  - Fix the minors and NITs.
  - Adding tests is authorised; don't weaken any.

### GREEN (review round 1) · rust-engineer · 2026-10-17

- Windows: the pid hint moved to an unlocked sidecar `.git/paper/lock-holder` (written best-effort after locking, removed on drop, read on `Busy`; missing or stale means unknown). The `lock` file is never read, written or deleted. On Windows the sidecar is opened by std with full share modes, so reading it while another process writes or removes it is fine; not run locally.
- Adopt: no more cleanup or unlinking. An existing `.git/paper/` is locked first (Busy before AlreadyInitialized); otherwise the head, branch and `ProfilePlan::prepare` checks run before the folder is created, then lock and re-check `version`. Tests added: lock file survives a refused init; a refused adoption creates no `.git/paper`.
- `config set --project` takes the lock (`ProjectLock::acquire_for_workdir`, no lock when there is no `.git/paper/`), same wait and Busy error. CLI test added.
- Busy wording: `why` no longer says "5 seconds"; `next` is "run the same command again" with the pid in its explanation. The `Busy` variants did not gain a wait field (existing tests match `Busy { holder }` without `..`).
- fd-lock dropped from the Cargo.toml comment; `LockOptions::default()` used; `# Panics` added. Spec updated (on-disk layout, lifecycle: never-deleted invariant, config set).

### GREEN (post-approval) · rust-engineer · 2026-10-17

Spec wording for the busy error and `lock-holder`; fresh init re-checks `version` under the lock (test added); `acquire_for_workdir` resolves the paper dir through `GitRepo`.

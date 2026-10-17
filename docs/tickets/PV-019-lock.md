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

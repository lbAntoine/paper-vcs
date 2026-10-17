# 03 — Architecture

## Overview

```
┌────────────┐  ┌──────────────────┐
│ paper-cli  │  │ other front-ends │
│ clap, TTY, │  │ (editors, apps,  │
│ render     │  │  services)       │
└─────┬──────┘  └────────┬─────────┘
      └──────────────────┤
                         ▼
               ┌───────────────────┐
               │    paper-core     │  domain model, commands, activity log,
               │  (library, pub)   │  snapshot, interop, config — NO printing
               └───┬───────────┬───┘
                   ▼           ▼
          ┌─────────────┐  ┌────────────┐
          │  paper-git  │  │ paper-diff │  word/line tokenizers + diff,
          │ gix + `git` │  │ (no git)   │  pure functions
          └─────────────┘  └────────────┘
```

`paper-cli` also depends on `paper-diff` directly, for the diff types it renders (`FileDiff`,
`to_unified`) that `paper-core` returns inside its results; it never reads git through it.

A single Cargo workspace in one repo (`paper-vcs`). Edition 2024, pinned via `rust-toolchain.toml`.

| Crate | Responsibility | Key deps |
|---|---|---|
| `paper-core` | Public API used by every front-end. `Project::open/init`, one method per command, each returning a serialisable `Outcome`. Owns the invariants from [01-concepts](01-concepts.md#invariants) | `serde`, `thiserror`, `fd-lock`, `toml`, `tracing` |
| `paper-git` | `GitRepo`, the gix-backed repository handle: objects, trees, refs, merge, checkout, status. Network ops (`fetch`, `push`, `clone`) shell out to the `git` binary | `gix`, `std::process` |
| `paper-diff` | Tokenise text (generic, LaTeX, Markdown, CJK-aware), word and line diffs, table diffs, binary detection. The `office` module extracts text and cells from Office files, and checks they're complete zips for snapshot | `similar` (patience algorithm), `unicode-segmentation`, `csv`, `zip`, `quick-xml`, `calamine` |
| `paper-cli` | Argument parsing, prompts, human and `--json` rendering, exit codes | `clap`, `anstream`, `insta` (tests) |

**Rule:** `paper-core` never prints or prompts. When a command needs input (for example a save
message), it returns `Outcome::NeedsInput`, and the front-end asks and calls again. This is
what lets the GUI reuse the core unchanged.

**Why gix plus the `git` binary?** See [ADR-0001](adr/0001-own-engine-on-gitoxide.md).
Before starting each milestone, check which gix features are ready (tree merge, worktree
checkout, status, index writing). Anything missing falls back to `git` subprocess calls
behind `GitRepo`'s methods (a `Store` trait can be extracted if a second backend is ever needed), and the gap is written down in the ADR.

## Domain model

All state that `undo` must cover lives in one immutable **View**:

```rust
struct View {
    current_line: LineName,
    lines: BTreeMap<LineName, Line>,
    milestones: BTreeMap<MilestoneName, Milestone>,
    remotes_seen: BTreeMap<RemoteRef, CommitId>,   // last fetched remote tips
}

struct Line {
    tip: CommitId,              // last save
    draft: CommitId,            // draft commit (parent = tip, or [tip, other] mid-keep/sync)
    kind: LineKind,             // Main | Try { origin: LineName, base: CommitId } | Named
    shared: bool,
    pending: Option<Pending>,   // Keep { from } | SyncMerge { remote } — conflict resolution in progress
}

struct Pending {
    kind: PendingKind,                              // Keep { from } | SyncMerge { remote }
    conflicts: BTreeMap<RepoPath, ConflictKind>,    // Markers | TheirsCopy { copy: RepoPath, theirs: BlobId }
}

struct Milestone { target: CommitId, description: String, venue: Option<String>, created: Timestamp }

struct Operation {
    id: OpId,                   // content hash
    parent: Option<OpId>,
    kind: OpKind,               // Save | Restore | Try | Keep | Undo{undone} | Redo{redone} | Import | Snapshot | ...
    command: String,            // argv as typed, for `paper activity`
    time: Timestamp, user: Identity,
    view: View,
    position: OpId,             // logical "current" op for undo/redo
    redo_stack: Vec<OpId>,
}
```

Commands are implemented as **pure transitions** `fn(&View, &Inputs) -> Result<(View, Effects)>`.
`Effects` are the side effects (write objects, update refs, check out a tree), and a single
executor applies them. The transitions are unit-tested without touching disk, and this fits
the immutability-first style.

## On-disk layout

```
<project>/
  .git/                         normal git repo (colocated)
    paper/
      version                   on-disk format version (integer; migrations keyed on it)
      lock                      advisory lock (fd-lock)
      activity/ops/<opid>.json  one file per operation (immutable, fsynced)
      activity/head             current op id (replaced atomically via rename)
      exported.json             git refs as paper last wrote them
      wc-state.json             stat cache for the checked-out draft (path → size,mtime,inode,blob)
      checkout-pending          exists only while the working files are being rewritten
  paper.toml                    project config (committed; shared with co-authors)
  .gitignore                    profile ignores merged in
```

Refs owned by paper live under `refs/paper/` and are never pushed:

| Ref | Purpose |
|---|---|
| `refs/paper/draft/<line>` | Draft commit of each line |
| `refs/paper/keep` | Keep-alive chain (see below) |

**Keep-alive chain.** Undo needs old commits, but `git gc` prunes anything unreachable.
Every operation that creates commits appends one synthetic commit to `refs/paper/keep`.
Its parents are the previous keep commit plus all new commit IDs (an octopus merge). This
uses one ref instead of thousands. `paper gc [--older-than 90d]` rebuilds the chain from
the operations inside the window and deletes older ops. Nothing is garbage-collected
automatically before v1.0.

**Change IDs** are 16 random bytes stored as the extra commit header `change-id`, and
displayed in `k`–`z` letters with the shortest unique prefix
([ADR-0003](adr/0003-change-ids-in-commit-headers.md)). If a commit has no header
(because it was made by plain git), its change ID is derived from the commit hash and
marked "unstable".

## Command lifecycle

Every command runs this pipeline while holding the lock. The lock waits up to 5 s, then fails with exit 3.

```
1. lock
2. recover      if checkout-pending exists → finish that checkout
                if head op's view ≠ exported.json → re-export (crashed after step 6)
3. import       if actual git refs/HEAD ≠ exported.json → someone used git: record Import op
4. snapshot     scan working files vs wc-state → new draft tree; amend draft commit if changed
                (skip .theirs copies; Office files must pass the completeness check, else keep previous blob)
5. transition   pure function on View → new View + Effects
6. commit op    write objects, write op json (fsync), rename activity/head
7. export       update refs/heads/*, tags, refs/paper/*, git HEAD → current line, git index → tip tree
                write exported.json
8. checkout     if the draft tree changed (undo, switch, restore, keep…): write checkout-pending,
                update working files, refresh wc-state, delete checkout-pending
9. unlock
```

**Snapshot and op folding.** For commands that change state, the snapshot from step 4 is
folded into that command's operation. `undo` after `save` then brings the edits back as the
draft, which is what users expect. For read-only commands (`status`, `diff`, `history`), a
changed snapshot is recorded as its own `Snapshot` op, but only if something changed.

**Crash safety.** The source of truth is `activity/head`. Steps 7 and 8 are idempotent and
are replayed by step 2. Losing power at any point leaves either the old state or the new
state, never a mixture that loses data. The working files are snapshotted before any
checkout, so a half-finished checkout is repaired without losing anything.

### Undo / redo algorithm

Operations are immutable, and undo and redo are themselves operations. So undo is
undoable, and redo can be reconstructed from the log after a crash.

```
normal op N:  parent=head, position=N, redo_stack=[]
undo:         P = head.position; require P.parent
              new op: view = P.parent.view, position = P.parent,
                      redo_stack = [P] + head.redo_stack
redo:         R = head.redo_stack.pop() or "nothing to redo"
              new op: view = R.view, position = R, redo_stack = rest
undo --to X:  new op: view = X.view, position = X, redo_stack = []
```

Undo never moves remote state. If an undone operation had pushed (`sync`), undo restores
the local view and warns: `This was already shared; your co-authors still have it.`

## Git interop

Paper treats git as a peer that may change things between runs. In step 3, it compares the
actual repo with `exported.json`. The table below is the target behaviour. v0.4 brings
the `git checkout` and branch-creation rows (roadmap § v0.4: Safe experiments, Interop);
full coverage of every row is v0.8 (roadmap § v0.8: Hardening).
*Today (v0.1 to v0.3):* paper does not import external changes. When the line moved under
it, `save` refuses and writes nothing, and the error names `git log <line>`; if git is on
another branch, it says to run `git switch <line>`. After a `git pull`, `status` shows the
pulled edits as unsaved changes and `history` doesn't list the pulled saves. Bringing in
co-authors' work arrives with `sync` in v0.6.

| External change (via `git`) | Paper's reaction |
|---|---|
| New commits on a branch (e.g. `git commit`) | Line tip moves. The draft is rebased onto it, and its tree is the current working files anyway, so nothing is lost |
| Branch created / deleted | Line created (kind Named, not shared) / line removed (recoverable via undo) |
| `git checkout other-branch` | Current line switches. Working files are what git left, so they are snapshotted as that line's draft |
| `git checkout <commit>` (detached HEAD) | Commands that change state refuse and suggest `paper switch <line>` or `git switch -`. Read-only commands work |
| `git reset --hard`, `git stash` | The line tip moves as git says. The previous draft stays recoverable through the activity log |
| `git rebase` / `--amend` of saves | New commits lose the `change-id` header, and each gets a new, unstable change ID. The activity log keeps the old state |
| Tags created / deleted | Annotated tags with the paper trailer (`Paper-Milestone: 1`) become milestones. Other tags are ignored |

**Invariant I6 checks** run in integration tests after every command: `git fsck`,
`git status` exits 0, `git log` shows the saves, and HEAD is attached to the current line.

## Configuration

Precedence runs from highest to lowest: CLI flags, then `PAPER_*` environment variables,
then project `paper.toml`, then user `config.toml`, then git config (identity only), then
built-in defaults.

Every key can be overridden by an environment variable: `PAPER_` plus the dotted key
upper-cased with `.` replaced by `_` (`PAPER_USER_NAME`, `PAPER_LARGE_FILES_MAX_MB`). An empty
variable counts as unset. Git config supplies only `user.name` and `user.email`
(`paper_git::configured_identity(path)`, which reads them as git would see them from `path`).
Each key has one or two homes: identity and looks only in the user config, `profile` only in
`paper.toml`, the rest in either (the project's wins). The key table is in
[02-cli](02-cli.md#paper-config-getsetlist---project-since-v01).

The user config is `$XDG_CONFIG_HOME/paper/config.toml`, else `~/.config/paper/config.toml`
on Linux and macOS, and `%APPDATA%\paper\config.toml` on Windows. `paper-core::config`
(`Key`, `ConfigFile`, `Settings`) loads and edits both files with `toml_edit`, so a `set` keeps
comments and layout, and resolves them against an injected environment lookup; it never prints
or prompts. The CLI feeds the result to `Project::with_options` (large-file thresholds) and
uses it for the author and the `--color` default.

```toml
# paper.toml (committed)
profile = "latex"

[large_files]
warn_mb = 10
max_mb  = 100          # GitHub's hard limit; files above are skipped (I7)

[diff]
default = "lines"      # lines (words arrives in v0.3)
```

```toml
# ~/.config/paper/config.toml (per user)
[user]
name  = "Mina Park"
email = "mina@uni.example"

[ui]
color = "auto"         # auto | always | never
```

## Errors

`paper-core` returns `PaperError { kind, message, why, next: Vec<Suggestion> }` with
`Suggestion { command, explanation }`. The CLI renders these in the three-part format from
[02-cli](02-cli.md#global-behaviour), and the GUI renders the same data as a dialog with
buttons. Panics are caught at the CLI boundary, write a report to
`~/.cache/paper/crash-<ts>.txt`, and exit with code 101.

`ErrorKind` decides the exit code: `User` → 1, `NeedsAttention` → 2, `Project` → 3. The three
parts are written in `paper-core/src/error.rs`, one `From<…> for PaperError` per core error
enum (`InitError`, `OpenError`, `SaveError`, `ResolveError`, `HistoryError`, `ShowError`,
`DiffError`, `StatusError`, `RestoreError`, `ParsePointError`), in researcher vocabulary. The
enums' own `Display` text is for logs and tests: one sentence about what happened, naming no
command (commands belong in `next`) and never one that doesn't exist yet. Failures of git or the disk are
`Project` errors whose `why` names the underlying cause and whose `next` is the issue-tracker
URL (in `command`, with the prose in `explanation`). The CLI's own failures use
`PaperError::no_identity()` and `PaperError::not_a_project(path, is_git_repo)`.
`--json` output uses CLI-owned serde types, so `paper-core` has no serde derives for it.

## Testing strategy

| Layer | Tooling | What it proves |
|---|---|---|
| Transitions | plain unit tests | Each command's `View → View` logic, edge cases |
| Invariants | `proptest`: random sequences of edits and commands | I1–I7. In particular, *undo after any command restores the exact view and the exact working files* |
| `GitRepo` | integration tests on temp repos | gix behaviour, keep-alive chain, crash recovery (kill between steps 6, 7 and 8 via fault injection) |
| Git interop | tests driving the real `git` binary | Every row of the interop table |
| Office extraction | A test set of real `.docx`/`.xlsx`/`.xls`/`.ods` files saved by Word, Excel, LibreOffice and Google Docs export, plus truncated (partly written) files | Text and cell extraction are stable, metadata-only re-saves show "no text changes", and truncated files are rejected by the completeness check |
| CLI | `assert_cmd` + `insta` snapshots | Wording, exit codes, `--json` schema stability |
| Cross-platform | GitHub Actions on macOS, Linux and Windows | Paths, line endings (no EOL conversion in v0.x, documented), file locking |

Coverage target: **80%+ on `paper-core` and `paper-diff`**, measured with `cargo-llvm-cov`.

## Performance targets

Reference repo: 5,000 files, 500 MB, 2,000 saves, on a 2022 laptop with an SSD.

| Operation | Target |
|---|---|
| `paper status` with no changes (stat cache hit) | < 100 ms |
| Snapshot after editing 1 file | < 150 ms |
| `save`, `switch`, `undo` | < 300 ms |
| `history -n 50` | < 100 ms |

## Distribution

**Ships now (v0.1.0, a pre-release).** `cargo-dist` (pinned in `dist-workspace.toml`) generates
`.github/workflows/v-release.yml`. A tag matching `v**[0-9]+.[0-9]+.[0-9]+*`
(`tag-namespace = "v"`) builds archives for `x86_64-apple-darwin`, `aarch64-apple-darwin`,
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`, a shell
installer, a PowerShell installer and SHA-256 checksums (no source tarball), and publishes them
as a GitHub Release. Pull requests run `dist plan`. The package is named
`paper-cli`, so the installers are `paper-cli-installer.sh` and `.ps1`; the binary is `paper`.
See [workflow § Releasing](../workflow.md#releasing).

**Later.**

- A Homebrew tap (needs its own repository and token) and a Windows MSI installer.
- Publishing the crates to crates.io (`paper-core` is public so other front-ends can reuse
  it), once the binary name is settled, because crates.io names are permanent.
- A **conda-forge** package ships in v0.6. Many researchers install their tools through conda.
- Code signing and notarisation.
- The `git` binary is required only for network commands (`clone`, `sync`), and a missing
  binary gets a clear error with install instructions.

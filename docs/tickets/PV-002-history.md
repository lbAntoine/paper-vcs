# PV-002: resolve points, history and show

- **Status:** Done
- **Branch:** `feat/PV-002-history`
- **Depends on:** none. Runs in parallel with PV-009 (paper-diff only, disjoint files).
- **Touches:** `crates/paper-git/src/repo.rs`, `crates/paper-git/tests/repo.rs`,
  `crates/paper-core/src/project/mod.rs`, `crates/paper-core/src/project/resolve.rs` (new),
  `crates/paper-core/src/project/history.rs` (new), `crates/paper-core/tests/resolve.rs` (new),
  `crates/paper-core/tests/history.rs` (new), `docs/specs/01-concepts.md`, `docs/specs/02-cli.md`
- **Specs:** `01-concepts.md` § Points and § Researcher-facing wording rules;
  `02-cli.md` § `paper history`, § `paper show`; ADR-0003 (change IDs)

## Goal

Every front-end can turn a point the user typed (`@`, `last~2`, `main`, `kmrw`, `3fa9c1`,
`date:2026-09-01`) into a commit, list the saves on a line, and show one save with its changes.
`diff`, `restore` and `status` (PV-010) build on this.

## Context

- `point::Point` parses the syntax already (`Draft`, `Last { back }`, `Date(DateSpec)`, `Name`);
  `DateSpec::inclusive_end` gives the civil cutoff. This ticket adds resolution against a project.
- `ChangeId::unique_prefix_len` and its tests (paper-git) give the shortest-prefix rule (min 4).
- `tree_diff::diff_trees` and `snapshot::Change` describe a save's changes.
- `GitRepo` has `read_commit`, but nothing walks history yet.
- Milestones don't exist until v0.5. Their slot in the precedence order is reserved but empty.

## Scope

1. **paper-git: `GitRepo::ancestors(&self, from: &[CommitId]) -> Result<Vec<Commit>, GitError>`.**
   Every commit reachable from `from` (all parents), each once, newest first by committer time,
   ties broken by commit id for determinism. (A loaded `Vec` is fine for v0.1, which targets
   about 2 000 saves.)
2. **paper-core: `Project::resolve(&self, point: &Point) -> Result<Resolved, ResolveError>`**,
   `Resolved { commit: CommitId, matched: PointKind }`, with
   `PointKind::{Draft, Last, Line(LineName), ChangeId, CommitHash, Date}`. Precedence, as in
   `01-concepts.md`:
   - `@` → the current line's draft.
   - `last~N` → follow first parents from the current line's tip N times.
     `NotEnoughHistory { back, available }` if it runs out.
   - `date:…` → the newest save on the current line's first-parent chain whose **committer time**
     is at or before `inclusive_end()`, read in the system's local time zone.
     `NoSaveBefore { date }` if none.
   - a name that is a **line** → that line's tip.
   - otherwise a **change ID prefix** (only letters `k`–`z`, any length ≥ 1), searched over
     `ancestors` of every line's tip and draft. Drafts count, so `@`'s change ID resolves too.
     Several matching commits with the *same* change ID resolve to the newest.
   - otherwise a **commit hash prefix** (hex, at least 4 characters), searched over the same set.
   - Several *different* matches → `Ambiguous { prefix, candidates }` (short ids with message).
     No match → `UnknownPoint { name }`.
3. **`Project::history(&self, request: &HistoryRequest) -> Result<Vec<HistoryEntry>, HistoryError>`**,
   `HistoryRequest { path: Option<String>, all_lines: bool, limit: Option<usize> }`.
   - Default: the current line's saves, following first parents from its tip, newest first.
     The draft is not a save, so it isn't listed.
   - `all_lines`: the saves of every line merged, each listed once, newest first.
   - `path`: only saves whose changes against their first parent (the empty tree for the first
     save) touch that file or anything under that directory (whole path components, like `save`).
   - `limit`: at most that many entries.
   - `HistoryEntry { commit, change_id: Option<ChangeId>, short_change_id: String, message,
     author: Identity, time: GitTime, tip_of: Vec<LineName> }`. `short_change_id` uses
     `unique_prefix_len` over every change ID known in the project. Commits from plain git have
     no change ID: show the first 7 characters of the commit hash instead.
4. **`Project::show(&self, point: &Point) -> Result<Shown, ShowError>`**,
   `Shown { entry: HistoryEntry, changes: Vec<Change> }`, where `changes` compares against the
   first parent (or the empty tree). `show @` works and shows the draft's changes against
   `last`.
5. **Specs:**
   - `01-concepts.md` § Points: the change-ID and hash prefix rules (letters, minimum lengths,
     the search set, ambiguity), that `date:` uses committer time in local time, and that
     milestones join the precedence order in v0.5.
   - `02-cli.md`: `history` and `show` in the same terms.

Errors in the style of `SaveError`, teaching (CLAUDE.md rule 6). Example: `NotEnoughHistory`
says how many saves the line has.

## Out of scope

- Milestones, `--graph`, output formatting and relative times (CLI, PV-003).
- Diff content of files (PV-009, PV-010).
- Performance beyond about 2 000 saves.

## Acceptance criteria

- [ ] `ancestors` returns every reachable commit once, newest first, including both parents of a merge (cross-checked with `git rev-list --all` or `--topo-order` for counts).
- [ ] `@` resolves to the draft, `last` to the tip, `last~2` two saves back; `last~N` past the first save → `NotEnoughHistory` with the count.
- [ ] A line name resolves to its tip, and wins over a change ID or hash prefix with the same spelling.
- [ ] A unique change ID prefix (including the draft's) resolves; an ambiguous one → `Ambiguous` listing candidates.
- [ ] A unique hash prefix (≥ 4 hex) resolves; fewer than 4 characters isn't treated as a hash.
- [ ] Unknown names → `UnknownPoint`.
- [ ] `date:` resolves to the newest save at or before the cutoff (a save made later that day counts for a day-only date), and → `NoSaveBefore` before the first save.
- [ ] `history` lists the current line's saves newest first, excluding the draft; `limit` truncates; `path` filters by file and by directory.
- [ ] `history` with `all_lines` merges lines without duplicates.
- [ ] Commits made with plain git appear with a hash-based short id.
- [ ] `short_change_id` is at least 4 letters and unique across the project.
- [ ] `show` of a save lists its changes against its parent; `show` of the first save lists every file as added; `show @` lists the draft's unsaved changes.
- [ ] Specs updated.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass.

## Test plan

- `paper-git/tests/repo.rs`: `ancestors` on a small history with a merge, built with `GitRepo`
  or the git binary.
- `paper-core/tests/resolve.rs` and `tests/history.rs`: build projects with `Project::init` +
  `Project::save`. For controlled timestamps (`date:`), adopt a repo built with the `git` binary
  using `GIT_COMMITTER_DATE`/`GIT_AUTHOR_DATE`, with saves days apart so time zones can't flip
  the result. Lines other than `main` can be created through adoption (`git branch`).

## Summary

`GitRepo::ancestors` (paper-git), plus `Project::resolve`, `Project::history` and `Project::show`
(paper-core).

- Points resolve in the order of 01-concepts.md: `@`/`last~N`, then `date:`, line name, change-ID
  prefix (`k`–`z`) and hash prefix (≥ 4 hex characters).
- Ambiguity lists candidates with readable short ids. Plain-git commits use the shortest unique
  hash prefix, minimum 7 characters.
- `date:` uses committer time in the local zone and never panics: past the timestamp range
  means "no limit", before it means `NoSaveBefore`.
- `history` supports a path filter (sharing `save`'s path rules), `--all` and a limit. `show`
  lists a save's changes against its parent.

Deviations: none. Review fixed a panic reachable from user input (`date:9999-12-31`) before merge.

## Handoff log

### RED · test-author · 2026-10-16

- `crates/paper-git/tests/repo.rs`: 4 new tests for `GitRepo::ancestors` (merge with both parents,
  several tips sharing history, time ties by commit id, empty input). Cross-checked with
  `git rev-list --count`. All fail on `todo!()`.
- `crates/paper-core/tests/resolve.rs`: 16 tests (`@`, `last`, `last~N`, `NotEnoughHistory`, line
  wins over change-ID and hash spellings, change-ID prefix incl. draft, ambiguity, same change ID
  resolves to newest, hash prefix >= 4 and < 4, hash ambiguity by growing a history until a
  4-char collision, `UnknownPoint`, four `date:` cases). All fail on `todo!()`.
- `crates/paper-core/tests/history.rs`: 21 tests (history order, entry fields, limit, file and
  directory path filters, whole-component matching, all lines, plain-git short id, short change ID
  length and uniqueness, show of save / first save / merge / `@`, read-only check with fsck and
  status). All fail on `todo!()`.
- Stubs: `GitRepo::ancestors`; `project/resolve.rs` (`Project::resolve`, `Resolved`, `PointKind`,
  `Candidate`, `ResolveError`), `project/history.rs` (`Project::history`, `Project::show`,
  `HistoryRequest`, `HistoryEntry`, `Shown`, `HistoryError`, `ShowError`), re-exported from
  `project`.
- Interpretation choices (flag if wrong): `NotEnoughHistory.available` is the total number of
  saves on the line (first save included), so `last~(available-1)` is the oldest reachable.
  `Ambiguous.candidates` is `Vec<Candidate { commit, short_id, message }>`. `NoSaveBefore { date:
  DateSpec }`. Path `sections/` (trailing slash) equals `sections`. `tip_of` lists lines whose
  tip is the commit. For `date:` tests, commit times are built with jiff in the system zone;
  author time is 2020 to prove committer time is used.
- Environment note: worktrees sharing CARGO_TARGET_DIR can reuse each other's `paper-git` /
  `paper-diff` artifacts (a stale build lacked `ancestors`; `touch crates/paper-git/src/lib.rs`
  fixed it). `paper-diff` unit failures in a workspace run come from PV-009's artifacts, not
  this branch.
- RED check: pre-existing tests pass (paper-core 30 unit + 12 init + 33 save + 13 snapshot,
  paper-git 16 unit + 18 repo); fmt and clippy clean.

### GREEN · rust-engineer · 2026-10-16

- `GitRepo::ancestors`: collects reachable commits with a seen-set, sorts by committer time
  desc then commit id.
- `Project::resolve` (resolve.rs), `history` and `show` (history.rs), with shared helpers
  `known()` (ancestors of every tip and draft) and `current_line_chain()`.
- Interpretations followed as recorded by RED. `HistoryEntry.time` is the committer time (as the
  test checks `%ct`); `author` is the author identity. Candidate lists for change-ID ambiguity
  hold the newest commit per distinct change ID.
- `date:` converts the cutoff with the system zone and takes the first commit on the
  first-parent chain at or before it (`expect` on an unrepresentable civil time; documented).
- Specs: 01-concepts.md § Points and 02-cli.md (`history`, `show`) updated.
- fmt, clippy and the full suite pass.

### Review · reviewer · 2026-10-16
- Verdict: CHANGES. Checks: tests pass, clippy pass (isolated target).
- BLOCKER: a panic reachable from user input. `resolve_date`'s `expect` fails for
  `date:9999-12-31` (past jiff's Timestamp max in every zone), and for `date:-9999-01-01` east
  of about +2h. DST gaps and folds are safe (compatible disambiguation) → a cutoff past the max
  means no limit (the tip); before the min, `NoSaveBefore`. Test both; remove the `# Panics` claim.
- MINOR: `NoSaveBefore` prints `{date:?}` (Rust Debug) → a readable date.
- MINOR: `NotEnoughHistory` says "only 1 saves" and doesn't name the line.
- MINOR: the 7-character hash fallback isn't checked for uniqueness → shortest unique hex
  prefix, minimum 7, also for `Ambiguous` candidates.
- MINOR: `ancestors` walked several times per command → reuse `Known` (`resolve_with`), filter
  `known.commits` for `--all`.
- MINOR: `history::touches` duplicates `save::matches` → share one function.
- NIT: an inclusive end in a DST fold uses the earlier offset; the later one would be more
  inclusive. Optional.
- Coordinator: fix all of these in one round. Adding tests is authorised; don't weaken any.

### GREEN (review round 1)

- BLOCKER: `resolve_date` no longer panics. A cutoff past jiff's maximum resolves to "no limit"
  (the tip); one before the minimum gives `NoSaveBefore`. Tests for `date:9999-12-31` and
  `date:-9999-01-01`; the "Never in practice" Panics claim is gone. No other `expect`/`unwrap` on
  user input in resolve.rs or history.rs.
- `DateSpec` implements `Display` (`2026-09-01`, `2026-09-01T14:30`); `NoSaveBefore` uses it.
- `NotEnoughHistory` gained `line: LineName` and a singular/plural message. The existing test's
  pattern needed a `..` to still compile; assertions unchanged.
- Plain-git short ids and `Ambiguous` candidates use the shortest unique hash prefix, min 7. A
  7-character collision is too costly to build with real commits, so the rule is covered by a
  unit test on `unique_hash_len` with synthetic hashes.
- `resolve_with(&Known)` is reused by `show`; `resolve` only walks history for names.
  `history --all` filters the single `known` walk by reachability from the tips.
- `history` uses `save::matches` (now `pub(super)`) instead of its own copy.
- NIT done: the inclusive end uses the later occurrence in a DST fold.

### Review round 2 · reviewer · 2026-10-16
- Verdict: APPROVE. Tests, clippy and fmt pass (isolated target). The blocker and every minor
  finding are fixed and verified.
- NITs, skipped: the extreme-date test reaches the before-min branch only east of about
  UTC-2 (CI runs in UTC); no message test for the singular "1 save"; no test for the DST-fold
  choice (it depends on the system zone).

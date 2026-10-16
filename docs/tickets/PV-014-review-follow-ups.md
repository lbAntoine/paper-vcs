# PV-014: review follow-ups from PV-011, PV-013, PV-005 and PV-004

- **Status:** In progress
- **Branch:** `feat/PV-014-review-follow-ups`
- **Depends on:** PV-004, PV-005, PV-011, PV-013 (merged). Runs alone.
- **Touches:** `crates/paper-cli/tests/common/mod.rs`, `crates/paper-cli/tests/*.rs` (and snapshots
  only if redaction changes them), `crates/paper-cli/src/inspect.rs`, `crates/paper-cli/src/config_cmd.rs`,
  `crates/paper-core/src/error.rs`, `crates/paper-core/src/config/{mod,file}.rs`,
  `crates/paper-core/src/profile/gitignore.rs`, `crates/paper-core/src/project/init.rs`,
  `crates/paper-core/tests/{resolve,history,profile,config}.rs`, `crates/paper-git/src/{config,change_id}.rs`,
  `docs/specs/02-cli.md`
- **Specs:** CLAUDE.md rule 6 (errors teach); `02-cli.md` § Global behaviour, § `paper diff`,
  § `paper config`; `03-architecture.md` § Testing strategy

## Goal

Close the small findings that four reviews deferred, starting with the one that can make tests
fail at random: the CLI test redaction that can rewrite ordinary words. After this the backlog has
no known review debt.

## Items

Source tickets in brackets. "Test" means a test that fails before the fix.

### Test reliability

1. **`Env::redact` rewrites words** [PV-013]: it replaces every ≥4-character prefix of every known
   change ID anywhere in the text, so an ID starting with a k–z word ("your", "most", "slow")
   corrupts a snapshot. The same applies to 7-character hash prefixes that spell a hex word.
   Redact only whole tokens (bounded by non-alphanumeric characters or the text's ends) that are a
   prefix of a known ID or hash. Test: unit tests of the helper with a known ID starting with a
   word that appears elsewhere in the text, in plain text and inside JSON strings. Every existing
   snapshot must stay identical (if one changes, it was relying on the bug: explain it).
2. **Shared `all_commits` helper** [PV-013] for `project_with_unambiguous_tip` and
   `build_avoiding_random_prefix` in `crates/paper-core/tests/resolve.rs`; clearer comment on the
   four-letter floor (line ~405) and exact odds in the `history.rs` comment (16^-11).
3. **Tighter assertion** [PV-004]: `a_damaged_block_during_init_suggests_running_init_again`
   checks `next: paper init`, not just `paper init`.

### Error wording (rule 6)

4. **File typos in `diff`** [PV-011]: `paper diff sections/nope.tex` gets the point-flavoured
   "is not a save, line, date or file". Keep that wording only when the word could be a point
   (the resolver stopped on `UnknownPoint` for a word that parses as a point); otherwise the
   file wording with `paper status` / `paper show <point>`. Test both.
5. **Broken `.git` at the `init` target** [PV-011]: `paper init` in a folder whose own `.git` is
   empty or damaged is a project error ("report it"). Make it a three-part user error ("this
   folder has a damaged .git…", next: inspect or move it). Test.
6. **Order-conflict direction** [PV-005]: when `warn_mb` must be lowered the suggestion says
   "at least". Use "at most" (or "lower … to N"). Test the warn-above-max case.
7. **Repeated words note** [PV-005]: "no effect until v0.3 (lines until v0.3)" →
   "`diff.default = words` has no effect until v0.3; comparisons stay line by line." Test the text.
8. **`unset` on Windows** [PV-005]: the env-origin suggestion is POSIX-only. On Windows suggest
   PowerShell `Remove-Item Env:PAPER_…` (with `cmd`'s `set PAPER_…=` in the explanation). Pure
   function chosen by `cfg`, unit-tested for both forms.
9. **`.gitignore` read errors** [PV-004]: a read failure other than not-found or bad UTF-8 (e.g.
   permission denied) is "could not update .gitignore", a project error. Permission denied is a
   user error naming the file; other I/O errors say "read" or "write" correctly. Test with a
   unit-level mapping (and a Unix-only permission test if reliable).

### Code quality and docs

10. **Duplicate `Origin` note** [PV-005]: `config::Origin::note` and `error.rs::origin_note` are the
    same; keep one (`Display` on `Origin` or a `pub(crate)` method).
11. **`ProfileSource` enum** [PV-004]: replace `Initialized.detected` + `from_paper_toml` with
    `profile_source: ProfileSource { Detected, Chosen, PaperToml }`; remove the
    `#[allow(clippy::struct_excessive_bools)]`. JSON keeps `detected` (true only for `Detected`);
    snapshots unchanged.
12. **Mixed line endings** [PV-005]: `ConfigFile` turns every LF into CRLF when the file has any
    CRLF. Keep existing bytes; new lines use the ending of the file's first line. Test with a
    mixed file.
13. **Docs:** `change_id.rs` says `l..=z` (it is `k..=z`) [PV-013]; over-long doc-comment lines in
    `paper-git/src/config.rs` [PV-011]; over-long lines in 02-cli § `paper init` and § `paper config`
    [PV-004, PV-005].

### Small addition

14. **`notes` in JSON** [PV-005]: `config get` and `config list` with `--json` also carry
    `"notes": [..]` (the privacy note for ignored keys, the `words` note), so scripts don't parse
    stderr. stderr notes stay for human output; under `--json` they move into the field
    (stdout stays the only JSON). Document in 02-cli; test; the key is always present (`[]`).

## Out of scope

- `config unset`, `ui.editor`, cross-file `max_mb ≥ warn_mb` check, `config set` JSON saying whether
  `.gitignore` changed (ideas, not debt).
- PV-012 (restore on case-insensitive filesystems), PV-008.

## Acceptance criteria

- [ ] Item 1: redaction unit tests pass; no existing snapshot changes (or each change is explained).
- [ ] Items 3–9, 12, 14: each has a test that failed before the fix.
- [ ] Items 2, 10, 11, 13 done; no `#[allow(clippy::struct_excessive_bools)]` left; JSON and snapshots unchanged by item 11.
- [ ] The full suite passes 20 times in a row locally (`--no-fail-fast`), recorded in the handoff log.
- [ ] Specs updated (02-cli).
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS, Windows).

## Test plan

- RED writes the failing tests for items 1, 3–9, 12 and 14; GREEN does every item.
- Redaction tests as unit tests in `tests/common/mod.rs` (or a `tests/redaction.rs` addition).
- Reuse the existing CLI isolation and core test helpers.

## Summary

## Handoff log

### RED · test-author · 2026-10-16

22 tests fail (all on `todo!()` or an assertion); every other pre-existing test passes.

- **Item 1** `crates/paper-cli/tests/redaction.rs` (+5 tests): `redact_ids(text, &[(change_id, hash)])` is a new pure stub (`todo!()`) in `tests/common/mod.rs`. `Env::redact` is untouched (so no snapshot test fails now); GREEN makes it call `redact_ids` for the id/hash part. Contract: whole alphanumeric tokens that are a prefix of a known id (>= 4 chars) or hash (>= 7 chars); words like "yourself" and "mostly" stay; JSON strings via `redact_strings`.
- **Item 3** `init.rs`: `a_damaged_block_during_init_suggests_running_init_again` now asserts `next: paper init` (changed assertion, tighter; passes today).
- **Item 4** `diff.rs` (+2): file wording for `sections/nope.tex` (fails); point wording kept for `lsat` (guard, passes).
- **Item 5** `init.rs` (+1): empty `.git` directory is a three-part user error, "damaged .git", no "report it" (fails: exit 3 today).
- **Item 6** `config.rs` (+2): warn above max says "at most 5", never "at least" (fails); max below warn still says "at least 5" (guard).
- **Item 7** `config.rs` (+1): exact sentence on stderr for get and list (fails).
- **Item 8** `paper-core/src/error.rs` (+2 unit): stub `unset_suggestion(var, windows) -> Suggestion` (`todo!()`, `#[allow(dead_code)]` until GREEN wires it into `invalid_value` with `cfg!(windows)`, then remove the allow). Posix: `unset VAR`; Windows: `Remove-Item Env:VAR`, explanation names `set VAR=`.
- **Item 9** `error.rs` (+3 unit): `GitignoreError::Io` is split into `Read` and `Write` (same fields, Display still "could not update", match arm in `from_gitignore` unchanged in behaviour). Tests want Display "could not read {path}" / "could not write {path}", and PermissionDenied on either to be a user error naming the file with `retry` as first next. No permission-based filesystem test (unreliable as root).
- **Item 12** `paper-core/tests/config.rs` (+3): mixed endings keep existing bytes on a key change; a new table uses the first line's ending (CRLF-first and LF-first cases). Fail today.
- **Item 14** `config.rs` (+3): `notes` in get/list JSON, stderr empty under `--json`.

Existing tests changed: `get_and_set_json_have_a_fixed_shape` and `list_json_has_schema_1_and_a_fixed_key_set` now expect the `notes` key (spec change, item 14); snapshot `config__config_list_json.snap` gets `"notes": []` (hand-edited, keys sort alphabetically). Both fail until GREEN. Item 3 as above.

Questions for GREEN/coordinator: (a) a lone 4-letter word equal to an id's first four letters ("your" alone) is still a whole token that is a prefix; the ticket's rule can't tell it from a real short id, so tests use longer words; GREEN may raise the minimum if it wants. (b) Whether `--quiet` suppresses JSON `notes` is unspecified; not tested. (c) Item 4's "could be a point" rule is left to GREEN; only `sections/nope.tex` (file) and `lsat` (point) are pinned.

### GREEN · rust-engineer · 2026-10-16

All 14 items done. fmt, clippy (`-D warnings`) clean; full suite (`--no-fail-fast`, `INSTA_UPDATE=no`) passed **20 of 20 consecutive runs, 0 failures** (built once, looped).

- **1** `redact_ids` tokenises on non-alphanumerics and replaces tokens that are a prefix of a known id (>= 4) or hash (>= 7). `Env::redact` calls it. Residual risk (a lone 4-letter word equal to an id's first four letters, about 1 in 65 536 per id per matching k-z word) is in the helper's comment. All snapshots stayed byte-identical under the new redaction.
- **2** `all_commits` helper in `resolve.rs`; floor comment clarified; `history.rs` comment gives 2 x 16^-11.
- **3** already tightened by RED.
- **4** A word with `/` or `.` is never treated as a possible point, so it gets the file wording. **One existing snapshot changed because of this**: `diff__diff_unknown_path.snap` (`paper diff mian.tex`) now says "No file matches: mian.tex" (file wording, `paper status` / `paper show <point>`). Not a redaction change; it is the intended item 4 behaviour for a word with a dot.
- **5** New `InitError::DamagedGit` (three-part user error). It is raised in the core and also checked up front in the CLI `init`, because reading the git identity from the damaged `.git` failed first with a project error.
- **6** `order_conflict` says "at most" when `warn_mb` must be lowered. **7** words note reworded. **8** `unset_suggestion` wired in via `cfg!(windows)`. **9** permission denied on `.gitignore` is a user error naming the file; `Read`/`Write` display "could not read/write".
- **10** `Origin::note` is `pub(crate)`; `error.rs::origin_note` removed. **11** `ProfileSource` enum (exported from `paper_core::project`); JSON `detected` is true only for `Detected`; `#[allow(clippy::struct_excessive_bools)]` removed from `Initialized`; JSON and snapshots unchanged.
- **12** `toml_edit` normalises CRLF to LF when parsing, so `ConfigFile` now keeps the raw text next to the document. After an edit, unchanged lines keep their bytes, a changed line keeps its old ending, new lines use the first line's ending.
- **13** `k..=z` doc fix, `paper-git/src/config.rs` comment wrapped, 02-cli `init` and `config` paragraphs wrapped.
- **14** `notes` is always present in `config get|list --json` (also under `-q`); stderr notes only for human output without `-q`. 02-cli updated for items 4, 5, 6, 8, 9, 12 and 14.

Deviations to review:
- `tests/redaction.rs`: the RED fixture `OTHER_ID = "mostlykmnopqrstu"` made the word "mostly" (in two tests that require it to stay) a whole-token prefix of a known id, which no whole-token rule can leave alone while replacing "most". Changed only the constant to `"mostkmnopqrstuvw"`; assertions untouched.
- `crates/paper-core/tests/profile.rs`: `initialized.detected` no longer exists (item 11); the five assertions now compare `profile_source` with `ProfileSource::Detected`.

### Review · reviewer · 2026-10-16
- Verdict: CHANGES. Tests, clippy and fmt pass. Items 1, 2, 10, 11 verified (no snapshot changed from item 1; the RED fixture change was a genuine fixture error; the resolve/history refactor keeps every assertion and bound). Item 12 probed with real files in many shapes; one shape fails.
- BLOCKER: item 12 regression. A config file without a final newline gets invalid TOML on `config set` (`name = "A"email = …`), breaking every later command; a CRLF file without a final newline gets an LF ending. `raw` and the `toml_edit` document drift when toml_edit adds the final newline.
- MAJOR: item 5 only works without a repository above. Inside a parent repo, a damaged `child/.git` gives "inside another project … paper init <outer>", which would adopt the outer repo.
- MINOR: the damaged-`.git` check is duplicated in core and CLI, and the core path has no test.
- MINOR: the item-4 comment and spec say a word with `/` or `.` can't be a line name; line names allow both.
- MINOR: wrong odds in a history.rs comment; `profile.rs` asserts `!= Detected` instead of the exact variant.
- MINOR, pre-existing: `config set` drops a comment line directly above a top-level key (`# c` above `profile = …`).
- NITs: the redaction residual-risk comment should mention longer k–z words; item 13's rewrapping is unfinished (02-cli lines 113, 369, 372).
- Coordinator decisions: fix everything in this round, the pre-existing comment drop included (same file, same promise). One core predicate for "the target's own .git is damaged" (target has `.git` and the enclosing workdir isn't the target), used by both core and CLI, with core and CLI tests including a parent repository. Add the `raw`/document invariant as a `debug_assert` with a safe fallback. Adding tests is authorised; don't weaken any.

### GREEN (review round 1) · rust-engineer · 2026-10-16

- **Blocker (final newline):** `toml_edit` ends the last line with a newline, so `raw` and the document drifted. `consistent_raw` now runs at load, after `with_doc` and after `with_new_table`: it adds the file's first-line ending when the last line had none, falls back to a rebuild from the document if they still disagree, and `debug_assert`s the invariant. Tests (LF and CRLF, no final newline): add a key to an existing table, edit the last key, add a new table; each reloads the written file and checks bytes.
- **Major (damaged `.git` in a parent repo):** one core predicate `paper_core::project::has_damaged_git` (`target/.git` exists and the enclosing workdir is not the target). Core `init` and the CLI pre-check both call it (the CLI check must run before settings are read, because that read also fails on the damaged `.git`). Tests: empty `child/.git` and a `.git` file pointing at a missing gitdir, inside a `git init` parent (core and CLI), the same file without a parent, and a valid worktree-style `.git` file still adopts.
- **Comments:** item-4 comment and 02-cli reworded (such words are treated as files, a line name with `/` or `.` also gets the file wording); history.rs odds now "about 1 in 9 x 10^12"; profile.rs asserts `Chosen` and adds `PaperToml`; redaction comment covers longer k-z words (16^-n); 02-cli rewrapping finished.
- **Comment above a key (pre-existing):** `set_keeping_decor` replaced the key and dropped its leading comment; it now replaces the item in place. Tests for a top-level key and a key in a table.
- Full suite (`--no-fail-fast`, `INSTA_UPDATE=no`): **20 of 20 consecutive runs, 0 failures**. fmt and clippy (`-D warnings`) clean.

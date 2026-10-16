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

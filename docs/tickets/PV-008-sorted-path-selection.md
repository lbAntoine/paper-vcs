# PV-008: save path selection with sorted lookups

- **Status:** In progress
- **Branch:** `feat/PV-008-sorted-path-selection`
- **Depends on:** PV-007 (merged). Runs in parallel with PV-012 (restore).
- **Touches:** `crates/paper-core/src/project/save.rs` (`select` only), `crates/paper-core/tests/save.rs`
- **Must not touch:** `crates/paper-core/src/project/restore.rs`, `crates/paper-core/tests/restore.rs` (PV-012),
  and the behaviour or signature of `save::matches` (used by restore, history and diff).
- **Specs:** `02-cli.md` § `paper save` (path rules: exact file or whole directory components,
  trailing `/` optional, `""` = everything); `03-architecture.md` § Performance targets

## Goal

`paper save <paths>` and `--all-but` stay fast when a draft has many changes and the user names many
paths, with exactly the same results as today.

## Context

- `select(selection, changes)` (`project/save.rs`) checks every selected path against every change
  twice: once to find paths with no changes (`NoChangesAt`), once to partition. That is O(paths ×
  changes). PV-007 already replaced the quadratic conflict closure with sorted lookups and left this
  (noted as PV-008).
- `matches(selected, file)`: the file itself, or a file under the directory; `""` matches everything;
  a trailing `/` is ignored.

## Decisions (coordinator, 2026-10-16)

- This is a behaviour-preserving refactor. The RED phase writes **characterisation tests**, which are
  expected to pass already (there is no new behaviour to fail on); they pin the current results
  so the refactor can't change them. The GREEN phase refactors, and every test must stay green.
- No timing assertions in tests (they flake on CI). The complexity is documented in a comment and
  checked in review; a large-input test proves correctness at scale, not speed.

## Scope

1. `select` uses sorted lookups: changes sorted by path; for each selected path, a range lookup over
   `[path, path + "/" …)` plus the exact entry (`BTreeMap`/`BTreeSet` ranges, or binary search on a
   sorted `Vec`). Target O((paths + changes) log changes).
2. Same results in the same order as today for `Only` and `AllBut`, including: `""` and `.`-derived
   empty paths, trailing `/`, whole-component matching (`sec` vs `sections/…`), paths matching
   nothing (`NoChangesAt` lists them in the order given), duplicates in the selection, overlapping
   selections (`sections` and `sections/intro.tex`), and paths that sort between related keys
   (`a`, `a b`, `a/b`, `a-b`, `a.b`; `/` sorts after ` `, `-` and `.`).
3. A comment states the complexity.

## Out of scope

- `restore`'s own path loops (PV-012 owns that file), `history`/`diff` filtering, the conflict closure.

## Acceptance criteria

- [ ] Characterisation tests pin `select`'s results for every case in scope item 2, through `Project::save` (and a unit test of `select` if useful), and pass before and after the refactor.
- [ ] A differential test compares `select` with a straightforward reference implementation (written in the test) over generated inputs: at least a few thousand random combinations of nested paths, including the tricky sort neighbours, with a fixed seed.
- [ ] A large-input test (e.g. 20 000 changes, 2 000 selected paths) gives the reference result.
- [ ] `select` no longer compares every path with every change; the complexity is in a comment.
- [ ] fmt, clippy (`-D warnings`) and the full suite pass (CI on Linux, macOS and Windows).

## Test plan

- `crates/paper-core/tests/save.rs` for behaviour through `Project::save` (real repositories, the
  `generic` profile as the other fixtures use), and unit tests next to `select` for the differential
  and large-input tests (pure, no git).
- Generated inputs from a fixed seed (`rand` is already a dependency) so failures reproduce.

## Summary

## Handoff log

### RED · test-author · 2026-10-16

Characterisation tests: all PASS against today's code by design (behaviour-preserving refactor).

- `crates/paper-core/tests/save.rs`: 13 new tests (section "PV-008 path selection"), through
  `Project::save` on real `generic`-profile repos, cross-checked with `git fsck --strict`.
  Cover `""`, `/`, trailing `/`, whole-component matching, unmatched paths in given order
  (duplicates kept), duplicate and overlapping selections, `Only` and `AllBut`, and the sort
  neighbours (`a b`, `a-b`, `a.b`, `a/b`, `a0`, `ab`), plus added/deleted changes.
- `crates/paper-core/src/project/save.rs` (`#[cfg(test)]`): 3 new pure tests: differential vs a naive
  reference (4000 rounds x 3 selections, xorshift seed fixed, asserts both Ok and Err outcomes
  are well represented), sort-neighbour sweep, and a 20 000-change / 2 000-path test.
  No timing assertions. The reference does not call `matches`.
- Stubs added: none; no production code changed (`select` is reachable from the child test module).
- Note: "a b/" and "a.b/" match (trailing `/` ignored); tests reflect that.
- Questions: none.

### GREEN · rust-engineer · 2026-10-16

`select` in `crates/paper-core/src/project/save.rs` now uses sorted lookups, O((P + C) log C) (times path depth), with the complexity in a comment.

- Unmatched check: change paths sorted once; each selected path is a binary search for the exact key plus the range `[path + "/", path + "0")` (so `a b`, `a-b`, `a.b`, `a0` are not matched). `""` selects everything; trailing `/` is trimmed.
- Partition: each change is tested against a `BTreeSet` of the trimmed selected paths via itself and its ancestor directories.
- `matches` is unchanged (still used by `close_over_conflicts` and the tests). No tests, specs, dependencies or manifests changed. All characterisation tests pass unchanged; fmt, clippy `-D warnings` and the full suite are green.

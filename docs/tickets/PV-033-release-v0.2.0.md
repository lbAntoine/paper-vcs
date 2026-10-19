# PV-033: release v0.2.0

- **Status:** In progress
- **Issue:** #60
- **Branch:** `chore/PV-033-release-v0.2.0`
- **Depends on:** every other v0.2-b ticket (merged: PV-025, PV-027, PV-029, PV-031, PV-018, PV-026, PV-030, PV-032).
- **Touches:**
  - `Cargo.toml` (version), `Cargo.lock`
  - `CHANGELOG.md` (`## 0.2.0`)
  - `README.md` (status block, Install URLs and version, the "There is no undo" sentences, the overview of what works)
  - `docs/specs/05-roadmap.md` (§ v0.2: the shipped `ignore` line, the Accept line marked as met)
  - `docs/tickets/PV-033-release-v0.2.0.md`
- **Must not touch:** any source file, test, CI workflow or `dist-workspace.toml`. `.github/workflows/v-release.yml` is generated and never edited by hand.
- **Specs:** `docs/workflow.md` § Releasing (the runbook), `05-roadmap.md` § v0.2

## Goal

v0.2.0 is published as a GitHub pre-release with installers. The CHANGELOG and README tell a
researcher, in plain words, what v0.2 does, what's new since 0.1.0, what still doesn't work,
and which known issues remain.

## Decisions (coordinator and maintainer, 2026-10-19)

- **Version and release type.** The version is `0.2.0`. It's a pre-release, like v0.1.0. cargo-dist can't mark an unsuffixed version as a pre-release, so the runbook's `gh release edit v0.2.0 --prerelease` step applies.
- **CHANGELOG `## 0.2.0`** follows 0.1.0's register: researcher words, no internals.
  - **Opening:** "Nothing is ever lost."
  - **What's new:**
    - `paper undo`, `paper redo`, `paper undo --to <id>`, and `paper activity` (every entry undoable, files included);
    - crash safety: if paper is stopped mid-command, the next command repairs it;
    - `paper gc`;
    - `paper ignore` / `unignore` / `--local` / `--why`;
    - `status` says when the line was changed with plain git;
    - the refusal after a plain-git commit now names a way back (`git reset --soft <id>`).
  - **Upgrading from 0.1.0:** projects upgrade themselves on first use. History from before the upgrade can't be undone.
  - **Known issues:**
    - **PV-035 (#67):** on macOS, undo and `restore` after renaming a file by changing only the Unicode form of an accented name can delete it. Avoid renames that change only an accent's encoding. Maintainer decision: listed as a known issue.
    - **PV-037 (#72):** a file edited while it's ignored ends the redo stack at the next command.
    - **Local rules (`--local`)** live in `.git/info/exclude`, outside history: undo changes tracking only.
    - **PV-036 (#71)** and **PV-034 (#66):** small ignore-listing gaps. Mention them only if they fit in one line.
  - **What doesn't work yet:** lines (`try`, `keep`), milestones, sync and `git pull`, word diff (v0.3).
- **README.**
  - Status block: "v0.2.0 is a pre-release …", with undo now in the list of what works.
  - Install section: URLs and `paper --version` updated to 0.2.0.
  - The "v0.1.0 has `init`, `save` and `history`" overview is updated.
  - Nothing new is promised.
- **Roadmap § v0.2.** Add a ticked line for `paper ignore`/`unignore`/`--local`/`--why`, and mark the Accept line as met (the PV-026 proptest).
- **Checks before the PR:**
  - `cargo build` updates `Cargo.lock`.
  - `dist plan` (cargo-dist 0.32.0, installed into `target/tools`) lists the five targets and the installers for 0.2.0.
  - The release-only test runs locally: `cargo test --release -p paper-cli --test crash release_builds_ignore_the_crash_variable`. CI never runs it (PV-025 known gap).
- **Goal verification.** The `verifier` agent writes a fresh VERIFICATION.md for v0.2 on `main` before the tag.
- **Tagging.** The maintainer approves the tag. The coordinator tags only after approval and runs the runbook's steps 4–7.
- **Last steps.** Close the v0.2 milestone and issue #60. Afterwards, a `/retro` for sprint v0.2-b.

## Acceptance criteria

- [ ] Version 0.2.0 in `Cargo.toml` and `Cargo.lock`; `paper --version` prints `paper 0.2.0`.
- [ ] CHANGELOG `## 0.2.0` and README updated as decided. Nothing in them is untrue for v0.2.0.
- [ ] Roadmap § v0.2 updated.
- [ ] `dist plan` is clean, and the release-only crash test passes locally.
- [ ] VERIFICATION.md for v0.2: PASSED, or its gaps handled.
- [ ] fmt, clippy and the full suite pass on CI (3 OSes). PR merged.
- [ ] Maintainer approval, then tag `v0.2.0`, release workflow green, marked as a pre-release, installer smoke-tested.
- [ ] v0.2 milestone and #60 closed.

## Summary

## Handoff log

### Prep · rust-engineer · 2026-10-19

- Version bumped to 0.2.0 (`Cargo.toml`, `Cargo.lock`); `paper --version` prints `paper 0.2.0`.
- CHANGELOG `## 0.2.0` written per the Decisions; claims checked against `02-cli.md` and `paper --help`. README status block, Install URLs and version, and the overview line updated. Roadmap § v0.2: ticked `ignore` line, Accept marked met (PV-026 proptest).
- `dist plan` (cargo-dist 0.32.0 in `target/tools`): "announcing v0.2.0", `paper-cli 0.2.0` with `paper-cli-installer.sh`, `paper-cli-installer.ps1`, `sha256.sum`, and archives for aarch64-apple-darwin, aarch64-unknown-linux-gnu, x86_64-apple-darwin, x86_64-pc-windows-msvc (.zip), x86_64-unknown-linux-gnu. No changes to `dist-workspace.toml` or the workflow.
- `cargo test --release -p paper-cli --test crash release_builds_ignore_the_crash_variable`: passed.
- fmt, clippy `-D warnings` and `cargo test --workspace --all-targets --no-fail-fast`: all passed.
- README's "Why" bullets (line 56, "`paper undo` reverts any operation, including itself") left as they were: they describe the goal and are true for v0.2. The sentence "There is no undo" existed only in the status block.

### Prep (review fixes) · rust-engineer · 2026-10-19

- Applied the eleven wording fixes from review to CHANGELOG, README and this ticket.
- Checked item 8 with the binary: after a `git pull` that brought in another clone's commit, `paper save` refuses and names `git reset --soft <id>` and `git log main`.
- PV-038 (#74) is not listed as a known issue, per the maintainer: it lands on main before the release.

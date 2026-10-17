# PV-017: spec honesty: version-mark promises v0.1 doesn't keep

- **Status:** In progress
- **Issue:** #34
- **Branch:** `feat/PV-017-spec-honesty`
- **Depends on:** none. Runs in parallel with PV-019 (lock), which edits other sections.
- **Touches:** `docs/specs/02-cli.md` (§ `paper init`, § `paper save`, § `paper restore`,
  § Escape hatches only), `docs/specs/03-architecture.md` (§ Git interop only), `README.md`
- **Must not touch:** code; 02-cli § Global behaviour and 03-architecture § Crates, § On-disk
  layout, § Command lifecycle (PV-019).
- **Specs:** `05-roadmap.md` (which version brings what); the v0.1 goal verification
  (VERIFICATION.md, 2026-10-17, untracked) and issue #34

## Goal

A researcher reading the README or the CLI reference knows exactly what v0.1/v0.2 does today and
which promised behaviours arrive in which version, so nobody runs `git pull` expecting it to work.

## Pipeline note

Docs only: there is no RED phase and no code. The rust-engineer writes the changes (it keeps
specs true in every ticket), then the reviewer reviews as usual.

## Scope

Each change says what happens today and which version brings the rest, using `05-roadmap.md`:

1. **`git pull`.** README § status: you can `git push` your saves; bringing in others' work with
   `git pull` isn't supported yet. After a pull brings new commits, `status` shows their edits as
   your unsaved changes, `history` doesn't show their saves, and `paper save` refuses until a later
   version (importing external git changes is v0.4 on the roadmap; syncing with co-authors is
   v0.6). Same note, briefly, in 02-cli § Escape hatches.
2. **Importing external git changes.** 03-architecture § Git interop and 02-cli § Escape hatches
   say paper imports external changes on its next run: mark this as v0.4 (roadmap § v0.4
   Interop), and say what v0.1–v0.3 do instead (`save` refuses when the line moved; the error
   names `git log <line>`).
3. **Cloud-sync warning at `init`** (02-cli § `paper init`): mark as v0.7 (roadmap § v0.7).
4. **`save --with-conflicts`** (02-cli § `paper save`): conflicts arrive with keeps in v0.4; mark it.
5. **`paper help concepts|git|large-files`** (02-cli § Escape hatches): not implemented yet (they
   are unknown subcommands today). Mark the version from the roadmap if it names one; otherwise
   say "planned" and point to the docs that exist instead.
6. **"Undoable"** in 02-cli § `paper restore`: undo arrives in v0.2. Say "undoable from v0.2"
   (it will be true when v0.2 ships; PV-023/PV-024 remove the qualifier if wanted).

Keep the specs' style: a short "*Today:* …" or "(since v0.N)" note, not a rewrite.

## Acceptance criteria

- [ ] Every item in scope is addressed in the listed sections, with the version taken from the roadmap.
- [ ] README states the `git pull` limitation plainly.
- [ ] No other section changed; no code changed.
- [ ] Claims are checked against the code (e.g. run `paper help concepts` and a pull scenario with the built binary to confirm what happens today).
- [ ] CI passes (fmt, clippy, tests unchanged).

## Summary

## Handoff log

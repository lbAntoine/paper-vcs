# PV-017: spec honesty: version-mark promises v0.1 doesn't keep

- **Status:** Done
- **Issue:** #34
- **Branch:** `feat/PV-017-spec-honesty`
- **Depends on:** none. Runs in parallel with PV-019 (lock), which edits other sections.
- **Touches:** `docs/specs/02-cli.md` (§ `paper init`, § `paper save`, § `paper restore`,
  § Escape hatches only), `docs/specs/03-architecture.md` (§ Git interop only), `README.md`, `docs/specs/05-roadmap.md` (intro sentence only)
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

The specs and README now say what v0.1 does today and which version brings each promised
behaviour (versions from 05-roadmap; "*Today:*" claims checked with the built binary).

- README: plain statement that `git pull` isn't supported (status shows pulled edits as unsaved,
  history hides the saves, `save` refuses); a lead-in marks the feature list as where paper is going.
- Importing external git changes starts in v0.4 (`git checkout` and branch creation), full
  interop-table coverage is v0.8, co-authors' work arrives with `sync` in v0.6 (02-cli § save,
  § Escape hatches; 03-architecture § Git interop, which also gives the exact `save` refusals).
- Version marks: cloud-sync warning at `init` (v0.7), `save --with-conflicts` (v0.4),
  `help large-files` (v0.3; `help concepts`/`git` have no roadmap version), restore undoable
  from v0.2.
- 05-roadmap intro: `git push` sharing works since v0.1, not v0.3.

Deviations: docs only, so no RED phase (engineer wrote the docs, reviewer reviewed); the
05-roadmap intro sentence was added to scope during review.

## Handoff log

### DOCS · rust-engineer · 2026-10-17

Docs only, no code changed. Edited README (status note, "Everything is undoable"), 02-cli §
`paper init`, § `paper save`, § `paper restore`, § Escape hatches, and 03-architecture § Git interop.

Checked with the built binary (isolated env, `git init -b main`, throwaway folder, deleted):
- `paper help concepts`, `help git`, `help large-files`: each prints `error: unrecognized
  subcommand '<topic>'`, exit 1.
- Pull scenario (bare remote, push from clone A with `paper init`, push a `b.txt` commit from clone B, `git pull` in A):
  `status` shows `Draft (unsaved): new b.txt +1 −0 lines`; `history` shows only `Start project`;
  `paper save "mine"` exits 1 with "The line 'main' was changed outside paper ... Paper can't
  save on top of that yet ... next: git log main".
- `init --help` and `save --help` have no `--yes` or `--with-conflicts`; no cloud-sync check in the code.

Versions, from `docs/specs/05-roadmap.md`:
- Importing external git changes: "v0.4: Safe experiments" bullet "Interop: `git checkout` and branch creation imported correctly".
- Syncing with co-authors: "v0.6: Collaboration" (`clone`, `remote`, `sync`, `share`).
- Cloud-sync warning: "v0.7: Office documents" bullet "...and the cloud-sync folder warning at `init`/`clone`".
- `--with-conflicts`: "v0.4" bullet "Fast-forward or merge on keep, in-progress keep with conflicts, `keep --abort`".
- `help large-files`: "v0.3" bullet "`warn_mb` / `max_mb`, "Skipped" in `status`, `help large-files`". The roadmap names no
  version for `help concepts` / `help git`; only "v0.8: Hardening" says "`paper help` topics complete", so I wrote "no version given".
- Undo: "v0.2: Nothing is ever lost" (`undo`, `redo`, `activity`).

Deviation: the README "Everything is undoable" bullet also got "(from v0.2)", which the ticket's scope didn't list but fits "README states what works today".
Follow-up: the spec's `--yes` flag and `warn` at init are also unimplemented (noted in 02-cli).

### Review · reviewer · 2026-10-17
- Verdict: CHANGES. Scope respected (no PV-019 sections, no code). Every roadmap version and every "*Today:*" claim checked against the roadmap and the built binary.
- MAJOR: 02-cli § Escape hatches says `save` names `git log <line>` after a branch change; after `git switch` it says `git switch <line>` instead (`HeadOnAnotherBranch`).
- MINOR: cites a "roadmap § v0.4: Interop" section that doesn't exist; v0.4 only promises importing `git checkout` and branch creation, and importing external commits has no version (co-authors' work arrives with `sync` in v0.6).
- MINOR: README marks only undo "(from v0.2)" while neighbouring bullets (word diffs v0.3, switching v0.4, milestones v0.5) are also future.
- NITs: a redundant "*Today (v0.1 to v0.3)*" sentence; a doubled "from v0.2" in § restore; two unwrapped lines; pre-existing: 05-roadmap's intro says `git push` sharing starts at v0.3, but it works since v0.1.
- Coordinator decisions: fix everything in this round. README: one lead-in line before the feature bullets ("Where paper is going; see Status for what works today") instead of per-bullet versions; drop "(from v0.2)". Import wording: "importing starts in v0.4 with `git checkout` and branch creation; bringing in co-authors' work arrives with `sync` in v0.6". Fix the 05-roadmap intro sentence in this PR (one sentence; 05-roadmap joins Touches).

### DOCS (review round 1) · rust-engineer · 2026-10-17

All five items fixed, docs only. Claims re-checked: roadmap "## v0.4: Safe experiments" has the bullet "Interop: `git checkout` and
branch creation imported correctly"; "## v0.6: Collaboration" lists `sync`; "## v0.8: Hardening" has "Every row of the git-interop table
covered by tests"; the intro now reads "Since v0.1, users can already share work by running `git push`" (matches the v0.1 Accept line
"push with plain `git push`"). Binary behaviour unchanged from the earlier run (log hint after a pull; branch-change message per
`HeadOnAnotherBranch` as the reviewer cited). README lead-in added, "(from v0.2)" removed, line 13 rewrapped; 05-roadmap added to Touches.

### DOCS (post-approval) · rust-engineer · 2026-10-17

Final wording: save import sentence, README "branch creation", restore rewrap.

### Review round 2 · reviewer · 2026-10-17
- Verdict: APPROVE. Branch-change and new-commit refusals confirmed on the built binary; every version citation matches 05-roadmap; scope avoids PV-019's sections; no code changed.
- Fixed after approval: the § save import sentence, "branch creation" in the README, one rewrap.

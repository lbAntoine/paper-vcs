# ADR-0004: One draft per line, and lines advance automatically on save

- **Status:** Accepted · 2026-09-25

## Context

jj has a single working-copy commit, and bookmarks don't move automatically. Beginners
often trip over "why didn't my bookmark move?" and over anonymous heads. Git, on the other
hand, needs a stash or a commit before you can switch branches.

## Decision

- **Each line owns its own draft** (`refs/paper/draft/<line>`). `switch` stores one draft
  and restores the other. There is no stash and no "commit before you switch" (I3).
- **Saving advances the current line**, as a git branch does. There are no anonymous heads
  in the default workflow.
- `try` moves the current draft into the new try by default ("this is getting experimental").

## Consequences

- This deliberately departs from jj. Power users lose jj's "edit any commit in place"
  workflow in v1. If there's demand, it can come back later as `paper edit <point>`.
- Switching lines rewrites the working files. That's safe because a snapshot always comes
  first, but it may confuse editors with unsaved buffers. Docs recommend saving in the
  editor first.

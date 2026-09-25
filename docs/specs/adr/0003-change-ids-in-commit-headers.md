# ADR-0003: Stable change IDs stored as an extra git commit header

- **Status:** Accepted · 2026-09-25

## Context

A save can be rewritten: its draft is amended on every snapshot, and a sync can rebase it.
Commit hashes then change. Users and the activity log need an identity that survives
rewriting, just like jj's change IDs.

## Options

1. **Commit message trailer** (`Change-Id: …`, Gerrit style). Visible and noisy in `git log`,
   and it pollutes messages that researchers wrote.
2. **Extra commit header** (`change-id <id>`) ✅. Invisible in normal `git log`, preserved
   by push, fetch and clone, and gix can read and write extra headers.
3. **Side table in `.git/paper/`**. Local only, so it's lost for co-authors and on clone.

## Decision

Option 2. The ID is 16 random bytes, rendered in the letters `k`–`z` (as jj does) so it
never looks like a hex hash, and shown as the shortest unique prefix (minimum 4).

## Consequences

- Plain `git commit --amend` or `git rebase` drops the header. Those commits get an
  *unstable* ID derived from their hash, and paper still works.
- **Open question:** jj writes a `change-id` header in colocated repos. If we match its
  exact encoding, paper and jj could share IDs. We'll check this against jj's source
  before v0.1 freezes the format.

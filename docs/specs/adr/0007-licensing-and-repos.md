# ADR-0007: Licence and crate publication

- **Status:** Accepted · 2026-09-25 · Revised 2026-10-15

## Context

paper-vcs should be free to use, study, modify and embed, for individuals, labs and
companies alike. Its core is meant to be reused by other front-ends, not only by the
`paper` CLI.

## Decision

- **Licence:** the whole workspace (`paper-core`, `paper-git`, `paper-diff`, `paper-cli`)
  is dual-licensed **Apache-2.0 OR MIT**, at the user's choice. This is the Rust ecosystem
  norm, friendly to academia, and Apache-2.0 adds an explicit patent grant.
- **Contributions** are accepted under the same terms (Apache-2.0 §5), so no contributor
  licence agreement is needed.
- **Publication:** `paper-core` is published to crates.io with a documented public API, so
  other tools can depend on it as a versioned crate.

## Consequences

- Anyone can build on the engine, including in closed-source products. That's intended.
- A stable, semver-versioned `paper-core` API becomes a real commitment from v1.0.
- All dependencies must stay permissively licensed (MIT, Apache-2.0, BSD, Zlib, Unlicense
  and similar). Checked on 2026-10-15: all 136 crates in the dependency tree comply.

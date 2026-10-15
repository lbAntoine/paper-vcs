# Contributing to paper

Thanks for your interest! paper is **pre-alpha**: the design is still settling and most
commands don't exist yet. The most useful contributions right now are:

- **Feedback on the design.** Read the [specs](docs/specs/README.md) and open an issue if
  something feels wrong, confusing or missing, especially if you're a researcher who
  versions papers, theses or analyses.
- **Bug reports** for anything that's already built.
- **Small, focused pull requests.** For anything larger than a bug fix, please open an
  issue first so we can agree on the approach before you spend time on it.

## Development setup

You need [rustup](https://rustup.rs) and `git`. The Rust version is pinned in
`rust-toolchain.toml`, and rustup installs it automatically on first use.

```console
$ git clone https://github.com/lbAntoine/paper-vcs.git
$ cd paper-vcs
$ cargo test --workspace
```

Before opening a pull request, run the same checks as CI:

```console
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo test --workspace --all-targets
```

CI runs the tests on Linux, macOS and Windows.

## How the code is organised

| Crate | Role |
|---|---|
| `paper-core` | Domain model and commands. **Never prints or prompts**: it returns data, and front-ends render it |
| `paper-git` | The only crate that talks to gitoxide. **No gix types in its public API** |
| `paper-diff` | Tokenisers and diff algorithms. Pure functions, no git |
| `paper-cli` | Argument parsing, prompts, human and `--json` output |

The [architecture spec](docs/specs/03-architecture.md) explains the rest: the on-disk
format, the command lifecycle and how git interop works.

## Conventions

- **Tests first.** Write the failing test, then the code. Unit tests sit next to the code
  in `#[cfg(test)]` modules, and cross-module behaviour goes in `crates/*/tests/`.
- **Plain git is a hard promise** ([ADR-0002](docs/specs/adr/0002-plain-git-compatibility.md)).
  Anything that writes to a repository is cross-checked with the real `git` binary in tests
  (`git fsck`, `git log`, `git cat-file`).
- **Values over mutation.** State transitions return new values (for example
  `View::with_line`) instead of mutating shared state.
- **Errors teach.** User-facing errors say what happened, why, and what to run next.
  Use researcher vocabulary (save, line, draft), not git jargon.
- **Lints are strict.** Clippy runs at pedantic level. Public functions that return
  `Result` or can panic document it in `# Errors` / `# Panics` sections.
- **Specs stay true.** If a change alters behaviour described in `docs/specs/`, update the
  spec in the same pull request. Significant design decisions get a new ADR in
  `docs/specs/adr/` (copy the structure of an existing one: context, options, decision,
  consequences).

## Commits and pull requests

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org):

```
<type>: <description>

<optional body explaining why>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, `ci`.

Keep pull requests small and focused on one change. Link the issue they address, and fill
in the checklist in the pull request template.

## Reporting security issues

Please **don't** open a public issue for a security problem. Use GitHub's private
vulnerability reporting instead (the **Security** tab → **Report a vulnerability**).

## Conduct

Be kind and assume good faith. Critique ideas, not people. Many future users of paper
aren't software engineers, so beginner questions are welcome and deserve patient answers.

## License

By contributing, you agree that your contributions are dual licensed under
[Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at the user's option, as described in
the [README](README.md#license). No contributor licence agreement is needed.

# paper

**Version control for researchers.** Plain git underneath, a much friendlier model on top.

> **Status: pre-alpha.** The engine is under active development (v0.1 in progress), and
> nothing is usable yet. The [design specs](docs/specs/README.md) describe where it's going.

## Why

Git is powerful, but its model gets in the way of people who write papers, theses and
analyses: the staging area, detached HEADs, "please commit before you switch", and the
feeling that work can be lost. Line-based diffs also say "1 line changed" when one word in
a paragraph moved.

paper keeps everything as a normal git repository, so GitHub, GitLab, the Overleaf git
bridge and your co-authors' tools keep working. On top of that, it borrows the best ideas
from [Jujutsu](https://github.com/jj-vcs/jj):

- **You can't lose work.** Your files are recorded automatically before every command.
- **Everything is undoable.** `paper undo` reverts any operation, including itself.
- **No dirty state.** There's no staging area and no stash, and switching is always safe.
- **Diffs that read like prose.** Word-level diffs for LaTeX, Markdown and Typst, with
  readable diffs for Word and Excel files planned.
- **Milestones.** Things like "Submitted to NeurIPS" or "camera-ready" are first-class,
  and `paper diff --since neurips-submission` answers "what changed since then?".

## A taste (planned interface)

```console
$ paper init --profile latex
$ paper save "Rewrite related work"
$ paper try restructure-intro      # experiment safely on a separate line
$ paper keep                       # …and fold it back in if it works
$ paper milestone "NeurIPS submission"
$ paper history
kmrwxq  2 hours ago   Rewrite related work
tqlpvn  yesterday     Add ablation figure         ◆ neurips-submission
```

The full command set is in the [CLI reference](docs/specs/02-cli.md), and the plan is in the
[roadmap](docs/specs/05-roadmap.md).

## Building from source

paper is written in Rust. The toolchain version is pinned in `rust-toolchain.toml` and
installed automatically by `rustup`.

```console
$ git clone https://github.com/lbAntoine/paper-vcs.git
$ cd paper-vcs
$ cargo test --workspace
$ cargo run -p paper-cli
```

The tests use the `git` binary to check that everything paper writes is valid plain git.

## Project layout

| Crate | Role |
|---|---|
| [`paper-core`](crates/paper-core) | Domain model and commands: the library every front-end uses |
| [`paper-git`](crates/paper-git) | Git storage layer on [gitoxide](https://github.com/GitoxideLabs/gitoxide) |
| [`paper-diff`](crates/paper-diff) | Prose-aware, table-aware diffing |
| [`paper-cli`](crates/paper-cli) | The `paper` command |

Design documents and decision records are in [`docs/specs`](docs/specs/README.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed
as above, without any additional terms or conditions.

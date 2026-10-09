# Contributing

## Reporting Issues

If you have found what you think is a bug,
please [file an issue](https://github.com/paritytech/trinity-user-agents/issues/new/choose).

## Suggesting New Features

Feature proposals live as markdown files in `docs/features/`. To propose a new feature:

1. Create a branch and add a new file to `docs/features/` (e.g., `docs/features/my-feature.md`)
2. Include YAML frontmatter (`title`, `type: feature`, `status: draft`, `author`, `pr`)
3. Describe the feature: summary, use cases, and proposed solution
4. Update `docs/features/_index.md` with a link to your file
5. Open a PR using the **feature** template (`?template=feature.md`) and add the `feature-request` and `proposal` labels

## RFCs

For larger changes that need cross-team discussion, use the RFC process:

1. Create a branch and add a new file to `docs/rfcs/<slug>.md` (e.g., `docs/rfcs/my-proposal.md`) — do **not** assign a number
2. Use `docs/rfcs/0001-template.md` as a reference for the expected structure and frontmatter
3. Open a PR using the **rfc** template (`?template=rfc.md`) and add the `rfc` label
4. The PR will be auto-added to the project board for tracking and review
5. When the PR is approved and merged, CI automatically assigns the next sequential number, renames the file, and appends it to `docs/rfcs/_index.md`

A CI check (`check-rfc.yml`) reads the RFC documents a PR touches. A new RFC
needs frontmatter with a `title` and an `owner`, a `## Summary`, a
`## Motivation`, and a section describing the approach, and must carry no
unedited template text and no `TODO`, `TBD` or `FIXME`. Editing an existing RFC
is judged only on the lines the change adds, so an RFC written before the
template is not held to defects its author never introduced.
Implementation is not required in the same PR: it is tracked on the RFC's issue,
which carries a task per host alongside the Rust one.

If you use Claude Code, the [`rfc`](.claude/skills/rfc/SKILL.md) skill is highly recommended for drafting RFCs — invoke it with `/rfc` to turn your notes into a well-structured document that follows the template above.

## Design Documents

Canonical design documentation lives in `docs/design/`. To propose updates or add new design docs:

1. Edit or add a file in `docs/design/`
2. Include YAML frontmatter (`title`, `type: design`, `status`, `author`, `created`, `pr`)
3. Open a PR using the **design** template (`?template=design.md`) and add the `design-doc` label

## Development

### Prerequisites

- Rust toolchain (stable + nightly for `cargo fmt`)
- Node.js and npm (for the TypeScript client)
- Yarn 1.x (for the playground)

### Repository layout

```
rust/crates/
  truapi/              Rust trait + type definitions (source of truth)
  truapi-codegen/      rustdoc JSON → TypeScript client generator
  truapi-macros/       #[wire_trait(id = N)] + #[wire(...)] proc-macros
js/packages/
  truapi/              @parity/truapi TypeScript package (generated TS is auto-generated and git-ignored)
playground/            Next.js interactive playground
hosts/dotli/           dotli host (git submodule)
scripts/codegen.sh     regenerate the TS client from the Rust crate
```

Common tasks are wrapped in the top-level `Makefile`. Run `make help` to see
the full list of targets.

### Getting started

```bash
make setup    # submodules, JS dependencies, and the generated outputs
make build    # Rust workspace + TypeScript client
```

The generated Rust, TypeScript and Swift outputs are git-ignored, so a fresh
checkout has none of them and the `truapi` runtime does not compile until they
exist. `make setup` produces them; `make codegen` regenerates them on their
own.

### Making changes to the API

The Rust crate in `rust/crates/truapi/` is the single source of truth for the
TrUAPI protocol. When you modify traits or types there:

```bash
make codegen      # regenerate the TS client and refresh the playground snapshot
make playground   # rebuild the playground against the refreshed snapshot
```

### Verification

```bash
make test     # Rust + TypeScript client tests
make check    # full suite: build, fmt, clippy, test, TS tests, playground build + lint
```
Every target that compiles the `truapi` runtime depends on `check-generated`, so a
missing generated file names itself and points at `make codegen` instead of
failing inside rustc.

## Pull requests

Maintainers merge pull requests by squashing all commits and editing the commit message if necessary using the GitHub
user interface.

Title the pull request as a conventional commit, since the title becomes the squashed commit: `<type>(<scope>): <subject>`, with `!` before the colon for a breaking change. [`.claude/skills/semver-pr-title/SKILL.md`](.claude/skills/semver-pr-title/SKILL.md) lists the types and says what counts as breaking for products, for testers of the host apps, and for the CLI. The `pr-title` check blocks a title that does not parse, and a `major` changeset whose title lacks `!`. It re-runs when the title is edited, so fixing one needs no new commit. RFC pull requests are `docs(rfc): <title>`. The nightly announcements list `!` titles first, marked `Breaking:`.

## Releasing

See [`docs/RELEASE_PROCESS.md`](docs/RELEASE_PROCESS.md) for the release flow, covering the npm packages and the iOS and Android host artifacts.

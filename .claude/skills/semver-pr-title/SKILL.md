---
name: semver-pr-title
description: Title a pull request in this repository as a conventional commit, and decide whether it is a breaking change that needs a `!`. Use before opening a pull request, before retitling one, when writing a changeset, and when completing a backport work order. The title becomes the squashed commit on main and is what the nightly announcements read to list breaking changes first.
---

# Pull request titles and breaking changes

Pull requests are squashed, so the title becomes the commit on `main`. Two
things read it: `.github/workflows/pr-title.yml`, which blocks a merge whose
title does not parse, and the nightly announcements, which list every title
carrying `!` first and mark it `Breaking:`. A breaking change titled without
`!` reaches testers as an ordinary line.

## The format

```
<type>(<scope>)!: <subject>
```

- **type** is one of `feat`, `fix`, `perf`, `refactor`, `docs`, `test`,
  `build`, `ci`, `chore`, `style`, `revert`, `release`.
- **scope** is optional and names the area: a crate (`truapi-server`), a
  package (`truapi-host`), a host (`ios`, `android`, `hosts`), or `cli`.
- **`!`** marks a breaking change. It goes after the scope, before the colon.
- **subject** starts lowercase, describes what the change does, and has no
  trailing period.

Some pull requests have a fixed shape:

- **An RFC** is `docs(rfc): <title>`. The `RFC: <Title>` form is for the
  tracking issue, which this check does not see.
- **A revert** keeps its generated `Revert "<original title>"`, nested or
  not, and so does a `Reapply "<original title>"`. It passes as long as the
  original did.
- **Titles a workflow writes into `main`**, such as backport work orders,
  releases and diagnosis reports, are already conventional. Keep them that way
  when editing those workflows. Pull requests into a release branch, such as the
  lifecycle's `Backport:` ones, are not checked.

```
feat(truapi-server): withdraw a call before it prompts
fix(ios): conform the chat bridge's inert host provider
refactor(truapi)!: rename Provider type to WireProvider
chore(hosts)!: backport 13 commits into hosts/ios
docs(rfc): scoped grants in trustedProducts
release: @parity/truapi 0.21.0
```

## What is breaking

Ask who has to change what they do because of this pull request. Each
audience has its own test, and a change is breaking if it meets any of them.

**Products and SDK consumers.** Someone has to change their code or their
configuration for it to keep working.

- A public export, type, method or field is removed or renamed.
- A signature changes in a way existing callers do not satisfy.
- A wire shape or a protocol version changes so that an older peer cannot
  talk to a newer one.
- A host has to implement something new, such as a required `HostBridge`
  callback or a new protocol requirement with no default.
- A default changes behaviour a caller relied on.

**Testers of the host apps.** Someone installing the next nightly loses
something or has to do something by hand.

- Stored data is dropped, reset or unreadable after the update.
- The user is signed out or has to pair or register again.
- A reinstall is needed.
- A feature is removed or put behind a flag that is off.

**The CLI.** A script that ran the previous version stops working: a flag is
removed or renamed, an exit code changes meaning, or output a script might
parse changes shape.

## What is not breaking

- Adding an export, a method, an optional field, or a protocol message older
  peers can ignore.
- A database migration that only adds an optional attribute with a default.
  CoreData and Room migrate that in place and nothing is lost. A migration
  that drops or rewrites data is breaking.
- Internal refactors, tests, CI and docs, even large ones.
- A bug fix, unless callers depended on the buggy behaviour. If they did,
  say so and mark it.

When unsure, mark it. A false `!` costs a line in the announcement. A missing
one costs a tester an afternoon.

## Changesets

A breaking change to a published npm package needs a `major` changeset and
`!` in the title. `pr-title.yml` fails a pull request that adds a `major`
changeset without `!`. The reverse does not hold: a host app or CLI change
can be breaking without touching any package, so `!` never needs a changeset.

## Backport work orders

A backport pull request carries upstream work the title does not describe.
Before completing one, read the pull requests listed in `BACKPORT-<host>.md`
against the tests above. If any of them is breaking, retitle the backport
with `!` and name what breaks in the description:

```
chore(hosts)!: backport 13 commits into hosts/ios
```

## Checking a title

The check uses `amannn/action-semantic-pull-request` and only gates pull
requests into `main`. It runs on every edit of the title, so fixing a failing
title needs no new commit. A title edited after the pull request has entered
the merge queue is not checked again, so retitle before queueing.

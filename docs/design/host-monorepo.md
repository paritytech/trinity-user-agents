---
title: "Host Monorepo: One Repository for Every Host"
type: design
status: draft
author: TarikGul
created: 2026-09-08
pr: 650
---

# Host Monorepo: One Repository for Every Host

_Argues the repository topology. Epic #669 holds the work items._

## Summary

**Recommendation: move every host into this repository alongside the core, compose their build systems rather than
merging them, and have each host build against the in-tree core at HEAD.** One change is then built and tested across
all hosts in a single pull request, and each pull request produces an installable build per host.

A host today consumes the core as a published artifact and adopts it separately, so one logical change becomes several
coordinated pull requests, and two hosts can sit on different core versions with no check failing.

## Why now

Four properties of the current arrangement, each removed by a specific part of the design:

| Friction                                                 | Consequence                                           |
| -------------------------------------------------------- | ----------------------------------------------------- |
| The core ships as a release, adopted per host repository | One change becomes several coordinated pull requests  |
| Hosts consume a published artifact                       | Two hosts can run different core versions, undetected |
| No installable build per commit                          | Reviewing a host change means reading a diff          |
| Product checks run against the core's own playground     | A change can pass CI here and be broken on a host     |

## Scope

This document answers three questions:

1. Should the hosts live in this repository, and under what layout?
2. How do several build systems coexist once they do?
3. What does a host build against, a pinned core or the core at HEAD?

Adopting the Rust core in place of native implementations is **out of scope**; it is sequenced after lockstep CI exists
and argued separately.

## The topology

```
                        ONE REPOSITORY
  +--------------------------------------------------------------+
  |  rust/crates/            the core: protocol, server, platform|
  |        ^                                                     |
  |        | generated bindings                                  |
  |  ios/truapi-host/        Swift SDK                           |
  |  android/truapi-host/    Kotlin SDK                          |
  |  js/packages/            TypeScript SDK                      |
  |        ^                                                     |
  |        | consumed by                                         |
  |  hosts/ios/  hosts/android/  hosts/web/                      |
  |        each a host application, presentation stays native    |
  +--------------------------------------------------------------+
             |                |                |
             +------- CI: one pull request ----+
             build and test every host, publish an
             installable artifact per commit

  core-owned : protocol, domain, data          (reviewed closely)
  sdk-owned  : generated bindings per language
  host-owned : presentation, platform UI
  CI-owned   : change gating, per-host build, preview artifact
```

`hosts/` already exists and holds the web host as a submodule, so the layout extends a convention rather than
introducing one. The SDK layer stays at `ios/` and `android/`.

## Option 1: published artifacts per host

The current arrangement. Each host repository depends on a released core version.

**Pros:** hosts are insulated from an unrelated core merge; each repository has its own release cadence and permissions.
**Cons:** a core change reaches hosts only after a release and N adoption pull requests; version drift between hosts is
invisible; no single pull request can prove a change works everywhere.

## Option 2: submodules per host

Keep the host repositories and reference them here as submodules, as the web host is referenced today.

**Pros:** history and ownership stay where they are; the move is cheap. **Cons:** a submodule change is a separate
commit in a separate repository, so a host referenced this way **cannot take part in a single-pull-request check**. That
defeats the goal rather than deferring it. It also leaves every developer with a submodule to keep in sync.

## Option 3: one repository, composed builds

Each host's source tree lives here. Build systems are composed, not merged: the root Gradle build includes each Android
host as a separate build, and each Swift host keeps its own package manifest.

**Pros:** one pull request builds and tests every host; version drift becomes impossible; one place to add a
preview-build pipeline. **Cons:** the repository grows; CI does more work per pull request, so gating matters; and a
core change can now fail a host's build, which is the intended trade but a real change in what a red build means.

## Comparison

Legend: Y = holds, N = does not hold, P = partial.

| Property                            | Option 1 | Option 2 | Option 3 |
| ----------------------------------- | -------- | -------- | -------- |
| One pull request reaches every host | N        | N        | Y        |
| Version drift detectable            | N        | P        | Y        |
| Installable build per commit        | N        | N        | Y        |
| Host insulated from core churn      | Y        | Y        | N        |
| Cheap to adopt                      | Y        | Y        | N        |

## Recommendation

**Adopt Option 3.** Reasons, in order:

1. Only Option 3 satisfies the first three rows of the comparison, which are the goals.
2. Option 2 looks like a cheaper Option 3 but fails the goal outright: a submodule cannot participate in a
   single-pull-request check.
3. Option 1's one advantage, insulation from core churn, is the property being traded away deliberately. Gating keeps
   its cost bounded; see below.

## How the builds compose

Two Gradle builds cannot share one settings file once their `dependencyResolutionManagement` blocks disagree, and they
will: the root build declares a repository policy and each host application already declares its own. `includeBuild`
composes them instead, leaving both intact so a host still builds standalone.

Direction matters, and it is not the obvious one. Dependency substitution flows from the **including** build into the
**included** build, so a host cannot see a project belonging to a build that includes it. Two inclusions are therefore
needed, and they are not symmetric:

    hosts/<platform>/settings.gradle.kts   includeBuild("../..")   so the host can consume
                                                                   the in-tree SDK
    settings.gradle.kts                    includeBuild("hosts/x") so one command at the
                                                                   root drives every host

Confirmed by prototype with both builds setting `FAIL_ON_PROJECT_REPOS` and declaring different repositories: the two
inclusions coexist without a cycle, the root can drive a host's build, and each host still builds on its own. Composing
in one direction only fails: a host included by the root cannot resolve the root's SDK project.

Two further details, each of which costs a failed run to discover. Tasks in a composed build are not addressable
directly, so the root needs one delegate task per host, and that delegate names the included build by its **directory**,
not by its `rootProject.name`.

Swift needs no equivalent. The root `Package.swift` stays at the repository root because external consumers resolve its
products by URL, and a host's own manifest lives in its own directory without conflict.

### What building at HEAD costs

Consuming the in-tree core rather than a published artifact means the host build produces what the artifact used to
supply. On Android that is the native library per ABI, which the published AAR exists precisely to spare consumers. On
iOS the generated bindings and the xcframework are build outputs rather than committed files, so an in-tree host
regenerates them.

That cost is the price of the guarantee, not an oversight: a host that consumes a prebuilt artifact cannot fail when the
core changes, which is the property being bought. It does mean host jobs are slower than they are today, and it is why
gating matters.

## Gating

CI cost is bounded by computing every path gate once. `.github/workflows/ci.yml` holds a change-detection job publishing
one output per gated area, and each job reads the output it depends on. Adding a host means adding one output and one
consumer.

Jobs skipped by their filter report as skipped, which the aggregate status job counts as a pass. A gate therefore cannot
stall a pull request it does not apply to, which is why the aggregate job is the check worth requiring.

Two limits on that as it stands. The filter evaluates every gate to true on any event other than `pull_request`, so a
merge queue run builds everything. That is deliberate caution rather than a bug, but it means gating saves nothing
there. And one compile gate still lives in its own workflow with its own `paths` filter, outside the aggregate, so "one
output and one consumer" describes the shape after #670 lands rather than the shape today.

## What is imported, and from where

The sources are `polkadot-ios-community`, `polkadot-android-community` and `dotli-community`. All three are public, so
importing them into this public repository publishes nothing that was not already public. The private application
repositories are not in scope.

`hosts/imports.json` records which source each tree came from and the revision it was taken at.
`scripts/refresh-host-import.sh status <host>` reports how far a tree has drifted from its source, and `refresh` moves it
forward, re-applying this repository's adaptations on top.

Each host's tracked tree at a recorded commit, not its history. This keeps the imports ordinary additive changes with no
rewriting, and keeps repository growth to the size of the trees.

The consequence is that `git log` and `git blame` on a host file stop at the import commit. Two things therefore hold:
the source repositories are kept **read-only permanently** rather than deleted, because they become the only copy of
that history; and each import commit records the exact source commit so any file can be traced back.

## Migration

Imports are additive and cannot break the default branch. Building against the core at HEAD is the only step that
changes behaviour, so it lands separately per host and is revertible on its own. Hosts move one at a time, and the next
does not start until the previous is linked and green. The tracking issue carries the sequence and the per-host
checklist.

## Open questions

1. **Does the shared core own view-model state?** Including it makes presentation a renderer; excluding it leaves view
   state native. This answer sets how much moves later and is not settled here.
2. **Who owns a red build when a core change breaks a host?** Building at HEAD makes this a routine event rather than an
   exception, so the answer should exist before the first host is linked, not after.
3. **What verifies the web host at link time?** The end-to-end job that exercises the playground inside the web host is
   currently disabled. Either it is re-enabled first, or the web host links with weaker verification than the others.

## References

- `.github/workflows/ci.yml`, the change-detection job and aggregate status job
- `Package.swift`, the root manifest external consumers resolve
- `settings.gradle.kts`, the root Gradle build
- Epic #669, for work items and sequence

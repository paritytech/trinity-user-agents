# Labels

Labels answer the questions a reviewer asks before opening a pull request: what does it touch, can I review it yet, how big is it, and how urgent is it. Each family of labels answers one of those questions, and its prefix says which.

Most labels are applied for you. Authors set a `status:` when a pull request stops waiting on review, and a `ci:` label when they want an app built.

Every label is defined in [`.github/labels.yml`](../.github/labels.yml). Change that file to add, rename or retire one; a pull request that does so lists exactly what it will change, and the merge applies it.

## The families

| Family | Answers | On | Set by |
| --- | --- | --- | --- |
| `area:` | What does it touch? | pull requests and issues | the labeler, from the paths changed |
| `status:` | Who acts next? | pull requests and issues | the author or reviewer; the first one automatically |
| `size:` | How much is there to read? | pull requests | automatically, from the lines changed |
| `priority:` | How soon? | both | whoever triages |
| `type:` | What kind of work? | issues | whoever files it |
| `ci:` | Which builds and checks run? | pull requests | the author |

A pull request usually carries an area, a size and a status, and nothing else.

## area

Applied by [`.github/labeler.yml`](../.github/labeler.yml) from the paths a pull request changes, so a pull request touching several places carries several. The same paths decide who is asked to review in `.github/CODEOWNERS`, and `scripts/check-path-routing.py` keeps the two files in step.

| Label | Paths |
| --- | --- |
| `area: core` | `rust/crates/` |
| `area: ios` | `hosts/ios/`, `ios/` |
| `area: android` | `hosts/android/`, `android/` |
| `area: js` | `js/`, `playground/`, `explorer/` |
| `area: ci` | `.github/workflows/`, `.github/actions/` |
| `area: docs` | `docs/`, any `*.md` |
| `area: rfc` | `docs/rfcs/` |

`area: hosts` is for issues that need work in both apps, or have not been narrowed to one yet.

Every area routes to the same team today, so the labels are for filtering the list. When areas get owners of their own, CODEOWNERS can route by the same paths without the labels changing.

## status

One at a time, named for who has to act next.

| Label | Means |
| --- | --- |
| `status: waiting on review` | Ready for a reviewer. Set automatically when a pull request opens ready, or leaves draft, with no status. |
| `status: waiting on author` | The author has changes to make. A reviewer sets it when they request changes. |
| `status: ready to merge` | Approved, and waiting only to be queued. |
| `status: needs spec` | Do not review or merge until a spec or design doc is agreed. |
| `status: needs decision` | Waiting on a decision, not on work. |
| `status: needs design` | An issue that cannot be built until the UI design exists. |
| `status: blocked` | Cannot move until something else lands. Say what in a comment. |

`status: needs spec`, `status: needs decision` and `status: blocked` fail the `Label gate` check, so a pull request carrying one cannot merge while it does. Removing the label clears the check.

A draft carries no status. Converting a pull request back to draft removes `status: waiting on review`.

### When a pull request needs a spec

Anyone reviewing a pull request can add `status: needs spec` when the change makes a design decision that nobody has agreed yet. Once it is on, nobody reviews the code: the conversation moves to the spec or design doc, and the label comes off when that merges. Which changes need a spec is decided with the design documentation work, and this section will link to it.

## size

Set on every pull request from the lines it adds and removes, and kept current as it changes.

| Label | Lines |
| --- | --- |
| `size: XS` | under 10 |
| `size: S` | 10 to 29 |
| `size: M` | 30 to 99 |
| `size: L` | 100 to 499 |
| `size: XL` | 500 to 999 |
| `size: XXL` | 1000 or more |

Lines a reviewer does not read are left out: anything `.gitattributes` marks `linguist-generated`, which GitHub also collapses in the diff, plus lockfiles and the database schemas the build writes. Marking a new generated file there keeps it out of the count. A backport of a vendored host tree carries no size, because its line count says nothing about how much there is to review.

`size: XL` and `size: XXL` are a prompt to split. A change too large to review in one sitting can usually land as a series of smaller pull requests into a feature branch, each reviewed by whoever knows that part, before the branch merges into main. When it genuinely cannot be split, say why in the description.

## priority

Most work carries none.

| Label | Means |
| --- | --- |
| `priority: release blocker` | The current release cannot ship without it |
| `priority: high` | Next in line once release blockers are clear |

## type

Issues carry one. A pull request's conventional commit title already says its type, so pull requests carry a `type:` only where a workflow sets it.

| Label | Means |
| --- | --- |
| `type: bug` | Something does not work as intended |
| `type: feature` | New behaviour, or a proposal for it |
| `type: tech debt` | Cleanup or refactoring the code needs but no feature asks for |
| `type: epic` | Tracks a body of work through its sub-issues |
| `type: backport` | A pull request bringing a vendored host tree up to its source, opened by `backport-host.yml` |
| `type: dependencies` | A pull request updating a dependency, opened by Dependabot or `update-chain-specs.yml` |

## ci

Each starts as soon as it is added and follows the branch from then on. The [README](../README.md#working-on-the-ios-host) covers the builds in detail.

| Label | Runs |
| --- | --- |
| `ci: ios simulator` | Builds the iOS app, runs its tests and attaches a simulator build |
| `ci: ios device` | Attaches a signed iOS build for registered devices |
| `ci: android device` | Attaches an Android APK for any phone |
| `ci: android emulator tests` | Runs the Android instrumented tests on an emulator |
| `ci: host playground e2e` | Runs the host-playground e2e inside both apps |
| `ci: no changeset` | Lets the changeset check pass for a change that ships nothing |

The first time a pull request touches an app, a comment lists the `ci:` labels that build it.

## What is not a label

- **A release.** Work for a release goes on that release's milestone. A label with a release in its name has to be replaced every release.
- **A breaking change.** A `!` in the pull request title marks it, and the nightly announcement lists those first.
- **Why an issue was closed.** GitHub records duplicates and issues not planned when the issue is closed.
- **Who reviews.** CODEOWNERS requests reviewers from the paths changed.

## Changing a label

Edit `.github/labels.yml`. Give every label a color, a description that says when to use it, a `target` and an `addedBy`. To rename one, put its old name under `previously`; the rename keeps it on everything that carries it. Listing a second old name merges that label into this one. A label that is no longer needed goes under `retired` with a reason, and is deleted when the change merges.

A workflow that keys on a label name changes in the same pull request as the label, so both land together.

## Not yet in place

- **Product tracks.** A `feature:` family for product tracks, once the tracks are agreed. The labels for them are held under `pending` in `labels.yml` until then.
- **Milestones.** One per release, replacing the release-specific labels still under `pending`.
- **Stale pull requests.** A scheduled job that marks pull requests with no activity for 30 days, so the open list stays current.
- **A required gate.** `Label gate` reports on every pull request, and holds the merge once it is a required check on main.

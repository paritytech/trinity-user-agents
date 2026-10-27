# Labels

Labels answer what a reviewer or a planner asks before opening something: what does it touch, which product needs it, is anything blocking it, who acts next, and how big is it. Each family answers one of those questions, and its prefix says which. Names are `family:value`.

Most labels are applied for you. On a pull request, the author or reviewer changes the `status:` when it stops waiting on review and adds a `needs:` label when something blocks it, and the author adds a `ci:` label to get an app built. On an issue, whoever files it sets the product, and the area is filled in when a pull request that closes it merges.

Every label is defined in [`.github/labels.yml`](../.github/labels.yml). Change that file to add, rename or retire one; a pull request that does so lists exactly what it will change, and the merge applies it.

## On issues

| Family | Answers | Set by |
| --- | --- | --- |
| `area:` (core, ios, android, js, ci, docs, rfc) | What does it touch? | Automatically when a pull request that closes it merges, or by hand |
| `needs:` (decision, spec, UI) | What is blocking it? | Whoever works on it |
| `spa:` (humanity, jollity, scarcity, seity) | Which product needs it? | Whoever files it |

An issue's type (Bug, Feature, Task), its status (Triage, Backlog, In progress), its priority and its target date are issue fields, not labels.

## On pull requests

| Family | Answers | Set by |
| --- | --- | --- |
| `status:` (waiting-on-review, waiting-on-author, ready-to-merge, blocked) | Who acts next? | `waiting-on-review` automatically, the rest by the author or reviewer |
| `needs:` (decision, spec, UI) | What is blocking it? | The author or reviewer |
| `size:` (XS to XXL) | How much is there to read? | Automatically, from the lines changed |
| `ci:` (ios-simulator, ios-device, android-device, android-emulator-tests, host-playground-e2e, no-changeset) | Which builds and checks run? | The author |

A pull request usually carries a size and a status, and nothing else.

## area

[`.github/areas.yml`](../.github/areas.yml) maps paths to areas. When a pull request merges, `issue-areas.yml` adds the areas of the paths it changed to every issue it closes. Only closing references count: `Closes #12`, `Fixes #12`, or an issue linked under Development. A mention like "part of #12" does not, so an epic does not collect every area its pull requests touch, and a pull request that never merges labels nothing. Areas are only added, so one set by hand when the issue is filed stays.

| Label | Paths |
| --- | --- |
| `area:core` | `rust/crates/` |
| `area:ios` | `hosts/ios/`, `ios/` |
| `area:android` | `hosts/android/`, `android/` |
| `area:js` | `js/`, `playground/`, `explorer/` |
| `area:ci` | `.github/workflows/`, `.github/actions/` |
| `area:docs` | `docs/` except `docs/rfcs/`, any `*.md` at the root |
| `area:rfc` | `docs/rfcs/` |

An RFC is under `docs/` but is only `area:rfc`, so filtering on `area:docs` leaves RFCs out.

The same paths decide who is asked to review in `.github/CODEOWNERS`, and `scripts/check-path-routing.py` keeps the two files in step.

New pull requests carry no area, which keeps their labels to what a reviewer acts on. Older pull requests keep the areas they had. Giving pull requests areas is a change to `issue-areas.yml`, not to the labels.

## needs

What is blocking the work, on an issue or a pull request.

| Label | Means |
| --- | --- |
| `needs:spec` | Accepted, but nothing is built or reviewed until a spec or design doc is agreed |
| `needs:decision` | Waiting on a decision, not on work |
| `needs:UI` | Cannot be built until the UI design exists |

On a pull request, any `needs:` label fails the `Label gate` check, so it cannot merge while one is on. Removing the label clears the check.

### When a pull request needs a spec

Anyone reviewing a pull request can add `needs:spec` when the change makes a design decision nobody has agreed yet. Once it is on, nobody reviews the code: the conversation moves to the spec or design doc, and the label comes off when that merges. Which changes need a spec is decided with the design documentation work, and this section will link to it.

## spa

Which product an issue is for: `spa:humanity`, `spa:jollity`, `spa:scarcity` or `spa:seity`. Set by whoever files it.

## status

One at a time on a pull request, named for who has to act next.

| Label | Means |
| --- | --- |
| `status:waiting-on-review` | Ready for a reviewer. Set automatically whenever a pull request that is not a draft has no status, and removed when it goes back to draft. |
| `status:waiting-on-author` | The author has changes to make. A reviewer sets it when they request changes. |
| `status:ready-to-merge` | Approved, and waiting only to be queued. |
| `status:blocked` | Cannot move until something else lands. Say what in a comment. |

`status:blocked` fails the `Label gate` check, like a `needs:` label. A draft carries no status, and converting a pull request back to draft removes `status:waiting-on-review`.

## size

Set on every pull request from the lines it adds and removes, and kept current as it changes.

| Label | Lines |
| --- | --- |
| `size:XS` | under 10 |
| `size:S` | 10 to 29 |
| `size:M` | 30 to 99 |
| `size:L` | 100 to 499 |
| `size:XL` | 500 to 999 |
| `size:XXL` | 1000 or more |

Lines a reviewer does not read are left out: anything `.gitattributes` marks `linguist-generated`, which GitHub also collapses in the diff, plus lockfiles, Room database schemas and Core Data models. Marking a new generated file there keeps it out of the count. A backport of a vendored host tree carries no size, because its line count says nothing about how much there is to review.

`size:XL` and `size:XXL` are a prompt to split. A change too large to review in one sitting can usually land as a series of smaller pull requests into a feature branch, each reviewed by whoever knows that part, before the branch merges into main. When it genuinely cannot be split, say why in the description.

## ci

Each starts as soon as it is added and follows the branch from then on. The [README](../README.md#working-on-the-ios-host) covers the app builds in detail, and [docs/RELEASE_PROCESS.md](RELEASE_PROCESS.md) covers changesets.

| Label | Runs |
| --- | --- |
| `ci:ios-simulator` | Builds the iOS app, runs its tests and attaches a simulator build |
| `ci:ios-device` | Attaches a signed iOS build for registered devices |
| `ci:android-device` | Attaches an Android APK for any phone |
| `ci:android-emulator-tests` | Runs the Android instrumented tests on an emulator |
| `ci:host-playground-e2e` | Runs the host-playground e2e inside both apps |
| `ci:no-changeset` | Lets the changeset check pass for a change that ships nothing |

The first time a pull request touches an app, a comment lists `ci:ios-simulator`, `ci:ios-device` and `ci:android-device`, whichever build what it touches.

## Outside the families

A few labels are applied by automation that names them, so they keep plain names: `backport-host` on the pull requests `backport-host.yml` opens, `dependencies` on Dependabot's and `update-chain-specs.yml`'s, and `diagnosis-report`, which the playground puts on the issues it opens.

## What is not a label

- **An issue's type, status and target date.** Those are issue fields.
- **A breaking change.** A `!` in the pull request title marks it, and the nightly announcement lists those first.
- **Why an issue was closed.** GitHub records duplicates and issues not planned when the issue is closed.
- **Who reviews.** CODEOWNERS requests reviewers from the paths changed.

## Changing a label

Edit `.github/labels.yml`. Give every label a color, a description that says when to use it, a `target` and an `addedBy`. To rename one, put its old name under `previously`; the rename keeps it on everything that carries it. When the new name already exists, the old label is merged into it instead. A label that is no longer needed goes under `retired` with a reason, and is deleted when the change merges. One whose replacement is not decided yet goes under `pending`, and is left as it is.

A workflow that keys on a label name changes in the same pull request as the label, so both land together.

## Not yet decided

- `Epic`: an issue type, or a feature with sub-issues.
- `enhancement`, `feature request` and `proposal`: replaced by the Feature issue type once issues move to types.
- `R2 blocker` and `R3+`: a target date, a field, or labels.
- `host-work`: an area for both hosts, or retired.
- Making `Label gate` a required check on main, so it holds a merge rather than only reporting.

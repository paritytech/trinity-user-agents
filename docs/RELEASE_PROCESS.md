# Release process

The `@parity/truapi` and `@parity/truapi-host` npm packages are published by
[`paritytech/npm_publish_automation`](https://github.com/paritytech/npm_publish_automation).
We never run `npm publish` locally or from a personal account; the
`Release` workflow in `.github/workflows/release.yml` packs the packages
and dispatches the automation.

Releases happen via a dedicated **release PR**. Nothing publishes
automatically on a normal feature merge — only PRs whose title (and
therefore squashed commit subject) starts with `release:` trigger a
publish, and only when they bump the package version.

## How to release

### 1. Bump the package version

```bash
npm run changeset            # interactive: pick patch / minor / major + a short summary
npm run version-packages     # consumes the changeset, bumps package.json + writes CHANGELOG.md
```

A change that reaches a published artifact carries its own changeset, added by
the pull request that makes it: the `Changeset guard` job in CI fails a pull
request that touches the sources the npm packages or the prebuilt CLI are built
from without adding one. This includes code generators, dependency manifests
and lockfiles, and package build scripts and configuration. Deleting, editing
or renaming an existing changeset, or adding `.changeset/README.md`, does not
satisfy the guard.

A change that ships nothing can use the `no-changeset` label. After adding the
label or correcting a title to `release:`, select **Re-run failed jobs** on CI;
the guard reads the current PR metadata, so no new commit or full CI run is
needed. Repository maintainers must create the label once before using it:

```bash
gh label create no-changeset --color d4c5f9 --description "No published artifact changes; changeset not required"
```

The first command writes a markdown file under `.changeset/`; the second
consumes it, bumps the selected package `package.json`, appends the package
`CHANGELOG.md`, deletes the changeset file, and then runs
`scripts/sync-release-versions.mjs`. That script keeps
`rust/crates/truapi/Cargo.toml` and the host package's `@parity/truapi`
dependency aligned with `js/packages/truapi/package.json`; the command then
refreshes `package-lock.json`. A protocol release should therefore include the
`@parity/truapi` package, its changelog, the Cargo version, the host dependency,
and the lockfile. A host-runtime-only release can bump
`@parity/truapi-host` without changing the Rust crate version.

### 2. Cut the protocol version

Run `scripts/cut-version.sh` after the package bump to crystallize wire types,
take an explorer snapshot under the new version, and generate the root
`CHANGELOG.md`:

```bash
scripts/cut-version.sh            # crystallize next/, snapshot, changelog
scripts/cut-version.sh --dry-run  # preview without making changes
```

The order matters. The script reads the version from
`js/packages/truapi/package.json`; running it before `version-packages` would
overwrite the previous release's explorer snapshot.

### 3. Open a release PR

Commit the resulting diff and open a PR using the **release** template:

```
https://github.com/paritytech/trinity-user-agents/compare/main...<your-branch>?template=release.md
```

The PR title must start with `release:`. Convention:

```
release: @parity/truapi 0.1.1
release: @parity/truapi-host 0.1.1
release: @parity/truapi 0.5.0, @parity/truapi-host 0.2.0
release: @parity/android-host 0.1.0
release: @parity/truapi 0.5.0, @parity/ios-host 0.5.0, @parity/android-host 0.1.0
```

Separate multiple package/version targets with commas. The workflow validates
each declared version against its package manifest and publishes every target
whose version is not already on npm in the same automation run.
Include every npm package whose version was bumped, including dependent
packages bumped by Changesets. If a version is deliberately left unpublished,
record that exact `package@version` and a reason in
[`.github/registry-drift-exceptions.json`](../.github/registry-drift-exceptions.json).
For example, an entry could be
`"@parity/example@1.2.3": "Held while its consumer catches up"`.
An exception applies only to that version; the next bump is checked normally,
so a package that should not be published at all is better marked `private` in
its own manifest, which the check skips outright.
The exception list is empty by default and does not control publishing.

### 4. Get the PR reviewed and merged

Merge via squash merge (the repo's default). The squash commit subject
defaults to the PR title, so the `release:` prefix carries over to
`main`. **Don't rewrite the squash subject in GitHub's merge dialog** —
the workflow checks the commit subject, and dropping the `release:`
prefix will silently skip the publish. If that does happen, open a
follow-up `release:` PR with any trivial change (a CHANGELOG note tweak,
say); the tag-already-exists guard makes re-runs safe.

The merge queue checks the release against the queued merge result, so there
is no required "rebase immediately before enqueueing" step. A changeset ahead
of or grouped with the release can still reject the group. Rebuild the release
from current `main` as described below, then enqueue it again; other PRs in a
rejected group may also need to be requeued.

### 5. Watch the publish

On merge, CI runs as usual. When CI passes, the `Release` workflow:

1. Confirms the commit subject starts with `release:`.
2. Reads each package/version target from the comma-separated release subject
   and validates it against the corresponding package manifest.
3. Asks npm whether each `<package>@<version>` already exists. Versions the
   registry already serves are skipped, so re-runs are idempotent.
4. Builds generated sources and the host WASM bundle, packs the tarballs, and
   dispatches to `npm_publish_automation`.
5. Polls the registry until every version being released is installable, for up
   to ten minutes.
6. Creates and pushes tags and publishes GitHub Releases, for the confirmed
   versions only.
7. Opens a bump issue on each repository that
   [`.github/consumers.json`](../.github/consumers.json) lists under a confirmed
   package.

An iOS release then uploads the XCFramework, creates a bare `<version>` tag from
a commit whose `Package.swift` records the live asset URL and checksum, and
builds that tag from a clean clone before pushing it. The workflow also opens a
generated manifest PR to keep `main` current. It dispatches CI explicitly
because a PR created with `GITHUB_TOKEN` does not start another workflow.

The dispatch in step 4 returns as soon as GitHub accepts it, which is why step 5
exists: the registry is the only proof a version landed. A green `Release` run
therefore means both packages are installable, not merely that the publish was
requested.

You can still watch the dispatched run under
[`paritytech/npm_publish_automation` Actions](https://github.com/paritytech/npm_publish_automation/actions),
which is where a publish failure reports its reason.

### The native artifacts

The npm packages are not the only release targets. `@parity/ios-host` and
`@parity/android-host` name artifacts that live outside npm, and each is
published by its own job once the release job succeeds.

`@parity/truapi <version>` also publishes the prebuilt `truapi-host` CLI
binaries, because `rust/crates/truapi-host-cli/Cargo.toml` tracks the protocol
version (`scripts/sync-release-versions.mjs` keeps it there, and
`npm run check-release-versions` fails the release if it drifts). The
`release-cli` workflow builds `aarch64-apple-darwin`,
`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` natively, uploads
each archive and its `.sha256` to the `@parity/truapi@<version>` release, and
only then moves the `truapi-host-cli-stable` pointer that the installer and the
in-binary updater read. There is no separate release subject entry for it.

`@parity/ios-host <version>` publishes two artifacts, because SwiftPM splits a
package across both. The xcframework goes to the `@parity/ios-host@<version>`
GitHub release as an asset. The Swift sources go to a plain semver tag named
`<version>`, whose commit carries the generated bindings, the FFI headers and
the container bundle alongside a `Package.swift` pointing at that asset. The
generated files are git-ignored on a branch, and SwiftPM resolves source
targets from the git checkout with no way to fetch them from an asset, so the
tag is what a consumer can actually resolve. Apps therefore pin the semver tag:

```swift
.package(url: "https://github.com/paritytech/trinity-user-agents", exact: "0.12.0")
```

`ios/truapi-host/scripts/tag-release.sh` builds that commit, reading the
required paths out of `Package.swift` so the file set cannot drift from what
SwiftPM looks for. The job clones the tag and compiles it against the published
asset before pushing, so a tag that cannot be resolved is never published.

`@parity/android-host <version>` publishes the Android host AAR as
`io.parity:truapi-host-android:<version>` to GitHub Packages. The job
cross-compiles `libtruapi.so` for arm64-v8a, armeabi-v7a and x86_64,
regenerates the UniFFI Kotlin bindings from the same source, and publishes the
AAR with the native libraries inside it, so consumers need only Gradle. Nothing
in the tree records the Android version, so there is no manifest to bump: the
release subject is the only place it appears. See
[`android/truapi-host/README.md`](../android/truapi-host/README.md) for the
consumer setup and the credentials a consumer needs.

Both native jobs are also reachable by a manual `workflow_dispatch` run with a
version input. For iOS that is also how a pre-release is cut: dispatching
`release-ios` from a branch with a version like `0.12.0-beta.1` publishes an
asset and a tag an app can pin, which is how an unmerged host change gets
tested in the app. A dispatched run leaves every branch alone, because it
passes no `manifest_branch`. Neither job has a tag trigger, because a tag push
cannot use the `workflow_run` gate on green CI and would be an unverified path
to a registry.

### Notifying the consumers

[`.github/consumers.json`](../.github/consumers.json) maps each package to the
repositories that consume it. `@parity/truapi` is consumed by
`paritytech/dotli-community` and `paritytech/product-sdk`; `@parity/truapi-host`
by `paritytech/dotli-community`. Every repository under a published package
receives an issue naming the versions it should move to, so subscribing a
repository to another package means adding it to that package's list.

Every release target appears in that file, an unsubscribed one with an empty
list, which is how `@parity/ios-host` and `@parity/android-host` sit today.
Publishing a package the file does not mention at all is a warning on the run,
since that means a new release target nobody wired up.

A release that publishes several packages a repository pins produces one issue
covering all of them, labelled `truapi-release`. An earlier notification on the
same repository is commented on and closed as superseded only once the new issue
covers every package it named at an equal or higher version, so a
`@parity/truapi`-only release leaves an issue that still tracks an unbumped
`@parity/truapi-host` open. A release cut from a release branch cannot close the
issue for a newer version, and re-running a release finds its own issue and adds
nothing.

Both of those decisions read the consumer's open issues, and GitHub serves that
listing from a replica that trails a write by a few seconds. Nothing is closed on
the strength of the listing alone: each candidate is re-read by number, which is
authoritative, so an issue another run already closed is left alone rather than
commented on twice. The listing can still omit an issue created seconds earlier,
which would produce a second issue for the same release. Runs minutes apart never
see that, and the `release` concurrency group means two releases cannot overlap,
so it is only reachable by firing the manual rehearsal twice in a row.

The routing and the issue text live in
[`scripts/lib/consumer-notifications.mjs`](../scripts/lib/consumer-notifications.mjs),
unit-tested under `npm run test:scripts`;
[`scripts/notify-consumers.mjs`](../scripts/notify-consumers.mjs) makes the API
calls. Authentication is the `truapi-release-notifications` GitHub App, owned by
`paritytech` and installed on the consumer repositories, which is why the issues
are opened by an app rather than by a person and why there is no token to rotate.
It is deliberately not installed on `paritytech/trinity-user-agents`, which holds only the
app's id and private key, as `CONSUMER_APP_ID` and `CONSUMER_APP_KEY`, and mints
an installation token per run.

That token is scoped to the repositories the run will write to, resolved from the
config, and to the app's `Issues` permission alone, at the read and write level
GitHub's API spells `issues: write`. Reading is part of that one level, which the
script needs in order to find the issues it supersedes. The token carries no
reach over the rest of the installation. The two lists therefore have to agree: a repository named in the
config that the app is not installed on fails the whole job rather than that one
repository, which is deliberate, since a consumer nobody can reach is a
configuration error and not a silent omission. Adding a consumer means an entry in
the config and an app installation on that repository.

To rehearse a change to any of that, run the `notify-consumers` workflow by hand
with a `published` value of `@parity/truapi||0.10.0|@parity/truapi@0.10.0` and a
`target_repo` of a scratch repository the app is installed on. It opens the real
issue there instead of on a consumer. The app is the reason the scratch repository
has to be one of ours rather than a personal one.

## Safety properties

- A feature PR that accidentally bumps `package.json` will **not**
  trigger a publish — only `release:` PRs do.
- A `release:` PR that forgets to bump package versions will be skipped at the
  version-already-on-npm check, not silently re-publish over an existing
  version. That check queries the registry, not the git tags.
- A publish that never reaches npm fails the release, so no tag or GitHub
  Release is created for a version nobody can install. If one package lands and
  another does not, only the one that landed is tagged and the run still fails.
  Re-run it once the publish is fixed; the tag and version checks make that
  safe.
- A release is one unit. The iOS and Android jobs run only after the release job
  succeeds, so a release naming both npm packages and `@parity/ios-host` or
  `@parity/android-host` publishes no XCFramework and no AAR while npm is
  unconfirmed. Re-running covers both. A native-only release is unaffected, since
  it has no npm package to confirm.
- A release that does not name `@parity/android-host` publishes no AAR. The
  Android artifact is opt-in per release, like the iOS one, so a playground-only
  npm bump does not cut an Android version.
- The Android job has no equivalent of the npm "is this version already
  published?" pre-flight check, so re-running a release re-attempts the publish
  and relies on the registry to refuse a duplicate coordinate. Prefer bumping the
  version over re-running a release whose Android publish already succeeded.
- A `release:` PR with mismatched `js/packages/truapi/package.json` and
  `rust/crates/truapi/Cargo.toml` versions is blocked at PR time by the
  `Release version check` workflow.
- `Release version check` also requires consumed changesets for a `release:`
  PR without an npm version bump, including a publish retry or native-only
  release. It remains alongside `Release guard`, which detects bumps without
  relying on the title and also runs in the merge queue. Both checks give the
  same recovery guidance for an unmerged release branch.
- A release commit publishes the versions its changesets computed, so those
  changesets have to be the ones present when it merges. The `Release guard` job
  in CI holds that: whenever a change bumps a published package version,
  `.changeset/` must contain no release markdown files (`README.md` is ignored).
  It runs in the merge queue as well as on the pull request, so it sees the
  merge result rather than the branch, and a release
  branch that went stale while it waited fails before the bump reaches `main`.
- Rebuild a stale release branch by resetting it to `main` and running
  `npm run version-packages` and `scripts/cut-version.sh` again. Rebasing and
  rerunning does not work: `version-packages` consumes the changeset files it
  reads and bumps from whatever version the manifests currently hold, so a
  second run on top of the first compounds the bump and writes a changelog entry
  for a version nobody publishes.
- `Registry drift` compares every published manifest version against npm on a
  daily schedule, excluding documented exceptions for specific versions, and
  opens an issue when the registry does not serve one. Manual runs also check
  the default branch. It updates one open issue when the report changes and
  leaves an identical report alone. Close the issue once resolved; there is
  no daily comment loop. Registry lookup errors fail the job without filing or
  updating an issue. A release commit is merged before the publish it describes is attempted and
  nothing reverts it, so the repository can otherwise read as released while
  consumers still install the previous version.
- Publishing uses the default `GITHUB_TOKEN`. The only other credentials are the
  org-level `NPM_PUBLISH_AUTOMATION_TOKEN` that the automation itself relies on,
  and the notification app's client id and private key, which are read by the
  notification job alone and can reach nothing in this repository.
- The consumer notifications are the last step and announce only the versions the
  registry confirmed. A repository that cannot be reached is a warning and does
  not stop the others, though the run still ends red so the failure is visible.
  Nothing about the publish depends on it: tags and GitHub Releases already
  exist by then, and nothing declares `needs: notify-consumers`, so the iOS and
  Android publishes are unaffected as well.
- Recover a failed notification with "Re-run failed jobs" rather than "Re-run all
  jobs". A full re-run restarts the release job, which finds the versions already
  on npm and so leaves `published` empty, at which point the notification job is
  skipped by its own `if:` and the notification is never sent, under a green run.
  Re-running only the failed job reuses the original outputs and retries the
  notification. Retrying is safe either way, because an issue that already
  announces the release is left alone.

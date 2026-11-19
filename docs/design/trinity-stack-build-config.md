---
title: "Trinity stack build config"
type: design
status: draft
author: Mikelle
created: 2026-11-18
pr: 2200
---

# Trinity stack build config

_A proposal for the config file of [#1899](https://github.com/paritytech/trinity-user-agents/issues/1899): how a host
build names the stack it runs and whether that stack may use reserved values. It covers "Build configuration" in
[RFC 2, Hosts load stacks from descriptors](https://github.com/paritytech/trinity-user-agents/pull/1564). It differs
from the RFC in one place, which is the first open question: the build does not fetch._

## Summary

A build config is one JSON file per build. It names the one stack the build runs, with its link and its reserved-values
policy. The signed descriptor copy of the stack is the file `<name>.json` beside the config. A build selects a config by
path, and the config decides which stack the core runs, so no Swift, Kotlin or TypeScript code picks it.

## The format

Parity nightly:

```json
{
  "$v": 1,
  "name": "paseo-next-v2",
  "link": "polkadotapp://stack?key=0x2b89d8ca74aa2e11ab55f5af1168cd6929b88d1998aecd979b42cfc1078d5377&url=https%3A%2F%2Fstack-paseo-next.parity-testnet.parity.io%2F.well-known%2Ftrinity-stack.json",
  "reservedValues": {
    "allowed": false,
    "extraGenesisHashes": [],
    "extraTlds": []
  }
}
```

- `name` matches `[a-z0-9][a-z0-9-]*` and names the copy `<name>.json`. The pattern has no dot, so a copy can never be
  named like a config.
- `link` is the stack link and must carry a key.
- `reservedValues` has the fields of `ReservedValuesPolicy` in
  [#2024](https://github.com/paritytech/trinity-user-agents/pull/2024). `allowed` is required and the two lists are
  optional. An entry of `extraGenesisHashes` is 32 bytes as 0x-prefixed hex. The core refuses `allowed: true` when
  either list has an entry, and an extra TLD no stack can have.
- A config with an unknown field or any `$v` but 1 is refused.
- The fields RFC 2 holds fixed for a stack allowed reserved values (genesis hashes, TLD, `assets`, `contracts`, `dotns`,
  `coinage` and the relay checkpoint) need no field here. The core reads them from the bundled copy.

Parity preview:

```json
{
  "$v": 1,
  "name": "previewnet",
  "link": "polkadotapp://stack?key=0x4d43362f03d5511632a210632d1fb7760b5a663580f12ffca34619f79357e239&url=https%3A%2F%2Fpreviewnet.substrate.dev%2F.well-known%2Ftrinity-stack.json",
  "reservedValues": { "allowed": false }
}
```

Parity mainnet fork. The link is a placeholder, because `stacks/` has no link or descriptor for polkadot-test:

```json
{
  "$v": 1,
  "name": "polkadot-test",
  "link": "polkadotapp://stack?key=<operator key>&url=<descriptor url>",
  "reservedValues": { "allowed": true }
}
```

PCF production:

```json
{
  "$v": 1,
  "name": "polkadot",
  "link": "polkadotapp://stack?key=0xdcddb708759dade2e3850368ce0add2072f424ba53f1a2128f3339d646cffe28&url=https%3A%2F%2Fstack.api.pcf.network%2F.well-known%2Ftrinity-stack.json",
  "reservedValues": { "allowed": true }
}
```

## Who checks a config

The CLI is the one validator of the whole format: the name pattern, a link with a key, unknown fields and `$v`. The
Swift, Kotlin and TypeScript readers decode strictly and refuse what they cannot decode. The Xcode and Gradle build
steps only copy files.

A build step cannot tell that a copy is unsigned, signed by another key than the config's link, or refused by the
config's policy. The core finds out when it starts. Each app's tests start the core on every bundled stack today, and
with one config per build they would start only the stack of the build under test. A test in `truapi-host-cli` takes
their place: it reads every `stacks/*.build.json` and runs the core's `Stack::bundled` on its link, copy and policy, so
every pull request checks every Parity config with no released binary. The same test asserts that a config whose `name`
has a `stacks/<name>.link` carries the same link.

## Where it lives and how a build selects one

Parity's configs sit beside the copies: `stacks/nightly.build.json`, `stacks/preview.build.json` and
`stacks/mainnet-fork.build.json`. A config in `stacks/` ends in `.build.json`, which the refresh script relies on.
`stacks/<name>.json` stays as it is.

| Build system | Setting                                                          | Defaults                                                                                    |
| ------------ | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Xcode        | build setting `TRINITY_STACK_BUILD_CONFIG`                       | Debug, DevCI: `preview.build.json`. Nightly, Safetynet: `nightly.build.json`. Release: none |
| Gradle       | Gradle property `truapi.stackBuildConfig.<buildType>`            | debug: `preview.build.json`. nightly, safetynet: `nightly.build.json`. release: none        |
| Web          | env var `TRINITY_STACK_BUILD_CONFIG`, read by the bundler config | set per deployment                                                                          |

For the configurations that have a default, it is the stack they run today. Release runs `polkadot` with reserved values
allowed today and has no default here, so a release build without a config fails instead of shipping a stack nobody
chose with reserved values allowed.

That has a cost inside this repository. Parity's own Release builds have no config until one is written for them: the
TestFlight build (`ios-testflight-distribution.yml`), the Xcode scheme's Profile and Archive actions, and Android's
`:feature:products:impl:testReleaseUnitTest` in `android-app.yml`, whose test tasks depend on `syncStacks`. The iOS and
Android questions at the end ask which config those builds get.

## How PCF supplies its own

PCF keeps a directory outside this repository with `production.build.json` and `polkadot.json`, and passes the config's
path through the setting above. Its pipeline writes `polkadot.json` with `truapi-host stack bundle --config`. The stack,
its key and its publisher are named only in that file, so there is nothing to fork.

## What each build system does

- **iOS.** `hosts/ios/Configs/base.*.xcconfig` set the build setting, and PCF sets it on the `xcodebuild` command line,
  which overrides them. The "Bundle Trinity stacks" phase copies the config to `trinity-stack/stack.build.json` in the
  app's resources and its `<name>.json` beside it, reading the name with `plutil -extract name raw`. It fails on an
  empty setting or a missing copy. `TrUAPIBundledStack` decodes `stack.build.json` when the runtime is first built and
  builds `BundledStack` from it, with no switch on `UNSTABLE`, `NIGHTLY` or `SAFETYNET`. A config that does not decode
  fails the same way a missing stack resource does today: products do not start and the rest of the app runs.
- **Android.** `:bindings:truapi-host` registers one `syncStacks` task per build type. Each reads its Gradle property
  when it runs, parses the config with `JsonSlurper`, and syncs `stacks/stack.build.json` plus its `<name>.json` into
  that build type's assets. `TrUAPIStack` reads `stacks/stack.build.json` and does not map `TestnetEnvironment` to a
  stack.
- **Web.** `@parity/truapi-host` reads no stack files. It takes `ProductRuntimeConfig.stack` from the page, and exports
  the config's type. In dotli, `packages/config/src/loadStack.ts` imports the deployment's config, loads its
  `<name>.json?raw`, and returns `{ pointer: link, descriptor, reservedValues }`, in place of its hand-written table and
  `.trinity/stacks.json`.

The bundled config is named `stack.build.json` because a stack's `name` cannot contain a dot, so no copy can overwrite
it.

`reservedValues` needs #2024. On a core without it, a host passes `allowed` as `allowsReservedValues` and refuses a
config where either list has an entry.

No app build fetches. Copies are committed and refreshed ahead of the build, so a build needs no `truapi-host` binary
and no network. #1899 and RFC 2 both put the fetch in the build step, where `stack bundle` refuses an unsigned or
expired copy. Here the refresh refuses those when it writes the copy, and the test above refuses an unsigned copy on
every pull request. A copy that expires after it was committed is not refused by a build: the core starts on it and
reports it as expired.

When an app update changes a config, the core already handles the stored copy. A new key is a new stack id, so the
stored record of the old one is not read. A tightened policy makes the core ignore a stored copy that breaks it and run
the bundled one. A change of URL alone keeps the stored copy.

## What the CLI changes

`truapi-host stack bundle` gains `--config <file>`. It does what `bundle <link> --out <dir>/<name>.json` does, then
checks the descriptor against the config's policy with `Stack::bundled`, so a testnet that starts naming a Polkadot
genesis hash fails at refresh, not at app start. It also refuses a config with `allowed: true` whose descriptor names no
reserved genesis hash and no reserved TLD, because that config gives up the protection and gains nothing.
`truapi-host stack verify` gains `--config <file>` too: it checks the copy on disk without fetching, for a release
pipeline.

`scripts/update-stacks.sh` also loops over `stacks/*.build.json`, and skips that loop while the released CLI it runs has
no `--config`, as it does for `--skip-same-projection`. `scripts/write-refresh-changeset.sh` selects `stacks/*.json` and
reads each match as a signed descriptor, so its pattern has to leave out `*.build.json`.

The CLI's built-in networks keep `stacks/<name>.link`, so the paseo-next-v2 and previewnet links exist in two files that
write the same copy. The test under "Who checks a config" keeps the two equal on every pull request. Without it, a key
changed in one file fails the weekly refresh, and a URL changed in one file is not noticed at all.

## Rejected alternatives

- TOML: nothing in Xcode's shell tools, Gradle's public API or a browser reads it. JSON has `plutil`, `JsonSlurper` and
  `import`.
- The config names the stack and a `.link` file keeps the pointer: the file that grants `allowed: true` would not show
  the key it grants it to, and changing publisher would not be a config change.
- A list of stacks in one config: RFC 2 runs one stack per build. Of the hosts, only a development deployment of dotli
  and its Docker image offer more than one.
- One file listing all builds: PCF would edit a Parity file, and every build would carry the others' entries.
- The descriptor embedded in the config: every refresh would rewrite the hand-reviewed file.
- A descriptor path field: app resources would be laid out differently from the directory on disk.
- Extending `.trinity/stacks.json`: its values are link strings in the published `@parity/trinity-stack`, and projects
  that are not hosts read it.

## Open questions

For the RFCs:

1. RFC 2 and #1899 have the build step fetch the descriptor. Is a committed copy, refreshed ahead of the build and
   checked before a release, enough? If the build step must fetch, the Xcode and Gradle steps run
   `truapi-host stack bundle --config` themselves, and every app build needs the binary and the network.
2. [RFC 3, Running more than one stack](https://github.com/paritytech/trinity-user-agents/pull/1565) lets a build choose
   whether it accepts Imported stacks. Is that choice a field of this file?

iOS:

3. Which configuration is the mainnet-fork build, and which config does Parity's Release build get? Release runs
   `polkadot` today and nothing runs polkadot-test.
4. Can Release have no default config? The TestFlight workflow and the scheme's Profile and Archive actions build
   Release.
5. Does PCF build from this tree or from polkadot-ios-community, where `$(SRCROOT)/../../stacks` does not exist?
6. The `UNSTABLE`, `NIGHTLY` and `SAFETYNET` flags still pick Firebase keys and chain tables. Should a build fail when
   they disagree with the config?
7. Should a brand be able to set the config in `brand.xcconfig`? The base configs assign after including it, so its
   value would need to be read as `$(BRAND_STACK_BUILD_CONFIG:default=...)`, and it would apply to all five
   configurations.

Android:

8. Which build type is the mainnet-fork build, and can release have no default? CI runs `testReleaseUnitTest`, which
   needs a release config.
9. Per-build-type assets in `:bindings:truapi-host`, or one asset set and a `BuildConfig` field naming the config?
10. Does PCF pass the path as a Gradle property (`gradle.properties` or `-P`), or through `local.properties` and its
    environment fallback, where the app name already comes from?
11. `TestnetEnvironment` still drives `KnownChains` and Remote Config keys. Should a build fail when it disagrees with
    the config, as for the iOS flags?

Web:

12. One development deployment of dotli offers two networks, and its Docker image takes its networks when the container
    starts. Does such a build take several configs, or does its choice of networks stay outside this format?
13. dotli can run pcf-devnet with reserved values allowed, which the table in #1899 does not list. Does it get a config?
14. dotli is its own repository without `stacks/`. Where do its config and copy live, and does it refresh them with the
    `truapi-host` binary or should `trinity-stack update` take `--config`?
15. Does polkadot-desktop read the same file?

Stacks:

16. Who publishes polkadot-test, under which key and URL? The mainnet-fork config waits on the answer.
17. Must `name` equal the descriptor's `id`? They are equal in all four copies in `stacks/`.

Untested assumptions:

- `JsonSlurper` works in a `.kts` build script.
- Per-build-type assets work in the bindings module.
- PCF builds from a checkout that has `stacks/`.

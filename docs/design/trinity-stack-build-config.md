---
title: "Trinity stack build config"
type: design
status: draft
author: Mikelle
created: 2026-11-18
pr: 2200
---

# Trinity stack build config

_How [#1899](https://github.com/paritytech/trinity-user-agents/issues/1899) implements "Build configuration" in
[RFC 2, Hosts load stacks from descriptors](https://github.com/paritytech/trinity-user-agents/pull/1564): the file a
host build names its stack in, and the build step that bundles the stack's descriptor. The RFC is normative; this page
gives the file format and what each build system does._

## Summary

A build config is one JSON file per build. It names the one stack the build runs, with its link and its reserved-values
policy. A build selects a config by path. Its build step fetches and verifies the stack's descriptor with
`truapi-host stack bundle` and bundles the copy with the config, and the app starts the core from the two. No Swift,
Kotlin or TypeScript code picks the stack.

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

- `name` matches `[a-z0-9][a-z0-9-]*` and names the descriptor copy `<name>.json`. The pattern has no dot, so a copy can
  never be named like a config.
- `link` is the stack link and must carry a key, because RFC 2 requires a built-in stack to be signed.
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

## The build step

RFC 2: "The build step fetches and verifies the descriptor and bundles the copy, as `truapi-host stack bundle` does: it
refuses an unsigned or expired copy and never lowers `sequence` below the copy it replaces."

Each host's build runs one command:

```sh
truapi-host stack bundle --config <config> --out <build output directory>
```

It validates the config, fetches the descriptor the link names, verifies it under the link's key, and checks it against
the config's policy with the core's `Stack::bundled`. It then writes `stack.build.json` (the config) and `<name>.json`
(the copy) into the output directory, which the build ships as resources. The command fails, and the build with it, when
the descriptor is unreachable, unsigned, signed by another key, expired, or refused by the policy.

The copy it replaces is `<name>.json` beside the config. The command reads it as the previous copy, so the bundled one
never has a lower `sequence` and keeps the TLD. It does not write beside the config, so a build leaves the source tree
as it was. `scripts/update-stacks.sh` keeps those copies current, as it does today.

The bundled config is named `stack.build.json` because a stack's `name` cannot contain a dot, so no copy can overwrite
it.

A build therefore needs the `truapi-host` binary and the network. CI installs the binary with
`.github/actions/install-truapi-host`, and a developer machine with `scripts/truapi-host-installer.sh`. The build step
fails with a message that names the installer when the binary is missing.

## Where a config lives and how a build selects one

Parity's configs sit in `stacks/`, beside the copies the build step replaces: `stacks/nightly.build.json`,
`stacks/preview.build.json` and `stacks/mainnet-fork.build.json`. A config in `stacks/` ends in `.build.json`, which the
refresh script relies on. `stacks/<name>.json` stays as it is.

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

PCF keeps a directory outside this repository with `production.build.json` and the last `polkadot.json` it bundled, and
passes the config's path through the setting above. Its build runs the same step against PCF's own pointer. The stack,
its key and its publisher are named only in that file, so there is nothing to fork.

## What each build system does

- **iOS.** `hosts/ios/Configs/base.*.xcconfig` set the build setting, and PCF sets it on the `xcodebuild` command line,
  which overrides them. The "Bundle Trinity stacks" phase runs the build step with the app's resources directory
  `trinity-stack/` as the output. It fails on an empty setting. `TrUAPIBundledStack` decodes `stack.build.json` when the
  runtime is first built and builds `BundledStack` from it and the copy, with no switch on `UNSTABLE`, `NIGHTLY` or
  `SAFETYNET`. A config that does not decode fails the same way a missing stack resource does today: products do not
  start and the rest of the app runs.
- **Android.** `:bindings:truapi-host` registers one `syncStacks` task per build type. Each reads its Gradle property
  when it runs and runs the build step with that build type's `stacks/` assets directory as the output. `TrUAPIStack`
  reads `stacks/stack.build.json` and does not map `TestnetEnvironment` to a stack.
- **Web.** `@parity/truapi-host` reads no stack files. It takes `ProductRuntimeConfig.stack` from the page, and exports
  the config's type. In dotli, the build runs the build step, and `packages/config/src/loadStack.ts` imports the bundled
  config and `<name>.json?raw` and returns `{ pointer: link, descriptor, reservedValues }`, in place of its hand-written
  table and `.trinity/stacks.json`.

`reservedValues` needs #2024. On a core without it, a host passes `allowed` as `allowsReservedValues` and refuses a
config where either list has an entry.

When an app update changes a config, the core already handles the stored copy. A new key is a new stack id, so the
stored record of the old one is not read. A tightened policy makes the core ignore a stored copy that breaks it and run
the bundled one. A change of URL alone keeps the stored copy.

## Checks before a build

A pull request that changes a config or a copy in `stacks/` is checked before any app is built. A test in
`truapi-host-cli` reads every `stacks/*.build.json`, validates it, and runs `Stack::bundled` on its link, the copy
beside it and its policy. Each app's tests start the core on every bundled stack today, and with one config per build
they would start only the stack of the build under test, so this test takes their place. It also asserts that a config
whose `name` has a `stacks/<name>.link` carries the same link.

The CLI is the one validator of the whole format: the name pattern, a link with a key, unknown fields and `$v`. The
Swift, Kotlin and TypeScript readers decode strictly and refuse what they cannot decode.

## What the CLI changes

`truapi-host stack bundle` gains `--config <file>`, with `--out` naming a directory. It reads the copy beside the config
as the previous copy, does what `bundle <link> --out <file>` does, and adds two checks. It checks the descriptor against
the config's policy, so a testnet that starts naming a Polkadot genesis hash fails the build, not the app. It refuses a
config with `allowed: true` whose descriptor names no reserved genesis hash and no reserved TLD, because that config
gives up the protection and gains nothing.

`scripts/update-stacks.sh` also loops over `stacks/*.build.json` with `stacks/` as the output, and skips that loop while
the released CLI it runs has no `--config`, as it does for `--skip-same-projection`.
`scripts/write-refresh-changeset.sh` selects `stacks/*.json` and reads each match as a signed descriptor, so its pattern
has to leave out `*.build.json`.

The CLI's built-in networks keep `stacks/<name>.link`, so the paseo-next-v2 and previewnet links exist in two files that
write the same copy. The test under "Checks before a build" keeps the two equal on every pull request. Without it, a key
changed in one file fails the weekly refresh, and a URL changed in one file is not noticed at all.

## Rejected alternatives

- TOML: the app reads the bundled config at start, and Foundation, the Android SDK and a browser decode JSON but not
  TOML.
- The config names the stack and a `.link` file keeps the pointer: the file that grants `allowed: true` would not show
  the key it grants it to, and changing publisher would not be a config change.
- A list of stacks in one config: RFC 2 runs one stack per build. Of the hosts, only a development deployment of dotli
  and its Docker image offer more than one.
- One file listing all builds: PCF would edit a Parity file, and every build would carry the others' entries.
- Committed copies with no fetch in the build: a copy that expired, or whose stack was wiped, since the last refresh
  would ship.
- The descriptor embedded in the config: every refresh would rewrite the hand-reviewed file.
- A descriptor path field: app resources would be laid out differently from the directory on disk.
- Extending `.trinity/stacks.json`: its values are link strings in the published `@parity/trinity-stack`, and projects
  that are not hosts read it.

## Open questions

For the RFCs:

1. [RFC 3, Running more than one stack](https://github.com/paritytech/trinity-user-agents/pull/1565) lets a build choose
   whether it accepts Imported stacks. Is that choice a field of this file?

Every host:

2. A build with no network fails. May a debug build fall back to the copy beside the config when the descriptor is
   unreachable, with a warning? A release build may not.
3. Which `truapi-host` version does a build use: the latest stable release, or one pinned beside the config?

iOS:

4. Which configuration is the mainnet-fork build, and which config does Parity's Release build get? Release runs
   `polkadot` today and nothing runs polkadot-test.
5. Can Release have no default config? The TestFlight workflow and the scheme's Profile and Archive actions build
   Release.
6. Does PCF build from this tree or from polkadot-ios-community, where `$(SRCROOT)/../../stacks` does not exist?
7. The `UNSTABLE`, `NIGHTLY` and `SAFETYNET` flags still pick Firebase keys and chain tables. Should a build fail when
   they disagree with the config?
8. Should a brand be able to set the config in `brand.xcconfig`? The base configs assign after including it, so its
   value would need to be read as `$(BRAND_STACK_BUILD_CONFIG:default=...)`, and it would apply to all five
   configurations.

Android:

9. Which build type is the mainnet-fork build, and can release have no default? CI runs `testReleaseUnitTest`, which
   needs a release config.
10. Per-build-type assets in `:bindings:truapi-host`, or one asset set and a `BuildConfig` field naming the config?
11. Does PCF pass the path as a Gradle property (`gradle.properties` or `-P`), or through `local.properties` and its
    environment fallback, where the app name already comes from?
12. `TestnetEnvironment` still drives `KnownChains` and Remote Config keys. Should a build fail when it disagrees with
    the config, as for the iOS flags?

Web:

13. One development deployment of dotli offers two networks, and its Docker image takes its networks when the container
    starts. Does such a build take several configs, or does its choice of networks stay outside this format?
14. dotli can run pcf-devnet with reserved values allowed, which the table in #1899 does not list. Does it get a config?
15. dotli is its own repository without `stacks/`. Where do its config and the copy its build replaces live, and does
    its build run the `truapi-host` binary or should `trinity-stack update` take `--config`?
16. Does polkadot-desktop read the same file?

Stacks:

17. Who publishes polkadot-test, under which key and URL? The mainnet-fork config waits on the answer.
18. Must `name` equal the descriptor's `id`? They are equal in all four copies in `stacks/`.

Untested assumptions:

- An Xcode script phase and a Gradle task can run `truapi-host` and reach the network in every CI job that builds an
  app.
- Per-build-type assets work in the bindings module.
- PCF builds from a checkout that has `stacks/`.

---
title: "Trinity stack build config"
type: design
status: draft
author: Mikelle
created: 2026-11-18
pr: 2200
---

# Trinity stack build config

## Summary

[RFC 2](https://github.com/paritytech/trinity-user-agents/pull/1564) says what a build's configuration holds: the
stack's pointer, a bundled copy of its descriptor, and whether it may use reserved values. It gives no file format. This
page proposes one, for [#1899](https://github.com/paritytech/trinity-user-agents/issues/1899).

Each host picks its stack in code today: a switch on compiler flags on iOS, a mapping from `TestnetEnvironment` on
Android, a hand-written table in dotli. Shipping another stack means changing that code.

A build config moves the choice into one JSON file per build. The file names the one stack the build runs, its link and
its reserved-values policy. A build selects a config by path. Its build step fetches and verifies the stack's descriptor
and bundles the copy with the config, and the app starts the core from the two.

## The format

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

- `name` matches `[a-z0-9][a-z0-9-]*` and names the descriptor copy `<name>.json`.
- `link` is the stack link. It must carry a key, because RFC 2 requires a built-in stack to be signed.
- `reservedValues` has the fields of `ReservedValuesPolicy` in
  [#2024](https://github.com/paritytech/trinity-user-agents/pull/2024). `allowed` is required and the two lists are
  optional. The core refuses `allowed: true` when either list has an entry.
- A config with an unknown field or any `$v` but 1 is refused.

The values RFC 2 pins for a stack allowed reserved values need no field. The core takes them from the bundled copy.

The four builds in #1899:

| Build               | `name`          | `allowed` | Link                                                  |
| ------------------- | --------------- | --------- | ----------------------------------------------------- |
| Parity nightly      | `paseo-next-v2` | `false`   | as in `stacks/paseo-next-v2.link`                     |
| Parity preview      | `previewnet`    | `false`   | as in `stacks/previewnet.link`                        |
| Parity mainnet fork | `polkadot-test` | `true`    | none yet, because no polkadot-test stack is published |
| PCF production      | `polkadot`      | `true`    | as in `stacks/polkadot.link`                          |

## The build step

Each host's build runs one command:

```sh
truapi-host stack bundle --config <config> --out <build output directory>
```

It validates the config, fetches the descriptor, verifies it under the link's key and checks it against the config's
policy with the core's `Stack::bundled`. It writes `stack.build.json` (the config) and `<name>.json` (the copy) into the
output directory, which the build ships as resources. A `name` has no dot, so no copy can overwrite the config. The
command fails, and the build with it, when the descriptor is unreachable, unsigned, signed by another key, expired or
refused by the policy.

`<name>.json` beside the config is the previous copy: the bundled one never has a lower `sequence` and keeps its TLD.
The command does not write beside the config, so a build leaves the source tree as it was. `scripts/update-stacks.sh`
keeps those copies current.

A build needs the `truapi-host` binary and the network. CI installs the binary with
`.github/actions/install-truapi-host`, a developer with `scripts/truapi-host-installer.sh`.

## Selecting a config

Parity's configs sit in `stacks/` beside the copies: `nightly.build.json`, `preview.build.json` and
`mainnet-fork.build.json`.

| Build system | Setting                                                          | Defaults                                                          |
| ------------ | ---------------------------------------------------------------- | ----------------------------------------------------------------- |
| Xcode        | build setting `TRINITY_STACK_BUILD_CONFIG`                       | Debug, DevCI: preview. Nightly, Safetynet: nightly. Release: none |
| Gradle       | Gradle property `truapi.stackBuildConfig.<buildType>`            | debug: preview. nightly, safetynet: nightly. release: none        |
| Web          | env var `TRINITY_STACK_BUILD_CONFIG`, read by the bundler config | set per deployment                                                |

Each default is the stack that configuration runs today. Release has none, so a release build without a config fails
instead of shipping a stack nobody chose with reserved values allowed. Until the owners choose one, that also fails
Parity's own Release builds: TestFlight, the Xcode scheme's Profile and Archive actions, and Android's release unit
tests in CI.

PCF keeps `production.build.json` and the last `polkadot.json` it bundled in a directory outside this repository, and
passes the config's path through the same setting. Its stack, key and publisher are named only in that file, so there is
nothing to fork.

## What each host does

- **iOS.** The xcconfigs in `hosts/ios/Configs/` set the build setting, and the `xcodebuild` command line overrides
  them. The "Bundle Trinity stacks" phase runs the build step into the app's `trinity-stack/` resources.
  `TrUAPIBundledStack` builds `BundledStack` from `stack.build.json` and the copy, with no switch on compiler flags. A
  config that does not decode fails as a missing stack resource does today: products do not start and the rest of the
  app runs.
- **Android.** `:bindings:truapi-host` registers one `syncStacks` task per build type, which runs the build step into
  that build type's `stacks/` assets. `TrUAPIStack` reads `stacks/stack.build.json` and does not map
  `TestnetEnvironment` to a stack.
- **Web.** `@parity/truapi-host` reads no stack files. It takes `ProductRuntimeConfig.stack` from the page and exports
  the config's type. dotli's build runs the build step and loads the bundled config and copy, in place of its
  hand-written table and `.trinity/stacks.json`.

`reservedValues` needs #2024. On a core without it, a host passes `allowed` as `allowsReservedValues` and refuses a
config where either list has an entry.

An app update that changes a config needs no migration. A new key is a new stack id, so the stored record of the old one
is not read. A tighter policy makes the core ignore a stored copy that breaks it and run the bundled one. A new URL
alone keeps the stored copy.

## CLI, scripts and checks

- `truapi-host stack bundle` gains `--config <file>`, with `--out` naming a directory. Beyond what `bundle <link>`
  checks, it checks the descriptor against the config's policy, so a testnet that starts naming a Polkadot genesis hash
  fails the build, not the app. It refuses `allowed: true` for a descriptor that names no reserved genesis hash and no
  reserved TLD, because that config gives up the protection and gains nothing.
- The CLI is the one validator of the format. The Swift, Kotlin and TypeScript readers decode strictly and refuse what
  they cannot decode.
- A `truapi-host-cli` test reads every `stacks/*.build.json`, validates it and runs `Stack::bundled` on its link, its
  policy and the copy beside it. Each app's tests start the core on every bundled stack today. With one config per build
  they would cover one stack, so this test takes their place.
- The CLI's built-in networks keep `stacks/<name>.link`, so the paseo-next-v2 and previewnet links exist in two files.
  The same test asserts they are equal.
- `scripts/update-stacks.sh` also refreshes through each `stacks/*.build.json`, and `scripts/write-refresh-changeset.sh`
  leaves `*.build.json` out of the descriptors it reads.

## Rejected alternatives

- TOML: the app reads the bundled config at start, and Foundation, the Android SDK and a browser decode JSON but not
  TOML.
- The config names the stack and a `.link` file keeps the pointer: the file that grants `allowed: true` would not show
  the key it grants it to.
- A list of stacks in one config: RFC 2 runs one stack per build. Only a development deployment of dotli and its Docker
  image offer more than one.
- One file listing all builds: PCF would edit a Parity file, and every build would carry the others' entries.
- Committed copies with no fetch in the build: a copy that expired since the last refresh would ship.
- The descriptor embedded in the config: every refresh would rewrite the hand-reviewed file.
- Extending `.trinity/stacks.json`: its values are link strings in the published `@parity/trinity-stack`, and projects
  that are not hosts read it.

## Open questions

For the RFCs:

1. [RFC 3, Running more than one stack](https://github.com/paritytech/trinity-user-agents/pull/1565) lets a build choose
   whether it accepts Imported stacks. Is that choice a field of this file?

iOS and Android:

2. A build with no network fails. May a debug build fall back to the copy beside the config, with a warning?
3. Which `truapi-host` version does a build use: the latest stable release, or one pinned beside the config?
4. Which config do Parity's Release builds get, and which configuration is the mainnet-fork build? Release runs
   `polkadot` today and nothing runs polkadot-test.
5. The `UNSTABLE`, `NIGHTLY` and `SAFETYNET` flags on iOS and `TestnetEnvironment` on Android still pick Firebase keys
   and chain tables. Should a build fail when they disagree with the config?
6. Does PCF build iOS from this tree or from polkadot-ios-community, which has no `stacks/`?

Web:

7. One development deployment of dotli offers two networks, and its Docker image takes its networks when the container
   starts. Does such a build take several configs, or does its choice of networks stay outside this format?
8. dotli can run pcf-devnet with reserved values allowed, which the table in #1899 does not list. Does it get a config?
9. dotli is its own repository without `stacks/`. Where do its config and previous copy live, and does its build run the
   `truapi-host` binary or should `trinity-stack update` take `--config`?
10. Does polkadot-desktop read the same file?

Stacks:

11. Who publishes polkadot-test, under which key and URL? The mainnet-fork config waits on the answer.
12. Must `name` equal the descriptor's `id`? They are equal in all four copies in `stacks/`.

Not tried yet: running `truapi-host` with network access from an Xcode script phase and from a Gradle task in every CI
job that builds an app, and per-build-type assets in the Android bindings module.

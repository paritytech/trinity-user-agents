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

[RFC 2](https://github.com/paritytech/trinity-user-agents/pull/1564) lists what a build's configuration holds: the
stack's pointer, a bundled copy of its descriptor, and whether the stack may use reserved values. It does not define a
file format. This page proposes one for [#1899](https://github.com/paritytech/trinity-user-agents/issues/1899).

Each host currently picks its stack in code. iOS switches on compiler flags, Android maps `TestnetEnvironment` to a
stack, and dotli has a hand-written table. To ship another stack, someone has to change that code.

With a build config, the choice is in a JSON file. There is one file per build and it names one stack: its name, its
link and its reserved-values policy. The build is given the path to the file. The build step downloads the stack's
descriptor, verifies it, and puts the descriptor and the config into the app. The app starts the core from those two
files.

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

- `name` matches `[a-z0-9][a-z0-9-]*`. The descriptor copy is saved as `<name>.json`.
- `link` is the stack link. It must include the key, because RFC 2 requires a built-in stack to be signed.
- `reservedValues` has the same fields as `ReservedValuesPolicy` in
  [#2024](https://github.com/paritytech/trinity-user-agents/pull/2024). `allowed` is required. The two lists are
  optional. If `allowed` is `true`, both lists must be empty, or the core refuses the policy.
- A config with an unknown field, or with `$v` other than 1, is refused.

RFC 2 pins some values for a stack that may use reserved values, such as its genesis hashes, TLD and contracts. The
config does not list them. The core reads them from the bundled copy.

The builds listed in #1899:

| Build               | `name`          | `allowed` | Link                              |
| ------------------- | --------------- | --------- | --------------------------------- |
| Parity nightly      | `paseo-next-v2` | `false`   | as in `stacks/paseo-next-v2.link` |
| Parity preview      | `previewnet`    | `false`   | as in `stacks/previewnet.link`    |
| Parity mainnet fork | `polkadot-test` | `true`    | no polkadot-test stack exists yet |
| PCF production      | `polkadot`      | `true`    | as in `stacks/polkadot.link`      |

## The build step

Each host's build runs one command:

```sh
truapi-host stack bundle --config <config> --out <build output directory>
```

The command does this:

1. Validates the config.
2. Downloads the descriptor from the URL in the link.
3. Verifies the signature with the key in the link.
4. Checks the descriptor against the config's policy, using the core's `Stack::bundled`.
5. Writes `stack.build.json` (the config) and `<name>.json` (the descriptor copy) into the output directory. The build
   ships both files as app resources.

If a step fails, the command fails and the build fails. This covers a descriptor that cannot be downloaded, is unsigned,
is signed by another key, has expired, or breaks the policy.

A `name` cannot contain a dot, so a descriptor copy can never be named `stack.build.json`.

The file `<name>.json` next to the config is the previous copy. The command compares the new descriptor with it. The new
one must not have a lower `sequence` or a different TLD. The command never writes next to the config, so a build does
not change the source tree. `scripts/update-stacks.sh` updates the copies in `stacks/`, as it does now.

Every build needs the `truapi-host` binary and network access. CI installs the binary with
`.github/actions/install-truapi-host`. Developers install it with `scripts/truapi-host-installer.sh`.

## Selecting a config

Parity's configs are in `stacks/`, next to the descriptor copies: `nightly.build.json`, `preview.build.json` and
`mainnet-fork.build.json`.

| Build system | Setting                                                          | Defaults                                                          |
| ------------ | ---------------------------------------------------------------- | ----------------------------------------------------------------- |
| Xcode        | build setting `TRINITY_STACK_BUILD_CONFIG`                       | Debug, DevCI: preview. Nightly, Safetynet: nightly. Release: none |
| Gradle       | Gradle property `truapi.stackBuildConfig.<buildType>`            | debug: preview. nightly, safetynet: nightly. release: none        |
| Web          | env var `TRINITY_STACK_BUILD_CONFIG`, read by the bundler config | set per deployment                                                |

The defaults for Debug, DevCI, Nightly and Safetynet are the stacks those configurations run now.

Release has no default. A release build fails when the setting is empty. Release currently runs `polkadot` with reserved
values allowed. If that stayed as a default, a publisher who did not set a config would ship it without choosing it.

This also breaks Parity's own Release builds until someone picks a config for them. Those are the TestFlight build, the
Profile and Archive actions of the Xcode scheme, and Android's release unit tests in CI.

PCF keeps its config, `production.build.json`, outside this repository, together with the last `polkadot.json` it
bundled. It passes the path through the same setting. PCF's stack and key appear only in that file, so PCF does not need
to change host code.

## What each host does

**iOS.** The xcconfig files in `hosts/ios/Configs/` set `TRINITY_STACK_BUILD_CONFIG`. A value on the `xcodebuild`
command line overrides them. The "Bundle Trinity stacks" build phase runs the build step and writes into the app's
`trinity-stack/` resources. `TrUAPIBundledStack` reads `stack.build.json` and the descriptor copy and builds
`BundledStack` from them. It does not switch on `UNSTABLE`, `NIGHTLY` or `SAFETYNET`. If the config cannot be decoded,
the app behaves as it does now when the stack resource is missing: products do not start and the rest of the app works.

**Android.** `:bindings:truapi-host` has one `syncStacks` task per build type. The task runs the build step and writes
into that build type's `stacks/` assets. `TrUAPIStack` reads `stacks/stack.build.json`. It does not map
`TestnetEnvironment` to a stack.

**Web.** `@parity/truapi-host` does not read stack files. The page passes the stack in `ProductRuntimeConfig.stack`. The
package exports the config's TypeScript type. In dotli, the build runs the build step, and the app loads the bundled
config and descriptor copy. dotli's hand-written table and `.trinity/stacks.json` are not used.

`reservedValues` depends on #2024. Until it merges, a host passes `allowed` to the core as `allowsReservedValues` and
refuses a config where either list has an entry.

An app update can ship a changed config. The core already handles the copy stored on the device:

- New key: the stack id changes, so the core does not read the old stored record.
- Stricter policy: if the stored copy breaks the new policy, the core ignores it and runs the bundled copy.
- New URL only: the core keeps the stored copy.

## CLI, scripts and tests

`truapi-host stack bundle` gets a `--config <file>` option. With it, `--out` is a directory. The command runs the checks
`bundle <link>` runs now, and two more:

- The descriptor must pass the config's policy. For example, if a testnet descriptor starts naming a Polkadot genesis
  hash, the build fails. Without this check the app would fail at start.
- `allowed: true` is refused when the descriptor names no reserved genesis hash and no reserved TLD. That stack does not
  need the permission.

Only the CLI validates the whole format: the name pattern, the key in the link, unknown fields and `$v`. The Swift,
Kotlin and TypeScript readers decode the file and refuse it if decoding fails.

A new test in `truapi-host-cli` reads every `stacks/*.build.json`. It validates the config, then runs `Stack::bundled`
with the config's link, its policy and the descriptor copy next to it. The app tests currently start the core on every
bundled stack. With one config per build they would test one stack, so this test covers the others.

The CLI's built-in networks still use `stacks/<name>.link`, so the paseo-next-v2 and previewnet links are each in two
files. The same test checks that the two are equal.

`scripts/update-stacks.sh` also refreshes the copy for each `stacks/*.build.json`. `scripts/write-refresh-changeset.sh`
reads every `stacks/*.json` as a descriptor, so it has to skip `*.build.json`.

## Rejected alternatives

- **TOML.** The app reads the bundled config at start. Foundation, the Android SDK and browsers decode JSON without
  extra code. None of them decodes TOML.
- **The config names the stack and a `.link` file holds the link.** The file that sets `allowed: true` would not show
  which key gets that permission.
- **Several stacks in one config.** RFC 2 has one stack per build. Only one development deployment of dotli and its
  Docker image offer more than one.
- **One file for all builds.** PCF would have to edit a file Parity owns, and every build would carry the other builds'
  entries.
- **Committed copies and no download in the build.** A build could ship a copy that expired after the last refresh.
- **The descriptor inside the config.** Every refresh would rewrite a file that people review by hand.
- **Extending `.trinity/stacks.json`.** Its values are link strings in the published `@parity/trinity-stack` package,
  and projects that are not hosts read it.

## Open questions

For the RFCs:

1. [RFC 3, Running more than one stack](https://github.com/paritytech/trinity-user-agents/pull/1565) lets a build choose
   whether it accepts Imported stacks. Is that choice a field of this file?

iOS and Android:

2. A build with no network fails. Can a debug build use the copy next to the config instead, with a warning?
3. Which `truapi-host` version does a build use: the latest stable release, or a version pinned next to the config?
4. Which config do Parity's Release builds get, and which configuration is the mainnet-fork build? Release runs
   `polkadot` now and nothing runs polkadot-test.
5. `UNSTABLE`, `NIGHTLY` and `SAFETYNET` on iOS and `TestnetEnvironment` on Android still select Firebase keys and chain
   tables. Should a build fail when they do not match the config?
6. Does PCF build iOS from this tree or from polkadot-ios-community? That repository has no `stacks/`.

Web:

7. One development deployment of dotli offers two networks, and its Docker image gets its networks when the container
   starts. Does such a build take several configs, or is its choice of networks outside this format?
8. dotli can run pcf-devnet with reserved values allowed. The table in #1899 does not list it. Does it get a config?
9. dotli is a separate repository with no `stacks/`. Where do its config and previous copy live? Does its build run the
   `truapi-host` binary, or should `trinity-stack update` take `--config`?
10. Does polkadot-desktop read the same file?

Stacks:

11. Who publishes polkadot-test, and under which key and URL? The mainnet-fork config cannot be written until then.
12. Must `name` equal the descriptor's `id`? They are equal in all four copies in `stacks/`.

Not tried yet:

- Running `truapi-host` with network access from an Xcode script phase and from a Gradle task, in every CI job that
  builds an app.
- Per-build-type assets in the Android bindings module.

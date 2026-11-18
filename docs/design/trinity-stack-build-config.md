---
title: "Trinity stack build config"
type: design
status: draft
author: Mikelle
created: 2026-11-18
---

# Trinity stack build config

_A proposal for the config file of [#1899](https://github.com/paritytech/trinity-user-agents/issues/1899): how a host
build names the stacks it runs and which of them may use reserved values. It follows "Build configuration" in
[RFC 2, Hosts load stacks from descriptors](https://github.com/paritytech/trinity-user-agents/pull/1564)._

## Summary

A build config is one JSON file per build. It lists the stacks the build runs, each with its link and its
reserved-values policy. The signed descriptor copy of a stack is the file `<name>.json` beside the config. A build
selects a config by path, and the app reads it at start, so no Swift, Kotlin or TypeScript code names a stack.

## The format

Parity nightly:

```json
{
  "$v": 1,
  "stacks": [
    {
      "name": "paseo-next-v2",
      "link": "polkadotapp://stack?key=0x2b89d8ca74aa2e11ab55f5af1168cd6929b88d1998aecd979b42cfc1078d5377&url=https%3A%2F%2Fstack-paseo-next.parity-testnet.parity.io%2F.well-known%2Ftrinity-stack.json",
      "reservedValues": {
        "allowed": false,
        "extraGenesisHashes": [],
        "extraTlds": []
      }
    }
  ]
}
```

- `stacks` is ordered and not empty. A host that runs one stack runs the first. A host with a network selector offers
  all of them and defaults to the first.
- `name` matches `[a-z0-9][a-z0-9-]*`, is unique in the file, and names the copy `<name>.json`.
- `link` is the stack link and must carry a key.
- `reservedValues` has the fields of `ReservedValuesPolicy` in
  [#2024](https://github.com/paritytech/trinity-user-agents/pull/2024). `allowed` is required and the two lists are
  optional. The core refuses `allowed: true` with extras.
- A reader refuses an unknown field and any `$v` but 1.
- The values a build pins need no field. RFC 2 takes them from the bundled copy.

Parity preview:

```json
{
  "$v": 1,
  "stacks": [
    {
      "name": "previewnet",
      "link": "polkadotapp://stack?key=0x4d43362f03d5511632a210632d1fb7760b5a663580f12ffca34619f79357e239&url=https%3A%2F%2Fpreviewnet.substrate.dev%2F.well-known%2Ftrinity-stack.json",
      "reservedValues": { "allowed": false }
    }
  ]
}
```

Parity mainnet fork. The link is a placeholder, because `stacks/` has no link or descriptor for polkadot-test:

```json
{
  "$v": 1,
  "stacks": [
    {
      "name": "polkadot-test",
      "link": "polkadotapp://stack?key=<operator key>&url=<descriptor url>",
      "reservedValues": { "allowed": true }
    }
  ]
}
```

PCF production:

```json
{
  "$v": 1,
  "stacks": [
    {
      "name": "polkadot",
      "link": "polkadotapp://stack?key=0xdcddb708759dade2e3850368ce0add2072f424ba53f1a2128f3339d646cffe28&url=https%3A%2F%2Fstack.api.pcf.network%2F.well-known%2Ftrinity-stack.json",
      "reservedValues": { "allowed": true }
    }
  ]
}
```

## Where it lives and how a build selects one

Parity's configs sit beside the copies: `stacks/nightly.build.json`, `stacks/preview.build.json` and
`stacks/mainnet-fork.build.json`. `stacks/<name>.json` stays as it is.

| Build system | Setting                                                          | Defaults                                                          |
| ------------ | ---------------------------------------------------------------- | ----------------------------------------------------------------- |
| Xcode        | build setting `TRINITY_STACK_BUILD_CONFIG`                       | Debug, DevCI: preview. Nightly, Safetynet: nightly. Release: none |
| Gradle       | property `truapi.stackBuildConfig.<buildType>`                   | debug: preview. nightly, safetynet: nightly. release: none        |
| Web          | env var `TRINITY_STACK_BUILD_CONFIG`, read by the bundler config | set per deployment                                                |

The defaults keep the stack each configuration runs today. Release has no default, so a release build without a config
fails instead of shipping the wrong stack with reserved values allowed.

## How PCF supplies its own

PCF keeps a directory outside this repository with `production.build.json` and `polkadot.json`, and passes the config's
path through the setting above. Its pipeline writes `polkadot.json` with `truapi-host stack bundle --config`. No code
names a stack, a key or a publisher, so there is nothing to fork.

## What each build system does

- **iOS.** `hosts/ios/Configs/base.*.xcconfig` set the build setting. PCF sets it in its `brand.xcconfig` or on the
  `xcodebuild` command line. The "Bundle Trinity stacks" phase copies the config to `trinity-stack/build.json` in the
  app's resources and each entry's `<name>.json` beside it, reading names with `plutil -extract stacks.<i>.name raw`. It
  fails on an empty setting or a missing copy. `TrUAPIBundledStack` decodes `build.json` and builds `BundledStack` from
  the first entry, with no switch on `UNSTABLE`, `NIGHTLY` or `SAFETYNET`.
- **Android.** `:bindings:truapi-host` registers one `syncStacks` task per build type. Each reads its property
  (`gradle.properties`, `local.properties` or `-P`), parses the config with `JsonSlurper`, and syncs `stacks/build.json`
  plus each `<name>.json` into that build type's assets. `TrUAPIStack` reads `stacks/build.json` and does not map
  `TestnetEnvironment` to a stack.
- **Web.** `@parity/truapi-host` reads no files. It takes `ProductRuntimeConfig.stack` from the page, and exports the
  config's type. In dotli, `packages/config/src/loadStack.ts` imports the selected config, loads `<name>.json?raw` as
  one chunk per stack, and returns `{ pointer: link, descriptor, reservedValues }`, in place of its hand-written tables
  and `.trinity/stacks.json`.

`reservedValues` needs #2024. On a core without it, a host passes `allowed` as `allowsReservedValues` and refuses a
config with extras.

No app build fetches. Copies are committed and refreshed ahead of the build, so a build needs no `truapi-host` binary
and no network. This departs from #1899, which puts `stack bundle` in each build.

## What `truapi-host stack bundle` changes

It gains `--config <file>`. For each entry it does what `bundle <link> --out <dir>/<name>.json` does, then checks the
descriptor against the entry's policy with the core's own check, so a testnet that starts naming a Polkadot genesis hash
fails at refresh, not at app start. `--offline` checks the copies on disk without fetching, for a release pipeline.
`scripts/update-stacks.sh` also loops over `stacks/*.build.json`. The weekly job installs the released CLI, so it gets
the flag one release later.

The CLI's built-in networks keep `stacks/<name>.link`, so the paseo-next-v2 and previewnet links exist in two files. A
key change that reaches only one fails the weekly refresh.

## Rejected alternatives

- TOML: nothing in Xcode's shell tools, Gradle's public API or a browser reads it. JSON has `plutil`, `JsonSlurper` and
  `import`.
- The config names stacks and `.link` files keep the pointers: the file that grants `allowed: true` would not show the
  key it grants it to, and changing publisher would not be a config change.
- One file listing all builds: PCF would edit a Parity file, and every build would carry the others' entries.
- The descriptor embedded in the config: every refresh would rewrite the hand-reviewed file.
- A descriptor path field: app resources would be laid out differently from the directory on disk.
- Extending `.trinity/stacks.json`: its values are link strings in the published `@parity/trinity-stack`, and projects
  that are not hosts read it.

## Open questions

iOS:

1. Which configuration is the mainnet-fork build? Release runs `polkadot` today and nothing runs polkadot-test.
2. Can Release have no default config, including in `ios-app.yml`?
3. Does PCF build from this tree or from polkadot-ios-community, where `$(SRCROOT)/../../stacks` does not exist?
4. The `UNSTABLE`, `NIGHTLY` and `SAFETYNET` flags still pick Firebase keys and chain tables. Should a build fail when
   they disagree with the config?

Android:

5. Which build type is the mainnet-fork build, and can release have no default?
6. Per-build-type assets in `:bindings:truapi-host`, or one asset set and a `BuildConfig` field naming the config?
7. Does PCF pass the path in `local.properties`, where the app name already comes from, or with `-P`?
8. `TestnetEnvironment` still drives `KnownChains` and Remote Config keys. Same question as 4.

Web:

9. dotli offers several networks per deployment through `VITE_NETWORKS`. Does a config with several stacks replace it,
   or does each deployment get a one-stack config?
10. dotli runs pcf-devnet with reserved values allowed, which the table in #1899 does not list. Which config holds it?
11. Does dotli refresh with the `truapi-host` binary, or should `trinity-stack update` take `--config`?
12. Does polkadot-desktop read the same file?

Stack operators:

13. Who publishes polkadot-test, under which key and URL? The mainnet-fork config waits on the answer.

Untested assumptions:

- An xcconfig assignment after `#include "brand.xcconfig"` wins over the brand's.
- `JsonSlurper` works in a `.kts` build script.
- Per-build-type assets work in the bindings module.
- PCF builds from a checkout that has `stacks/`.

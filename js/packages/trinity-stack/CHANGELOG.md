# @parity/trinity-stack

## 0.2.0

### Minor Changes

- 2c8a677: A descriptor can state its stack's dotNS contract addresses in `dotns`, by protocol registry key, so readers
  take them from the descriptor instead of reading them from Asset Hub. `truapi-host stack` and `@parity/trinity-stack`
  validate the keys and addresses.
- b3e3dbb: `npx trinity-stack add` and `update` write a project's signed stack copies to `.trinity/`, and
  `trinity-stack generate` rebuilds the git-ignored module that bundles them without fetching. `createStackReader` from
  `@parity/trinity-stack/reader` runs from those copies and refreshes them by a `"never"`, `"once"` or `{ maxAgeMs }`
  policy, with `If-None-Match`, `onChange` and `require`.
- 2dfc294: Descriptors can name a `web-gateway`, the gateway that serves the sites of the stack's dotNS names, and an
  `evm-explorer`, an Ethereum block explorer for Asset Hub. `webUrl(descriptor, name)` gives the URL a name opens at on
  the stack's gateway. A `web-gateway` may use plain `http` on `localhost`, for a stack run on the reader's machine.

### Patch Changes

- c1a894d: A descriptor or pointer URL that is not ASCII is now refused, since IDNA readers map some hosts differently.
  A host outside ASCII must be written in its `xn--` form. `truapi-host stack` now also refuses a URL that contains `\`,
  as `@parity/trinity-stack` already does.
- 210e742: A descriptor or pointer URL that contains `\` is now refused, since parsers disagree on which host it names.

## 0.1.0

### Minor Changes

- 6a47121: Add `@parity/trinity-stack`, which reads, verifies, signs and publishes Trinity stack descriptors with the
  rules of the `trinity-stack` crate compiled to WebAssembly.

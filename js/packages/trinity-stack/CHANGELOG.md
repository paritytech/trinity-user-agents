# @parity/trinity-stack

## 0.3.0

### Minor Changes

- e48f5b7: The package and the crate list the links, signed or unsigned, of the known Trinity stacks: paseo-next-v2,
  pcf-devnet, polkadot, previewnet and westend. `knownStacks` maps each name to its link, and the crate's `known_stack`
  and `known_stack_links` return the same. `trinity-stack add <name>` without a link adds the signed known stack of that
  name, and `trinity-stack known` prints each name and link and marks the unsigned ones, which are listed for their link
  only.

### Patch Changes

- c0a94d4: A reader on a `maxAge` policy ignores a request time that is ahead of the clock, as it already ignored such a
  fetch time. A clock that is corrected backwards no longer delays the next fetch by the size of the correction. The
  `trinity-stack` crate's `StackReader` also has `due_in`, the seconds until its refresh policy makes a stack's fetch
  due, so a caller sleeps that long instead of polling `get`.
- 4b23681: A descriptor in which two chains share a genesis hash is refused with `duplicate-genesis-hash`.

## 0.2.1

### Patch Changes

- 86f727a: The `trinity-stack` Rust crate is now published to crates.io with the same version as this package, so Rust
  projects can depend on `trinity-stack = "0.2"` instead of a git dependency.
- c38ac5b: The `trinity-stack` Rust crate reads a validated descriptor's values as bytes: `Chain::genesis` returns the
  genesis hash, `Descriptor::service` the service of a kind, and `Descriptor::dotns_address` a dotNS address. They
  decode hex that validation already checked, and panic only on a descriptor that skipped validation. The wasm API of
  `@parity/trinity-stack` is unchanged.

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

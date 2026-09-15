# @parity/truapi-host

## Unreleased

### Patch Changes

- Keep monitoring accepted Lite username claims until chain ownership is confirmed
  or the wallet activation is disposed. Recover from transient chain-read failures
  without resubmitting registration or reporting a fixed polling cutoff as failure.

## 0.11.0

### Minor Changes

- Reserved person and identity keys derive under the network's dotNS suffix. `SigningHostConfig` carries that suffix,
  and every reserved derivation (`uid.<suffix>`, `peopl.<suffix>`) scopes to it, so one seed resolves to the same person
  across the WASM, native and CLI hosts on a given network. Derivation vectors for `.paseo` and `.testnet` are pinned
  against an independent RFC-0022 implementation.

  One seed therefore resolves to a different person than it did under the unsuffixed derivation, and the CLI requires
  version 2 account and pairing stores: existing signer state must be discarded and devices paired again.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.14.0

## 0.10.1

### Patch Changes

- Preserve buffered subscription event order.
- Resolve the dotNS controller whether the gateway stores a dispatcher or the controller.
- Page dotNS `pendingClaims` through its `(address,uint256,uint256)` view, and retain claims from complete earlier pages
  when a later page reverts.
- Read Resources parameters through runtime view functions.
- Updated dependencies
  - @parity/truapi@0.13.1

## 0.10.0

### Minor Changes

- Rename the PreviewNet dotNS top-level domain from `.test` to `.testnet`.

### Patch Changes

- Updated dependencies
- Updated dependencies
  - @parity/truapi@0.13.0

## 0.9.0

### Minor Changes

- 8983638: Support the development-only raw proof context used by `development_createAccountProof`.
- 654c0cf: Expose the host's selected language through `locale.subscribe()`.

### Patch Changes

- Updated dependencies [654c0cf]
  - @parity/truapi@0.12.0

## 0.8.0

### Minor Changes

- fa7d8db: Expose the current canonical product identifier through `system.getProductContext()`.

### Patch Changes

- Updated dependencies [fa7d8db]
  - @parity/truapi@0.11.0

## 0.7.0

### Minor Changes

- Host runtime over the current Rust core. A JS host can serve Chat as an optional capability: bot registration, every
  message variant forwarded, manifest execution-kind matching, and custom chat rendering. The runtime retains and
  exposes session identity material, emits the opening auth state with a typed `LoginFailed` kind, forwards session
  activation, yields the named theme from `subscribe_theme`, and reads person usernames from Asset Hub dotNS. External
  navigation is gated on a per-host remote grant, own-account subtree consent is gated with a bounded deadline, and
  statement-store allowance renewal pools PGAS slots and reports what the last pass achieved. Fixes: workers are
  disposed cleanly, a misbehaving product or host no longer aborts the process, and the wasm glue is imported by a
  literal specifier.

### Patch Changes

- Updated dependencies [d872d64]
- Updated dependencies [d49f253]
  - @parity/truapi@0.10.0

## 0.6.0

### Minor Changes

- The host runtime backs the ring-VRF registry with a product-scoped key store, so registration, listing, and direct
  signing resolve against registered member keys, and a foreign key is refused unless its owner allowlisted the caller.

  Statement-store allowances renew themselves as they approach expiry and replace the oldest slot once a period is full,
  so long-lived products keep a usable slot without a manual top-up. Allowance operations reuse cached chain metadata,
  rings, and a single shared extension-info resolver instead of re-reading them per call.

  Product identifiers accept per-network dotNS TLDs, so a product name resolves against the host's configured network
  rather than a single hard-coded suffix.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.9.0

## 0.5.0

### Minor Changes

- Host callbacks gain `supportedChains()`, returning the host's environment plus one `(ChainIdentifier, genesisHash)`
  entry per chain role. The core answers `chain.getChainInfo` (RFC 0026) from this single callback; web hosts implement
  it on the `features` callback group.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.8.0

## 0.4.0

### Minor Changes

- Publish the RFC-0022 mobile host cutover and completed RFC-0023 account VRF signing runtime. Pairing hosts persist
  product-scoped AutoSigning keys, sign matching same-product requests locally, and require structured host and Account
  Holder confirmations before forwarding every other request.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.7.0

## 0.3.0

### Minor Changes

- Update the WASM host runtime and generated callbacks for tagged 32-byte product-account derivation indexes. Implement
  sr25519 VRF signing through both local AutoSigning authorization and account-holder confirmation flows.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.6.0

## 0.2.1

### Patch Changes

- Update the WASM host runtime so Bulletin preimage submission survives `chainHead_follow` interruptions without
  double-storing: an interrupted watch re-checks finalized blocks for the already-broadcast transaction before any
  retry, retries re-broadcast the identical signed bytes instead of re-signing with a fresh nonce, and a bounced
  re-broadcast surfaces as inclusion-unverified rather than a failure. Allowance propagation waits are now bounded by
  wall-clock time instead of a best-block count, keeping the budget stable across changes in Bulletin's block cadence.

## 0.2.0

### Minor Changes

- Update the WASM host runtime for junction-based ring locations and contextual alias/proof reviews. The runtime also
  exposes login progress after wallet approval, routes product and DotNS identity raw signing through their matching
  account-holder messages, and retries transient preimage inclusion lookups.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.5.0

## 0.1.0

### Minor Changes

- Initial public release of `@parity/truapi-host`: a WASM-backed TrUAPI host runtime that embeds the Rust core. Subpath
  entries expose the shared host types (`.`), the browser iframe + Web Worker runtime (`/web`), the Worker entry
  (`/worker-runtime`), and the packaged WASM bundle (`/wasm/web`).

# @parity/truapi-host

## Unreleased

### Patch Changes

- Keep monitoring accepted Lite username claims until chain ownership is confirmed
  or the wallet activation is disposed. Recover from transient chain-read failures
  without resubmitting registration or reporting a fixed polling cutoff as failure.
- Treat browser screen capture as optional Media functionality. Browsers such as
  Mobile Safari can open calls and render host-composited video without
  `getDisplayMedia`; requesting screen capture still fails closed when the API
  is unavailable.
- Build browser host WASM with the lockfile's `wasm-bindgen` version and the
  pinned Binaryen release used for published artifacts.

## 0.27.0

### Major Changes

- 0bd0f9b: Host integrations implement `requestWorkerTransition(productId, transition)` for worker lifecycle requests.
  `Start` requires the worker to run; `Stop` allows it to stop. Native Rust integrations use
  `request_worker_transition`, and the ledger controller is `WorkerController` with `install_worker_controller`.

### Minor Changes

- ff34f26: The core builds its own extrinsics through one builder: statement-store and long-term-storage allowances,
  PGAS claims, Bulletin preimages and the CLI's dotNS registrations. Every extrinsic is mortal, born at the finalized
  head for 64 blocks, and signs the genesis the host configured. Its version follows the new
  `transactionExtensionVersions` host callback, in the `extrinsicVersion` group: the chains the host builds as V5
  general transactions and the transaction extension pipeline each uses; every other chain is V4. A web host
  implementing `RequiredHostCallbacks` has to answer it; an empty list builds every chain as V4, which leaves the core
  unable to build the allowance, PGAS and dotNS extrinsics, since those exist only as V5. The `truapi-host` CLI answers
  People and Asset Hub on pipeline 1.
- d61da55: `system.info()` reports `truapiVersion`, the TrUAPI release whose runtime the host embeds, so a product can
  tag telemetry with it. The runtime fills it in on every host, including iOS and Android. A product built before this
  release ignores the field. A product built with it cannot decode the answer of an older host, so it should treat that
  error as the version being unknown.
- 8c23b5e: Add the `NftPurse` API. Native hosts create purses owned by the calling product and list an owner's purses,
  asking for the new `NftPurseRead` permission before showing Root's or another product's. Its other methods answer
  unavailable.
- e9b6d73: Native hosts serve `NftPurse.createItems` and `NftPurse.queryPurse`. `createItems` derives 1 to 1000
  receiving keys in a purse, on the hard path `//nft//<purse>//<installation>//<index>` under the installation the host
  reports through the new `currentInstallation` host callback. `queryPurse` pages through the keys that hold an NFT on
  Asset Hub at the best block, at most 100 at a time. Another owner's purse needs the `NftPurseRead` grant for both. The
  browser core answers both `Unsupported`.
- e6621cd: Account access, operation reviews and cross-product signature approvals go through one `ProductConsent` per
  runtime. An Allow once answer to an account-access prompt now covers that product's later requests until restart,
  lock, a wallet switch or that product's reset, only one account-access prompt is shown at a time, and a request that
  waited for it reuses the answer. Requests from a paired host share the phone's stored decisions and are always
  reviewed on the phone.
- 747865c: If a product sends a request in a version the host doesn't know, the host rejects it with an
  `UnsupportedVersion` error instead of failing to decode it, and the call fails as `Unsupported`. Older hosts still
  answer `MalformedFrame`.

### Patch Changes

- f67e9cb: The core closes a chain connection on which a request has waited 30 s with no frame arriving. The waiting
  requests fail with `json-rpc connection closed: no frame for 30s while a request waited`, the host is told to close
  the socket, and the next call opens a new connection without waiting for the host to miss a pong. The deadline
  outlasts the largest frame a chain sends on a slow link, the 1.3 MB Asset Hub runtime metadata, so a slow answer is
  not mistaken for a dead socket.
- 662da7f: Native hosts screen a product-supplied chat room icon through the core rather than reimplementing it.
  `screen_chat_icon` applies the rules `create_room` and `register_bot` already apply, for a host to call wherever it
  takes a room icon from a product, and raises `NativeChatFieldError`.
- f67e9cb: The SDK warns `[truapi] host link lost: <reason>` with the close reason each time its host connection drops,
  including each refused redial, and the host container warns `[truapi] host link back after N ms` when the link
  returns, timed from the first loss. The core warns when a json-rpc reply takes more than 5 s, when a json-rpc
  connection closes while requests wait, naming each with its age, and when `createTransaction` waits more than 2 s for
  the current block or cannot get it.
- f67e9cb: Every connection the runtime opens to a chain shares the output of the runtime calls that depend only on the
  runtime, such as the metadata. Products that start together and the transaction builder of the host download the 1 to
  1.5 MB metadata of each runtime once between them, keyed by the hash of the runtime code at the block they ask about.
  A product that asks while the download is on its way waits for it, and is told the operation is inaccessible if that
  download fails, so it asks again.
- c18c267: Runtime, CLI, and generated WASM host callbacks use shared error conversions while retaining protocol error
  variants and JavaScript rejection values. CLI storage and session failures retain nested causes, and allowance
  exhaustion is recognized through the error chain.
- f67e9cb: A call made while the SDK has lost its host connection waits for the connection it redials and is sent once
  that connection completes its handshake. A call or subscription whose frame already reached the host fails with
  `ConnectionResetError`, since the host may have started it. The waiting call fails with the same error when the SDK
  stops redialing: the page is hidden, the retries are spent, or the connection was disposed. The 120 s request deadline
  still bounds the wait.
- Updated dependencies [f67e9cb]
- Updated dependencies [b5e1aa4]
- Updated dependencies [0eb93a9]
- Updated dependencies [ff34f26]
- Updated dependencies [d61da55]
- Updated dependencies [f67e9cb]
- Updated dependencies [f231850]
- Updated dependencies [dce190f]
- Updated dependencies [8c23b5e]
- Updated dependencies [e9b6d73]
- Updated dependencies [0c77379]
- Updated dependencies [747865c]
- Updated dependencies [f67e9cb]
- Updated dependencies [c18c267]
- Updated dependencies [f67e9cb]
  - @parity/truapi@0.27.0

## 0.26.0

### Major Changes

- 8462d88: `parse_navigate` reads a Pocket deeplink as the host's own URL.
  `polkadotapp://pocket/add?product=<id>&card=<id>` and `polkadotapp://pocket/open?product=<id>&card=<id>` classify as
  `NavigateDecision::Pocket`, naming the product from the `product` parameter, and `polkadotapp://pocket` classifies as
  `NavigateDecision::PocketCollection`, which carries no `identifier`. A Pocket link with an action the core does not
  serve, or with no product or no card, is rejected. A product's own paths are App routes:
  `polkadot://<product>.<tld>/-/pocket/...` classifies as `DotName` and opens the product, so a product that links to
  the Pocket uses the new form. `navigate_to` passes a Pocket link without the `OpenUrl` grant other `polkadotapp://`
  links need.
- 8c6f4c9: `preimage.submit` raises no per-upload confirmation. The `PreimageSubmit` permission authorizes it:
  `AllowAlways` covers every upload, and `AllowOnce` covers a single upload. Allocating or renewing the Bulletin
  allowance may still ask the paired wallet. `UserConfirmationReview` has no `PreimageSubmit` variant and
  `PreimageSubmitReview` is not exported, so hosts that match on either remove that case; the remaining review variants
  keep their wire discriminants.
- 1a2efef: When the user refuses a resource allocation on the product's own host, the product receives a response whose
  outcomes are all `Rejected`, as RFC 0010 defines, instead of an error. A refusal on the phone for a paired host
  already answered this way.
- 7472b77: `development_createAccountProof` is not exported, and signing and pairing hosts derive every ring-VRF proof
  context from its product id. A `raw:` product id has no special meaning: its context is derived like any other
  product's, so a proof can no longer be bound to 32 caller-chosen bytes.

### Minor Changes

- 073b4f2: When a product signs with another product's account, the user is asked once for each kind of signature and
  each owning product, rather than for every signature. The approval lasts until the calling product is closed. This
  applies to payload, watermarked raw, transaction and statement proof signatures. If the user declines, the next
  signature asks again. Transactions that name contacts and unwatermarked raw bytes still ask every time. Only one of
  these prompts is open at a time, and a signature the user already approved does not wait for it.
- 7bf5a3e: Durable transactions can be scheduled before they are signed. A domain registers a `SubmissionPolicy`; a
  scheduled transaction waits in the ledger until the core's submission executor asks that policy to build it, then is
  broadcast and followed like a presigned one. A presigned transaction may name a policy too. When a transaction with a
  policy is proven failed, the policy decides whether it is built again instead of failing. `durable_work_changed(true)`
  also covers transactions waiting to be built, so the host keeps recovery running until they are. No domain schedules
  transactions yet.
- 28fa81c: A wallet-local session reads its usernames from the dotNS contracts on Asset Hub, the same lookup a paired
  session uses. Activation does not wait for it: the session starts under the lite username the host passes, if any, and
  when the dotNS record holds a name, the record's lite and full usernames replace it and the core reports them through
  `authStateChanged`. Hosts no longer need to read usernames from the People chain.
- 743f0ba: Add the `Motion` device permission. Products request it through `requestDevicePermission("Motion")` or the
  standard `DeviceMotionEvent.requestPermission()`, and the iOS host answers WebKit's motion request from the product's
  saved, one-use, or prompted decision. A host built before this release cannot decode `Motion` and rejects the request
  with a malformed-frame error, so products should treat that error as motion being unsupported.

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

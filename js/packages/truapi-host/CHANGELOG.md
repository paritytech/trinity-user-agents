# @parity/truapi-host

## 0.23.0

### Minor Changes

- 7663ece: `create_transaction` and `create_transaction_with_legacy_account` read `txExtVersion` as the version of the
  transaction extensions in `extensions`, as the runtime numbers them. With `0` the host builds a V5 general transaction
  when transaction extension version 0 includes `VerifyMultiSignature`, and a signed V4 transaction otherwise. A
  non-zero value builds V5 with that version. A version the runtime does not declare returns `NotSupported` naming the
  declared versions.

### Patch Changes

- ca44c7f: Native host storage and Pocket card removal are asynchronous. The core awaits `core_storage_read`,
  `core_storage_write`, `core_storage_clear`, `local_storage_read`, `local_storage_write`, `local_storage_clear` and
  `NativePocketCallbacks::remove_card`, so a host backend that waits on disk, a keystore or a database no longer holds a
  core thread. On Android, `HostStorage`, `HostCoreStorage` and `PocketHostBridge.removeCard` are `suspend` functions,
  which is a breaking change for `@parity/android-host` implementers. On iOS the protocols are unchanged, since
  synchronous implementations satisfy the async requirements. `chain_send` and `chain_close` stay synchronous so
  requests keep their order, and must only enqueue work.
- 57475ce: Native hosts run the whole core on one process-wide tokio runtime. Subscriptions and background loops spawned
  by the core and the localhost WebSocket bridge's connections share its workers, so a bridged product's request and the
  subscriptions it opens run on the same executor. The runtime constructor fails with `RuntimeUnavailable` when that
  runtime cannot start.
- Updated dependencies [7663ece]
  - @parity/truapi@0.23.0

## 0.22.0

### Patch Changes

- d3d3312: Every native build of the core includes the localhost WebSocket bridge and the debug sink. There are no
  `ws-bridge` or `debug-sink` Cargo features: native targets compile both with the default `runtime` feature, and wasm32
  builds never include them.
- 336be6e: The shared Rust core asks blessed products only for device permissions and legacy-account signing. Other
  product operations bypass permission prompts and recorded decisions.
- 4ef4efc: Build the core WASM without wasm-opt's one-caller inlining, which merges functions into bodies that compress
  poorly. The `web` core is 719.5 KiB brotli and 952.5 KiB gzip, down from 775.4 KiB and 1.02 MiB; the raw size is
  unchanged at 2.66 MiB.
- Updated dependencies [a285523]
- Updated dependencies [60940ff]
- Updated dependencies [de343d4]
- Updated dependencies [3ef2191]
  - @parity/truapi@0.22.0

## 0.21.0

### Minor Changes

- 579f450: Load `verifiable` on demand in the browser. The core WASM is 2.64 MiB raw and 775 KiB brotli, down from 7.76
  MiB and 5.40 MiB: `verifiable` and its 4.5 MiB of powers of tau live in a separate module, `truapi_verifiable.js` and
  `truapi_verifiable_bg.wasm` beside the core's files in each bundle, fetched in the background once a pairing session
  connects, or when a ring-VRF operation first runs. Bundlers that follow `new URL(…, import.meta.url)`, such as Vite,
  emit both files, and a host serving a bundle directory as a whole needs no change; a host that copies individual files
  out of it must copy both too.
- db004aa: SSO request cancellation. A pairing host whose caller withdraws a request it has published sends a `Cancel`
  naming it, when that request is still the newest one on the session; a host timeout sends nothing. The core's
  signing-host responder reads `Cancel` while it is still serving earlier requests: a running request stops at its
  confirmation prompt or before its next allocation step and posts no response, and one not yet started never runs.
  `Cancel` is appended to the SSO catalog at the next index, so a peer that predates it logs the message and serves the
  request as before.
- 0e56d37: Take the wire debugger's dial from the embedding host instead of `localStorage`.

  `createWebWorkerPairingHostRuntime` accepts a `debugger` option; a dev build may carry a default in
  `VITE_TRUAPI_DEBUGGER_URL`. The host's own value wins, and `null` or `""` refuses the dial outright. The dial is
  resolved once at construction and cannot be changed from the page afterwards, so whether a session is observed is a
  property of the build and the host rather than of anything typed into a console later.

  While a dial is live the host shows a small dev-only badge naming every endpoint frames are going to, suppressible
  with `debuggerIndicator: false` for a host that renders its own. The badge belongs to the runtimes that are dialling,
  so an embedder with one worker runtime per product surface can give a dial to some of them without the rest taking the
  badge down.

  A dial URL must be `ws://` on `localhost`, 127.0.0.0/8 or `[::1]`, the same set the native sink accepts. An
  IPv4-mapped literal such as `ws://[::ffff:127.0.0.1]` is refused.

  **Breaking for anyone enabling the debugger today:** the `truapi:debugger` `localStorage` key is no longer read, and
  setting it has no effect. Pass the `debugger` option, or build with `VITE_TRUAPI_DEBUGGER_URL`.

- 5a9f4b9: The `Permissions` host callbacks name the product that asked: `devicePermission(product, request)` and
  `remotePermission(product, request)` take the requesting `ProductContext` first, like the other product-scoped
  callbacks, so a host can title the prompt with the product and key any grant it keeps itself by the product. Stored
  decisions stay keyed by `productId` alone.

  The `truapi-host` CLI names the requesting product in its permission approvals.

### Patch Changes

- cf1702f: Initialize built-in personhood ring keys when an authorized product lists the personhood owner's keys, so
  full and lite handles are available without prior registration on the device.
- dadcabd: A withdrawn request stops on the host, not only at the product. A call cancelled while its confirmation
  prompt is open stops waiting, and an answer given afterwards authorizes nothing. A withdrawn call never publishes its
  paired-host request, never sends a broadcast, statement, notification or navigation it had not yet sent, and a
  broadcast already sent is stopped when the node gave it an operation id. Permission prompts still finish and record
  their answer. Every stop reaches the product as `CallError.Cancelled`.
- Updated dependencies [ffdd9b4]
- Updated dependencies [5a9f4b9]
- Updated dependencies [cf1702f]
  - @parity/truapi@0.21.0

## 0.20.0

### Patch Changes

- Updated dependencies [5f3dc71]
- Updated dependencies
  - @parity/truapi@0.20.0

## 0.19.0

### Minor Changes

- 5709e2b: `@parity/truapi-host/testing` exposes a mock host products can be tested against. `createMockHost` answers
  the host callbacks from memory and records what the core asked for, so a test asserts on the confirmation reviews,
  permission answers, navigations and storage writes the core produced rather than on a host's internals.
  `./testing/playwright` provides a fixture that runs the real core in a Web Worker with the product in an iframe,
  `./testing/server` the node server behind it, `./testing/client` a no-iframe variant for unit tests, and
  `./testing/dev-accounts` named accounts that sign with real sr25519 from fixed entropy.

  A second WASM bundle at `./wasm/testing` carries the `wasm-signing-host` and `test-host` Cargo features, which is what
  lets the test host own keys and answer resource allocation as granted without allocating anything. Neither feature is
  on in the `./wasm/web` production bundle, so no shipping browser host has an entry point to either.

  Statement Store and Bulletin allowance allocation compiles for `wasm32` as well as native, so a browser signing host
  reaches the same on-chain allocation path a native one does. That adds about 1.7 KB to the `./wasm/web` bundle.

  Statement-store controls are served through the chain connection the host owns: `injectStatement` publishes a
  SCALE-encoded statement into the product's subscriptions, `getSubmittedStatements` reads submissions back off the
  transport, and `injectChatAction` publishes a host-authored Chat action into the product's action stream.

  Payments throw with the reason rather than returning a plausible value: the protocol declares them but no host
  implements them, so a test reaching that path learns why instead of passing against a fake. `setLoginBehavior` throws
  too, pointing at the `loginBehavior` fixture option, which is where the test host takes it.

### Patch Changes

- Updated dependencies [a78d73c]
- Updated dependencies [303b163]
- Updated dependencies [5d5fd1c]
  - @parity/truapi@0.19.0

## 0.18.0

### Major Changes

- 26b957f: `navigate_to` hands a host a dotNS product destination as `polkadot://<product_id>.<tld>/<path>` rather than
  rewriting it to `https://`. That is the form the Pocket deeplink grammar already defines for a product URL, and the
  one a Pocket target already arrives as, so a host routes on the scheme instead of guessing from the domain. An
  `http(s)` destination is unchanged and still gated on a per-host grant.

  A host that recognised `https://<name>.<tld>` and converted it back should match `polkadot://` instead.

### Minor Changes

- 9b54ceb: A `context` grant admits the grantee acting in the granting product's own proof context as well as in its
  own. Both are matched by product label within one network, so a product's other executables count and a namesake on
  another network does not.

  Refusing the granting product's context left the scope unusable: `PeopleLite.set_alias_account` verifies against
  `Score.score_context`, which names the personhood product for every prover, so a grantee held to its own context alone
  can produce no proof such a chain accepts. A context naming a third product is still refused, as is the `raw:`
  development context. A grantee's own namesake on another network is refused too, which is new. Both the ring-VRF proof
  and the contextual alias read follow the same rule, because they come out of one VRF evaluation.

- 448c1d4: - Host `device_permission` and `remote_permission` callbacks return `PermissionDecision` (`AllowOnce`,
  `AllowAlways`, or `Deny`). Native callbacks leave grant storage and consumption to Rust. Embedders must update their
  callbacks; product-facing responses remain boolean.
  - Keep one-use grants in memory. Internal `authorize_remote_permission` and `authorize_device_permission` methods
    consume them using the public request types, without exposing the methods in the public SDK or API documentation.
  - Normalize remote domains, match legacy wildcard coverage, and keep a shared blessed-domain list.
  - Require `OpenUrl` for external navigation, including allowed application schemes. A lasting grant covers all
    external destinations; domain permissions govern outbound network requests.
  - Require `Notifications` before `send_push_notification`, prompting when undecided and rejecting delivery when
    denied.
  - Present per-action confirmations without misleading persistent-permission choices.
- e9c45b8: Authorize browser fetch, asynchronous XHR, WebSocket connections, media capture and WebRTC through the
  internal `authorize_remote_permission` and `authorize_device_permission` APIs. Install the shared sandbox in native
  product views. Synchronous XHR, `Worker`, `WebTransport` and `getDisplayMedia` screen capture are unavailable.

  Camera and microphone grants are consumed per capture attempt, camera first. A later microphone denial or native
  capture failure does not restore an already consumed grant. Product consent is enforced by the container; native media
  delegates enforce OS permission separately.

  Update native integrations: Swift `installProductScripts(into:endpoint:)` now takes a `WKWebView` and is synchronous,
  without an execution argument. Swift and Kotlin `LocalhostBridgeBootstrap.script` no longer take `webRtcAllowed`.

- ea5e2f9: Add `localStorage.subscribe` and worker pending operations.

  `localStorage.subscribe(key)` streams a key's value within the product's own namespace, emitting the current value
  immediately and then one item per later write or clear from any of the product's runtimes. A write that leaves the
  stored bytes unchanged emits nothing.

  `worker.beginOperation` / `worker.endOperation` keep a product's worker runtime alive while it holds at least one open
  operation. Both are gated to the Worker execution kind. `endOperation` is idempotent. An open operation counts as
  demand on the product's worker, so it reaches a host through the same worker-demand signal an on-screen surface
  produces.

  Hosts implement `ProductOperations` and `ProductStorage.subscribeStorage` to back these.

### Patch Changes

- 27b8f07: A pairing attempt is bounded. Reading the stored pairing identity, connecting to the statement store,
  subscribing to the pairing topic and waiting for the wallet's handshake are each raced against a single 300s deadline,
  beside the cancellation they already honoured. Resolving the session and persisting it carry their own budgets and are
  not covered by it.

  A peer answering on a different SSO envelope publishes statements this host cannot open, which is indistinguishable
  from a peer that has not answered yet: both are silence on the topic. The attempt now ends with a reason naming that
  as the likely cause.

- b7b2c94: Product executions under one host runtime share a single localhost WebSocket listener, each with its own
  token. Closing a connection disposes the runtime that served it, so its host-core subscriptions and chat state are
  released rather than held for the life of the listener. Admitted connections are bounded per execution and
  listener-wide, with the per-execution cap a share of the listener-wide one; handshakes in flight are bounded
  separately, and a full backlog evicts its oldest entry so a stalled peer cannot lock out other executions.
- Updated dependencies [d9eaece]
- Updated dependencies [9b54ceb]
- Updated dependencies [33f9222]
- Updated dependencies [c2e5674]
- Updated dependencies [a63b0e8]
- Updated dependencies [448c1d4]
- Updated dependencies [d3ec891]
- Updated dependencies [e9c45b8]
- Updated dependencies [ea5e2f9]
  - @parity/truapi@0.18.0

## 0.17.0

### Minor Changes

- 221d972: Make the `context` scope effective. A `trustedProducts` grant of `context` now admits a cross-product
  ring-VRF proof or signature against the granting product's key, where before the runtime admitted the caller and the
  authority holding the keys refused it again with the same error a product granting nothing would produce. Each
  authority resolves the grant from the owner's published manifest itself rather than accepting a verdict relayed by the
  caller, because on a paired host the calling product id arrives over the wire as a self-asserted field.

  The authority derives the key from the identity it authorised rather than the spelling it was handed, so authorisation
  and derivation cannot diverge. A granted call is held to the caller's own proof context: the contextual alias is a
  function of the owner's key and the context, so an unconstrained context would let a grantee produce the alias the
  owner presents to a third product that granted nothing and cannot consent.

  The identity read `context` names is covered by the grant rather than prompted for again. The contextual alias and the
  ring-VRF proof come out of one VRF evaluation, so a grantee that may `create_account_proof` already holds the alias
  that proof attests; `get_account_alias` accepting the same grant makes the two calls agree about what the scope means.
  A product without a grant still takes the prompt.

  A stored account-access refusal still overrides a grant, and is now recorded against the product on both sides rather
  than one spelling of it, matching the granularity a manifest grant uses: a refusal for `peopl.dot` also covers
  `app.peopl.dot`. Decisions written by earlier releases under the full product id are still honoured, so upgrading does
  not discard a refusal a user has already given.

  `create_account_proof` with a foreign key handle and no active session now answers `Rejected` where it previously
  answered `NotAllowlisted`. The session is consulted before the grant so that the pair of refusals cannot be used to
  probe which product granted which; a product branching on the old tag for that case sees a different one.

  Product identifiers are rejected when they carry no name: an empty label, control characters, invisible bidirectional
  or zero-width formatting, whitespace, or path separators. Each of these reached the grant key, the manifest cache key,
  the account-access prompt and the logs. Internationalised names are unaffected.

  A granted cross-product access is logged. It raises no prompt and writes no stored decision, so previously only the
  refusal was audible: a publisher's grant could let one product act with another's keys leaving no trace on the device.
  This does not make the access revocable, which needs a surface for the user to record a decision about a pair they
  were never asked about.

  Resolving a grant can require reading a product manifest from dotNS, so the lookup is bounded by the caller's timeout
  and cancellation rather than running outside both. A manifest is cached under the label it resolves by, so every
  executable of one product shares one entry, and an entry stamped in the future is re-read rather than treated as fresh
  forever. Entries written by earlier releases under the full product id are no longer read and nothing evicts them, so
  they sit in core storage until the cache is cleared.

- b0d95f0: Serve the `pocket` service from the host runtime. A host supplies the optional `pocket` callbacks to back
  `listSubscribe` and `removeCard` over the calling product's cards; a host that supplies none answers both with
  `Unsupported`.
- 221d972: Cap product identifiers at 256 bytes. `has_dotns_tld` only inspects the suffix after the last `.`, so every
  length of `aaa...aaa.dot` was a distinct valid id, and a cross-product call carries that string from the wire where it
  is self-asserted. An identifier longer than the cap after NFC normalisation is now rejected, reported by length rather
  than by echoing the value back into an error string and a log line. This bounds the size of one identifier, not how
  many exist.

  Signing hosts also require an Asset Hub genesis hash. Product manifests are read from the dotNS contracts deployed
  there, so without one no manifest resolves and every cross-product `trustedProducts` grant not already cached is
  refused, indistinguishably from the other product having granted nothing. A custom build enabling the Rust
  `wasm-signing-host` feature must supply `runtimeConfig.assetHub` alongside `runtimeConfig.networkSuffix`; the shipped
  bundle is built `--no-default-features` and carries no signing constructor.

### Patch Changes

- Updated dependencies [4034118]
- Updated dependencies [221d972]
- Updated dependencies [befde58]
- Updated dependencies [75a9f2e]
- Updated dependencies [cc1823d]
- Updated dependencies [0e86a3a]
- Updated dependencies [a9731b1]
- Updated dependencies [a9731b1]
- Updated dependencies [a9731b1]
  - @parity/truapi@0.17.0

## 0.16.0

### Minor Changes

- 4c20296: Subscriptions can fail at any point, not only at start. A stream that breaks ends with a typed reason that
  reaches the product's `error` handler, so a platform failure surfaces instead of freezing the last value or passing
  for a clean finish; a method the host has not implemented, and a chain follow that cannot be opened, say so rather
  than completing quietly. A product that serves a host-initiated subscription, today only the chat custom renderer, now
  takes a handler of the request plus `send` and `interrupt` callbacks instead of returning an observable, which retires
  `ObservableSource`. Subscription consumers keep the shapes they had, and the wire bytes are unchanged.
- 4a93ac4: Add temporary deprecated unwatermarked signing for product and legacy accounts to unblock runtime ownership
  proofs while runtimes adopt watermarked verification (#612). Existing signing APIs retain their names and wrapping
  behavior. The temporary implementations log a deprecation warning when called. Raw signing reviews now include a
  `watermarked` flag; host confirmation UIs must display the matching bytes and warn that unwatermarked signatures may
  authorize transactions.
- 9252985: `Renderer` is the one service through which a product draws a body inside a host surface. The host starts
  `renderer.onRender` with a `RenderContext` (`ChatMessage`, `InputWidget`, `PocketCard`) and an opaque payload; the
  product streams `RendererNode` trees, and a press inside a tree reaches `renderer.actionSubscribe` as
  `{ context, actionId, payload }`. `Chat` has no `custom_message_render`; a `Custom` chat message renders through
  `Renderer` with a `ChatMessage` context, and `ChatActionPayload.ActionTriggered` carries only host-drawn `Actions`
  button presses.

  `RendererNode` replaces `CustomRendererNode` with `Image` (`ImageSource`, `ImageFit`), `Effect`, `Shape.Square`,
  `Modifier.Opacity` and `Modifier.BlendingMode`; `Spacer`, `TextField` and `Image` carry no `children`, and the
  single-field `Modifier` and `Shape` variants are tuple variants.

  Hosts call `provider.render(request, sink)` and `provider.publishRendererAction(item)`; `publishChatAction` is the
  path for posted messages, commands and host-drawn `Actions` buttons.

- fa67882: The core counts references on each product's worker and tells the host when that count crosses zero, so a
  worker runs only while something needs it.

  `acquireWorker(productId)` takes one reference for a modality holder that is on screen or in flight, and
  `releaseWorker(productId)` gives it back; releasing with none held is a no-op. The first reference and the last
  release are the only ones that report anything. `subscribeWorkerDemand(listener)` is where that report arrives: the
  listener receives every product wanted right now, then each change as it happens, and `wanted: false` for everything
  still wanted when the runtime is disposed. Starting and stopping the worker executable stays with the host, and a
  `wanted: false` is permission to stop rather than an order, so a host may keep one warm. The core keeps no clock and
  runs no timers.

### Patch Changes

- Updated dependencies [4c20296]
- Updated dependencies [4a93ac4]
- Updated dependencies [9252985]
  - @parity/truapi@0.16.0

## 0.12.0

### Minor Changes

- e8ee375: Address every frame with a two-byte `(trait, method)` wire discriminant. The trait byte names the API trait
  and the method byte addresses a method within it, so each trait owns a full 256-slot method space and method ids
  restart at 0 in every trait.

  A third envelope byte, `message_type`, names which leg of a method's exchange a frame carries, so a method costs one
  id whatever its shape. The payload is the plain SCALE encoding of that leg's type and carries its own version.

  `TrUApiTransport.codecVersion`, `CreateTransportOptions.codecVersion` and `GeneratedClientTransport` are removed.
  Generated handshake calls read `TRUAPI_CODEC_VERSION` directly, so there is no longer a way to advertise a codec
  version that differs from the one the envelope is actually framed in. `CreateTransportOptions` itself remains,
  carrying `requestTimeoutMs` alone, and `createClient` takes a `TrUApiTransport` (every value that satisfied
  `GeneratedClientTransport` satisfies it unchanged).

  This is wire codec version 2. A codec version 1 peer cannot exchange frames with a codec version 2 peer in either
  direction: the handshake itself rides the changed envelope, so the mismatch cannot be negotiated in band. Hosts and
  products must move together.

### Patch Changes

- Updated dependencies [d36911f]
- Updated dependencies [e8ee375]
  - @parity/truapi@0.15.0

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

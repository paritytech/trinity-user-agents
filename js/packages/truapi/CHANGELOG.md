# @parity/truapi

## 0.23.0

### Patch Changes

- 7663ece: `create_transaction` and `create_transaction_with_legacy_account` read `txExtVersion` as the version of the
  transaction extensions in `extensions`, as the runtime numbers them. With `0` the host builds a V5 general transaction
  when transaction extension version 0 includes `VerifyMultiSignature`, and a signed V4 transaction otherwise. A
  non-zero value builds V5 with that version. A version the runtime does not declare returns `NotSupported` naming the
  declared versions.

## 0.22.0

### Minor Changes

- a285523: A `Contacts` trait lets a product ask the host to open its contact picker. The user selects one person and
  the product receives one opaque 32-byte handle: never the list, a name, or an account. The handle is the same value
  for that contact in every product and on every host of this user, keyed on the user's own entropy, and the core
  resolves it back to an account: a transaction payload declares the handles its call names, and the host replaces them
  with those accounts before the call is shown or signed. Hosts serve it through the optional `ContactsPlatform`
  capability, and a UniFFI host installs one with `setContactsCallbacks` on the runtime; a host that installs none
  leaves `contacts.pick` answering `Unsupported`.

  `ProductAccountTxPayload` carries `contacts` as a required field, so a call naming nobody declares an empty list. It
  is on the wire as well as in the type: a product and the host it talks to have to agree on it.

- 60940ff: `sign_payload`, `create_transaction`, `sign_raw` and the statement-store product proof accept another
  product's account when that product's manifest grants the caller `context`, the same grant `create_account_proof` and
  `ring_vrf_sign` already honour. Such a signature is always confirmed by the user, and the signing reviews carry the
  calling product so a host can name it beside the account it is signing with.

### Patch Changes

- de343d4: The `truapi-host` CLI previewnet preset and truapi-provider's previewnet catalog carry the relay, Asset Hub,
  Bulletin and People genesis hashes previewnet reports after its latest reset, and the bundled previewnet chain specs
  match them.
- 3ef2191: A visible page retries a failed host reconnect after 250 ms, 1 s and 4 s before it waits for its next call. A
  page that returns to the foreground while the host is still rebinding its listener reconnects on its own instead of
  staying offline until the product calls again.

## 0.21.0

### Minor Changes

- ffdd9b4: `truapi-host` streams product frames to a wire debugger behind `--debugger <ws-url>` (or
  `TRUAPI_DEBUGGER_URL`).

  The commands that serve frames, `pairing-host`, `dev` and `signing-host`, resolve the switch before their frame
  listener binds, so a non-loopback URL fails startup rather than at first dial. Each of those commands reports the
  outcome once as a lifecycle event, naming the endpoint and the switch clap read it from, or saying the debugger is
  off. Each accepted connection gets its own channel id, `<product-id>#<n>`, so concurrent peers under one host do not
  share a trace key.

- 5a9f4b9: The `Permissions` host callbacks name the product that asked: `devicePermission(product, request)` and
  `remotePermission(product, request)` take the requesting `ProductContext` first, like the other product-scoped
  callbacks, so a host can title the prompt with the product and key any grant it keeps itself by the product. Stored
  decisions stay keyed by `productId` alone.

  The `truapi-host` CLI names the requesting product in its permission approvals.

### Patch Changes

- cf1702f: Initialize built-in personhood ring keys when an authorized product lists the personhood owner's keys, so
  full and lite handles are available without prior registration on the device.

## 0.20.0

### Minor Changes

- 5f3dc71: Regenerate Rust and TypeScript sources before building the headless CLI so source installation works in fresh
  and updated checkouts. Fix invalid documentation links and reject rustdoc warnings during code generation and CI.
- Stop freezing built-in prototypes in the container so Next.js products hydrate in the CLI and native web views.

## 0.19.0

### Minor Changes

- a78d73c: Keep the same client after a native host connection is interrupted. The shared SDK transport replaces its
  WebSocket, while pending calls and subscriptions fail with `ConnectionResetError` without replay. Providers can
  recreate their read/watch subscriptions after reconnection. Older SDKs still start through the MessagePort but need a
  page reload after connection loss.

  Browser permission checks use generated internal SDK calls. Hosts protect their shared authorization dependencies
  before product code runs; public SDK methods remain replaceable, but products cannot patch the protected built-in
  prototypes.

  SCALE boolean decoding rejects values other than 0 and 1.

- 303b163: Share the container's web API permission checks between CLI scripts and the development bootstrap tag.
  Scripts remain in Bun with filesystem, environment, subprocess and import access. Dev keeps its existing app URL,
  assets and hot reload. Public SDK calls and permission checks share one product execution.

### Patch Changes

- 5d5fd1c: Add reusable `/script` projects with dependency installation, editor types and TypeScript checking. Edit,
  rerun or create projects from the CLI. Missing npm imports use Bun's runtime fallback, and Product SDK scripts
  discover the active host automatically.

## 0.18.0

### Minor Changes

- d9eaece: An RFC-0010 `AutoSigning` grant covers `signing.sign_raw`, `signing.sign_payload` and
  `signing.create_transaction` for the granting product's own accounts: the host serves them from the granted key
  without a per-call confirmation, and a pairing host serves them without reaching the signing host. The deprecated
  unwatermarked raw-signing API is never covered and always prompts. A pairing host also signs product statement proofs
  under the grant, which the SSO raw-signing protocol cannot carry otherwise.
- 9b54ceb: A `context` grant admits the grantee acting in the granting product's own proof context as well as in its
  own. Both are matched by product label within one network, so a product's other executables count and a namesake on
  another network does not.

  Refusing the granting product's context left the scope unusable: `PeopleLite.set_alias_account` verifies against
  `Score.score_context`, which names the personhood product for every prover, so a grantee held to its own context alone
  can produce no proof such a chain accepts. A context naming a third product is still refused, as is the `raw:`
  development context. A grantee's own namesake on another network is refused too, which is new. Both the ring-VRF proof
  and the contextual alias read follow the same rule, because they come out of one VRF evaluation.

- 33f9222: Host applications running their own statement-store traffic reach the account the paired wallet granted an
  allowance to: `SessionUiInfo.deviceStatementAccountId` carries the id and `CoreAdmin.getDeviceStatementKey` the
  sr25519 secret behind it. A signing host also answers `Pending(AllowanceAllocation)` before allocating the device
  slot, so a pairing host shows progress instead of an already-scanned QR, and `Failed(reason)` if the allocation then
  fails.
- c2e5674: `hasTrustedRemotePermissions(productId)` answers whether a product is one the host grants every
  `RemotePermission` without prompting. It reads the compiled-in list alone: a stored user decision wins over that list,
  so a host mediating product network access in its own code — a webview interceptor, a `fetch` shim, a service worker —
  asks it only for the branch where its own store reads undetermined, and a host holding a runtime asks
  `permissionAuthorizationStatus` instead, which folds both together. Exported on the UniFFI and wasm surfaces.
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
- d3ec891: Request cancellation. Every generated request method takes `options?: CallOptions` last; aborting its
  `signal` sends a `Cancel` frame on that method's own address, correlated by the same `requestId`. The call still
  settles with exactly one response: a withdrawn call answers `CallError.Cancelled`, and a cancel that arrives too late
  is dropped so the real result stands. The client's own deadline sends `Cancel` before it rejects.

  Additive on the wire. `CallError` gains `Cancelled` as its last variant, so every existing discriminant keeps its
  SCALE index, and `WIRE_CODEC_VERSION` is unchanged. A host that predates the `Cancel` leg drops the frame with no
  reply and the call settles on the client's deadline instead. A product cannot detect that first, so an abort such a
  host never understood is indistinguishable from one it honoured.

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

- a63b0e8: The generated client stamps `TRUAPI_CODEC_VERSION` from `truapi::WIRE_CODEC_VERSION`, so it speaks the codec
  version `system.handshake()` accepts. A client advertising any other codec is answered with
  `UnsupportedProtocolVersion` on the first frame it sends, before a product reaches any other method.

## 0.17.0

### Major Changes

- 75a9f2e: Remove the hard-coded well-known chain constants (`PASEO_NEXT_V2_ASSET_HUB`, `PASEO_NEXT_V2_INDIVIDUALITY`,
  `PREVIEWNET_ASSET_HUB`, `PREVIEWNET_INDIVIDUALITY`) and the `WellKnownChain` type. Products resolve genesis hashes
  through `chain.getChainInfo` (RFC 0026), which answers from the host's own configuration and therefore survives a
  testnet wipe or an environment move without a new product bundle. It also covers Bulletin and Relay, which the
  constants never did.

  Replace `SOME_CHAIN.genesis` with the `genesisHash` from the matching role:

  ```ts
  const assetHub = await truapi.chain.getChainInfo({ chain: "AssetHub" });
  if (assetHub.isErr()) return;
  const genesisHash = assetHub.value.genesisHash;
  ```

  Genesis hashes are now only available asynchronously from a connected host, so code that needed one at module load has
  to resolve it inside the call that uses it.

- a9731b1: Every request, response and subscription item on the wire is an explicit versioned wrapper, empty payloads
  included. `statementStore.submit` resolves a `RemoteStatementStoreSubmitResponse` whose V1 carries no payload, and the
  six subscriptions that take no request data send a payload-less V1 request envelope on their start frame. Two frame
  payloads therefore change size: a `submit` success is two bytes, and a start frame for those subscriptions is one.
- a9731b1: Subscription interrupts carry a versioned error envelope. The nine subscriptions that reported a bare
  `GenericError` now resolve a per-method wrapper whose V1 is that same payload, so an interrupt frame is one byte
  longer and its domain error downgrades to the version the caller subscribed in.
- a9731b1: The wire codec version is 3. Seven frame legs change shape, so a peer built against codec 2 is refused at the
  handshake rather than failing per call when the first mismatched frame arrives. Hosts and products negotiate this at
  connect time and need no coordinated deploy.

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

- befde58: The unprefixed name for a versioned type belongs to the newest version any wrapper selects.
  `HostLocalStorageReadError` is the v02 shape; the v01 one is `V01HostLocalStorageReadError`.
- 0e86a3a: Add the `pocket` service: `listSubscribe` over the calling product's cards, and `removeCard`.

### Patch Changes

- 4034118: The chat key a host publishes on chain is the X25519 key it actually holds, so a peer that looks up that
  identity can encrypt to it. It was previously derived from an unrelated key tree, which left chat unreachable for
  every identity a host registered. Identities registered before this carry an unusable key and have to be
  re-registered.
- cc1823d: `truapi-host` keeps one base path on one signer identity. A `--session` name survives promotion and keeps
  selecting the session it created, a lost or stale `current-session` pointer resolves against the provisioned sessions
  instead of provisioning beside them, and `--serve` reports a missing signer and announces the minutes-long first
  registration instead of staying silent.

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

## 0.15.0

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

- d36911f: Retry iframe readiness until the host transfers a channel, and reject unanswered requests after a
  configurable bounded deadline.

## 0.14.0

### Minor Changes

- Reserved person and identity keys derive under the network's dotNS suffix. `SigningHostConfig` carries that suffix,
  and every reserved derivation (`uid.<suffix>`, `peopl.<suffix>`) scopes to it, so one seed resolves to the same person
  across the WASM, native and CLI hosts on a given network. Derivation vectors for `.paseo` and `.testnet` are pinned
  against an independent RFC-0022 implementation.

  One seed therefore resolves to a different person than it did under the unsuffixed derivation, and the CLI requires
  version 2 account and pairing stores: existing signer state must be discarded and devices paired again.

## 0.13.1

### Patch Changes

- Follow the previewnet and paseo-next-v2 testnet wipes: the well-known chain genesis hashes, the `truapi-host` CLI
  preset, and the bundled light-client chain specs match the live chains again.

## 0.13.0

### Minor Changes

- Accept pasted pairing QR images in the `truapi-host` CLI terminal UI.
- Rename the PreviewNet dotNS top-level domain from `.test` to `.testnet`.

## 0.12.0

### Minor Changes

- 8983638: Add `development_createAccountProof`, a development-only helper for creating a proof with an exact 32-byte
  context.
- 654c0cf: Expose the host's selected language through `locale.subscribe()`.

## 0.11.0

### Minor Changes

- fa7d8db: Expose the current canonical product identifier through `system.getProductContext()`.

## 0.10.0

### Minor Changes

- d872d64: Export `PREVIEWNET_INDIVIDUALITY` and `PREVIEWNET_ASSET_HUB` well-known chains, so a product on previewnet
  can pin the genesis hashes it signs `CheckGenesis` over the same way a product on `paseo-next-v2` does. Pairs with the
  CLI gaining a `previewnet` network preset.
- d49f253: Add `createWebSocketProvider(url)` for hosts that serve protocol frames over a WebSocket, and
  `connectWebSocketHost(url)` on the sandbox path so a plain browser tab using such a host is detected as hosted and
  shares the cached client. Both native host READMEs already pointed products at `createWebSocketProvider`, which until
  now did not exist, so every browser product had to hand-write the bridge. `truapi-host signing-host --frame-listen` is
  now reachable from an ordinary tab, and the CLI's own TCP provider delegates to the shared implementation.

## 0.9.0

### Minor Changes

- Add the RFC-0024 ring-VRF key management surface. `account.registerRingVrfKey` registers a product-owned member key
  for a ring and returns its public key, `account.listRingVrfKeys` reports an owner product's registry entries at either
  `Anonymized` or `PublicKey` disclosure, and `account.ringVrfSign` signs bytes directly with a registered key.

  `account.getAccountAlias` and `account.createAccountProof` take a `keyHandle` naming the registered member key the
  host must use, and ring locations address the collection directly without a pallet-instance junction. Their error
  unions carry `KeyNotRegistered` and `KeyNotInRing`; proof creation also reports `NotAllowlisted` when a foreign key's
  owner has not allowlisted the caller.

## 0.8.0

### Minor Changes

- Add `chain.getChainInfo` (RFC 0026): products resolve a `ChainIdentifier` role (`Relay`, `AssetHub`, `People`,
  `Bulletin`) against the host's configured environment and receive the network string plus the chain's genesis hash, so
  genesis hashes no longer need to be hard-coded into product bundles.

## 0.7.0

### Minor Changes

- Publish the package version paired with the RFC-0022 mobile host cutover and the completed RFC-0023 account VRF
  signing flow.

## 0.6.0

### Minor Changes

- Represent product-account derivation indexes as tagged selectors that support both compact numeric values and raw
  32-byte values. Add the general-purpose sr25519 `account.signVrf` API and its generated request, response, transcript,
  error, and callback types.

## 0.5.1

### Patch Changes

- Support the sandbox client in legacy Nova and dotli iframe hosts while the Rust Core transport migration rolls out.

## 0.5.0

### Minor Changes

- Redesign account alias and ring-VRF proof requests around stable, junction-based ring locations and product-scoped
  proof contexts. Proof responses now include the contextual alias, ring index, and ring revision, with distinct
  `RingNotFound` and `NotMember` errors.

## 0.4.1

### Patch Changes

- Treat Firefox's masked `"null"` `location.ancestorOrigins` entries as an unknown host origin in the sandbox bootstrap.
  The ready ping falls back to the source-checked wildcard instead of throwing
  `SyntaxError: An invalid or illegal string was specified`, which left iframe-hosted products permanently offline in
  Firefox.

## 0.4.0

### Minor Changes

- Add the `coinPayment` client namespace (RFC 0017 Coinage Payment): `createPurse`, `queryPurse`, `rebalancePurse`,
  `deletePurse`, `deposit`, `refund`, `createCheque`, `createReceivable`, and `listenForPayment`, with the
  `CoinPayment*` / `HostCoinPayment*` / `VersionedHostCoinPayment*` request/response/error types and their wire
  discriminants.

  **Breaking:** the `CallError<D>` SCALE codec now decodes to a tagged `CallErrorValue<D>` union (`Domain` / `Denied` /
  `Unsupported` / `MalformedFrame` / `HostFailure`) instead of projecting only the domain error and throwing on
  framework-level failures. The `Transport.truapiVersion` field is removed and `Transport.codecVersion` is deprecated;
  generated handshake calls read the codec version directly.

## 0.3.2

### Minor Changes

- Rename the exported `Provider` transport type to `WireProvider` to make its role explicit. It is the low-level
  SCALE-wire-frame pipe (a `MessagePort` or iframe `postMessage` channel) that `createTransport` runs on. The
  `createIframeProvider` / `createMessagePortProvider` factories are unchanged; only the type name moves. Consumers
  importing `Provider` should import `WireProvider` instead.
- Add the `@parity/truapi/sandbox` entry point: host-environment detection (`isCorrectEnvironment`), a lazily-built
  cached client (`getClientSync`, `null` outside a host container), and a `subscribeConnectionStatus`
  connected/disconnected listener. Browser-embedded hosts can bootstrap a client without assembling the transport by
  hand.

## 0.3.1

### Patch Changes

- Fixed `HostPaymentTopUpError` SCALE variant ordering: `PartialPayment` (index 2) now precedes `Unknown` (index 3),
  matching the canonical wire layout.
- Fixed explorer 0.3.1 snapshot import paths.

## 0.1.0

### Minor Changes

- Initial public release of `@parity/truapi`: TrUAPI transport, SCALE codecs, and the generated TypeScript API client
  for protocol v1.0.

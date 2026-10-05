# Reviewer: Architecture Checklist

Walk this checklist for any file path that touches an architectural seam. Cite the doc and section when flagging. Tag severity:

- **blocking** — breaks an architectural invariant or RFC-0002 alignment.
- **major** — clearly violates a documented architectural pattern.
- **minor** — naming, structure, or maintenance smells.

---

## Module boundaries (`architecture/multi-module.md`)

- **blocking** — Any `feature/X/impl` depending on another `feature/Y/impl`.
- **blocking** — `chains`, `database`, `common`, or `design` depending on a `feature/*` or `app`.
- **blocking** — New module added with **no** entry in `CODEOWNERS` (no reviewer for the module).
- **minor** — New module added with only one owner in `CODEOWNERS` (single-owner module). Reviewer surfaces this; doesn't block, since the agent can't pick the right co-owner — that's a human decision.
- **major** — A logical cycle: `feature/A/api` depends on `feature/B/api` AND `feature/B/impl` depends on `feature/A/api`. Propose extracting the shared concept (see "people → members" precedent).
- **major** — New module added without a corresponding `CODEOWNERS` update in the same PR.
- **major** — Feature-specific entity placed in shared `database` without a feature-prefixed name (e.g. `SessionEntity` instead of `CoinageSessionEntity`).
- **major** — Cross-feature wiring done via a one-off direct injection instead of the established multibinding seam (`@IntoSet ChatExtension`, `@IntoSet AppInitializer`).
- **minor** — Package leaves lowercase-glued (`pairrequest`) instead of `camelCase` (`pairRequest`).

## Layer boundaries (`architecture/layered.md`)

- **blocking** — Domain layer importing Compose / Android UI / Room types.
- **major** — ViewModel calling a repository directly (should go through an interactor — one interactor per VM as a convention).
- **major** — Room entity leaking out of the data layer.
- **major** — Domain model carrying presentation fields (`@StringRes Int`, `isLoading`, `isPressed`).
- **major** — DTO → domain mapping missing or located outside `feature/X/impl/data/mappers/`.
- **minor** — Mapper as a class when a pure extension function would suffice (or vice versa).
- **major** — Single-line passthrough `UseCase` that just wraps a repo call — propose inlining.

## Maintainability (`architecture/maintainability.md`)

- **blocking** — A class has clearly two unrelated concerns (load + decide + render) that should be separated.
- **major** — Domain identifier passed as raw `String`/`Long` with no invariant enforcement; should be either a value class (if invariant) or typealias (if pure naming).
- **major** — Inheritance hierarchy without exhaustive-matching or shared-finalize justification.
- **major** — Factory-of-factory-of-factory chain when a single `Factory.create(scope, config)` would do.
- **major** — A new flag/escape-hatch parameter to work around a modeling issue (e.g. an `AlwaysFirst` ordering on top of `PinToTop` to handle ties).
- **major** — Class exposes mutable fields to be set from outside for testability instead of injecting collaborators.
- **minor** — Code duplicating an existing primitive (manual QR, shimmer, loading state) instead of reusing `design/` / `common`.

## Coinage (`architecture/coinage.md`)

- **blocking** — Submitting a coinage transfer extrinsic without first marking selected coins as `SPENT_LOCALLY` (optimistic-spent pattern).
- **blocking** — Hand-rolled keypair derivation outside `CoinKeypairDerivation`. Path is `//pps//coin//<n>`.
- **blocking** — Truncating a voucher batch to stay under a cap when proof count must match exactly.
- **major** — New coinage origin built outside `CoinageTransactionOriginFactory`. Add a new `AsCoinageInfo` sealed branch instead.
- **major** — New transfer strategy implemented outside `TransferPlanner`; should be a new `tryGet*Plan()` method.
- **major** — `ExternalPaymentService` wiring that doesn't route through coinage's transfer planner and submission use case.
- **major** — Chat-extension watching on-chain coinage events but holding state inside the bot class (use a `*StateHolder`).
- **blocking** — A coinage retry that mints fresh outputs instead of re-arming the row with its registered ones (claims, in-chat sends).
- **major** — A transfer strategy that builds or registers extrinsics itself instead of returning transactions to schedule.
- **major** — A coinage submission policy that reimplements the wait / build / give-up / retry decisions instead of composing `InputGatedSubmissionPolicy` with a `CoinageRebuild`.

## Statement-store communication (`architecture/statement-store-communication.md`)

- **blocking** — New feature reusing another feature's `SharedSecretDerivationDomain` value.
- **blocking** — Submitting a statement without ensuring slot allocation (`StatementStoreSlotAllocator.allocate(...)`).
- **major** — Hand-rolled polling/decrypt/verify loop instead of `CommunicationSession`.
- **major** — Multiple consumers sharing one `CommunicationSession` — sessions aren't multiplexed; one session per (feature, peer pair).
- **major** — Raw JSON or non-SCALE payloads on the wire; should be `@Serializable` + `BinaryScale`.
- **major** — Reusing a peer's session pin for an unrelated use case.
- **major** — Message fragmentation across multiple statements; redesign to fit `maxStatementSize`.

## Data transport (`architecture/data-transport.md`)

- **blocking** — Reusing a reserved `UseCaseId` (`webrtc_renegotiation_internal_use_case`, `webrtc_media_state_use_case`).
- **major** — Signaling logic implemented inside the transport instead of behind a `PeerChannelSignaling` impl owned by the consumer.
- **major** — Consumer reaching into the underlying WebRTC `DataChannel` directly; the only entry points are `subscribeMessages` / `send`.
- **major** — Calling `send(...)` before `awaitOpen()` (or without an `isOpen()` guard).
- **major** — Custom reconnect logic inside the transport; the consumer chooses the policy.
- **major** — Mixing media tracks and data-channel payloads through the same API — media is `MediaTrackProvider`, data is `DataTransport`.
- **minor** — Hardcoded use-case id literal at the call site instead of a `const val`.

## Chat / ChatExtension (`architecture/chat-extension.md`)

- **blocking** — A `ChatExtension` that owns N rooms implemented as a `ChatBot` (Bot is 1-room-only).
- **blocking** — Overlay influences screen layout below it (e.g. screens add bottom padding because of pill).
- **blocking** — Fragment classes plumbed through a bot's API to the chat host.
- **blocking** — Renderer state held inside the bot class instead of a dedicated state holder / VM.
- **blocking** — A v1 idiom baked in that v2 (RFC-0002) will retire — e.g. hard-coding that the Host owns the message store.
- **major** — Style parameter silently overridden by an ambient theme value (`LocalGlassMessageBubbles` style).
- **major** — `reverseLayout = false` in the chat feed.
- **major** — Custom item-entry animation instead of `LazyListScope.animateItem()`.
- **major** — Chat module reaching into `ChatEngine` from another module (Engine isn't part of the api).
- **major** — Specific-to-feature flag (`is_read`) abused outside the chat feature.
- **major** — Custom message rendered globally per-extension but with per-room hacks tacked on (accept the v1 limitation; v2 will fix).
- **minor** — Overlay component named after a specific feature (`GamePillOverlay`) when it's generic.

## Products / HostApi (`architecture/host-api-products.md`)

- **blocking** — A new host call added **without** a referenced RFC stating its permission model. (If no RFC exists / RFC doesn't address permissions, the reviewer must escalate to the user.)
- **blocking** — `ProductId` constructed from arbitrary strings; should go through `ProductId.fromUrl(...)` / `fromStoredValue(...)`.
- **blocking** — WebView ownership ambiguity: two classes both call `destroy()` on the same WebView.
- **major** — Container script loading split inconsistently across environments (use `ContainerInjectionStrategy` uniformly).
- **major** — Handler group reaching into a global "current product" instead of injected `CallingProductIdProvider`.
- **major** — Multi-room product implemented as multiple `ChatExtension`s instead of a single `ProductChatExtension` with multi-room behavior.
- **major** — TrUAPI bridge logic reimplemented on the Kotlin side instead of delegated to `HostApiInteractor` (`ProductTrUAPIHostBridge` is a thin adapter; the Rust core dispatches).
- **major** — A `HostBridge` callback violating the threading contract: dispatcher-thread callbacks blocking, or UI touched off the main thread (see `host-api-products.md`).
- **major** — A `UserConfirmationReview` variant mapped without coverage in `ConfirmationReviewMappingTest`, or a non-signing variant routed anywhere but the fail-closed `null` path.
- **major** — Runtime selection decided anywhere other than `ProductRuntimeSettings` read at session creation.
- **major** — `NavigationPolicy` branching on URL string inside the policy (classification is external via `CoreNavigateClassifier`).

## Transactions (`architecture/transactions.md`)

- **blocking** — Background work submitting an extrinsic without `BackgroundChainConnection.Session`.
- **blocking** — Sharing a single keypair across roles (e.g. identity keypair as device keypair); should derive a separate one.
- **blocking** — Truncating a batch when the proof count must match the on-chain operation count.
- **major** — Manual binary encoder for an argument when `BinaryScale` / `autoEncodedArgs` covers it.
- **major** — Origin's `paysFees` flag overridden / second-guessed at the caller.
- **major** — Multi-extrinsic batches missing `ExtrinsicBuilderSequence` and managing nonces by hand.
- **major** — Inheritance from `AsPersonTransactionExtension` (or similar base) when composition via `SetTransactionExtensionOrigin` is enough.
- **minor** — Polling chain state instead of subscribing (`observe`).
- **blocking** — A durable verdict about a submitted attempt written outside `DurableVerdictWriter`.
- **blocking** — `AsyncDurableSubmissionPolicy.canRetry` retrying `DISPATCH_FAILED` / `REJECTED` failures without a bound (`architecture/transactions.md` rule 11).
- **blocking** — A policy rebuild that consumes or mints anything other than the assets registered for its row.
- **major** — `canRetry` reading the chain; work shared by a bucket repeated per transaction in `prepareSubmission`.

## Chain integration (`architecture/chain-integration.md`)

- **blocking** — Editing existing hex in a SCALE conformance test (canary tripped).
- **blocking** — Manual binary serializer for a new type when `@Serializable` / kotlinx-serialization SCALE works.
- **major** — New `QueryableStorageEntry` declared with `binding = ::bindXxx` (legacy) instead of the reified `storageN<T>(name)` form. Make the value `@Serializable`.
- **major** — Hand-rolled `storageType.fromHex(...)` + `Scale.decode(...)` at a call site — declare a typed `QueryableStorageEntry` and use `.query()` / `.observe()`.
- **major** — Raw `ByteArray` in a domain model (use `DataByteArray`).
- **major** — Raw `String` for `AccountId` / `EncodedPublicKey` / chain hash (use typed wrappers).
- **major** — `getOrThrow()` on a chain call inside a ViewModel.
- **major** — Manual `Result<>` for a runtime API that returns `Result<T, E>`; should use `ScaleResult.toResult()`.
- **major** — Diffing subscription emissions before persisting (overwrite is fine).
- **minor** — Hard-coded base URL when `NetworkApiCreator` provides one.

---

## RFC-0002 (chat north-star) alignment

When the diff touches chat, ask explicitly:

- Does this proposal assume the Host owns the message store? (v1: yes; v2: no — message source is the Product.)
- Does this proposal hard-code that custom message rendering is per-extension global? (Accept v1 limitation; don't add hacks.)
- Does this introduce a new room-shape that won't translate to "room scoped to a Product"?
- Does this add a footer / toolbar / action that can't easily become a per-room handler registration in v2?

A "yes" to any of these isn't automatically blocking, but it must be **named** in the review with a recommendation.

---

## Verdict format

```
### Architecture
- 2 blocking, 4 major, 3 minor.
- Module boundaries clean. Transactions OK.
- Chat-extension section needs rework: overlay drives screen layout (blocking) and bot owns renderer state (blocking).
- Recommend: split into 2 PRs — render-only changes vs state-ownership refactor.
```

> The "Pattern-emergence flag" procedure (when to propose new doc sections) lives in `.claude/skills/reviewer.md` — load that skill for the full output format. It's procedural, not a rule list.

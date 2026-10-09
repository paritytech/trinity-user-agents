# Products & Host API Architecture

## Overview

Products are web mini-apps running in a WKWebView, bridged to native iOS via a JavaScript-to-Swift interface. The Products module (33 sub-modules) handles product discovery, display, and interaction.

## Three-Tier Architecture

```
Products (UI)  →  HostApi (JS↔Swift bridge)  →  ChatExtension (messaging integration)
```

1. **Products** — WKWebView container, product lifecycle, account management
2. **HostApi** — JavaScript bridge (ContainerBridge pattern) for native capabilities
3. **ChatExtension** — integration with chat for product-related messages

## Key Components

### Products Package (`Packages/Products/`)
- Product account models
- WebView JS bridge implementation
- Product container with `product-container/` web resources

### Module (`Modules/Products/`)
- 33 sub-modules for various product screens and flows
- `Modules/SPA/` — Smart Proposal Agent product
- `Modules/SPAMoreActions/` — SPA additional actions

## Host API Bridge

The HostApi allows web products to:
- Access wallet/account information
- Request transaction signing
- Send/receive chat messages
- Access device capabilities (camera, etc.)
- Navigate within the app

### Rules for Host API

1. **One WebView owner per runtime** — never share a WebView instance across products
2. **RFC-first for new host calls** — new JS→Swift bridge methods require design review
3. **Permission model required** — products must declare required capabilities
4. **Deep links supported** — products can be opened via deep links (see `AppConfig` deep link definitions)
5. **Parse parameters into typed models at the boundary** — when receiving JS bridge params, parse into a Swift model immediately (e.g., `ScheduledNotificationRequest`), don't pass raw dictionaries deeper
6. **WebView features MUST have proper VIPER modules** — even for WKWebView-based features, create a full VIPER module. Business logic and async tasks go in the Interactor, NOT the Wireframe
7. **Configuration URLs in AppConfig** — all product URLs, API endpoints go in `AppConfig`, not hardcoded in ViewFactory or module code
8. **Typed error codes over string matching** — errors conforming to `HostCallCodedError` (ContainerBridge) serialize as `{"error": {"code", "message"}}` so product scripts reconstruct typed errors (e.g. `CreateProofErr`). New host calls should use coded errors instead of JS-side substring matching.

9. **Accounts-protocol calls share one handler between surfaces** — a host call that also arrives
   over SSO gets a single `AP*Handler` created per caller by `APPersonhoodHandlerFactory`; user
   confirmation and caller-input bounds live in that handler (bounds checked BEFORE prompting), so
   they exist once for both the in-WebView host API and the SSO request handler. Concrete case:
   `accountSignVrf` (RFC-0023) → `APSignVrfHandler` — prompt-only for every caller, 32-item / 8 KB
   transcript bounds.
10. **Accounts riding over the bridge use the `ProductAccountId` tuple** — pass the account as the
    `[productId, derivationIndex]` tuple (like `accountGet`) so payload coding stays fully derived;
    don't flatten it into separate fields. The second element is the `Either<u32, [u8;32]>`
    account selector: a JSON number (primary form) or a `0x`-prefixed 32-byte hex string;
    parse it as `ProductAccountSelector`, never as a bare `UInt32`.
11. **Product account paths carry the index as a hex segment** — `//product//{productId}/{index}`
    renders the 32-byte index via `DerivationIndex32.asPathSegment()` (the junction parser maps a
    hex segment straight to the raw chain code). Build paths via `ProductAccountId.derivationPath()`
    / `ProductDerivationPath` (KeyDerivation) — never hand-format the index segment or feed a
    numeric index into a path.

## Product Runtimes

A product runs in one of two runtime modes: **native** (Swift handlers behind the
ContainerBridge host API) or **rust** (the TrUAPI core serves product requests over a
localhost ws-bridge; no ContainerBridge). Runtime selection is by the
`truApiRuntimeEnabled` settings flag, read only at runtime-creation time. Toggling it
from Debug Settings prompts a restart alert: confirm terminates the app (`exit(0)`) so
the next launch builds every surface against the new flag; cancel reverts the flag.
There is no live runtime switching.

**Rust is the default.** Read the flag only through
`SettingsManagerProtocol.isTrUAPIRuntimeEnabled`
(`Common/UserDefaults/SettingsManager+ProductRuntime.swift`), never through the generic
`value(for:)` helper — that helper defaults every boolean setting to `false` and so
silently selects native. An unset flag means rust in every build that ships the Debug
Settings toggle (`TESTNET_FEATURE`: Debug, DevCI, Nightly, Safetynet); Release ships no
toggle and stays on native, so the default there is deliberately still native.

Layout under `polkadot-app/Modules/Products/`: `ProductRuntimeProtocol.swift` at the root
defines the runtime protocols (+ the SPA factory protocol); `Chat/Native/`, `Chat/Rust/`,
`SPA/Native/`, `SPA/Rust/` hold the per-surface, per-mode runtimes and scripts factories.
Shared rust infrastructure lives in `TrUAPI/`: `RustRuntimeEnvironment` +
`RustRuntimeSessionParams` (per-runtime core assembly), `RustRuntimeBridge`
(HostCallbacks), `TrUAPIConfirmationPresenter`, `Connection/` (chain connection pool, rpc
adapter, frames), `Storage/` (`TrUAPILocalStorage`), and `Preimage/`
(`TrUAPIPreimageCache`). The rust core itself is the remote `TrUAPIHost` SPM package. The
debug playground lives in `Modules/DebugSettings/` (`TrUAPIPlaygroundViewFactory`,
presented by `DebugSettingsWireframe.showTrUAPIPlayground(from:)`), and assembles its SPA
view via `SPAViewFactory.createRustView`; it runs the production session (root entropy +
settings username) — there is no env-var/e2e launch contract. `Modules/SPA/` keeps just
the VIPER screen and web-view helpers — no runtime code.

Rules:

1. **Runtimes are assembled at creation time, per call.** SPA uses
   `SPARuntimeFactoryProtocol.createRuntime(for productId:)` (the factory is per-view: it
   carries `SPAConfiguration`, resolver, scheme handler proxy, routers); chat rust
   runtimes are built by `ProductBotFactory.createRustRuntime(product:)`, parallel to
   `createNativeRuntime` — no separate chat factory type. There is no session/handle
   entity: each creation assembles core + bridge + connection pool + ws-bridge via a
   fresh `RustRuntimeEnvironment`. Nothing rust-related is built at app startup.
2. **The runtime owns setup and teardown.** `SPARuntimeProtocol.start(with engine:)`
   activates the local session and starts the localhost ws-bridge
   (`CoreModel.startSession()` — creation via `makeRustCore` is side-effect free), resolves
   content, publishes the scheme handler, builds and injects the scripts with the fresh
   bootstrap, and returns the page URL; `dispose()` destroys the engine and tears down the
   rust core. Interactors only coordinate: create runtime, `attach(presentationView:)`,
   start, forward URL/error.
3. **Rust pieces sit behind protocols.** Runtimes hold `TrUAPIHostCoreProtocol`
   (TrUAPIHost package) + `TrUAPIChainConnecting` and call `stopWsBridge()` /
   `closeAll()` once on `dispose()` (idempotent). Runtimes are actors: `dispose` flips
   `disposed` before its first await and `start` re-checks it after every await, so a
   start superseded by dispose can never re-activate the session. There is NO deinit
   teardown — the reference holder must call `dispose()` before releasing a started
   runtime (holders enforce it from their deinit via `Task { [runtime] in … }`; a debug
   assert fires on a started-but-undisposed drop). Unit tests mock both protocols — the
   Rust cdylib never boots in unit tests — and must dispose any runtime they started.
4. **Session data lives in the environment.** `RustRuntimeEnvironment.sessionParams`
   (`RustRuntimeSessionParams`: secret + `liteUsername`) is resolved fresh each time the
   environment is built — per runtime creation (root entropy + settings username in
   production; the playground uses the same). No override types, no activation
   branching, no startup-frozen session data.
5. **Shared rust-runtime JS ships inside the TrUAPIHost package** — the bundled lockdown
   container behind the public `ContainerScriptBundle.load()` accessor. Its TypeScript
   source and esbuild live in the truapi repo, not in this one.
6. **Routers are consumed through `ProductRoutersFacadeProtocol`.** One facade instance per
   context, composed via `ProductRoutersFacade.chatExtension()` / `.sso()` /
   `.spa()` — never pass router parameter lists. The facade exposes signing,
   navigation, permission, top-up, payment, create-proof prompt, sign-vrf prompt,
   allowance prompt, and statement-sign prompt routers; one `setPresentationView` fans the
   anchor out to all of them. Rust-core user confirmations
   (`TrUAPIConfirmationPresenter`) resolve through the same facade: signing reviews
   (signPayload/signRaw/createTransaction) present the confirm-only `ProductsSignConfirm`
   sheet via the signing router (the rust core signs after approval). That sheet is
   wallet-free: `ProductsSignConfirmModelFactory` renders the review (call decode via the
   shared `PolkadotSigningCallRenderer`, details JSON, `<Bytes>`-wrapped raw payload) with
   no account/wallet resolution, and falls back to raw hex when a call can't be decoded —
   the user reviews the raw bytes before approving, so the "never fall back to raw
   silently" rule (which guards the actual signing path) does not apply to this display.
   Statement signing
   uses the statement-sign prompt router, identity/preimage/account-access/alias reviews
   reuse the permission prompt (prompt-only — nothing is persisted, the core owns
   permission memory), and proof/allowance/VRF reviews reuse their native prompt routers.
   Rust-side cancellation of a pending confirmation resolves false immediately and
   dismisses the prompt, unless another prompt is presented above it. Prompts are
   registered with the task's `PromptPresentationScope` when `ProductsRouter` presents
   them; withdrawing the scope dismisses them and refuses later ones. The SSO
   coordinators use this for a pairing host's `Cancel`: the native queue withdraws the
   named request (drops it if queued, cancels it if running, remembers up to 64 not yet
   received) and drops any response to it; the TrUAPI coordinator hands `Cancel` to the
   core ahead of its queue.
   No router presents on `topmostViewController`: every routing protocol has
   `setPresentationView`, and prompts deliver denial/rejection when no view is attached.
   The SPA runtime factory receives the view (`setPresentationView`) and anchors the
   routers on every `createRuntime` — interactors never see the view; chat runtimes
   forward `attach(presentationView:)` to their facade; the SSO facade is owned by
   `MessageExchangeSignInHostCoordinator` and its view is bound by
   `MainTabBarViewFactory` through `serviceCoordinator.signInHostCoordinator` (prompts
   arriving before the tab bar exists deny).
7. **One scripts protocol per surface, one factory per runtime mode.** `SPAScriptsMaking`
   (`[JSEngineScript]`, doc-start injection) is implemented by
   `SPANativeRuntimeScriptsFactory` and `SPARustRuntimeScriptsFactory`; `ChatScriptsMaking`
   (`[String]`, evaluated in order — Products package, consumed by `ProductsScriptExecutor`)
   is implemented by `ChatNativeRuntimeScriptsFactory` and `ChatRustRuntimeScriptsFactory`.
   Rust factories load the TrUAPI bundle directly — no adapter types.
   Bootstrap-before-container script ordering is load-bearing.
8. **Bridge params/results go through Codable DTOs, not hand-rolled JSON.** In a
   `registerRequestHandler`/`registerSubscriptionHandler`, decode the whole `params` with
   `params.map(to: SomeRequestDto.self)` and encode results with `result.toScaleCompatibleJSON()` —
   do not pull individual fields via `params.mapOrMissing(for:)` or assemble
   `JSON.dictionaryValue([...])` by hand. Model requests/results as `Codable` structs using the
   SubstrateSdk property wrappers (`@StringCodable`, `@HexCodable`, `@OptionStringCodable`,
   `@OptionHexCodable`); encode tagged unions as a `tag: String` struct plus the fields that tag
   carries (see `StatementProofDto`, `CreateStatementProofAuthorizedDto`,
   `PaymentTopUpRequestDto`/`HostPaymentTopUpStatusDto`). Keeps the wire contract in one typed place,
   off the handler bodies.

## Seams

| Seam                          | Where                                    | When to touch                        |
|-------------------------------|------------------------------------------|--------------------------------------|
| Product container bridge      | `Packages/Products/`                     | Adding new JS↔Swift bridge methods   |
| Payment approval              | `Modules/Products/PaymentRequest/`       | Changing who sees the payment sheet (allowlist lives in `ProductAutoAllowList`) |
| Product module sub-modules    | `polkadot-app/Modules/Products/`         | Adding new product screens           |
| Deep link handlers            | `AppConfig/AppConfig.swift`              | Adding product deep links            |
| SPA module                    | `polkadot-app/Modules/SPA/`             | Smart Proposal Agent changes         |
| DotNs contract reads          | `Packages/Products/Sources/Products/DotNs/ReviveDotNsContractApi.swift` over `Revive` (architecture/revive.md) | Resolver / registry lookups, cache policy |
| Product runtimes              | `polkadot-app/Modules/Products/{Chat,SPA}/{Native,Rust}/` | Adding a runtime mode, changing runtime lifecycle |

## Anti-Patterns

| Anti-pattern                              | Do instead                                    |
|-------------------------------------------|-----------------------------------------------|
| Sharing WebView between products          | One WebView owner per product runtime         |
| Adding host API methods without review    | RFC-first approach for new bridge methods     |
| Hardcoding product URLs                   | Use configuration/remote config               |
| Direct native calls from JS              | Go through the HostApi bridge layer            |
| Hand-parsing/encoding bridge JSON fields | Decode/encode Codable DTOs (`params.map(to:)` / `toScaleCompatibleJSON()`) |

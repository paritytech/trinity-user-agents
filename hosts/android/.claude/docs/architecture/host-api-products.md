# Products / Host runtimes / ChatExtension

Three distinct concepts. Don't conflate. Products run on one of **two host
runtimes** — the native JS-bridge HostApi or the Rust TrUAPI core — selected
per session by `ProductRuntimeSettings` (debug toggle, default TrUAPI).

## Rules at a glance

1. **`blocking`** — A new host call added without a referenced RFC stating its permission model. New host calls are protocol: they live in the Rust core (`paritytech/trinity-user-agents`) and, on the native runtime, in a `HostCallHandlerGroup`. If no RFC or the RFC is silent on permissions, escalate to the user; don't invent a policy.
2. **`blocking`** — `ProductId` constructed from arbitrary strings. Always `ProductId.fromUrl(uri)` / `fromStoredValue(...)`.
3. **`blocking`** — WebView ownership ambiguity: two classes both call `destroy()` on the same WebView. Single owner.
4. **`major`** — Container script loading split inconsistently across environments. Use `ContainerInjectionStrategy` uniformly.
5. **`major`** — Handler group reaching for a global "current product" instead of an injected `CallingProductIdProvider`.
6. **`major`** — `NavigationPolicy` branching on URL string inside the policy. Classification is external (`DotNsUtils.classifyNavigation`).
7. **`major`** — Multi-room product implemented as multiple `ChatExtension`s instead of a single `ProductChatExtension` (see `chat-extension.md`).
8. **`major`** — Factory-of-factory-of-factory chain — collapse to single `Factory.create(scope, config)`.
9. **`major`** — TrUAPI bridge logic reimplemented on the Android side instead of delegated to `HostApiInteractor` (signing, permissions, chain, theme). `ProductTrUAPIHostBridge` is a thin adapter, not a second dispatch layer.
10. **`major`** — A `HostBridge` callback violating the threading contract: prompt-driven suspend callbacks may stay pending on a user decision; dispatcher-thread callbacks must return promptly and never block (see § Threading contract).
11. **`major`** — A `UserConfirmationReview` variant mapped in `ConfirmationReviewMapping` without coverage in `ConfirmationReviewMappingTest`, or a non-signing variant routed anywhere except the fail-closed `null` path.
12. **`major`** — Runtime selection decided anywhere other than `ProductRuntimeSettings` read at session creation (no mid-session switching, no per-feature overrides).
13. **`minor`** — Inlining "derive product id from URL" at a call site when the `CallingProductIdProvider` abstraction is already in scope.



| | **Product** | **Host runtime** | **ChatExtension** |
|---|---|---|---|
| What it is | A web-based mini-app | Native HostApi (JS↔Kotlin bridge) **or** the Rust TrUAPI core | A native chat plug-in |
| Where it lives | Loaded into a WebView from `.dot` script | `feature/products/impl/.../hostApi/` (native) / `.../domain/truapi/` + `:bindings:truapi-host` (TrUAPI) | `feature/chats/api` + per-feature impls |
| Identified by | `ProductId` (from `.dot` domain) | n/a — serves whoever's connected | `ChatExtensionId` |
| Lifecycle | Per-WebView session | Per `HostApiSession` / per `ProductTrUAPIHostBridge` (scoped to a WebView lifetime) | Hilt singleton |
| Composition | Selected by navigation; loaded from DotNs | `HostCallHandlerGroup`s in `HostApiEnvironment` (native); Rust core dispatches, Kotlin implements `HostBridge` callbacks (TrUAPI) | `@IntoSet` Dagger multibinding |

A product can *integrate* with chat (see `chat-extension.md § Products and chat`). That integration produces a `ProductChatExtension` — a single `ChatExtension` that hosts the product's JS in a hidden WebView and forwards `ChatHostCalls`. Chat products run the **native runtime only**; the core has no chat host yet.

---

## Products

### What is a Product

`feature/products/api/.../model/Product.kt`:

```kotlin
data class Product(
    val id: ProductId,
    val name: String,
    val scriptUrl: String,    // legacy, debug menu only
    val contentHash: String?, // hash of DotNs content
) : Identifiable
```

`ProductId` is a data class derived from the `.dot` domain (`feature/products/api/.../model/ProductId.kt`). Construction is restricted: `ProductId.fromUrl(uri)` or `fromStoredValue(value)`. Anything that wants to "use the current URL as a ProductId" is wrong.

### Product environments

Three environments where products run:

| Env | Where | Rendering | Navigation | Runtimes |
|---|---|---|---|---|
| **SPA Browser** | Full-screen WebView | Visible | Inline (same .dot and cross-.dot stays in WebView) | native / TrUAPI |
| **Explore** | Catalog → product list | Visible | Same .dot in WebView; cross-.dot opens a new SPA browser | native / TrUAPI |
| **Chat** | Hidden WebView under a `ProductChatExtension` | Invisible | Disabled (rejects all navigation) | native only |

On the native runtime each environment composes a different `HostApiEnvironment`; on TrUAPI each builds a `ProductTrUAPIHostBridge` via `TrUAPISessionStarter`.

### ProductRepository / installation

Products are stored in the Room DB and resolved via `ProductRepository`. Scripts are seeded through DotNs and cached with their content hash.

> **Workers load by URL, never as an inlined string.** A worker's entry module can use relative imports against sibling files in the same archive, which only resolve when the script has an origin. Loading it as a blob/`evaluate`-style string silently breaks multi-file workers. The injection must be a `<script type="module" src="…">` pointing at a real served URL.

---

## Runtime selection

`ProductRuntimeSettings` (`feature/products/api/.../domain/runtime/`) is the single switch: a prefs-backed, **debug-only** toggle defaulting to the TrUAPI core (release builds always run native), surfaced in the debug menu. It is read **once per session creation** — flipping it affects the next session, never a live one. The seams that read it:

- `RuntimeSelectingSpaHost` — the `SpaHost` binding; picks `NativeSpaHost` or `TrUAPISpaHost` per `createSession`.
- `ProductTabSessionFactory` — picks `NativeProductTabSessionFactory` or `TrUAPIProductTabSessionFactory` per browser tab.
- `ExploreProductsViewModel` — forks its `SessionComponents` per Explore session.

Don't add a fourth decision point without wiring it through `ProductRuntimeSettings` the same way.

Flipping the toggle prompts a restart (`RestartAppUseCase`, cancel reverts the
flag) because the two runtimes namespace product storage differently and keep
their permission memory in different places: a product switched mid-session
reads an empty store, and anything already open keeps the runtime it booted
with. Restarting makes the switch a clean boot rather than a half-migrated one.
Matches `polkadot-app-ios-v2`, which prompts and calls `exit(0)`.

---

## HostApi — the native runtime

The JS↔Kotlin bridge. Products call JS methods that route through to Kotlin handlers.

### `ContainerBridge`

`feature/products/impl/.../jsEngine/ContainerBridge.kt`:

Generic request/response + subscription handler registry. Message format:
```json
{ "type": "request" | "subscribe" | "unsubscribe", "id": "...", "method": "...", "params": {...} }
```

`bridge.registerHandler<P, R>(method) { params -> ... }` for request/response.
`bridge.registerSubscription<P, E>(method) { params, emit -> ... }` for streams.

### `HostApiSession`

Orchestrates runtime + bridge + handlers for one product instance, scoped to a `CoroutineScope`. Auto-disposes when scope cancels.

### `HostApiEnvironment` — composition

```kotlin
class HostApiEnvironment(
    val navigationPolicy: NavigationPolicy,
    val injectionStrategy: ContainerInjectionStrategy,
    val handlerGroups: List<HostCallHandlerGroup>,
)
```

Three orthogonal axes (navigation, injection, handlers), composed per environment.

### `HostCallHandlerGroup` — the modular handler unit

```kotlin
interface HostCallHandlerGroup {
    fun registerOn(bridge: ContainerBridge)
}
```

Handler groups in code (today):

**Shared across all environments**: `AccountHostCalls`, `ChainHostCalls`, `SigningHostCalls`, `NavigationHostCalls`, `PaymentHostCalls`, `PermissionHostCalls`, `StatementHostCalls`, `PreimageHostCalls`, `AllowanceHostCalls`, `EntropyHostCalls`, `NotificationHostCalls`, `UserIdHostCalls`, `StorageHostCalls`.

**Chat-only**: `ChatHostCalls`.

`HostCallGroupFactory.createShared(...)` builds the shared list; `createChatGroup(...)` adds chat-specific calls.

### Adding a new host call — **always RFC-first**

A new host call is a public protocol surface. New host calls **require an RFC** in `paritytech/trinity-user-agents` that states:
- Method name, params, response.
- **Permission model** — what permission this call requires (if any), how the user grants it, scoping per product.
- Caching / subscription semantics.

If an RFC doesn't exist or doesn't address the permission model: **stop and ask the user**. Do not invent a permission policy on the fly.

On the native runtime the call is implemented as a `HostCallHandlerGroup` (register in `HostCallGroupFactory.createShared`, or `createChatGroup` if chat-only, and add the matching `ProductsBotApi` method). On TrUAPI the call is implemented in the `truapi` runtime; the Android side only implements any new `HostBridge` platform callback it needs.

### `CallingProductIdProvider`

Each handler group needs to know **which product** made the call.

- **Chat**: `FixedProductId(productId)` — constant for the extension's lifetime.
- **SPA / Explore**: `UrlDerivedProductId { webView.url }` — extracted from current WebView URL.

Always inject the provider; never let the handler reach into a global "current product".

### Permission gating

`PermissionHostCalls` is the centralized permission gate on the native runtime. Sensitive calls (signing, payments, account access, push) route through it. The exact list of "sensitive" categories is whatever the RFC for each call declares.

---

## TrUAPI — the Rust-core runtime

The product's `@parity/truapi` client talks to the shared Rust core
(`libtruapi`, via `:bindings:truapi-host`) over an authenticated
loopback WebSocket. The core owns wire framing, dispatch, subscriptions, and
orchestration; the Android side implements only native platform callbacks.

The core is compiled from a checkout outside this repo, pinned by SHA as
`truapi_ref` in `.github/actions/install/action.yaml`. `scripts/setup-truapi.py`
puts that checkout in place and points `truapi.dir` at it. The pin and the
Android `HostBridge` implementation move together: bumping one without checking
the other is how a callback goes silently dead, because the generated interface
defaults most members.

### Runtime and executions

The core is one process-wide `TrUAPIHostRuntime` with a `TrUAPIProductExecution`
per product connection, and the Android side mirrors that split:

- `TrUAPIHostRuntimeProvider` (`@Singleton`) builds the runtime lazily on first
  use from `HostRuntimeConfig` (host identity, People/Bulletin genesis hashes,
  the local session) and keeps it for the process. Its runtime-level
  `HostBridge` serves only what the signing runtime asks for — core storage,
  auth state, signing-side chain access, and the confirmations SSO raises.
  Product-scoped calls have no product there and fail closed, as on iOS. It
  re-activates the local session when the wallet account changes.
- `ProductTrUAPIHostBridge` (`feature/products/impl/.../domain/truapi/`) opens
  one execution per product WebView with a product-scoped `HostBridge` and
  delegates every wired callback to `HostApiInteractor` — the same facade the
  native handler groups sit on. Do not reimplement dispatch on the Kotlin side.
  Closing the bridge closes its execution; the runtime stays up.

`TrUAPISessionStarter` owns the boot sequence for all consumers: resolve the
shared runtime, register the bootstrap at document start
(`WebViewCompat.addDocumentStartJavaScript`), then trigger the initial page
load — the bootstrap must be in place before the page loads or the product
never connects. The bootstrap carries the stored WebRTC decision as a literal
(`permissionAuthorizationStatus`, a peek that never prompts), so a fresh grant
only applies once the page reloads.

### Local session

SSO pairing is not in the core yet (truapi#334), so the core has no session of
its own and without one it holds no keys — every account and signing call fails
regardless of what the user approves. `TrUAPILocalSessionSource` supplies
`HostRuntimeConfig.localSessionSecret` from the wallet's **raw BIP-39 entropy**,
which the core derives the session's root and identity keypairs from directly;
anything derived would give the same recovery phrase different product accounts
than iOS derives from it. The core activates the session inside the runtime
constructor, so `TrUAPIHostRuntimeProvider` builds it off the main thread. A
wallet with no readable passphrase logs and boots without a session — products
still load, signing still fails.

### Typed FFI payloads

Structured payloads cross the FFI **typed** (UniFFI records/enums:
`NativeUserConfirmationReview`, permission and feature requests) — there is no
hand SCALE decoding and no byte-parity risk. The only hand-written layer is
`ConfirmationReviewMapping`, which maps every review variant onto a
`TrUAPIConfirmation` (signing variants via the app's `SigningRequestBody`). It
is **exhaustive with no catch-all**, so a variant added upstream breaks the
compile instead of silently becoming a denial, and **fail-closed** on a payload
it cannot describe. Every variant is covered by
`ConfirmationReviewMappingTest`.

### Threading contract

- **Prompt-driven callbacks** (`confirmUserAction`, `devicePermission`, `remotePermission`, `navigateTo`, `featureSupported`) are `suspend` and awaited by the core — they may stay pending until the user decides.
- **Dispatcher-thread callbacks** (chain, theme, storage, core log) run inline and **must return promptly** — no blocking work. Chain I/O goes through the non-blocking `WebSocketChainProvider`, one per bridge so each notifies the runtime or execution it dials for; theme is served from a cached value.
- `devicePermissionStatus` and `currentLocale` are left on the shell defaults (not applicable / system locale) — wire them when a product needs them.

### Chain agreement

`supportedChains`, `featureSupported(Chain)` and `chainConnect` all answer from
one `TrUAPIChains` snapshot — resolved at `attach` per execution, and once at
boot for the runtime — through
`TrUAPIChains.canDial`. Never answer any of them from the registry directly: the
registry holds chains with no `wss` endpoints, so the host would advertise or
promise a chain `chainConnect` then refuses. `TrUAPIChainAgreementTest` pins the
invariant.

### Storage

`EncryptedHostStorage` (product-scoped) and `EncryptedHostCoreStorage` (core
auth / pairing state) back the core's storage callbacks, encrypted at rest and
namespaced per product. Don't bypass them. The `Prefs*` pair in the binding's
`androidTest` source set is the diagnostics test double, not a production path.

---

## JsRuntime — abstraction over the JS engine (native runtime)

`feature/products/impl/.../jsRuntime/JsRuntime.kt`:

```kotlin
interface JsRuntime {
    suspend fun initialize()
    suspend fun loadInitialPage(content: PageContent)
    fun evaluate(script: String)
    fun evaluateAsModule(script: String)
    suspend fun waitForReady(): JsRuntime  // returns self once ready
    // ...
}
```

Today there's `WebViewRuntime` (Android WebView). Future work may add a QuickJS-backed runtime. **Code consuming a `JsRuntime` must not assume WebView semantics.**

### `ContainerInjectionStrategy`

Two strategies based on environment:

| Strategy | Used by | Behavior |
|---|---|---|
| `ExplicitInjection` | Chat | Load empty page → wait ready → eval bridge + container scripts once. |
| `PageLoadInjection` | SPA / Explore | Hook `onPageStarted` → inject bridge + container scripts on every page load (refresh-safe). |

### WebView ownership

**One owner per WebView.** `BrowserWebViewProvider` owns the WebView lifecycle; UI subscribes to a read-only `StateFlow<WebView?>` to render. UI must not also call `destroy()`. `PageLifecycleSource` (implemented by the provider) is the narrow interface callers use to observe `onPageStarted` / `onPageFinished` without holding the full provider.

---

## NavigationPolicy — strategy per environment

`feature/products/impl/.../hostApi/navigation/NavigationPolicy.kt`:

```kotlin
sealed interface NavigationPolicy {
    fun handleNavigation(type: DotNsNavigationType, destination: Uri): NavigationResult

    object Disabled : NavigationPolicy                                  // Chat: reject all
    class InlineNavigation(...) : NavigationPolicy                      // SPA
    class HostApiNavigation(...) : NavigationPolicy                     // host-driven loads
    class CatalogNavigation(val onProductSelected: (ProductId) -> Unit) // Explore
}
```

Classification is external and shared by **both runtimes**:
`CoreNavigateClassifier.classify(origin, destination)` wraps the core's
`parse_navigate` so every TrUAPI platform categorizes `.dot` navigation
identically. A `null` result means the core rejected the input — it must be
blocked, never loaded. The policy only dispatches on the classified type; never
branch on URL inside the policy. (`DotNsUtils.classifyNavigation` is gone.)

---

## Per-product storage isolation

Native runtime: `StorageHostCalls` provides a per-product key-value store namespaced by `ProductId`. TrUAPI: the core namespaces per product over the prefs-backed stores. Either way, two products cannot read each other's storage — don't bypass it.

---

## Pocket — the host-owned card collection

Spec: trinity-user-agents RFC "Pocket modality" (#609), core implementation #706, on top of Unified Renderer (#633) and
Worker Lifecycle (#632). The host owns the collection and is its only writer; a product observes its own cards over
the wire (`Pocket::list_subscribe`) and may remove one (`Pocket::remove_card`). There is no add call: a card enters
when the user follows a `polkadotapp://<product>.<tld>/-/pocket/add?card=<id>` deeplink and approves the sheet. The
first path segment `-` is reserved for host-handled targets; `ProductSpaDeepLinkHandler` declines it.

Seams, all in `feature/products/api/.../domain/pocket/`:
- `PocketCollection` — `observeCards()` and user removal. One pinned card, Humanity on the personhood product
  (`AssetPinnedPocketCards`), is a constant with a bundled face; added cards live in Room (`pocket_cards`) with the
  face they were approved with. Balance and Scarcity are not pinned: the products behind them publish no Pocket
  cards, and the host already draws its own native cards for that ground. The newest face a product streams is kept
  for every card,
  pinned ones included, so a card wears what its product last drew rather than the bundled stub at cold start.
- `PocketFaceSource.observeFace(key)` — the cached face first, then every tree the product streams; `sendAction`
  carries a press back; `resolveImage` fetches `Image` sources from the archive or the Bulletin gateway.
- The face vocabulary is the renderer's: `RendererNodeJsonDecoder` reads a `{ tag, value }` preview into the core's
  `RendererNode`, `RendererNodeMapping` turns that (or a streamed tree) into `JsWidget`, `JsWidgetRenderer`
  (`api/presentation/widget`) draws it. Trees deeper than 32 levels are rejected.
- A card expands in place rather than into a sheet: the wallet's Pocket screen moves the same card element to the top
  of a full-screen page with the shared-element transition the native cards use, and hosts the product under it through
  `SpaHost` (`api/presentation/spaHost`, with `ProductWebViewHost`). `ExpandedProductPage` keeps one session at a time
  and cancels its scope when the card collapses. The RFC expands into the `widget` executable; Android has no widget
  host, so the launch URL is the product's app with `?card=<id>`.

Core side (`feature/products/impl/.../domain/truapi/`):
- Pocket is core-only, whatever `ProductRuntimeSettings` says: the native runtime has no Pocket surface.
- `TrUAPIWorkerSupervisor` starts a product's worker on the core's `Start` transition (runtime bridge
  `workerDemandChanged`): hidden `ChatWebViewProvider` WebView, `openProductExecution(WORKER, pocket = ...)` through
  `ProductTrUAPIHostBridge.attach(kind = WORKER)`, bootstrap at document start, entry module by URL. `Stop` tears it
  down. Chat is not served on this path; chat products keep the native worker, so a product with both runs two.
- `TrUAPIPocketFaceStreams` takes one `acquireWorker` reference per collected face, opens `render` on
  `RenderContext.PocketCard`, retries the first `render` while the worker's client is still connecting, and publishes
  renderer actions. Both callbacks of `ProductPocketHostBridge` run on the core's dispatcher thread: the list is a
  snapshot, and a removal completes inline and answers `Removed`, `Absent` or `Privileged`, because the core reads the
  list again right after and republishes it.
- Deeplinks are classified by the core's `parse_navigate` behind `PocketDeeplinkParser`; card ids are screened at
  manifest load with the chat identifier rules (`PocketCardIdentifier`).

Demo: `feature/products/product-sample/pocket-worker`, a worker on the core's own client that publishes one card,
draws its face live and gives it up on its Remove button.

---

## Where new things live

| Concept | Goes in |
|---|---|
| New host call (native runtime) | `feature/products/impl/.../hostApi/handlerGroups/<Name>HostCalls.kt` + `ProductsBotApi` method |
| New host call (TrUAPI) | RFC + implementation in `paritytech/trinity-user-agents` (the `truapi` runtime); a `HostBridge` callback here only if a new native capability is needed |
| New `HostBridge` callback wiring | `feature/products/impl/.../domain/truapi/ProductTrUAPIHostBridge.kt`, delegating to `HostApiInteractor` |
| New review-variant mapping | `feature/products/impl/.../domain/truapi/ConfirmationReviewMapping.kt` + `ConfirmationReviewMappingTest` |
| New `NavigationPolicy` variant | `feature/products/impl/.../hostApi/navigation/NavigationPolicy.kt` |
| New product runtime mode | the consuming feature (SPA Browser VM, Explore VM, ChatExtension) — keep construction close to the user-facing entry point, selected via `ProductRuntimeSettings` |

---

## North star

- **RFC-0020 `host_create_transaction`** — the canonical extrinsic-creation host call. New transaction-shaped host calls compose with it rather than duplicating signing/origin logic.
- **RFC-first host calls** — every new host call carries a referenced RFC stating its permission model before implementation. The `host-api-products.md § Adding a new host call` rule is the gate (already `blocking` in the Rules at a glance).
- **The TrUAPI runtime replaces the native one** — dual-runtime is a transition state, native is the compatibility path. New host-side capability lands in the core first; the native HostApi gets fixes, not features.
- **Thin native bridge** — `ProductTrUAPIHostBridge` adapts `HostBridge` callbacks to `HostApiInteractor`; it does not re-implement dispatch or business logic.
- **Composable `HostApiEnvironment`** — the three axes (`navigationPolicy`, `injectionStrategy`, `handlerGroups`) are orthogonal and remain so. New product modes pick a triple; don't introduce a fourth axis without a design discussion.
- **One owner per WebView** — the provider owns the lifecycle. Treat any drift back toward UI-managed WebView destruction as a regression.
- **Per-product storage isolation** — never bypass the namespaced stores on either runtime.

Probe questions on every host-API / host-bridge PR (named-but-not-blocking):
- Does this host call have an RFC + documented permission model?
- Does the change respect the `HostBridge` threading contract, and is any new review mapping covered by `ConfirmationReviewMappingTest`?
- Does this proposal introduce a new orthogonal axis to `HostApiEnvironment`, or compose within the existing three?
- Does this WebView interaction respect single ownership?

A "no" on the first two or a "yes" on the third must be named in the architect plan.

## Anti-patterns flagged by past PRs

- Factory soup — `SpaProductWebViewProvider` had a factory-of-factory-of-factory chain. Default to single `Factory.create(scope, config)` returning the working instance.
- WebView ownership ambiguity — engine *and* UI both reaching `destroy()`. Single owner.
- `ProductId` constructed from arbitrary strings — restrict construction via `ProductId.fromUrl(uri)` / `fromStoredValue(...)`.
- Container script loading inconsistent across environments — unify via `ContainerInjectionStrategy`; don't have different layers in different envs.
- Reaching into product-specific knowledge from `SessionManager` / generic services — generic services expose generic subscription APIs; specific knowledge lives at the calling site.
- Reimplementing host-API dispatch on the Kotlin side instead of delegating to `HostApiInteractor` and letting the Rust core dispatch.
- Adding new host calls without an RFC (per user direction) — **always require an RFC** that defines the permission model.

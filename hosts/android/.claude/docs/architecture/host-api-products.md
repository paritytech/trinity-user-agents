# Products / Host runtimes / ChatExtension

Three distinct concepts. Don't conflate. Products run on the Rust TrUAPI core
in every build.

## Rules at a glance

1. **`blocking`** — A new host call added without a referenced RFC stating its permission model. New host calls are protocol: they live in the Rust core (`paritytech/trinity-user-agents`). If no RFC or the RFC is silent on permissions, escalate to the user; don't invent a policy.
2. **`blocking`** — `ProductId` constructed from arbitrary strings. Always `ProductId.fromUrl(uri)` / `fromStoredValue(...)`.
3. **`blocking`** — WebView ownership ambiguity: two classes both call `destroy()` on the same WebView. Single owner.
4. **`major`** — Product bootstrap installed anywhere other than `TrUAPISessionStarter` / `TrUAPIBootstrapInstaller`.
5. **`major`** — Host code reaching for a global "current product" instead of an injected `CallingProductIdProvider`.
6. **`major`** — `NavigationPolicy` branching on URL string inside the policy. Classification is external (`DotNsUtils.classifyNavigation`).
7. **`major`** — Multi-room product implemented as multiple `ChatExtension`s instead of a single `ProductChatExtension` (see `chat-extension.md`).
8. **`major`** — Factory-of-factory-of-factory chain — collapse to single `Factory.create(scope, config)`.
9. **`major`** — TrUAPI bridge logic reimplemented on the Android side instead of delegated to `HostApiInteractor` (signing, permissions, chain, theme). `ProductTrUAPIHostBridge` is a thin adapter, not a second dispatch layer.
10. **`major`** — A `HostBridge` callback violating the threading contract: prompt-driven suspend callbacks may stay pending on a user decision; dispatcher-thread callbacks must return promptly and never block (see § Threading contract).
11. **`major`** — A `UserConfirmationReview` variant mapped in `ConfirmationReviewMapping` without coverage in `ConfirmationReviewMappingTest`, or a non-signing variant routed anywhere except the fail-closed `null` path.
12. **`minor`** — Inlining "derive product id from URL" at a call site when the `CallingProductIdProvider` abstraction is already in scope.



| | **Product** | **Host runtime** | **ChatExtension** |
|---|---|---|---|
| What it is | A web-based mini-app | The Rust TrUAPI core | A native chat plug-in |
| Where it lives | Loaded into a WebView from `.dot` script | `feature/products/impl/.../domain/truapi/` + `:bindings:truapi-host` | `feature/chats/api` + per-feature impls |
| Identified by | `ProductId` (from `.dot` domain) | n/a — serves whoever's connected | `ChatExtensionId` |
| Lifecycle | Per-WebView session | Per `ProductTrUAPIHostBridge` (scoped to a WebView lifetime) | Hilt singleton |
| Composition | Selected by navigation; loaded from DotNs | Rust core dispatches, Kotlin implements `HostBridge` callbacks | `@IntoSet` Dagger multibinding |

A product can *integrate* with chat (see `chat-extension.md § Products and chat`). That integration produces a `ProductChatExtension` — a single `ChatExtension` that drives the product's worker on the core through `TrUAPIChatWorker`.

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

| Env | Where | Rendering | Navigation |
|---|---|---|---|
| **SPA Browser** | Full-screen WebView | Visible | Inline (same .dot and cross-.dot stays in WebView) |
| **Explore** | Catalog → product list | Visible | Same .dot in WebView; cross-.dot opens a new SPA browser |
| **Chat** | Hidden worker WebView under a `ProductChatExtension` | Invisible | Through `ChatWorkerNavigation`, only while the chat is open |

Each environment builds a `ProductTrUAPIHostBridge` via `TrUAPISessionStarter`.

### ProductRepository / installation

Products are stored in the Room DB and resolved via `ProductRepository`. Scripts are seeded through DotNs and cached with their content hash.

> **Workers load by URL, never as an inlined string.** A worker's entry module can use relative imports against sibling files in the same archive, which only resolve when the script has an origin. Loading it as a blob/`evaluate`-style string silently breaks multi-file workers. The injection must be a `<script type="module" src="…">` pointing at a real served URL.

---

## Host calls

### Adding a new host call — **always RFC-first**

A new host call is a public protocol surface. New host calls **require an RFC** in `paritytech/trinity-user-agents` that states:
- Method name, params, response.
- **Permission model** — what permission this call requires (if any), how the user grants it, scoping per product.
- Caching / subscription semantics.

If an RFC doesn't exist or doesn't address the permission model: **stop and ask the user**. Do not invent a permission policy on the fly.

The call is implemented in the `truapi` runtime; the Android side only implements any new `HostBridge` platform callback it needs.

### `CallingProductIdProvider`

Host code that acts for a product needs to know **which product** made the call.

- **Chat**: `FixedProductId(productId)` — constant for the extension's lifetime.
- **SPA / Explore**: `UrlDerivedProductId { webView.url }` — extracted from current WebView URL.

Always inject the provider; never reach into a global "current product".

---

## TrUAPI — the Rust-core runtime

The product's `@parity/truapi` client talks to the shared Rust core
(`libtruapi`, via `:bindings:truapi-host`) over an authenticated
loopback WebSocket. The core owns wire framing, dispatch, subscriptions, and
orchestration; the Android side implements only native platform callbacks.

The core is compiled from the enclosing `trinity-user-agents` tree, two levels up.
The core and the Android `HostBridge` implementation move together: changing one
without checking the other is how a callback goes silently dead, because the
generated interface defaults most members.

### Runtime and executions

The core is one process-wide `TrUAPIHostRuntime` with a `TrUAPIProductExecution`
per product connection, and the Android side mirrors that split:

- `TrUAPIHostRuntimeProvider` (`@Singleton`) builds the runtime lazily on first
  use from `HostRuntimeConfig` (host identity, the Trinity stack the build
  bundles, the local session) and keeps it for the process. Its runtime-level
  `HostBridge` serves only what the signing runtime asks for — core storage,
  auth state, signing-side chain access, and the confirmations SSO raises.
  Product-scoped calls have no product there and fail closed, as on iOS. It
  re-activates the local session when the wallet account changes.
- `ProductTrUAPIHostBridge` (`feature/products/impl/.../domain/truapi/`) opens
  one execution per product WebView with a product-scoped `HostBridge` and
  delegates every wired callback to `HostApiInteractor`. Do not reimplement
  dispatch on the Kotlin side.
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

### Stack

The core takes every chain it uses, and the chain info it reports to products,
from the Trinity stack descriptor in `HostRuntimeConfig.stack`. `TrUAPIStack`
maps the build's `TestnetEnvironment` to a stack: `TESTNET` runs previewnet,
`NIGHTLY` paseo-next-v2 and `PRODUCTION` Polkadot, the only one allowed
Polkadot's reserved genesis hashes and `dot` TLD. Each stack ships as its link
and its signed descriptor under the `stacks/` assets, which
`:bindings:truapi-host` syncs at build time from `stacks/` in the core
checkout, so the app holds no copy. `scripts/update-stacks.sh` refreshes the
descriptors there. The core checks each descriptor against its link's key, so
a copy it refuses fails the runtime boot.

### Chain agreement

`featureSupported(Chain)` and `chainConnect` both answer from one
`TrUAPIChains` snapshot — resolved at `attach` per execution, and once at boot
for the runtime — through `TrUAPIChains.canDial`. Never answer either from the
registry directly: the registry holds chains with no `wss` endpoints, so the
host would promise a chain `chainConnect` then refuses.
`TrUAPIChainAgreementTest` pins the invariant.

### Storage

`EncryptedHostStorage` (product-scoped) and `EncryptedHostCoreStorage` (core
auth / pairing state) back the core's storage callbacks, encrypted at rest and
namespaced per product. Don't bypass them. The `Prefs*` pair in the binding's
`androidTest` source set is the diagnostics test double, not a production path.

---

## JsRuntime — abstraction over the JS engine

`feature/products/impl/.../jsRuntime/JsRuntime.kt` runs a product's worker in the hidden WebView that
`TrUAPIWorkerSupervisor` drives. Today there's `WebViewRuntime` (Android WebView). **Code consuming a
`JsRuntime` must not assume WebView semantics.**

### WebView ownership

**One owner per WebView.** `BrowserWebViewProvider` owns the WebView lifecycle; UI subscribes to a read-only `StateFlow<WebView?>` to render. UI must not also call `destroy()`.

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

Classification is external:
`CoreNavigateClassifier.classify(origin, destination)` wraps the core's
`parse_navigate` so every TrUAPI platform categorizes `.dot` navigation
identically. A `null` result means the core rejected the input — it must be
blocked, never loaded. The policy only dispatches on the classified type; never
branch on URL inside the policy. (`DotNsUtils.classifyNavigation` is gone.)

---

## Per-product storage isolation

The core namespaces product storage per product over the encrypted stores, so two products cannot read each other's storage — don't bypass it.

---

## Pocket — the host-owned card collection

Spec: trinity-user-agents RFC "Pocket modality" (#609), core implementation #706, on top of Unified Renderer (#633) and
Worker Lifecycle (#632). The host owns the collection and is its only writer; a product observes its own cards over
the wire (`Pocket::list_subscribe`) and may remove one (`Pocket::remove_card`). There is no add call: a card enters
when the user follows a `polkadotapp://pocket/add?product=<product>.<tld>&card=<id>` deeplink and approves the sheet.
`polkadotapp://pocket/open?product=<product>.<tld>&card=<id>` lands on the Pocket tab with that card expanded when the
user holds it, and `polkadotapp://pocket` alone on the tab. The open link's other query parameters are appended to
the card's page URL for that open only. A product's `navigateTo` uses the same links, which
`PocketDeepLinkHandler` reads like any other app link. A product's own paths are App routes.

Seams, all in `feature/products/api/.../domain/pocket/`:
- `PocketCollection` — `observeCards()` and user removal, backed by the collection the core keeps in its database
  (`CorePocketCardRepository` over the runtime's `pocketCards()`, `addPocketCard`, `removePocketCard` and
  `pocketCardFace`). One pinned card, Humanity on the personhood product (`AssetPinnedPocketCards`), is handed to the
  core in `HostRuntimeConfig.pinnedPocketCards` and has a bundled face; added cards are stored with the face they were
  approved with. Balance and Scarcity are not pinned: the products behind them publish no Pocket cards, and the host
  already draws its own native cards for that ground. The core keeps the newest face of every card, pinned ones
  included, from each Pocket card render, so a card wears what its product last drew rather than the bundled
  placeholder at cold start.
- `PocketCardOpenRequests` — the card a deeplink asked the Pocket tab to expand. A link can arrive before the tab has
  been shown, so the request is held until the Pocket screen finds the card in its collection, and cleared there.
- `PocketFaceSource.observeFace(key)` — the cached face first, then every tree the product streams; `sendAction`
  carries a press back; `resolveImage` fetches `Image` sources from the archive or the Bulletin gateway, except that an
  `Archive` image of a pinned card's product that the app ships under `assets/pocket/` loads from the app.
- The face vocabulary is the renderer's: the core's `parseRendererNodeJson` reads a `{ tag, value }` preview into its
  `RendererNode`, with the same 64-level bound the render stream carries, `RendererNodeMapping` turns that (or a
  streamed tree) into `JsWidget`, `JsWidgetRenderer` (`api/presentation/widget`) draws it.
- Materials: each face with a `Materials` node carries a `MaterialsResolution` in `PocketFace`. The Remote Config
  switch `materials_enabled` (`RemoteConfigMaterialsSwitch`, passed as `HostRuntimeConfig.materialsEnabled` when the
  runtime starts) decides whether its nodes can draw; with it off the core settles every node `Unsupported`. A face
  the product drew resolves on the execution that drew it, so the product hears of its fallbacks; a kept face resolves
  on the runtime and reports to no product, and it shows plain until the runtime answers. `RealPocketFaceSource`
  closes a resolution when the next face replaces it. The layers draw on OpenGL ES 3.0 through `MaterialsSurface`
  on the injected `MaterialsGl` (`api/presentation/widget/materials/`). The Pocket card provides
  `LocalJsMaterialsRenderer` (`wallet/impl/.../product/materials/`), and its `CardMaterials`, held by the card's
  ViewModel scope, loads the layers' images and reports failures; every other surface draws the children plain
  through the default renderer, and chat also reports its nodes `Unsupported` to the product.
- A card expands in place rather than into a sheet: the wallet's Pocket screen moves the same card element to the top
  of a full-screen page with the shared-element transition the native cards use, and hosts the product under it through
  `SpaHost` (`api/presentation/spaHost`, with `ProductWebViewHost`). `ExpandedProductPage` keeps one session at a time
  and cancels its scope when the card collapses. The page opens at the product's own origin with `?card=<id>` and is
  served the product's `widget` executable, or its app when the product publishes no widget.

Core side (`feature/products/impl/.../domain/truapi/`):
- `TrUAPIWorkerSupervisor` starts a product's worker on the core's `Start` transition (runtime bridge
  `requestWorkerTransition`): hidden `ChatWebViewProvider` WebView, `openProductExecution(WORKER)` through
  `ProductTrUAPIHostBridge.attach(kind = WORKER)`, bootstrap at document start, entry module by URL. `Stop` tears it
  down.
- `TrUAPIPocketFaceStreams` takes one `claimWorker` claim per collected face, opens `render` on
  `RenderContext.PocketCard`, retries the first `render` while the worker's client is still connecting, and publishes
  renderer actions. The core serves a product's Pocket calls from its own collection, so the worker's execution takes
  no Pocket callback.
- Deeplinks are classified by the core's `parse_navigate` behind `PocketDeeplinkParser`; card ids and titles are
  screened at manifest load by the core's `screenPocketCardId` and `screenPocketCardTitle`.

Demo: `feature/products/product-sample/pocket-worker`, a worker on the core's own client that publishes one card,
draws its face live and gives it up on its Remove button.

### Host-placed chat

The chat analogue of the pinned card. `HostPlacedProducts` (`feature/products/impl/.../domain/bot/external/`) names
the products the host itself puts in chat, Jollity on the reserved game product, and `ProductExternalExtensionProvider`
unions them with the installed chat products, placed first and listed once when the user also installed one. Nothing
is written to the product tables, so nothing can uninstall them, and the product the registry sees never changes, so
the worker is not restarted over a resolved name. A placed product's chat extension holds the worker like any other,
and the product registers its rooms through its own `createRoom`. A host-placed chat is pinned above the rest
(`ProductChatPreviewDelegate`, through `Order.PinToTop`, for the `PlacedByHost` origin) and its menu offers no removal.

---

## Where new things live

| Concept | Goes in |
|---|---|
| New host call | RFC + implementation in `paritytech/trinity-user-agents` (the `truapi` runtime); a `HostBridge` callback here only if a new native capability is needed |
| New `HostBridge` callback wiring | `feature/products/impl/.../domain/truapi/ProductTrUAPIHostBridge.kt`, delegating to `HostApiInteractor` |
| New review-variant mapping | `feature/products/impl/.../domain/truapi/ConfirmationReviewMapping.kt` + `ConfirmationReviewMappingTest` |
| New `NavigationPolicy` variant | `feature/products/impl/.../hostApi/navigation/NavigationPolicy.kt` |

---

## North star

- **RFC-0020 `host_create_transaction`** — the canonical extrinsic-creation host call. New transaction-shaped host calls compose with it rather than duplicating signing/origin logic.
- **RFC-first host calls** — every new host call carries a referenced RFC stating its permission model before implementation. The `host-api-products.md § Adding a new host call` rule is the gate (already `blocking` in the Rules at a glance).
- **Core-first** — new host-side capability lands in the core; the Android side adds only the platform callbacks it needs.
- **Thin native bridge** — `ProductTrUAPIHostBridge` adapts `HostBridge` callbacks to `HostApiInteractor`; it does not re-implement dispatch or business logic.
- **One owner per WebView** — the provider owns the lifecycle. Treat any drift back toward UI-managed WebView destruction as a regression.
- **Per-product storage isolation** — never bypass the namespaced stores.

Probe questions on every host-API / host-bridge PR (named-but-not-blocking):
- Does this host call have an RFC + documented permission model?
- Does the change respect the `HostBridge` threading contract, and is any new review mapping covered by `ConfirmationReviewMappingTest`?
- Does this WebView interaction respect single ownership?

A "no" on any of them must be named in the architect plan.

## Anti-patterns flagged by past PRs

- Factory soup — `SpaProductWebViewProvider` had a factory-of-factory-of-factory chain. Default to single `Factory.create(scope, config)` returning the working instance.
- WebView ownership ambiguity — engine *and* UI both reaching `destroy()`. Single owner.
- `ProductId` constructed from arbitrary strings — restrict construction via `ProductId.fromUrl(uri)` / `fromStoredValue(...)`.
- Bootstrap installation inconsistent across environments — every environment boots through `TrUAPISessionStarter`.
- Reaching into product-specific knowledge from `SessionManager` / generic services — generic services expose generic subscription APIs; specific knowledge lives at the calling site.
- Reimplementing host-API dispatch on the Kotlin side instead of delegating to `HostApiInteractor` and letting the Rust core dispatch.
- Adding new host calls without an RFC (per user direction) — **always require an RFC** that defines the permission model.

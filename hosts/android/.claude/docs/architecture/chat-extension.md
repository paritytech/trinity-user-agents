# Chat & ChatExtension Architecture

> **What this doc is:** rules and seams for the chat subsystem.
> **What this doc is NOT:** a tour of the chat engine. The PR-litigated anti-patterns are the canonical learning material.

Native `ChatExtension`s plug renderers, overlays and state into the chat. Product rooms follow **Chat Extension v2** ([RFC](../../../../../docs/rfcs/chat-extension-v2.md)): the product declares each room (metadata, configuration, preview, footer, features), the host persists the declarations and renders everything with native elements. A room is in one of two modes: **host-store** (the host database keeps the messages and the product pushes feed updates into it) or **product-source** (the product keeps the messages, the host asks for pages and keeps only a window). The RFC is implemented in the shared Rust core and served on the core runtime; this doc covers the Android side.

Paths below are relative to `hosts/android`. `chats-api` is `feature/chats/api/src/main/kotlin/io/paritytech/polkadotapp/feature_chats_api`, `chats-impl` is `feature/chats/impl/src/main/java/io/paritytech/polkadotapp/feature_chats_impl`, `products-impl` is `feature/products/impl/src/main/java/io/paritytech/polkadotapp/feature_products_impl`.

---

## Glossary

- **`ChatExtension`** (api): composition-based plug-in; renderers returned as nullable overrides, plus the room hooks `roomLifecycle`, `roomQuerying` and `onUserEvent` (`chats-api/domain/extension/ChatExtension.kt`).
- **`ChatBot`**: abstract `ChatExtension` for **1-room-per-extension** extensions. Auto-creates the room; auto-routes messages to `onTextMessage` / `onCustomMessage`. Built-in bots only (`SampleBot`).
- **Direct `ChatExtension`**: for **N-room** extensions (like `ProductChatExtension`) and **no-room** processors (like `CoinagePaymentProcessingExtension`).
- **`ChatExtensionRegistry`**: Dagger `Set<@JvmSuppressWildcards ChatExtension>` plus the `ExternalExtensionProvider` extensions. Add a built-in extension via `@Binds @IntoSet`.
- **`ChatEngine`**: orchestrator; **not exposed** from `feature/chats/api`. Other modules cannot reach into it. Its `RealChatExtensionContext` is what an extension writes through.
- **Renderers**: `CustomChatMessageRenderer<T>`, `CustomChatFooterRenderer`, `CustomChatHeaderRenderer`, `CustomChatMenuRenderer`, `CustomChatOverlayRenderer`.
- **`ProductChatExtension`** (`products-impl/domain/bot/ProductChatExtension.kt`): the one `ChatExtension` per installed product; owns N rooms. It binds `ProductChatMessaging` onto the product's shared worker (`ProductWorkerRefCounter`) and answers the room hooks for the product's rooms. Which worker runs is decided at boot by `RealWorkerBootFactory`: the core runtime (`TrUAPIChatWorker`) when `ProductRuntimeSettings` selects it, else the native JS runtime with `ChatHostCalls`. Only the core runtime serves Chat Extension v2.
- **`ChatRoomDeclarations`** (`chats-api/domain/model/ChatRoomDeclarations.kt`): what a product declared for one room, persisted in `chat_room_declarations` and read through `ChatRoomDeclarationsRepository`. Effective values fall back to the host defaults.
- **`ChatRoomPolicy`** (`chats-impl/domain/roomPolicy/ChatRoomPolicy.kt`): what a product room offers, derived from the declarations and the host defaults (02-DESIGN §4). The single input of the chat list, the room screen, the menus and the input.
- **Message source**: `ChatMessageSource` (`chats-impl/domain/messageSource/ChatMessageSource.kt`), the seam the room screen reads from. `HostStoreMessageSource` reads the database; `ProductMessageSource` pages from the product.
- **Room-scoped message ids**: a product picks a message id per room, the store needs one id across all chats. `ProductMessageIds` (`chats-api/domain/productRoom/ProductMessageIds.kt`) stores a pushed message as `<chatId>/<product id>` and converts back for every id that leaves the app.

---

## Rules

1. **`blocking`**: A `ChatExtension` that owns N rooms is implemented as a `ChatBot` is wrong. `ChatBot` is the 1-room special case only. Use direct `ChatExtension` for N-room or no-room.
2. **`blocking`**: Overlay must not influence the screen below. `PILL_CLEARANCE`-style bottom padding driven by an overlay is forbidden.
3. **`blocking`**: Renderer state lives in a dedicated VM (or `@Singleton` state holder), **never** inside the bot/extension class.
4. **`blocking`**: `Fragment` classes do not leak through bot APIs to chat host code. Compose-only API; fragment names allowed only via `ownedFragmentClasses()`.
5. **`blocking`**: The room screen reads messages only through `ChatMessageSource`. Code that subscribes to `ChatMessageRepository` for a room's feed, or assumes the host database holds every message, breaks product-source rooms, which store nothing.
6. **`blocking`**: Everything a product room shows comes from `ChatRoomPolicy`. Do not read `ChatRoomDeclarations` in a screen, and do not branch on "is this a product room" where a policy field can carry the difference.
7. **`blocking`**: A product message id crosses the store boundary only through `ProductMessageIds`. Every id that reaches the product (events, `user_action`, render contexts) is the product's id, never the stored one.
8. **`blocking`**: The worker is held per open room, not per installed product. Hold it through `ChatRoomLifecycle` (one `close` for every successful `open`) or the brief holds of `WorkerHolds`; never acquire for the life of the extension.
9. **`blocking`**: Style parameters silently overridden by ambient `CompositionLocal`s are forbidden. If a caller passes `style: ChatMessageSurfaceStyle`, `direction`, etc., the renderer must honor it.
10. **`blocking`**: `reverseLayout = true` always in the chat feed. The newest message is the first item; the paging rows for older and newer pages are the last and first items.
11. **`major`**: Custom item-entry animations are forbidden; use `LazyListScope.animateItem()`.
12. **`major`**: Other modules access chat capabilities only through the `ChatExtension` interface and `chats-api`; never reach into `ChatEngine` or chat-impl types.
13. **`major`**: Overlay components are named generically (`ChatExtensionOverlay*`), not feature-named (`GamePillOverlay`).
14. **`major`**: Bots do not decide when to send their messages if a higher-level engine should drive it. Don't change business logic for a UI-only feature.
15. **`major`**: Chat-specific fields (`is_read`, etc.) are not smuggled through generic flags for other features.
16. **`major`**: Custom message renderers are registered per extension. The product's renderer receives the room and message in `RenderContext.ChatMessage`, so per-room behaviour belongs in the product, not in per-chat hacks on the host.
17. **`major`**: Events go where the RFC puts them. In a product-source room the user's send, edit, delete, reaction and seen go to `user_action` only; in a host-store room they go to `action_subscribe`. The other events go to `action_subscribe` in both modes.

## Seams

| Seam | What it does | When to extend it |
|---|---|---|
| `ChatExtension` interface | composition over inheritance: renderer overrides returned as nullable, room hooks with defaults | New chat plug-in (1-room → `ChatBot`; N-room/no-room → direct) |
| `@Binds @IntoSet ChatExtension` | Registry auto-discovery | Wire the new extension |
| `customGlobalOverlayRenderer()` + `ownedFragmentClasses()` | Cross-screen overlay surface | New floating UI driven by an extension |
| `CustomChatMessageRenderer<T>` + stable `rendererId` | Custom message types | New on-wire message type |
| `subscribeNewMessages(contentTypes = ...)` | Background processor pattern | "Chat watches on-chain event" extensions (e.g. coinage payment) |
| `ChatExtensionContext` v2 setters (`setRoomConfig`, `setRoomMetadata`, `setRoomPreview`, `setRoomFooter`, `setRoomFeatures`, `setRoomCall`, `applyFeedUpdate`, `deleteRoom`) | An extension declares its rooms and pushes feed updates; `ProductRoomDeclarer` and `ProductFeedApplier` (`chats-impl/domain/productRoom/`) check ownership and write | A new room declaration |
| `ChatRoomDeclarationsRepository` (`chats-api/domain/ChatRoomDeclarationsRepository.kt`) | Read the persisted declarations | Never written from outside the engine |
| `ChatRoomPolicyProvider` (`chats-impl/domain/roomPolicy/ChatRoomPolicyProvider.kt`) | One `ChatRoomPolicy` per room, or per list of rooms | A new aspect a product can declare: add a policy field with its §4 default |
| `domain.productRoom` (`chats-api/domain/productRoom/`) | `ChatRoomLifecycle` (open and close a room, hold the worker), `ProductChatQuerying` (`messagesPage`, `userAction`), `ProductFeedListeners` (live updates to open screens), `ProductMessageIds`, and the models of pages, user actions, feed updates and events | Another query the room screen asks of its product |
| `ChatMessageSource` + `ChatRoomSourceContext` (`chats-impl/domain/messageSource/`) | Picks the source of a room from its policy; runs open, close and retry in request order (retry closes and opens again, so a worker whose start failed starts again); releases the room when the screen goes away | A different store mode |
| `ChatFeedInteractor` `openRoom`, `closeRoom`, `retryRoom`, `releaseRoom`, `roomQuerying`, `sendUserEvent` | What the room view model calls: open on screen start, close on screen stop, release in `onCleared` | New room-screen lifecycle event |
| `ChatResourceResolver` (`chats-impl/presentation/resources/ChatResourceResolver.kt`) | Resolves a `ChatResourceModel` (inline bytes, preimage through the IPFS raw-block link, v1 `data:`/`https` icons) to an image model | New resource kind |
| `ChatMediaLoader` + `ChatResourceDiskCache` (`chats-impl/presentation/resources/`, `chats-impl/data/resources/`) | Puts the bytes of a media item (inline, preimage through `lookupRawHash`, v1 `data:`) in one 64 MiB LRU file cache; `ProductMediaController` (`LocalProductMediaController`) draws, plays and opens the files and offers a retry when a preimage does not load | New media behaviour |
| `ProductChatHostBridge` (`products-impl/domain/truapi/ProductChatHostBridge.kt`) | The core's `ChatHostBridge` for one product: maps each callback onto `ProductChatMessaging`; `updateFeed` with `persist` stores, without it delivers to `ProductFeedListeners` | A new core chat callback |
| `TrUAPIChatWorker` + `WorkerHolds` (`products-impl/domain/truapi/worker/`) | The core-runtime `ProductWorker`: one worker reference per open room, brief holds for a message in flight and for a product with no room yet, the queries with their timeouts and one retry after an unavailable worker, every `ChatEvent` | A new event or query to the product |
| `ProductChatExtension` wrapper | Products as N-room chat extensions | Don't fork; extend the wrapper |

## Products and chat

A product integrates with chat through its worker. On the core runtime, `TrUAPIWorkerSupervisor` boots the worker in a hidden WebView as a `WORKER` execution and passes a `ProductChatHostBridge` when the worker manifest `includes` chat; the same worker serves Pocket. The flow for one room:

1. **Declare.** The product calls `create_room` with the room's configuration, then the `set_room_*` requests. The core checks ownership, limits and rate limits, then calls `ProductChatHostBridge`, which goes through `ProductChatMessaging` to `ChatExtensionContext` and `ProductRoomDeclarer`. The declarations are persisted, so the chat list renders without a running worker.
2. **List.** `ChatRoomPolicyProvider.observePolicies` feeds the chat list: product preview text and ongoing dots, badge (count, image, hidden or the host count), pin and timestamp. `chatListOrder` (`chats-impl/domain/models/ChatListOrder.kt`) merges product timestamps with the local message order.
3. **Open.** The room view model calls `openRoom`. `ChatRoomSourceContext` calls `ChatRoomLifecycle.open`, which holds the worker; a failure shows the room as unavailable with Retry. Screen stop releases the reference.
4. **Messages.** In a host-store room `HostStoreMessageSource` reads the database, and feed updates with `persist` go through `ProductFeedApplier`. In a product-source room `ProductMessageSource` asks `messages_query` for pages of 50 into a window of 500, pages at both ends, jumps to a quoted message, and takes live feed updates as a `ProductFeedListener`; nothing is stored. User actions go to `user_action` with an optimistic row (`Preparing`, then the product's answer, `Rejected` removes it, a time-out marks it `Failed`).
5. **Events.** Taps on buttons and menu entries reach the extension as `ChatRoomUserEvent` through `onUserEvent`, and `TrUAPIChatWorker` publishes them as `ChatEvent`s with ids converted by `ProductMessageIds`.

The native runtime (release builds, and debug builds with the toggle off) still runs chat products through `ChatHostCalls`. It serves v1 only: no room declarations, no queries, no product-source rooms.

## Anti-patterns

| Anti-pattern | Severity | Fix |
|---|---|---|
| `ChatExtension` with N rooms implemented as `ChatBot` | blocking | direct `ChatExtension` |
| Overlay drives screen layout (`PILL_CLEARANCE`) | blocking | one-way data: overlay reads, never writes layout |
| Bot owns renderer state (requires `ComputationalScope` hack) | blocking | move state to a `*StateHolder` + dedicated VM |
| Fragment class plumbed through bot API | blocking | Compose-only; use `ownedFragmentClasses()` for names |
| Style param silently overridden by ambient `CompositionLocal` | blocking | route via `ChatConfig` / explicit param |
| `reverseLayout = false` in chat feed | blocking | always `true` |
| `synthesizeSnapshot()` workaround for state outside service lifetime | blocking | snapshot is a global state holder regardless of service running |
| Room screen reads `ChatMessageRepository` directly for its feed | blocking | read `ChatMessageSource` through `ChatFeedInteractor.subscribeMessages` |
| Screen reads declarations or checks "product room" to decide what to show | blocking | add a field to `ChatRoomPolicy` with its host default |
| Stored message id sent to the product, or a product id used as a store key | blocking | convert with `ProductMessageIds` |
| Worker acquired for the life of the extension | blocking | `ChatRoomLifecycle` per open room, `WorkerHolds` brief holds |
| Persisting a feed update of a product-source room | blocking | deliver it to `ProductFeedListeners`; the screen asks again on open |
| Custom message-entry animation | major | `LazyListScope.animateItem()` |
| Always-on `Modifier.blur` without API 31+ guard | major | guard or polyfill |
| Reaching into `ChatEngine` from another module | major | go through `ChatExtension` interface |
| Feature-named overlay component | major | rename `ChatExtensionOverlay*` |
| `is_read` / chat-specific fields abused in other features | major | dedicated entity (e.g. reactions get their own) |

## North star: Chat Extension v2

The RFC is implemented in the core and on Android up to product-source rooms. What remains on Android is in the plan: product menus, participants, the custom footer, the placeholder, and the core runtime as the release default for chat products.

- **Product declares, host renders.** Rooms, metadata, preview, configuration, footer and features are product declarations, persisted by the host. The host draws them with native elements and its own theme.
- **Two store modes.** The host owns the message store only in a host-store room. In a product-source room the product is the data source and the host keeps a window.
- **Per-room scope.** Every declaration, query and event names a room. Nothing is per extension except the renderer registration.
- **SCALE over the core.** The wire is the core's SCALE codec; Android sees typed UniFFI values through `ChatHostBridge` and the execution's queries.

Probe questions on every chat PR:
- Does this assume the host owns the message store?
- Does this read a product room's state anywhere other than `ChatRoomPolicy`?
- Does this send a stored id to the product, or hold the worker without an open room?
- Does this add a footer or menu entry that the product cannot declare for its own room?

A "yes" isn't automatically blocking, but must be named in the architect plan with a recommendation.

## Canonical examples

- 1-room bot: `SampleBot`.
- N-room product-driven: `ProductChatExtension`.
- No-room background processor: `CoinagePaymentProcessingExtension`. **This is the template for "chat watches on-chain event".**
- Product-source room: `ProductMessageSource` with `ProductMessageSourceTest`; the worker side in `TrUAPIChatWorker` with `TrUAPIChatWorkerRoomsTest`.
- Room policy: `ChatRoomPolicy.from` with `ChatRoomPolicyTest`.
- Overlay done right: anywhere that uses `customGlobalOverlayRenderer()` + dedicated `*OverlayViewModel` + `ownedFragmentClasses()` suppression.
- Wiring: `ChatBotsModule` `@Binds @IntoSet`; `ChatRoomPolicyModule` and `ChatMessageSourceModule` for the v2 seams.

## Where new things live

| Concept | Goes in |
|---|---|
| New extension (bot or direct) | `feature/<X>/impl/.../domain/extension/<X>Extension.kt` |
| Renderer | `feature/<X>/impl/.../presentation/chat/renderer/` (with its own VM in a sibling package) |
| Custom message content type | `feature/<X>/impl/.../data/scale/<X>MessageContent.kt` (`@Serializable`) |
| Extension registry binding | the feature's `di/` module, `@Binds @IntoSet` |
| New product room declaration | model in `chats-api/domain/model/ChatRoomDeclarations.kt`, column in `ChatRoomDeclarationsLocal` with its SCALE conformance test, policy field in `ChatRoomPolicy` |
| New product query or user action | model in `chats-api/domain/productRoom/`, call in `TrUAPIChatWorker`, use in `ProductMessageSource` |
| Mapping a core chat type | `products-impl/domain/truapi/` beside `ProductChatHostBridge` |

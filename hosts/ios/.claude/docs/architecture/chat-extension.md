# Chat & Product Rooms Architecture

## Overview

Chat has two kinds of room. A person chat is end-to-end encrypted and goes over the statement store.
A product room belongs to a chat product (Chat Extension v2): the product's Worker owns the room's
data, and the host owns the screen. Both kinds open the same room screen (`Modules/Chat/`). The
design is the Chat Extension v2 plan, `HOST-CHAT-EXTENSION-V2/02-DESIGN.md` (outside this repository).
Its §15 and §19 to §24 record what changed while building and win over §8. The wire contract is
`docs/rfcs/chat-extension-v2.md` at the repository root; `ios/truapi-host/README.md` documents the bridge.

## Person chats

- Modules: `Chat`, `ChatCall`, `ChatRequestList`, `ChatAttachments` (see `architecture/chat-attachments.md`),
  `IncomingChatRequest`, `ContactsList`, `SearchContact`.
- Packages: `MessageExchangeKit` (encrypted messaging), `StatementStore`. Persistence is Core Data in the
  app (`CDChatMessage`, `CDChat`); there is no separate storage package.
- Services on `ServiceCoordinator`: `chatCoordinator`, `callCoordinator`, `attachmentUploadService`,
  `attachmentDownloadService`. Sign-in lives outside chat (`architecture/sso.md`).
- Message flow: compose → encrypt with `MessageExchangeKit` → publish a statement → the peer's
  subscription receives it → decrypt → store in `CDChatMessage` → the room screen reads it.
- Calls: PushKit (VoIP) wakes the device, WebRTC carries audio and video, `callCoordinator` owns the call
  lifecycle (`architecture/data-transport.md`).

**Push payload.** After the statement is posted, the sender also pushes the message through the relay
(`POST /api/v1/notify`), so the recipient's Notification Service Extension renders it without a
statement-store connection. `message` is `hex(ChaCha20-Poly1305(SCALE(payload)))`. Hex doubles the size,
so an APNs alert (4 KB) leaves under 2 KB of plaintext. The payload is `Chat.NotificationPayload`:
`messageId ‖ timestamp ‖ version u8 = 0 ‖ kind u8 ‖ content`. Kind `1` (Full) carries
`RemoteMessageContentV1` unchanged. Kind `0` (Stripped, `Chat.StrippedContentV1`) is used when the full
content exceeds 1,800 bytes (`ChatNotificationPayloadBuilder`).

**Attachment thumbnails.** The optional `thumbnail: Data` of image and video metadata is a BlurHash
string in UTF-8: 4×3 components, from an image of at most 128 points on its longest side. Receivers parse
it through the typed `BlurHash` boundary. Invalid UTF-8, a malformed hash or legacy binary bytes read as
"no preview"; the full download continues.

**Persisted key ids.** `signKeyId` is a substrate derivation path. Product-account ids use the hex index
segment; build them with `WalletDerivationPath` / `ProductDerivationPath`, never by hand.
`encryptionKeyId` is a `ChatEncryptionDomain` raw value, not a path.

## Chat extensions

`ChatExtensionsRegistry` (`ChatExtension/ChatExtensionRegistring.swift`) is built by
`ChatExtensionsRegistry.createDefault(productBotProvider:)`. It holds one static extension,
`ChatReactionExtension`, and one `ProductBot` (a `ChatExtensionBot`) per chat product, which
`ChatExtensionStore` adds and removes as products resolve. There is no payment extension. Coinage
transfer bubbles render the `coinageSend` content from the transfer state row (`architecture/coinage.md`).

`ProductBotFactory.create(resolved:)` gives the bot its runtime (a `ChatRuntimeProtocol`):
`TrUAPIChatHandler` when `isTrUAPIRuntimeEnabled` is on, else `ManagedChatRuntime`. Release builds use
`ManagedChatRuntime`. It serves no v2 query, so a product-source room shows "unavailable" there.

## Chat Extension v2: product rooms

A room is host-store (the default) or product-source (`ChatRoomConfig.productSource`). A host-store room
keeps its messages in `CDChatMessage`; the product writes with `post_message` and `update_feed`. A
product-source room keeps its messages in the product; the host asks pages with `messages_query` and
keeps only an in-memory window. The chat list draws every room from persisted declarations, without the
worker.

### Seams

| Seam | Where | Role |
|---|---|---|
| Core bridge | `Products/TrUAPI/RustChatExecutionBridge.swift` | `ChatHostBridge` conformer; maps bindings to Swift domain values and calls `ProductChatMessaging`. Rejects `registerBot` |
| Bindings → domain | `Products/TrUAPI/ChatExtensionV2+Domain.swift`, `ChatFeedUpdate+Domain.swift`, `ChatMenus+Domain.swift`, `ChatParticipants+Domain.swift` | The only place that maps `TrUAPIHost` types to `Chat.*` types |
| Product writes | `Products/ProductChatMessaging.swift` (`BoundProductChatMessaging`) | Routes create, post, declarations and feed updates to the discover context |
| Store writes | `ChatExtension/ChatExtensionDiscoverContext+Declarations.swift` (`ChatProductRoomDeclaring`), `+Feed.swift` (`ChatProductFeedApplying`) | Core Data writes of declarations and host-store feed updates |
| Declarations | `Model/ChatRoomDeclarations.swift`, `Model/CoreData/ChatRoomDeclarationMappers.swift` | `CDChatRoomMetadata` on `UserDataModel55`, one partial-update mapper per request |
| Room policy | `Model/ChatRoomPolicy.swift` | One value from the declarations plus the §4 defaults. Read by the list, the view model factory, menus and footer |
| Message source | `Service/ChatMessageSourcing.swift` | `CoreDataMessageSource` (person chats, host-store rooms) or `ProductMessageSource` (window of pages, 50 per page, 500 messages at most) |
| Source choice | `Service/ChatRoomSourceContext.swift` | Picks the source once from the policy; owns open, close and retry of the room |
| Live feed | `Products/Chat/ProductFeedListeners.swift` | `persist: false` updates go to the open screens of the room; with no screen they are dropped |
| Queries | `Service/ProductChatQuerying.swift`, `Service/ProductRoomInteracting.swift` | Pages and user actions; menus, participants, reactions, events. `RegistryProductRoom` finds the bot on each call |
| Room lifecycle | `Service/ChatRoomLifecycle.swift` | `open(roomId:)` holds the worker and sends `RoomOpened`; `close(roomId:)` undoes it |
| Rust runtime | `Products/Chat/Rust/TrUAPIChatHandler.swift` (+`RoomQueries`, +`WorkerLoss`) | Worker references, queries with timeouts, events, renders, worker-loss handling |
| Room screen parts | `ProductRoom/ChatProductRoomPresenter*.swift` | Menus, toolbar, participants, who reacted, footer, extras, attachments, background close |
| Participants | `Modules/ChatParticipants/` (VIPER), `ProductRoom/ChatParticipantDirectory.swift` | Paged list through `participants_query`; author names and avatars |
| Resources | `Service/Resources/ChatResourceResolver.swift`, `ChatResourceDiskCache.swift` | Inline bytes, preimage keys and `https` for bodies, avatars and widgets |
| Room links | `Modules/URLHandling/ChatRoomLinkService.swift` | `polkadot://<product>.<tld>/-/chat/<room>`; an unknown room opens the product's App |

### The room screen

1. `ChatPresenter` calls `interactor.openRoom()` on `viewWillAppear` and `closeRoom()` on
   `viewWillDisappear`. `ChatRoomSourceContext` passes them to `ChatRoomLifecycle` for every product room.
   `ChatRoomBackgroundObserver` closes the room while the app is in the background.
2. A product-source room asks the newest page (`Before`, no cursor). The source never asks `After`: it asks
   a dropped page again with the request that first fetched it. `ProductMessageCache` keeps the newest page
   of the last 32 rooms, so a reopen draws at once.
3. A user action draws an optimistic entry and goes out as `user_action`. `MessageCreated` replaces it; a
   timeout or `Rejected` marks it failed or reverts it.
4. A host-store room writes to Core Data and then publishes the `ChatEvent`.
5. The readiness of the window is `loading`, `ready` or `unavailable` (`ChatRoomReadiness`). "Unavailable"
   is a pinned line with Retry; the window keeps its cached pages.

### Worker lifetime

The worker runs only while something holds it (D10). `TrUAPIChatHandler` asks `TrUAPIWorkerManager` with
`ensureWorker(for:)` and gives each request back with `releaseWorker(for:)`. The core counts the
requests and starts or stops the one Worker execution of the product.

- **Holders.** Each open room screen (`open` to `close`) and each query (`withWorker`, for its own length).
  The core also holds the worker for each open render stream: a custom body streams only while a view shows
  it (`ProductWidgetViewModel.bodyAppeared()` / `bodyDisappeared()`), and the custom footer while its room
  is open. The bot's discovery, the decoder cache and the chat list hold nothing.
- **First-room hold.** A product with no room cannot be opened from a room screen. `ProductFirstRoomHold`
  holds its worker at product start until the first room appears, then 2 s more, at most 30 s. A product
  that has a room gets no hold (06-OPEN-ITEMS O25).
- **Worker faults.** An uncaught error in the worker's web view ends the worker (O26).
  `TrUAPIWorkerRuntime.faults` reports it, and `TrUAPIWorkerManager` stops the worker. The core still
  counts references and sends no new start, so the manager starts the worker again on the next
  `ensureWorker`. `TrUAPIChatHandler+WorkerLoss` marks each open room `unavailable` through
  `ProductFeedListeners.productStopped(for:)`; the next worker gets `RoomOpened` for those rooms again.
  An unhandled promise rejection does not end the worker.
- **Notification wake.** iOS has no hold for a message in flight (O3). A product-source room's badge and
  preview are as fresh as the product's last run.

**Core runtime start.** `TrUAPIHostRuntime.init` blocks on `Db::open`. `ServiceCoordinator` calls
`TrUAPIHostRuntimeProvider.prepareInBackground()` at start, which builds the shared runtime on a
background task (`TrUAPIRuntimeWarmUp`, up to 30 attempts 1 s apart). A screen that asks later finds it
built (O24).

### Rendering and performance

- Product bodies are content kind `productBody` (stored type 247, SCALE). Feed updates map to cells in
  `ChatViewModelFactory`; custom bodies draw through `ProductMessageDecoder` and the renderer.
- In a product-source room a `RichText` body with text and no media becomes the same content as a `Text` body
  (`.text` or `.reply`), with `Chat.LocalMessage.hasMarkdownText`, so it keeps its quote, its edit and the "Edited"
  mark. The factory draws it in the Markdown subset (`ChatMarkdownText`, a line ending is a line break) and draws a
  `Text` body as plain text with its `https` links; a tapped link opens after the host confirmation.
- Product message ids are stored as `Chat.ProductMessageId` (`"<chatId raw>/<product id>"`), because
  `CDChatMessage.messageId` is unique across chats.
- The room screen draws at most one list update per frame (`ChatPresenter+ListUpdates.swift`). A burst of
  feed updates costs one draw.
- `ChatAttributedTextCache` keeps the Markdown result per text; `ProductURLDetector` keeps one
  `NSDataDetector` for the app.
- `ChatResourceResolver.decodedImage` decodes pictures off the main thread and scales a picture above
  2,048 px down while it decodes. Decoded pictures are bounded at 64 MiB in memory.
- `ChatResourceDiskCache` is LRU, bounded at 64 MiB on disk. It reads the directory once and keeps an
  in-memory index, so a store does not list the directory.
- The participant list appends the rows of each page (`ChatParticipantsPresenter`) and inserts only them.

### Accessibility

- A reaction chip is one VoiceOver button (emoji, count, selected) with a "Reactions" action that shows
  who reacted (`ReactionsDisplayView`).
- Media `alt` reaches VoiceOver (`ProductRichText.alts`, `ChatMediaStackView`). The alts are stored after
  the media in the SCALE body; an older row decodes with none.
- Product text follows Dynamic Type (`scaledTypography`, `ChatMarkdownText`, `ChatMessageTextView`).
- The activity line (`ChatActivityLine`) and the chat list's ongoing dots (`DSChatListOngoingText`) stop
  under Reduce Motion.

### Tests and e2e

Swift Testing suites live in `polkadot-appTests/Chat` (`ProductMessageSourceTests`, `ChatRoomPolicyTests`,
`ChatRoomSourceContextTests`, `ChatAccessibilityTests`, `ProductRoom/*`), `polkadot-appTests/TrUAPI`
(`TrUAPIChatHandlerRoomTests`, `TrUAPIChatHandlerFirstRoomTests`, `TrUAPIRuntimeWarmUpTests`) and
`polkadot-appTests/Products` (`TrUAPIWorkerManagerTests`, `TrUAPIWorkerRuntimeTests`). The simulator e2e
is `make ios-chat-v2-run` (`docs/chat-ios-simulator.md` at the repository root). Its harness (`TrUAPIE2E*.swift`,
`ChatProductRoomPresenter+E2E.swift`, `ChatParticipantsViewController+E2E.swift`) runs each gesture's code
path and proves rows from Core Data, the product's event log and `[e2e:ui]` console markers.

### Chat showcase

Builds with `TESTNET_FEATURE` have a chat product that lives in the app, for checking by hand what the room screen
draws. Debug settings (shake the device) → "Chat showcase product" turns it on, and the app restarts. The chat list then
shows "Chat Showcase" and "Showcase (host store)".

`Products/Chat/Mock/` holds it. `MockChatProduct` adds a `ProductBot` whose runtime is `MockChatRuntime`, an actor that
keeps the rooms in memory in place of `TrUAPIChatHandler`. It needs no worker and no network. `MockChatHost` sends its
declarations and feed updates the way `RustChatExecutionBridge` sends a worker's: wire values through the `+Domain`
mappers into `ProductChatMessaging`, and live updates to `ProductFeedListeners`. The room events of the additions go to
the core, which does not know this product, so `ServiceCoordinator` routes them to the runtime instead.

- `showcase` keeps its messages in the product: every body kind, media kind, participant kind and message status,
  personas, reactions, replies, an edit, a reasoning row, a message with branches, a thread root, and 60 older
  messages for paging. It declares every configuration flag, the toolbar, quick replies that follow the input, slash
  commands, search, read aloud and call buttons.
- `showcase-thread` is the thread of one message of `showcase`. The host stores it, and the list hides it.
- `store` is stored by the host, so its bodies go through Core Data and the message decoder.
- The `/` commands show what messages alone cannot: `/stream`, `/typing`, `/ring`, `/call`, `/hangup`,
  `/footer <input|payment|empty|custom>`, `/preview <…>`, `/notice` and `/burst`. A message "reject" is refused and
  "fail" is created as failed.

The showcase cannot show what needs the core: preimage and sealed attachments, wakes, and calls with a media session.
`MockChatShowcaseTests` fails when the wire gains a body, status, media, participant or renderer kind that the
showcase does not show.

## Hard Rules

1. **Never hold the worker for the life of the product.** Hold it for an open room, a query or the bounded
   first-room hold. The bot, the chat list and a runtime start hold nothing.
2. **Every query has its §3.6 timeout** from `ChatQueryTimeout`: 10 s for a page, 30 s for a sent message,
   10 s for another user action, 5 s for a lookup. A page asks once more after 1 s on `unavailable`
   (`ProductMessageSource.ask`). A lookup that fails uses the host default.
3. **Product-source rooms write nothing to `CDChatMessage`.** Their messages live in the window and in
   `ProductMessageCache` only.
4. **Declarations are partial-update mappers on Model 55.** Each request replaces one declaration and
   keeps the others (`code/data-persistence.md`). The stored bytes have no version prefix, so a change of
   a declaration type needs a migration.
5. **Every id that leaves the app is converted back** from `Chat.ProductMessageId`: events,
   `ActionTrigger`, render contexts.
6. **Every change to `ChatRuntimeProtocol` needs a native implementation too** (`ManagedChatRuntime`).
   Release builds run it.
7. **The notification extension compiles the declaration files from an explicit list** in
   `project.pbxproj` (`membershipExceptions` of the NotificationServiceExtension target). Add a new model
   file there when that target needs it, and keep these files free of `TrUAPIHost`.
8. **Extensions never block the main chat flow.** Product setup is asynchronous and off the send path.
   An extension failure stays inside its cell or room; it does not throw out of the chat interactor.
9. **Never block the main thread on the core.** Build the runtime with `prepareInBackground()`; call the
   core from a task.
10. **Simulator-only harness code stays under `#if targetEnvironment(simulator)`.**
11. **Encryption is mandatory for person chats.** Every person message goes through `MessageExchangeKit`.

## Anti-Patterns

| Anti-pattern | Do instead |
|---|---|
| Holding the worker in the bot or for the product's life | Hold per open room and per query; let `ProductFirstRoomHold` cover a product with no room |
| Opening a render stream when a body decodes and keeping it | Stream while a view shows the body; keep the last tree for the next appearance |
| Swallowing an uncaught worker error and keeping the worker | Report it on `TrUAPIWorkerRuntime.faults`; the manager stops the worker and the rooms go `unavailable` |
| Converting the whole window on every feed update | Coalesce to one list update per frame; cache attributed text |
| Reading Core Data on the main thread | Read through the message source or a data provider off the main actor |
| Blocking the main thread on the core (`Db::open`, a query) | Warm the runtime in the background; `await` the query in a task |
| Opening an `NSDataDetector` per message | Reuse the one in `ProductURLDetector` |
| Decoding a picture with `UIImage(data:)` on the main thread | `ChatResourceResolver.decodedImage`, off the main thread and scaled |
| Writing a product-source message into `CDChatMessage` | Keep it in `ProductMessageSource` and `ProductMessageCache` |
| Sending a stored id to the product | Convert it back from `Chat.ProductMessageId` |
| Awaiting extension readiness on the send path | Render a placeholder; let send and receive go on |
| Mapping `TrUAPIHost` types outside the `+Domain.swift` files | Map once at the bridge into `Chat.*` domain types |

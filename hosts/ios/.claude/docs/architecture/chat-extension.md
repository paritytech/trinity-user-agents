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

A room is host-store (the default) or product-source (`ChatRoomConfig.productSource`). Only `set_room_config`
gives a room a configuration; `create_room`, `post_message` and `set_room_footer` keep their first-version shapes,
and a room without `set_room_config` and without a contact looks and behaves as at the merge base. Its footer is
the `set_room_footer` one (`TextInput` or `Empty`); a room with a contact draws the native input of its contact
state, with the attachment button when the room has `hop_attachments` and the payment button as the native person
chat has it. A host-store room
keeps its messages in `CDChatMessage`; the product writes with `post_message` and `update_feed`. A
product-source room keeps its messages in the product; the host asks pages with `messages_query` and
keeps only an in-memory window. The chat list draws every room from persisted declarations, without the
worker.

A room of the person chat product (`contact_rooms`, `RoomDeclarations.contact`) draws as the native person
chat with that contact, and nothing more: the native header, menu, input, request rows, bubbles, list row,
"+" panel, blocked users, call screens and notification texts. Product messages map to native message
contents (`Service/ProductChatMessage+Local.swift`), so the native view model factory draws them. A product
room has no feature that the native person chat lacks: no description, avatar, participants, menus, search,
mute, archive, read aloud, personas, custom bodies or footers, room links, delete or retry.

### Seams

| Seam | Where | Role |
|---|---|---|
| Core bridge | `Products/TrUAPI/RustChatExecutionBridge.swift` | `ChatHostBridge` conformer; maps bindings to Swift domain values and calls `ProductChatMessaging`. Rejects `registerBot` |
| Bindings → domain | `Products/TrUAPI/ChatExtensionV2+Domain.swift`, `ChatFeedUpdate+Domain.swift` | The only place that maps `TrUAPIHost` types to `Chat.*` types |
| Native contents | `Service/ProductChatMessage+Local.swift` | A product message as a `Chat.LocalMessage` of the native person chat: text, reply, media, request, info rows, call, transfer |
| Product writes | `Products/ProductChatMessaging.swift` (`BoundProductChatMessaging`) | Routes create, post, declarations and feed updates to the discover context |
| Store writes | `ChatExtension/ChatExtensionDiscoverContext+Declarations.swift` (`ChatProductRoomDeclaring`), `+Feed.swift` (`ChatProductFeedApplying`) | Core Data writes of declarations and host-store feed updates |
| Declarations | `Model/ChatRoomDeclarations.swift`, `Model/CoreData/ChatRoomDeclarationMappers.swift` | `CDChatRoomMetadata` on `UserDataModel57`, one partial-update mapper per request; the contact and the last message of the preview |
| Room policy | `Model/ChatRoomPolicy.swift`, `Model/ChatPeerMetadata+Init.swift` | One value from the declarations plus the §4 defaults; the native header, menu and input of a contact room |
| Message source | `Service/ChatMessageSourcing.swift` | `CoreDataMessageSource` (person chats, host-store rooms) or `ProductMessageSource` (window of pages, 50 per page, 500 messages at most) |
| Source choice | `Service/ChatRoomSourceContext.swift` | Picks the source from the policy, and again when `set_room_config` changes the message store of an open room; owns open and close of the room |
| Live feed | `Products/Chat/ProductFeedListeners.swift` | `persist: false` updates go to the open screens of the room and to the call screen that waits for its offer to be read; with no screen they change the cached page |
| Queries | `Service/ProductChatQuerying.swift`, `Service/ProductRoomInteracting.swift` | Pages and user actions; who reacted (with the time of each reaction) and edit history. `RegistryProductRoom` finds the bot on each call |
| Room lifecycle | `Service/ChatRoomLifecycle.swift` | `open(roomId:)` holds the worker; `close(roomId:)` releases it |
| Rust runtime | `Products/Chat/Rust/TrUAPIChatHandler.swift` (+`RoomQueries`, +`WorkerLoss`) | Worker references, queries with timeouts, renders, worker-loss handling |
| Room screen parts | `ProductRoom/ChatProductRoomPresenter.swift` | Call buttons, who reacted, edit history, background close |
| Outbox | `ProductRoom/ProductOutbox.swift` (+`App`, `ProductOutboxStore.swift`) | Every text, photo and video of the user, under a UUID that the host chose (`NewMessage.messageId`), kept in files until the product has it and sent again under the same id |
| Photos and videos | `ProductRoom/ProductMediaDownloads.swift`, `ProductMediaProgress.swift` | HOP download of `hop_attachments` with the progress that the core reports, and the progress of uploads, for the native media grid |
| Contacts | `Service/ProductContactSearch.swift`, `URLHandling/ProductContactChatOpener.swift` | The "+" panel and the blocked users from the contacts of the rooms; opening a contact through `open_link` |
| Calls | `Products/TrUAPI/AppCallMediaHostBridge.swift`, `ChatCall/VoIP/ProductVoIPCalls.swift` | The native call screens over TrUAPI `callMedia`; the CallKit ring of a VoIP push |
| Push | `NotificationServiceExtension/ProductPushHandler.swift` | The core opens the push (`decodeChatPush`) and gives its kind (`ChatPushContent`); the extension words it as `NewMessageHandler` does, counts it in the room badge, and a tap opens the room (`ProductRoomPushRouteHandler`). A push with nothing to show shows nothing |

### The room screen

1. `ChatPresenter` calls `interactor.openRoom()` on `viewWillAppear` and `closeRoom()` on
   `viewWillDisappear`. `ChatRoomSourceContext` passes them to `ChatRoomLifecycle` for every product room.
   `ChatRoomBackgroundObserver` closes the room while the app is in the background.
2. A product-source room asks the newest page (`Before`, no cursor). The source never asks `After`: it asks
   a dropped page again with the request that first fetched it. `ProductMessageCache` keeps the newest page
   of the last 32 rooms, so a reopen draws at once.
3. A send goes to `ProductOutbox` under a UUID that the host chose, and the window shows it at once as a
   pending message of the native chat. The outbox sends the messages of a room in order, and sends a message
   that got no answer again under the same id: after a wait that doubles up to a minute, when the worker of
   the product starts again, when the room opens and when the app starts. The product gives the same message
   for the same id. A message that the product refuses stays pending and does not go again. An edit or a
   reaction on a pending message goes once the product has the message. An edit or a reaction that the
   product refuses reverts. Nothing else shows, as in the native chat.
4. A host-store room writes to Core Data. A text of the user reaches the product on `action_subscribe`, as at
   the merge base.
5. A message of the contact is new while the product gives it `Delivered`, and seen once it gives `Read`. The
   "New messages" separator, the scroll-down counter and `Seen` follow that status. A reply quotes the
   `replyPreview` of the message when the window does not hold the quoted message, and a reaction summary that
   is `unseen` shows the scroll-to-reaction button.
6. The readiness of the window is `loading`, `ready` or `unavailable` (`ChatRoomReadiness`). The native
   screen draws no status line for it; the window keeps its cached pages, and the next open asks again. Pages
   load without a row, as the native chat loads; a page that failed is asked again when the user scrolls near
   its end, at most once a second.
7. A call button opens the native outgoing call screen at once under a UUID that the host chose
   (`CallRequested.callId`). The product uses it as the call id and the id of the offer message, and the
   screen shows "Ringing" when the status of that message turns `Read`. An offer that rings while a call runs
   is turned away (`CallAnswered` with `accept: false`). A VoIP push of an offer rings CallKit, wakes the
   worker, and does not ring for a blocked contact; CallKit stops ringing when the product ends the call that
   it rang.

### Worker lifetime

Every product but the chat product keeps its worker from the start of its bot to its dispose, and keeps it after
an uncaught error, as at the merge base. The rest of this section is the chat product.

The worker of the chat product runs only while something holds it (D10). `TrUAPIChatHandler` asks `TrUAPIWorkerManager` with
`ensureWorker(for:)` and gives each request back with `releaseWorker(for:)`. The core counts the
requests and starts or stops the one Worker execution of the product.

- **Holders.** Each open room screen (`open` to `close`) and each query (`withWorker`, for its own length).
  The core also holds the worker for each open render stream of a widget. The bot's discovery, the decoder
  cache and the chat list hold nothing.
- **First-room hold.** A product with no room cannot be opened from a room screen. `ProductFirstRoomHold`
  holds its worker at product start until the first room appears, then 2 s more, at most 30 s. A product
  that has a room gets no hold (06-OPEN-ITEMS O25).
- **Worker faults.** An uncaught error in the worker's web view ends the worker of the chat product (O26).
  `TrUAPIWorkerRuntime.faults` reports it, and `TrUAPIWorkerManager` stops the worker. The core still
  counts references and sends no new start, so the manager starts the worker again on the next
  `ensureWorker`. `TrUAPIChatHandler+WorkerLoss` marks each open room `unavailable` through
  `ProductFeedListeners.productStopped(for:)`; the next open holds the next worker for those rooms.
  An unhandled promise rejection does not end the worker.

A room of the person chat product (`contact_rooms`, `RoomDeclarations.contact`) draws as the native person
chat with that contact, and nothing more: the native header, menu, input, request rows, bubbles, list row,
"+" panel, blocked users, call screens and notification texts. Product messages map to native message
contents (`Service/ProductChatMessage+Local.swift`), so the native view model factory draws them. A product
room has no feature that the native person chat lacks: no description, avatar, participants, menus, search,
mute, archive, read aloud, personas, custom bodies or footers, room links, delete or retry.
- **Notification wake.** iOS has no hold for a message in flight (O3). A product-source room's badge and
  preview are as fresh as the product's last run.

**Core runtime start.** `TrUAPIHostRuntime.init` blocks on `Db::open`. `ServiceCoordinator` calls
`TrUAPIHostRuntimeProvider.prepareInBackground()` at start, which builds the shared runtime on a
background task (`TrUAPIRuntimeWarmUp`, up to 30 attempts 1 s apart). A screen that asks later finds it
built (O24).

### Rendering and performance

- Product messages are not stored as product bodies. A host-store room keeps native contents; a
  product-source room maps each page to native contents in `ProductMessageSource`.
- Product message ids are stored as `Chat.ProductMessageId` (`"<chatId raw>/<product id>"`), because
  `CDChatMessage.messageId` is unique across chats.
- `ProductURLDetector` keeps one `NSDataDetector` for the app.

### Tests

Swift Testing suites live in `polkadot-appTests/Chat` (`ProductMessageSourceTests`, `ChatRoomPolicyTests`,
`ChatRoomSourceContextTests`, `ContactRoomRenderingTests`, `ContactRoomListTests`), `polkadot-appTests/TrUAPI`
(`TrUAPIChatHandlerRoomTests`, `TrUAPIChatHandlerFirstRoomTests`, `TrUAPIRuntimeWarmUpTests`) and
`polkadot-appTests/Products` (`TrUAPIWorkerManagerTests`, `TrUAPIWorkerRuntimeTests`).

## Hard Rules

1. **Never hold the worker for the life of the product.** Hold it for an open room, a query or the bounded
   first-room hold. The bot, the chat list and a runtime start hold nothing.
2. **Every query has its §3.6 timeout** from `ChatQueryTimeout`: 10 s for a page, 30 s for a sent message,
   10 s for another user action, 5 s for a lookup. A page asks once more after 1 s on `unavailable`
   (`ProductMessageSource.ask`). A lookup that fails uses the host default.
3. **Product-source rooms write nothing to `CDChatMessage`.** Their messages live in the window and in
   `ProductMessageCache` only.
4. **Declarations are partial-update mappers on Model 57.** Each request replaces one declaration and
   keeps the others (`code/data-persistence.md`). The stored bytes have no version prefix, so a change of
   a declaration type needs a migration.
5. **Every id that leaves the app is converted back** from `Chat.ProductMessageId`: events, user actions,
   render contexts.
6. **Every change to `ChatRuntimeProtocol` needs a native implementation too** (`ManagedChatRuntime`).
   Release builds run it.
7. **The notification extension compiles the declaration files from an explicit list** in
   `project.pbxproj` (`membershipExceptions` of the NotificationServiceExtension target). Add a new model
   file there when that target needs it, and keep these files free of `TrUAPIHost`.
8. **Extensions never block the main chat flow.** Product setup is asynchronous and off the send path.
   An extension failure stays inside its cell or room; it does not throw out of the chat interactor.
9. **Never block the main thread on the core.** Build the runtime with `prepareInBackground()`; call the
   core from a task.
10. **A product room draws only what the native person chat draws.** A feature that the native chat lacks
    stays out of the product room too.
11. **Encryption is mandatory for person chats.** Every person message goes through `MessageExchangeKit`.

## Anti-Patterns

| Anti-pattern | Do instead |
|---|---|
| Holding the worker of the chat product in the bot or for the product's life | Hold per open room and per query; let `ProductFirstRoomHold` cover a product with no room |
| Swallowing an uncaught worker error and keeping the worker | Report it on `TrUAPIWorkerRuntime.faults`; the manager stops the worker and the rooms go `unavailable` |
| Reading Core Data on the main thread | Read through the message source or a data provider off the main actor |
| Blocking the main thread on the core (`Db::open`, a query) | Warm the runtime in the background; `await` the query in a task |
| Opening an `NSDataDetector` per message | Reuse the one in `ProductURLDetector` |
| Writing a product-source message into `CDChatMessage` | Keep it in `ProductMessageSource` and `ProductMessageCache` |
| Sending a stored id to the product | Convert it back from `Chat.ProductMessageId` |
| Awaiting extension readiness on the send path | Render a placeholder; let send and receive go on |
| Mapping `TrUAPIHost` types outside the `+Domain.swift` files | Map once at the bridge into `Chat.*` domain types |

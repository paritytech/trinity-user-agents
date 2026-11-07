# Chat

The Chat screen shows one list of conversations from several providers. The host runs no person-to-person chat of its
own. [Contact Chat](contact-chat.md), the host's chat, is a product worker. [Nova Chat](nova-chat.md) (prototype) talks
to the Nova apps' chat.

## Chat providers

`ChatProvider` (`src/chat/provider.ts`) is the boundary between the screen and each chat. It is this host's interface,
not TrUAPI, and a native host could copy it.

- **A provider owns** its protocol, storage, delivery, retries and read receipts. It gives conversations, history pages
  (newest page first, opaque cursor), the composer state, and change events.
- **The screen calls** `send`, `retry`, and `markRead` for the newest message whose end was on screen while the page was
  visible. Opening a conversation is not reading it.
- **Optional parts:** start a conversation, draw `Custom` messages, reactions, files, calls. The screen shows a control
  only for providers that have it. A disabled control says why.
- **Lifetime.** `src/chat/inbox.ts` builds the providers when a session starts and disposes them when it ends. The view
  drops a reply that arrives after the conversation, session or provider epoch changed.

| Provider                            | Source                                                                  | One per       |
| ----------------------------------- | ----------------------------------------------------------------------- | ------------- |
| `src/chat/product-room-provider.ts` | [Product rooms](#product-rooms): TrUAPI Chat from product workers       | Product       |
| `src/chat/draft/provider.ts`        | [Chat products](#chat-extension-v2-draft-bridge), over the draft bridge | Listed worker |

## Product rooms

A product chats through its worker executable, for example a game's weekly bot. These rooms are TrUAPI Chat, so the
inspector's TrUAPI view decodes their frames.

### Workers

- Product → **Workers** adds a product name. The host reads `worker.<name>`: its `executable` record must be a worker
  manifest (`$v` 1, `kind` `worker`, an `entrypoint`, `includes.chat` true), and its `contenthash` locates the files.
- The host verifies the archive like a named product. It loads the entry in a hidden frame under the product's mount, as
  the product's `Worker` execution. The worker shares the product id, storage and accounts with its app.
- The list is per network and wallet. Workers run while the wallet is signed in, whatever screen is in front. Sign-out,
  a switch or a reload stops them. **Remove** stops a worker. Its rooms stay.
- `ProductWorkers` takes per-network `defaults`, which run for every wallet with no **Remove**. This host lists none.
- A worker runs on the host's origin and has the same trust as a named product.

### Rooms and messages

- The host serves TrUAPI Chat (`src/chat/product-chat.ts`). It refuses `registerBot`, as iOS does, with a reason that
  says "not supported", which products read as unsupported rather than failed. The worker's rooms work without a bot.
- It keeps rooms and the last 200 messages of each per network and wallet in
  `truapi-web-signing-host:network:<id>:product-chat:<wallet>:v1`.
- The core screens what a worker sends. The host stores and draws it. iOS and Android accept only `Text` and `Custom`.
- **Rich text and files.** Media shows only from an `https` URL or an inline raster image. A file shows as given,
  unchecked. Only an `https` file is a link, opened with no referrer. The host fetches no file.
- **Actions.** Buttons show in a column or a grid. A press sends `ActionTriggered` from the peer `native`.
- **Reactions.** A product reacts only to its own messages (it knows no other ids). The user reacts to the product's
  messages: the host sends `MessagePosted(Reaction)` and `MessagePosted(ReactionRemoved)` from `native`, one emoji per
  message. The native hosts refuse reactions in product rooms.
- **Custom messages.** The host asks the worker to render `RenderContext::ChatMessage` and draws the tree as DOM
  (`src/products/render.ts`): text never as markup, clamped sizes, `--pr-*` colour tokens, images only from the worker's
  verified archive or the Bulletin gateway. It does not draw a tree deeper than 48 or larger than 2000 nodes.
- **Text input.** A `TextInput` footer shows a composer. Text goes as `MessagePosted` from `native`, as on iOS.
- **Game reminder.** `game.remindNextGame` and `game.cancelNextGame` are allowed only for `dim2`. The host keeps one
  reminder per product in memory and logs it. It rings no alarm and does not survive a reload.

### Where TrUAPI Chat stops

A synthetic chat product run as a worker (`fixtures/products/chat-product-worker.ts`) shows the limits:

- It must push all its history. The host keeps the last 200 messages and never asks for a page.
- Every message shows as the product's, also messages that its own model says other people sent.
- The user's text reaches it with no message id, so the bubble stays `sent`.
- It can get a contact handle from the picker, but nothing turns that handle into a chat peer.

### Not served

Worker input surfaces, worker demand from the core, bot registration, `beginOperation` hints, and room actions (block,
leave, delete, upload, call). Pocket only for the Humanity card.

## Chat Extension v2 draft bridge

**Draft, not TrUAPI.** A page-local bridge for the Chat Extension v2 HOST–PRODUCT contract. The product keeps its own
history, and the host asks it for pages.

- **Contract:** `paritytech/trinity-user-agents` branch `lc/chat-product` at `3b2316285` (`api/chat.rs`, `v01/chat.rs`,
  `api/call_media.rs`, `chat-queries.ts`, and the RFCs `chat-extension-v2.md`, `chat-extension-v2-additions.md`,
  `call-media.md`). `src/chat/draft/contract.ts` copies its types. Nothing in this repository's crates or packages
  changes.
- **Connection:** only a `Worker` whose manifest includes chat. Its frame posts
  `{ type: "truapi-chat-v2-draft", protocol: "truapi-chat-v2-draft/1" }` with one `MessagePort`. The host takes it only
  from that frame and origin, binds it to the product id, and ends it with the worker or session. The product side is
  `connectChatV2()` in `src/chat/draft/product.ts`.
- **Scope:** the host scopes the bridge, not the core. A product reaches only its own rooms. The bridge carries no keys,
  entropy or grants. The inspector does not show it.
- **Provider** (`chat-v2:<product>`):
  - It reads pages from `messages_query`.
  - A sent message stays pending under a local id until `MessageCreated`. Any refusal leaves it failed, with Retry.
  - It sends `Seen` only when the room sets `reportSeen`.
  - It sends a user's file inline, up to 1 MiB.
  - A worker restart reloads the open room.
- **Calls:** the room declares `features.calls`. The call belongs to the product. `callMedia.start` runs only within 180
  s of a press or accept. This host has no call engine, so `start` answers `Unsupported`.
- **Capabilities:** `product_source_rooms`, `participants`, `inline_attachments`, `call_buttons`.
- **Room menu entries** (draft, in no proposal): `setRoomActions` adds up to four entries above **Block**. A choice
  sends `RoomActionTriggered { actionId }` on `actionSubscribe`. Labels that read as **Block**, **Unblock** or
  **Cancel** are refused. Entries end with the worker.
- **Departures from the reference** (the provider interface stays as it is):
  - A `Rejected` message stays failed.
  - A `Deleted` message outside the newest page goes only on reopen.
  - Call buttons are disabled, not hidden. `Actions` draw disabled. `Custom` shows its preview text.
- **Not covered:**
  - host-store rooms, `reactions_query`, `message_actions_query`, `search_query`, `open_link`, `room_calls`, the Media
    service;
  - preimage and sealed attachments, bots, mute, archive, replies, edits, rate limits;
  - persistence: declarations are lost on reload.
- **Users:** Contact Chat, in every build. `fixtures/products/synthetic-chat-product.ts` (synthetic data, for the Chat
  screen tests). No published product.

### Contact rooms

Draft additions, beside the unmerged `setRoomContact` proposal (`lc/product-only-chat` at `081dc4e43`).

- `setRoomContact { roomId, contact: { handle, state } }` names a room's person by the core's contact handle, with a
  state (`Outgoing`, `Incoming`, `Established`, `PeerLeft`, `Invite`). Only a product that the host allows (Contact
  Chat) may set it.
- The host names the person only from its own contacts, through `contactHandles`. An unknown handle shows as
  **Contact**. The product never learns the name.
- `startConversation { handle, welcome? }`: **+** opens the host picker. An existing room opens. Anyone else gets the
  request form, and only **Send request** asks the product.
- A product cannot resolve a handle to an account. So a chat engine that addresses peers by account cannot run as a
  product. Contact Chat reaches people only through its host's request service.
- The people in a product's rooms appear only in the host's **+**. They never become `contacts.pick` results or payment
  recipients.
- `src/chat/draft/conversations.ts` (`ConversationEngine`) shows any engine over the bridge.

## Inbox (prototype)

The Inbox lists requests that wait for an answer: chat invitations and profile sharing requests. It is not a toast and
not the OS notification permission.

- Chat's **Inbox** tab and the Apps shelf's **Inbox** open the same list.
- A row is seen when 60% of it shows while the page is visible. Counts and the Apps dot read the same items. Seen is not
  an answer.
- **Dismiss** hides the row on this device. Nothing is sent. The item keeps its state, gets `dismissedAt` and still
  expires.
- **Accept** hands the intent to the owning product. The row shows **Accepting…**. A refusal, a closed product or no
  answer in 30 s shows the reason and keeps the request waiting. Accepted means that the recipient's product replied.
- **Sent** lists the user's pending requests, with **Cancel** (local only) and **Paused** with a reason.
- Storage: `truapi-web-signing-host:network:<id>:notifications:<wallet>:v1`, per network and wallet or demo user. The
  list is in `src/notifications/`.
- Dev server: More → **Add sample notifications** (from `fixtures/notifications/sample-notifications.ts`, Dismiss only)
  and **Clear samples**.

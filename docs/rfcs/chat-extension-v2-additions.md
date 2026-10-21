---
title: "Chat Extension v2 additions"
owner: "@valentin-parity"
status: draft
---

# RFC — Chat Extension v2 additions

## Summary

[Chat Extension v2](chat-extension-v2.md) lets a product's Worker own the data of its chat rooms while the host draws
them. This RFC adds what the base leaves out: a product can ask the host what it serves, declare optional features per
room, put a call into a room, learn what the user does with these features, answer a search the host asks, and take the
payload of a room link the user opens. `Chat` gains six methods, ids `22`, `23` and `25` to `28`, and no existing
request, item or event of the base changes. The host also gains behaviour that needs no wire method:
large attachments stored as preimages paid by the product, bounded worker wakes, bots as personas, and the user's own
mute and archive state. This RFC names the base and does not change it, so the base can be accepted on its own.

## Motivation

The base gives a product a complete chat room, and a product that wants more learns what a host serves only by failure:

- A product cannot tell what a host serves before it declares a room. It learns from `Unsupported`, from a "this host
  does not serve X" answer, or from `NotAvailable`, one call at a time. Hosts differ already: a host may not draw the
  payment button, and a web host may serve none of Chat.
- A user cannot send a file larger than 1 MiB into a product room. Every user attachment is inline.
- A product-source room changes only while something holds the worker. A notification for the room cannot refresh its
  preview, and a reply sent just before the user closes the room may never reach the product.
- The host can search only the rooms it stores. A product-source room has no search at all.
- `register_bot` exists, but no host draws a bot, so a product cannot speak as more than one named voice in its rooms.
- An accessible chat needs text read aloud. The base has no such action.
- A call needs buttons in the room header, a ringing state and a call bar. The base has no call state, and the
  [Realtime Media Sessions](https://github.com/paritytech/host-rust-core/blob/rfc/realtime-media-sessions/docs/rfcs/realtime-media-sessions.md)
  design has no way to show a session inside a chat room.
- The user cannot mute or archive a product room, and a product cannot respect a choice it never hears of.
- A room link opens a room, but a product cannot be given anything with it. A chat product that shares an invitation,
  a contact link or a ticket as a link has no way to receive it when someone else opens that link.

## Approach

The design has these parts:

- **Rules**: what every addition follows.
- **Trait**: the six new methods.
- **Types**: the payloads of the new methods.
- **Capabilities**: the feature names a host lists and what the core removes.
- **Room features and calls**: the rules of the new requests.
- **Room events**: the second event stream and its snapshot.
- **Features**: bots, mute and archive, preimage and sealed attachments, search, read aloud, wakes, calls and room link
  payloads.
- **Limits and timeouts**: the bounds the core enforces and the time the host waits.

### Rules

1. **No payload the product sends gets a new version.** The TypeScript client encodes the newest version of every
   request and host-initiated item, and a host on an older core cannot decode a version it does not know. Hosts update
   more slowly than products, because app stores release them. So the additions are new methods, and every request,
   item and event of the base keeps its current version. A product built on the new client keeps every base call working
   on a host that serves only the base; the new calls answer `unavailable` there and the product falls back.
2. **Host-initiated requests stay `V1`.** The core sends a query's request as `V1`, so a new question is a new query
   and never a new version of an old one.
3. **Everything new is opt-in per room**, through `ChatRoomFeatures` or a new request. A room that declares nothing
   draws as the base draws it.
4. **Every feature is named in `capabilities`.** A host lists a feature only when it serves it end to end, and the core
   removes a name it cannot back. A product that reads no capabilities still works: an unserved feature is stored and
   ignored, a live request for it is refused with `Unknown`, and an unserved query is never asked.
5. **The host does all network work.** Storing a preimage, receiving a notification and carrying a call go through the
   core and the host's own services. Nothing here gives a product a connection to a node, a statement store or a
   chain, and no product holds an allowance or chain key.
6. **The core screens, hosts render.** The core enforces every bound of [Limits](#limits) before a host sees a value,
   as for the base.

### Trait

`Chat` keeps trait id `4` and every method of the base. The new methods take ids `22`, `23` and `25` to `28`, and every
one has a default body that answers `unavailable`.

```rust
    // ---- Product to host requests ------------------------------------------

    /// The Chat features this host serves and its attachment bounds.
    #[wire(id = 22)]
    async fn capabilities(&self, cx: &CallContext, request: HostChatCapabilitiesRequest)
        -> Result<HostChatCapabilitiesResponse, CallError<HostChatCapabilitiesError>>;

    /// Declare the optional features of a room. Persisted by the host;
    /// replaces the previous declaration whole.
    #[wire(id = 23)]
    async fn set_room_features(&self, cx: &CallContext, request: HostChatSetRoomFeaturesRequest)
        -> Result<HostChatSetRoomFeaturesResponse, CallError<HostChatRoomError>>;

    /// Show, change or clear the call of a room.
    #[wire(id = 25)]
    async fn set_room_call(&self, cx: &CallContext, request: HostChatSetRoomCallRequest)
        -> Result<HostChatSetRoomCallResponse, CallError<HostChatRoomError>>;

    /// What the user does with the added features of the product's rooms:
    /// room state, wakes, calls. Starts with a snapshot of the
    /// rooms the user muted or archived.
    #[wire(id = 26)]
    async fn room_events_subscribe(&self, cx: &CallContext, request: HostChatRoomEventsSubscribeRequest)
        -> Subscription<HostChatRoomEventsSubscribeItem, CallError<HostChatRoomEventsSubscribeError>>;

    // ---- Host to product queries -------------------------------------------

    /// Messages of a product-source room that match what the user typed.
    /// One item, then complete.
    #[wire(host_initiated, id = 27)]
    fn search_query(&self, cx: &CallContext, request: ProductChatSearchQueryRequest)
        -> Subscription<ProductChatSearchQueryItem, CallError<ProductChatQueryError>>;

    /// The payload of a room link that the user opened. One item, then
    /// complete.
    #[wire(host_initiated, id = 28)]
    fn open_link(&self, cx: &CallContext, request: ProductChatOpenLinkRequest)
        -> Subscription<ProductChatOpenLinkItem, CallError<ProductChatQueryError>>;
```

Versions of each leg:

| Method                              | Leg       | Version                                    |
| ----------------------------------- | --------- | ------------------------------------------ |
| `22`, `23`, `25` to `28`            | All legs  | `V1`, first shape                          |
| Every method of the base            | All legs  | Unchanged                                  |
| `register_bot` (`1`)                | All legs  | Unchanged; the meaning is in [Bots](#bots) |

The responses of methods `23` and `25` are bare `V1` unit responses, as for the base's `set_room_*` requests. The
requests of `22` and `26` carry no payload, and their errors are `V1` of `GenericError`.

### Types

The new types reuse `ChatMessage`, `Bytes32`, `HostChatRoomError` and `ProductChatQueryError` of the base unchanged.

```rust
/// Request: `HostChatCapabilitiesRequest { V1 }`, no payload.
pub struct HostChatCapabilitiesResponse {
    /// Names of the features this host serves. A product ignores a name it
    /// does not know.
    pub features: Vec<String>,
    /// Largest media item the host sends inline for the user.
    pub inline_attachment_max_bytes: u32,
    /// Largest attachment the host stores as a preimage; 0 when the host
    /// does not list `preimage_attachments`.
    pub preimage_attachment_max_bytes: u32,
    /// Largest attachment the host seals; 0 when the host does not list
    /// `sealed_attachments`.
    pub sealed_attachment_max_bytes: u32,
}
// Error: `HostChatCapabilitiesError { V1 => GenericError }`.

/// Call buttons in a room header.
pub struct ChatRoomCalls { pub audio: bool, pub video: bool }

/// Optional features of one room. Every field at its default selects the
/// behaviour of the base.
pub struct ChatRoomFeatures {
    /// A user attachment above the inline bound is stored as a preimage paid
    /// from the product's Bulletin allowance.
    pub preimage_attachments: bool,
    /// The product answers `search_query`. Read only for product-source rooms.
    pub search: bool,
    /// Call buttons in the header; `None` shows none.
    pub calls: Option<ChatRoomCalls>,
    /// The host offers "Read aloud" on text messages.
    pub read_aloud: bool,
    /// A user attachment above the inline bound is sealed: encrypted by the
    /// core and stored in parts paid from the product's Bulletin allowance.
    pub sealed_attachments: bool,
}
pub struct HostChatSetRoomFeaturesRequest { pub room_id: String, pub features: ChatRoomFeatures }

/// The call of a room.
pub enum ChatRoomCall {
    /// An incoming call the product received; the host rings in the room.
    Ringing { call_id: String, video: bool, caller: Option<String> },
    /// A call in progress. With `media_session`, the host presents that
    /// session of the Media service; without, it shows the call bar only.
    Active { call_id: String, video: bool, started_at: u64, media_session: Option<Bytes32> },
}
pub struct HostChatSetRoomCallRequest { pub room_id: String, pub call: Option<ChatRoomCall> }

/// What the user does with the added features of a room.
pub enum ChatRoomEvent {
    /// The user muted, unmuted, archived or unarchived the room; also sent
    /// for every muted or archived room when the subscription starts.
    UserStateChanged { muted_until: Option<u64>, archived: bool },
    /// The host holds the worker for a notification routed to this room.
    Woken { watch_id: String },
    /// The user pressed a call button of the header.
    CallRequested { video: bool },
    /// The user answered the ringing call.
    CallAnswered { call_id: String, accept: bool },
    /// The user tapped the call bar of a call without a media session.
    CallOpened { call_id: String },
    /// The user pressed "End" on the call bar.
    CallEnded { call_id: String },
}
// Request: `HostChatRoomEventsSubscribeRequest { V1 }`; error `{ V1 => GenericError }`.
pub struct HostChatRoomEventsSubscribeItem { pub room_id: String, pub event: ChatRoomEvent }

pub struct ProductChatSearchQueryRequest {
    pub room_id: String,
    /// What the user typed; screened display text.
    pub text: String,
    /// `None` asks for the newest matches.
    pub cursor: Option<String>,
    pub page_size: u32,
}
pub struct ProductChatSearchQueryItem {
    /// Newest first.
    pub hits: Vec<ChatMessage>,
    /// Cursor for older matches; `None` ends them.
    pub next: Option<String>,
}

pub struct ProductChatOpenLinkRequest {
    /// The room the link names.
    pub room_id: String,
    /// The text after `#` in the link, percent-decoded. The host does not read it.
    pub payload: String,
}
pub struct ProductChatOpenLinkItem {
    /// The room the host shows next; `None` keeps the room of the link.
    pub room_id: Option<String>,
}
```

As in the base, the variant order of every enum is part of the wire: a later addition goes after the last variant.

### Capabilities

`capabilities` answers stable feature names and two byte bounds. A host lists a name only when it serves the feature
end to end on that build.

| Name                                                          | Feature                                                                                                   |
| ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `product_source_rooms`                                        | Product-source rooms with paging, optimistic sends, `messages_query` and `user_action`                    |
| `reaction_details`, `participants`, `message_menus`           | The three lookup queries of the base                                                                      |
| `custom_bodies`, `custom_footer`                              | `Custom` bodies and the `Custom` footer drawn through `Renderer::render`                                  |
| `inline_attachments`                                          | The user's photos, videos and files sent inline, up to `inline_attachment_max_bytes`                      |
| `payment_button`                                              | `ChatInputPayment` drawn and wired                                                                        |
| `room_links`                                                  | `polkadot://<product>.<tld>/-/chat/<room>` opens the room                                                 |
| `preimage_attachments`                                        | User attachments above the inline bound stored as preimages ([Preimage attachments](#preimage-attachments)) |
| `sealed_attachments`                                          | User attachments above the inline bound sealed and stored in parts ([Sealed attachments](#sealed-attachments)) |
| `search`                                                      | In-room search, and `search_query` for product-source rooms ([Search](#search))                           |
| `bot_personas`                                                | Personas drawn for `bot:<bot_id>` participants ([Bots](#bots))                                            |
| `mute_archive`                                                | The user's mute and archive of product rooms ([Mute and archive](#mute-and-archive))                      |
| `read_aloud`                                                  | "Read aloud" in the message menu ([Read aloud](#read-aloud))                                               |
| `wake`                                                        | Notification wakes and `Woken` events ([Wakes](#wakes))                                                   |
| `call_buttons`                                                | Header buttons, `Ringing`, the call bar and the four call events ([Calls](#calls))                        |
| `room_calls`                                                  | `Active { media_session: Some(_) }` presented by the host ([Calls](#calls))                               |
| `room_link_payloads`                                          | The payload of an opened room link reaches the product ([Room link payloads](#room-link-payloads))         |

The core answers the host's list minus what it cannot back: `custom_bodies` and `custom_footer` without a `Renderer`
host, `room_calls` while the runtime serves no Media service, and `payment_button` until the footer payment path
exists. It drops names it does not know. The bounds are core constants: 1 MiB inline, 8 MiB for a preimage, or 0 when
`preimage_attachments` is not listed, and 32 MiB for a sealed attachment, or 0 when `sealed_attachments` is not listed. The Bulletin chain's own per-transaction bound can be lower; staging
reports it.

A product that wants a feature reads `capabilities` first and falls back when the name is missing. On a host that
serves only the base, `capabilities` answers `unavailable`, and the product treats every added feature as missing.

### Room features and calls

- **`set_room_features`** needs a room the product hosts, as every `set_room_*` request does. The host stores the
  declaration whole, keyed by product and room, with a version byte, and keeps it across restarts. It stores a feature
  it does not serve and ignores it, so a later host build applies it.
- **`set_room_call`** needs a room the product hosts, at most two calls per second per room. `Ringing` and
  `Active { media_session: None }` need `call_buttons`. `Active { media_session: Some(_) }` needs `room_calls` and a
  live Media session that belongs to this product. Otherwise the core answers `Unknown` with a reason that names the
  feature. `None` clears the call.

### Room events

`room_events_subscribe` is a second product stream beside `action_subscribe`, so the base's `ChatEvent` keeps its
version. Its items are what the user does with the added features: `UserStateChanged`, `Woken` and the four call
events.

- When a subscription starts, the core sends `UserStateChanged` for every room the user muted or archived, then the
  events queued while no one subscribed, then live events. A restarted worker therefore learns the user's state first.
- The core queues at most 64 events per product while no subscription is open, and drops the oldest beyond that. The
  queue is per product, not per connection, so an event published while the worker is stopped reaches its next start.
- A second subscription on one connection replaces the first, as for `action_subscribe`.

### Bots

A bot is a **persona**: a name and an icon the product speaks as in its own rooms.

- `register_bot { bot_id, name, icon }` stores a persona for the product. It answers `New` the first time and `Exists`
  after, and a repeat replaces the name and icon. A product has at most 8 personas; the ninth is refused with the
  catch-all error.
- The participant id `bot:<bot_id>` is reserved, like `"user"` and `"product"`. The host draws it with the persona's
  name and icon without a `participants_query`. A product writes as a persona with `origin.participant_id` set to
  `bot:<bot_id>` in `update_feed` `New` and in `messages_query` pages.
- The core refuses an origin `bot:<bot_id>` that is not a registered persona of the product, with `Unknown` and the
  reason `unknown bot <bot_id>`.
- `list_subscribe` never lists a room as `ChatRoomParticipation::Bot`. A bot in a person chat or in another product's
  room stays out of scope: a person chat is end-to-end encrypted between people, and a product in it would read it.
- A product that answers `participants_query` may still list `bot:` entries; the host prefers the persona for the name
  and icon.
- On a host without `bot_personas`, `register_bot` is refused as before, and a `bot:` origin draws as the product. The
  product reads the capability and posts as `"product"`.

### Mute and archive

Mute and archive are the user's choices for a product room, kept by the host. The product cannot set either.

- `muted_until` is `None`, a time in milliseconds since the Unix epoch, or `u64::MAX` for always. The host offers one
  hour, eight hours, one week and always.
- A muted room shows a muted mark and a grey badge, and its notifications are suppressed: an immediate
  `send_push_notification` whose deeplink is a room link of this product into a muted room answers `Ok` with an id and
  shows nothing, as a silenced notification does today. A scheduled notification is checked when it fires where the
  host controls firing; where the operating system fires it, it is not checked. A wake still runs, so the preview stays
  fresh.
- An archived room moves to an archived list and stays there when new messages arrive until the user unarchives it.
  A product preview or badge on it is still stored. Deleting the room works as before.
- The product learns both through `UserStateChanged`, live and as the snapshot at subscription start, and can stop
  sending notifications to a muted room.
- Person chats and host-placed rooms are not changed by this RFC. On a host without `mute_archive` nothing changes and
  the product never receives the event.

### Room notifications

A notification of a product whose deeplink is a room link of the same product (`/-/chat/<room_id>`) is a notification
of that room. The host shows it as it shows a notification of a person chat:

- The title is the name of the room. The text is the text of the notification, so a product sends the message
  alone, not the sender name before it. A notification that opens no room of the product that the host knows keeps
  the title of other product notifications.
- While the app is in the foreground, the host shows it as its in-app banner, except while that room is on screen or
  where the host also hides person chat notifications (on iOS, the chat list). In the background the operating
  system shows it.
- A tap opens the room, as the room link does.
- A muted room shows nothing (see Mute and archive).

So a product sends a notification for each event that the user must know about, also while its worker runs in the
foreground, and does not follow which room is on screen.

### Preimage attachments

A room that declares `preimage_attachments` lets the user send an attachment above the inline bound. The host stores it
as one preimage and sends the message with `ChatResource::Preimage { key }`, which every media item of the base can
already carry. No wire method is added: the host stages the attachment through a runtime call of the core, so the
worker need not run.

- **The product pays.** The preimage is submitted from the product's own Bulletin allowance account, as if the product
  had called `Preimage::submit`. The user sends into the product's room, the product opted in, and allowances are
  already partitioned per product ([RFC 0010](0010-allowance.md)). The host's own chat allowance is not used, because it
  would link the product's traffic to the user's chat identity.
- **Permission.** The core requires `PreimageSubmit` for the product, prompting when it is undecided, exactly as
  `submit` does.
- **Public storage.** Bulletin data is readable by anyone who has the key. Before the first staged upload for a
  product, the host shows a one-time notice per product that files over the inline bound sent in that product are
  stored on the public Bulletin chain. Declining keeps that attachment unsent. The attachment is not encrypted: a key
  field would version every body type, against rule 1. A product that needs private files keeps the flag off.
- **Bounds.** One preimage per attachment, at most 8 MiB and never above the Bulletin chain's per-transaction bound,
  which the core reads at runtime from the `MaxTransactionSize` constant of the chain's `TransactionStorage` pallet.
  There is no chunking. A refusal names the bound that held.
- **The declaration the core reads.** The staging call carries the product and the room, not the room's features: a
  host does not vouch for its own opt-in. The core reads the `preimage_attachments` flag from the last
  `set_room_features` it accepted for the room in this runtime, which a `delete_room` forgets. When it has none, as after
  a restart, it reads the features the host keeps through the chat adapter of the product's connection: the runtime's
  own on a web host, the open Worker execution's on a native host, which also shows the `PreimageSubmit` prompt. With
  neither, the room declares nothing and nothing is uploaded. The allowance key never leaves the core.
- **Send flow.** In a product-source room the host draws the message `Preparing`, stages the attachment, then sends
  `user_action { NewMessage }` with the preimage media item and its `ChatMediaMeta`. In a host-store room the host
  stores the message `Preparing`, stages, then marks it `Sent` and publishes `MessagePosted`. A rejection or a time-out
  marks the message `Failed`, with retry and delete.
- The product receives a preimage item and fetches its bytes with `Preimage::lookup_subscribe` when it needs them.
- Without the flag, the host keeps the 1 MiB inline rule. A host can make an attachment above the bound smaller
  before it refuses it: the iOS host encodes a photo again as JPEG and scales it down, and exports a video at 640x480.
  A product that encrypts its files declares `sealed_attachments` instead ([Sealed attachments](#sealed-attachments)).

### Sealed attachments

A room that declares `sealed_attachments` lets the user send an attachment above the inline bound that the Bulletin
chain never holds in plain form. The host seals it through a runtime call of the core, and sends the message with a new
resource:

```rust
pub enum ChatResource {
    Inline { mime_type: String, bytes: Vec<u8> },
    Preimage { key: Vec<u8> },
    /// XChaCha20-Poly1305 of the bytes under `key` and `nonce`, with the
    /// associated data `truapi/chat/sealed/v1`, cut into `parts` in order.
    Sealed { parts: Vec<Vec<u8>>, key: Vec<u8>, nonce: Vec<u8>, size: u64, digest: Vec<u8> },
}
```

- **Sealing.** The core makes a new 32-byte key and 24-byte nonce for each attachment, encrypts the bytes, cuts the
  ciphertext into parts of at most one Bulletin transaction (the `MaxTransactionSize` it reads at runtime), and
  submits each part from the product's Bulletin allowance under `PreimageSubmit`, as for a preimage attachment.
  `size` is the length of the ciphertext and `digest` its SHA-512. At most 32 MiB and 64 parts.
- **Who reads it.** The resource holds the key, so a product sends it only through its own encrypted channel; the
  verbosity chat puts the parts and the key in the file description of its end-to-end encrypted message. A host that
  draws a `Sealed` item reads the parts with its preimage lookup, checks each part against its key and the
  ciphertext against `size` and `digest`, and decrypts. The core offers the same opening for hosts that draw through it.
  A host keeps an opened file, so that it outlives the two weeks that the chain keeps the parts.
- **No notice.** The chain holds only ciphertext, so the host shows no public-storage notice.
- **The declaration.** As for preimage attachments: the core reads the flag from the last `set_room_features` it
  accepted, else from the features the host keeps. A room that declares both kinds gets sealed attachments.
- **Images and icons.** A `Sealed` resource is a media item only. The core refuses it as an avatar, an icon or a badge.
- **Hosts.** The iOS and Android apps seal, open, and list the feature. The CLI host roles seal and list the
  feature; their development page shows a sealed item as its size and parts, as it shows a preimage as its key. The web
  hosts do not list the feature, and a product keeps the 1 MiB inline rule there.

### Search

- **In-room search, both room modes.** The host searches a host-store room in its own store. For a product-source room
  that declares `search`, the host asks `search_query { text, cursor, page_size }` once the user stops typing, with a
  page size of 30 and at most 50, and asks older pages with `next`. The core screens the answer as it screens a page of
  `messages_query`, and holds at most `page_size` hits. `NotAvailable` or a time-out shows that search is not
  available. A product-source room without `search` shows no search action.
- The core asks `search_query` only for a product-source room that declares `search`; it answers `NotAvailable` itself
  otherwise, without starting the worker.
- **List search.** A search over the chat list shows the stored hits first. Then it asks `search_query` of the
  product-source rooms that declare `search`, newest rooms first, at most 8 rooms with a page of 10, and only of a
  product whose worker runs now, so a search never starts a worker. The hits that arrive within 4 s join the stored hits,
  newest first; a room that fails or answers late gives none.
- On a host without `search` the product's handler is never called.

### Read aloud

With `features.read_aloud`, the message menu gains a built-in "Read aloud" on `Text`, `RichText` and `Custom` with
`preview_text`, spoken by the platform's text to speech in the host's locale. Leaving the room stops it. It is off
unless the room declares it, and it is not a hideable built-in action, because `ChatBuiltInMessageAction` is a base
enum that rule 1 does not reopen.

### Wakes

A **wake** is a bounded hold: the host takes one worker reference for 30 s for a reason, and the product does what it
needs in that time.

| Source              | When                                                                                                       | Hold                              | Event                                   |
| ------------------- | ---------------------------------------------------------------------------------------------------------- | --------------------------------- | --------------------------------------- |
| Message in flight   | A user message, a user action or a room event is sent to the product while no room screen holds the worker | 30 s                              | None: the product receives the message  |
| Product start       | A chat product starts and owns no room yet                                                                 | 30 s, once                        | None                                    |
| Notification        | An authenticated receiving event arrives on a watch of the product routed to `/-/chat/<room_id>`           | 30 s; at most 12 per product per rolling hour | `Woken { watch_id }` in that room |

- A product has at most one wake reference; a new wake moves its deadline. The product extends past 30 s only with
  `Worker::begin_operation`, which the operating system may still end on a phone.
- During a wake the product may do anything a running worker does: read the statement store through the host, update
  its feed and preview, and send a notification with the room link as deeplink, which the host suppresses for a
  muted room.
- There are no scheduled wakeups and no periodic background refresh. A scheduled notification opens the room when
  tapped, and the room screen holds the worker.
- The push transport of a notification wake is the receiving layer's: a relay watches the product's channels for the
  enrolled device and pushes, the core verifies the frames and calls the wake. The product never sees a push token or
  a relay credential, and the app never reads the statement store outside the core.
- `wake` is listed only where notification wakes work end to end. The other two sources need no capability: they are
  how the host keeps the [Worker Lifecycle](worker-lifecycle.md) promise for a message in flight.

### Calls

The chat side of a call is a room state the product sets and events the user produces; the media belongs to the Media
service of Realtime Media Sessions.

- **Buttons.** `features.calls { audio, video }` draws call buttons in the room header. A press sends
  `CallRequested { video }`, and the product starts the call its own way.
- **Ringing.** `set_room_call(Some(Ringing { .. }))` rings in the room with the caller's name and avatar, Accept and
  Decline, and shows the call in the chat list and, with the app elsewhere, as an in-app banner or a notification.
  Accept or Decline sends `CallAnswered { call_id, accept }`. The ringing clears on `set_room_call(None)` or after
  60 s, the lifetime of a Media offer.
- **Active.** `Active { .. }` shows a call bar with the elapsed time and End. Without a media session, a tap on the bar
  sends `CallOpened` so the product opens its own call page, and End sends `CallEnded`. With `media_session`, a tap opens
  the host's call screen, which presents that session itself: remote pictures, the self preview, and the device
  controls of the host's Media backend. The product places no picture rectangles for a session bound to a room. End
  ends the session and sends `CallEnded`.
- **Worker.** A room with a `Ringing` or `Active` call holds the worker until the call is cleared or the room is
  deleted. The core keeps this hold, so every host gets it. A `Ringing` call holds it for at most 60 s, the time the host
  rings before it clears the banner. If the worker stops, the host clears a ringing call; an active session lives on in
  the Media service and the bar follows its state.
- A product call rings only while the product's worker runs. Ringing a closed app needs notification wakes and the
  platforms' native call screens, which this RFC leaves out.
- `call_buttons` covers the buttons, ringing, the bar and the four events, so it ships before the Media service.
  `room_calls` covers host-presented sessions and needs the Media service in the runtime, with the matching amendment
  to the Realtime Media Sessions RFC: a session bound to a room is presented by the host, `set_surfaces` for it is
  refused, and the core checks that the session belongs to the product that binds it.

### Room link payloads

A room link is `polkadot://<product>.<tld>/-/chat/<room_id>`, or its web form on a share host,
`https://<name>.<root>.li/-/chat/<room_id>`, which a phone opens in the app. The text after `#` is a **payload** for
the product. A product puts in it what it wants the person who opens the link to give it: a contact link, an
invitation, a ticket. A browser does not send the fragment of a web link to a server, so the payload stays between the
person who shares the link and the host of the person who opens it.

- The host reads the room from the link and shows it, as for any room link, or the App of the product when it has no
  such room yet. Then it asks the product `open_link { room_id, payload }` with the payload percent-decoded, and holds
  the worker until the answer. The host does not read, show or store the payload.
- The product answers the room the host shows next, or `None` to keep the room of the link. A product without a
  handler answers `Unsupported`, and the user stays in the room of the link: a product built before this method keeps
  working.
- The core asks for any room of the product, needs no declaration, and answers `NotAvailable` itself for a room the
  product does not have. It refuses an empty payload or one above the bound with `Unknown`, and refuses an answer that
  names an invalid room.
- Keep a link short. iOS cuts a link that another app or the system opens at 2,047 bytes, so a payload that makes the
  link longer arrives cut. A product that needs more puts a short reference in the payload, such as a username, and
  gives the long form another way, for example as text the user pastes.
- An opened link is the user's choice, but the link can come from anyone. A product treats the payload as untrusted
  input, and asks the user before it acts on it with an effect the user did not ask for, such as a connection.
- A host scanner that reads a room link of a product, in either form, gives it to the link pipeline of the app, so a
  QR code of a room link works from the main scanner of the host and from the camera of the phone.

### Limits

The core enforces these bounds before a host sees a value, with the identifier and display-text screening of the base:

| Value                                                                                                                    | Bound                                                                                    |
| ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------- |
| `features` names in `capabilities`                                                                                       | Identifier, at most 64 entries                                                           |
| `call_id`, `watch_id`, `caller`, persona `bot:<bot_id>`                                                                  | Identifier, 256 bytes                                                                    |
| Search `text`                                                                                                            | Display text, 1 grapheme to 256 bytes                                                    |
| Search `page_size` the host asks                                                                                         | 1 to 50, host default 30; hits at most `page_size`                                       |
| Search cursor                                                                                                            | 1 KiB                                                                                    |
| Staged attachment                                                                                                        | 8 MiB, and not above the Bulletin chain's per-transaction bound                          |
| Personas per product                                                                                                     | 8                                                                                        |
| `set_room_call`                                                                                                          | 2 per second per room                                                                    |
| Wake                                                                                                                     | 30 s per hold; at most 12 notification wakes per product per rolling hour                |
| Room events queued with no subscriber                                                                                    | 64 per product, oldest dropped                                                           |
| Room link `payload`                                                                                                      | 1 byte to 8 KiB of UTF-8                                                                 |

A rate over its bound fails with `RateLimited`, and any other bound fails with `Unknown` naming the field.

How long the host waits:

| What                | Host waits                                                  | Then                                                 |
| ------------------- | ----------------------------------------------------------- | ---------------------------------------------------- |
| `search_query`      | 10 s                                                        | Shows that search is not available; the field stays  |
| Staging a preimage  | The `Preimage::submit` time-out, 360 s                      | The message is `Failed` with retry                   |
| Wake hold           | 30 s, extended only by the product's own `begin_operation` | Releases the reference                               |
| `Ringing`           | The product's `set_room_call(None)`, or 60 s                | The ringing clears                                   |
| `open_link`         | 90 s, without blocking the screen: the link can start the worker | Logs the failure; the user stays in the room of the link |

## Trade-offs

- **Methods, not versions.** Adding fields to `ChatMessage` as a `V2`, or new events to `ChatEvent` as a `V3`, gives
  cleaner types, but the client always encodes the newest version and an older host cannot decode it. New methods keep
  every base call of a new product working on an older host.
- **Names, not an enum.** `capabilities` answers strings, so a later feature needs no new version. A
  `System::feature_supported` variant per feature, or a `host_info` `V2`, were dropped: the first versions a request
  for each feature, the second versions a method every product calls. The call sits on `Chat` because only a Worker
  uses chat.
- **One features value.** `ChatRoomFeatures` is a new persisted declaration beside `ChatRoomConfig`. New fields in
  `ChatRoomConfig` would need a `V2` and a storage migration on every host, because hosts store the configuration
  without a version prefix.
- **The product pays for staged attachments.** A host purse does not exist in RFC 0010, and the host's chat allowance
  would link product traffic to the user's chat identity. A product that does not want to pay keeps the flag off.
- **One public preimage per attachment.** Chunked uploads with a manifest, peer-to-peer handoff tickets and host-side
  encryption were dropped for now: each needs a new body field or a new transport, and the one-time notice states that
  the file is public.
- **List search asks only running workers.** Asking every product while the user types in the list search would start
  every worker on a keystroke. The list asks only the products whose worker runs, after the typing pause, with a limit
  and a deadline; a product-source room of a stopped product matches by its name only.
- **Calls before media.** Splitting `call_buttons` from `room_calls` lets the buttons, ringing and call bar ship
  before the Media service is accepted. Waiting for the Media service, or letting a product place picture rectangles
  in a chat screen, were dropped. Native call screens for a closed app are left out.
- **Read aloud is opt-in.** It is off by default, so a product opts in per room.
- **Bounded wakes.** A worker kept alive while notifications are enabled, or a periodic background refresh, would
  break the Worker Lifecycle rule that a worker runs only while referenced. A 30 s hold with an hourly budget keeps a
  product from running in the background on its own.

## Open questions

- **A push relay for product wakes.** Notification wakes need a relay that pushes to iOS and Android devices with the
  apps' push credentials. Only browser push is wired today. Until a relay exists, hosts do not list `wake`.
- **The receiving layer.** Notification wakes route a delivered receiving event to a room through the watch's route
  `/-/chat/<room_id>`. That layer is not yet on the base branch, and its route field is assumed to carry this form.
- **The Media service.** `room_calls` needs the Realtime Media Sessions RFC accepted with the host-presented amendment,
  a final trait id, and native Media backends. Until then the core removes `room_calls`.
- **Background reach on iOS.** A background push gives an iOS app about 30 s and the system throttles silent pushes.
  Whether a worker that is a headless web view boots and answers in that window is not measured, so a cold wake on iOS
  is best effort.
- **Retention of staged attachments.** Bulletin data may be deleted after its retention period. A staged attachment
  then draws as a missing preimage, as a product's own preimage does. Whether hosts or products renew it is not
  settled.

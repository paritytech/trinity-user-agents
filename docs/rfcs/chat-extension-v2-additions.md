---
title: "Chat Extension v2 additions"
owner: "@valentin-parity"
status: draft
---

# RFC — Chat Extension v2 additions

## Summary

[Chat Extension v2](chat-extension-v2.md) lets a product's Worker own the data of its chat rooms while the host draws
them. This RFC adds what the base leaves out: a product can ask the host what it serves, declare optional features per
room, decorate a message with a thread summary, a branch position or a reasoning flag, put a call into a room, learn
what the user does with these features, and answer a search the host asks. `Chat` gains six methods, ids `22` to `27`,
and no existing request, item or event of the base changes. The host also gains behaviour that needs no wire method:
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
- An assistant chat needs threads, alternative answers and a visible reasoning step, and an accessible chat needs text
  read aloud. The base has none of them.
- A call needs buttons in the room header, a ringing state and a call bar. The base has no call state, and the
  [Realtime Media Sessions](https://github.com/paritytech/host-rust-core/blob/rfc/realtime-media-sessions/docs/rfcs/realtime-media-sessions.md)
  design has no way to show a session inside a chat room.
- The user cannot mute or archive a product room, and a product cannot respect a choice it never hears of.

## Approach

The design has these parts:

- **Rules**: what every addition follows.
- **Trait**: the six new methods.
- **Types**: the payloads of the new methods.
- **Capabilities**: the feature names a host lists and what the core removes.
- **Room features, meta and calls**: the rules of the new requests.
- **Room events**: the second event stream and its snapshot.
- **Features**: bots, mute and archive, preimage attachments, search, threads, branches, reasoning rows, read aloud,
  wakes and calls.
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

`Chat` keeps trait id `4` and every method of the base. The new methods take ids `22` to `27`, and every one has a
default body that answers `unavailable`.

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

    /// Replace the thread summary, branch position and reasoning flag of one
    /// message. Persisted in both room modes.
    #[wire(id = 24)]
    async fn set_message_meta(&self, cx: &CallContext, request: HostChatSetMessageMetaRequest)
        -> Result<HostChatSetMessageMetaResponse, CallError<HostChatUpdateFeedError>>;

    /// Show, change or clear the call of a room.
    #[wire(id = 25)]
    async fn set_room_call(&self, cx: &CallContext, request: HostChatSetRoomCallRequest)
        -> Result<HostChatSetRoomCallResponse, CallError<HostChatRoomError>>;

    /// What the user does with the added features of the product's rooms:
    /// room state, wakes, calls, branches. Starts with a snapshot of the
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
```

Versions of each leg:

| Method                              | Leg       | Version                                    |
| ----------------------------------- | --------- | ------------------------------------------ |
| `22` to `27`                        | All legs  | `V1`, first shape                          |
| Every method `0` to `21`            | All legs  | Unchanged                                  |
| `register_bot` (`1`)                | All legs  | Unchanged; the meaning is in [Bots](#bots) |

The responses of methods `23` to `25` are bare `V1` unit responses, as for the base's `set_room_*` requests. The
requests of `22` and `26` carry no payload, and their errors are `V1` of `GenericError`.

### Types

The new types reuse `ChatMessage`, `Bytes32`, `HostChatRoomError`, `HostChatUpdateFeedError` and
`ProductChatQueryError` of the base unchanged.

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
}
// Error: `HostChatCapabilitiesError { V1 => GenericError }`.

/// Call buttons in a room header.
pub struct ChatRoomCalls { pub audio: bool, pub video: bool }

/// The message a thread room answers.
pub struct ChatThreadParent { pub room_id: String, pub message_id: String }

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
    /// This room is the thread of a message of another room of the product.
    pub thread_of: Option<ChatThreadParent>,
    /// The host offers "Read aloud" on text messages.
    pub read_aloud: bool,
}
pub struct HostChatSetRoomFeaturesRequest { pub room_id: String, pub features: ChatRoomFeatures }

/// What the host draws under a message that has a thread.
pub struct ChatThreadSummary {
    /// The thread room; its `thread_of` names this message.
    pub room_id: String,
    pub reply_count: u32,
    /// Milliseconds since the Unix epoch.
    pub last_reply_at: Option<u64>,
    /// Up to three repliers, drawn as small avatars.
    pub participant_ids: Vec<String>,
}

/// Which of several alternatives of a message is shown.
pub struct ChatBranchPosition {
    /// From 1.
    pub index: u16,
    pub count: u16,
}

/// The message is the product's reasoning, drawn as a collapsible row.
pub struct ChatReasoning {
    /// How long the product thought; shown once the status leaves `Preparing`.
    pub duration_ms: Option<u32>,
}

pub struct ChatMessageMeta {
    pub thread: Option<ChatThreadSummary>,
    pub branch: Option<ChatBranchPosition>,
    pub reasoning: Option<ChatReasoning>,
}
pub struct HostChatSetMessageMetaRequest { pub room_id: String, pub message_id: String, pub meta: ChatMessageMeta }

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
    /// The user picked another alternative of a message.
    BranchSelected { message_id: String, index: u16 },
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
```

As in the base, the variant order of every enum is part of the wire: a later addition goes after the last variant.

### Capabilities

`capabilities` answers stable feature names and two byte bounds. A host lists a name only when it serves the feature
end to end on that build.

| Name                                                          | Feature                                                                                                   |
| ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `product_source_rooms`                                        | Product-source rooms with paging, optimistic sends, `messages_query` and `user_action`                    |
| `reaction_details`, `participants`, `message_menus`, `participant_menus` | The four lookup queries of the base                                                            |
| `toolbar`, `suggestions`, `commands`, `activity`              | The base's `set_room_*` declarations of the same names                                                    |
| `custom_bodies`, `custom_footer`                              | `Custom` bodies and the `Custom` footer drawn through `Renderer::render`                                  |
| `inline_attachments`                                          | The user's photos, videos and files sent inline, up to `inline_attachment_max_bytes`                      |
| `payment_button`                                              | `ChatInputPayment` drawn and wired                                                                        |
| `room_links`                                                  | `polkadot://<product>.<tld>/-/chat/<room>` opens the room                                                 |
| `preimage_attachments`                                        | User attachments above the inline bound stored as preimages ([Preimage attachments](#preimage-attachments)) |
| `search`                                                      | In-room search, and `search_query` for product-source rooms ([Search](#search))                           |
| `bot_personas`                                                | Personas drawn for `bot:<bot_id>` participants ([Bots](#bots))                                            |
| `mute_archive`                                                | The user's mute and archive of product rooms ([Mute and archive](#mute-and-archive))                      |
| `threads`, `branches`, `reasoning_rows`, `read_aloud`         | The meta and features of [Threads, branches, reasoning rows and read aloud](#threads-branches-reasoning-rows-and-read-aloud) |
| `wake`                                                        | Notification wakes and `Woken` events ([Wakes](#wakes))                                                   |
| `call_buttons`                                                | Header buttons, `Ringing`, the call bar and the four call events ([Calls](#calls))                        |
| `room_calls`                                                  | `Active { media_session: Some(_) }` presented by the host ([Calls](#calls))                               |

The core answers the host's list minus what it cannot back: `custom_bodies` and `custom_footer` without a `Renderer`
host, `room_calls` while the runtime serves no Media service, and `payment_button` until the footer payment path
exists. It drops names it does not know. The two bounds are core constants: 1 MiB inline, and 8 MiB for a preimage, or
0 when `preimage_attachments` is not listed. The Bulletin chain's own per-transaction bound can be lower; staging
reports it.

A product that wants a feature reads `capabilities` first and falls back when the name is missing. On a host that
serves only the base, `capabilities` answers `unavailable`, and the product treats every added feature as missing.

### Room features, meta and calls

- **`set_room_features`** needs a room the product hosts, as every `set_room_*` request does. The host stores the
  declaration whole, keyed by product and room, with a version byte, and keeps it across restarts. It stores a feature
  it does not serve and ignores it, so a later host build applies it. A `thread_of` names a room of the same product
  that is not the room itself and has no `thread_of` of its own: threads are one level deep.
- **`set_message_meta`** needs a room the product hosts and passes the bounds of [Limits](#limits). A thread summary
  names a room of this product whose `thread_of` names this room and this message. It shares the `update_feed` rate
  bound of the room. The host keeps the meta in both room modes: a host-store message's meta goes with the message,
  and a product-source room keeps the meta of its newest 2,000 messages by timestamp, so a page loaded later draws its
  chips and rows. Meta for a product-source message the screen does not hold is stored and applied when the page
  arrives. In a host-store room, a message the host does not hold answers `UnknownMessage`.
- **`set_room_call`** needs a room the product hosts, at most two calls per second per room. `Ringing` and
  `Active { media_session: None }` need `call_buttons`. `Active { media_session: Some(_) }` needs `room_calls` and a
  live Media session that belongs to this product. Otherwise the core answers `Unknown` with a reason that names the
  feature. `None` clears the call.

### Room events

`room_events_subscribe` is a second product stream beside `action_subscribe`, so the base's `ChatEvent` keeps its
version. Its items are what the user does with the added features: `UserStateChanged`, `Woken`, the four call events
and `BranchSelected`.

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
  `bot:<bot_id>` in `update_feed` `New` and in `messages_query` pages, and names it in `set_room_activity`.
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
- Without the flag, the host keeps the 1 MiB inline rule.

### Search

- **In-room search, both room modes.** The host searches a host-store room in its own store. For a product-source room
  that declares `search`, the host asks `search_query { text, cursor, page_size }` once the user stops typing, with a
  page size of 30 and at most 50, and asks older pages with `next`. The core screens the answer as it screens a page of
  `messages_query`, and holds at most `page_size` hits. `NotAvailable` or a time-out shows that search is not
  available. A product-source room without `search` shows no search action.
- The core asks `search_query` only for a product-source room that declares `search`; it answers `NotAvailable` itself
  otherwise, without starting the worker.
- **List search.** A search over the chat list matches product-source rooms by name only and never asks a product, so
  it never starts a worker.
- On a host without `search` the product's handler is never called.

### Threads, branches, reasoning rows and read aloud

- **Threads.** A thread is a room. The product creates it, declares `thread_of: Some({ room_id, message_id })`, and
  sets the summary of the root message with `set_message_meta`, keeping it current as replies arrive. Every room feature
  works in a thread with no new mechanism. The host hides rooms with `thread_of` from the chat list, draws the summary
  under the root as a chip with the reply count, up to three avatars and the last reply time, and opens the thread room
  on a tap with the root quoted at the top. Starting a thread is a product action: a product menu entry, then
  `system.navigate_to` on the thread's room link. `delete_room` of a parent also deletes its thread rooms.
- **Branches.** `meta.branch = Some({ index, count })` draws a position control under the message. A tap on an arrow
  sends `BranchSelected { message_id, index }`. The product shows the alternative with `update_feed` `Edited`, and
  `Deleted` and `New` for the messages after it if its alternatives fork the conversation, and moves the position with
  `set_message_meta`. Without an answer in 5 s the control reverts.
- **Reasoning rows.** `meta.reasoning = Some({ duration_ms })` draws the message as a reasoning row, not a bubble. While
  its status is `Preparing`, the row streams the text the product edits in. Once the status leaves `Preparing`, it
  collapses to the time the product thought and expands on a tap. A reasoning row is not the list preview, does not
  count as unread, and its menu offers Copy only.
- **Read aloud.** With `features.read_aloud`, the message menu gains a built-in "Read aloud" on `Text`, `RichText` and
  `Custom` with `preview_text`, spoken by the platform's text to speech in the host's locale. Leaving the room stops
  it. It is off unless the room declares it, and it is not a hideable built-in action, because
  `ChatBuiltInMessageAction` is a base enum that rule 1 does not reopen.

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
  its feed, preview and meta, and send a notification with the room link as deeplink, which the host suppresses for a
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
- **Worker.** A room with a `Ringing` or `Active` call holds the worker until the call is cleared. If the worker stops,
  the host clears a ringing call; an active session lives on in the Media service and the bar follows its state.
- A product call rings only while the product's worker runs. Ringing a closed app needs notification wakes and the
  platforms' native call screens, which this RFC leaves out.
- `call_buttons` covers the buttons, ringing, the bar and the four events, so it ships before the Media service.
  `room_calls` covers host-presented sessions and needs the Media service in the runtime, with the matching amendment
  to the Realtime Media Sessions RFC: a session bound to a room is presented by the host, `set_surfaces` for it is
  refused, and the core checks that the session belongs to the product that binds it.

### Limits

The core enforces these bounds before a host sees a value, with the identifier and display-text screening of the base:

| Value                                                                                                                    | Bound                                                                                    |
| ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------- |
| `features` names in `capabilities`                                                                                       | Identifier, at most 64 entries                                                           |
| `call_id`, `watch_id`, thread `room_id`, branch `message_id`, `caller`, `participant_ids` entries, persona `bot:<bot_id>` | Identifier, 256 bytes                                                                    |
| `ChatThreadSummary.participant_ids`                                                                                      | At most 3, unique                                                                        |
| `ChatBranchPosition`                                                                                                     | 1 ≤ `index` ≤ `count` ≤ 64                                                               |
| `thread_of`                                                                                                              | A room of the same product with no `thread_of`, not the room itself                      |
| Search `text`                                                                                                            | Display text, 1 grapheme to 256 bytes                                                    |
| Search `page_size` the host asks                                                                                         | 1 to 50, host default 30; hits at most `page_size`                                       |
| Search cursor                                                                                                            | 1 KiB                                                                                    |
| Staged attachment                                                                                                        | 8 MiB, and not above the Bulletin chain's per-transaction bound                          |
| Personas per product                                                                                                     | 8                                                                                        |
| `set_message_meta`                                                                                                       | Shares the `update_feed` bound of the room, 20 per second                                |
| `set_room_call`                                                                                                          | 2 per second per room                                                                    |
| Wake                                                                                                                     | 30 s per hold; at most 12 notification wakes per product per rolling hour                |
| Meta kept for a product-source room                                                                                      | Newest 2,000 messages by timestamp                                                       |
| Room events queued with no subscriber                                                                                    | 64 per product, oldest dropped                                                           |

A rate over its bound fails with `RateLimited`, and any other bound fails with `Unknown` naming the field.

How long the host waits:

| What                | Host waits                                                  | Then                                                 |
| ------------------- | ----------------------------------------------------------- | ---------------------------------------------------- |
| `search_query`      | 10 s                                                        | Shows that search is not available; the field stays  |
| Staging a preimage  | The `Preimage::submit` time-out, 360 s                      | The message is `Failed` with retry                   |
| Wake hold           | 30 s, extended only by the product's own `begin_operation` | Releases the reference                               |
| `Ringing`           | The product's `set_room_call(None)`, or 60 s                | The ringing clears                                   |

## Trade-offs

- **Methods, not versions.** Adding meta to `ChatMessage` as a `V2`, or new events to `ChatEvent` as a `V3`, gives
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
- **Search stays in the open room.** Asking every product while the user types in the list search would start every
  worker on a keystroke. Product-source rooms match the list search by name only.
- **Calls before media.** Splitting `call_buttons` from `room_calls` lets the buttons, ringing and call bar ship
  before the Media service is accepted. Waiting for the Media service, or letting a product place picture rectangles
  in a chat screen, were dropped. Native call screens for a closed app are left out.
- **A thread is a room.** A thread id on `ChatMessage` would version the message and need a thread query; a built-in
  "Reply in thread" would need a new query too. A child room reuses every room feature, at the cost of one level of
  threads and a cascade on delete.
- **Meta, not a body kind.** A `Reasoning` body kind would add a variant every host must draw and version the body.
  Meta on an ordinary message degrades to a plain message on a host that ignores it. Read aloud is off by default, so
  a product opts in.
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

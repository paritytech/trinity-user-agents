---
title: "Chat Extension v2 additions"
owner: "@valentin-parity"
status: draft
---

# RFC — Chat Extension v2 additions

## Summary

[Chat Extension v2](chat-extension-v2.md) lets a product's Worker own the data of its chat rooms while the host draws
them. This RFC adds what the base leaves out: a product can declare optional features per room, put a call into a
room, learn what the user does with these features, give a room a contact so that the host draws it as its own
person-to-person chat, answer the edit history of a message and a search of a room's messages, and take what the user
opened from an entry point of the host. `Chat` gains seven methods, ids `23` and `25` to `30`, and no existing request,
item or event of the base changes its version. The host also gains behaviour that needs no wire method: attachments
uploaded to the HOP pool of the person-to-person chat, and bounded worker wakes. This RFC names the base and does not
change it, so the base can be accepted on its own.

## Motivation

The base gives a product a complete chat room, but not everything a person chat needs:

- A product-source room changes only while something holds the worker. A notification for the room cannot refresh its
  preview, and a reply sent just before the user closes the room may never reach the product.
- A call needs buttons in the room header and a ringing state, and the base has no call state.
- The person chat product (`chat.<tld>`) keeps the conversations of the native person-to-person chat. Users expect the
  room to look and behave as the native chat of their app: the contact's avatar and name, chat requests that wait for an
  answer, blocking, call and payment bubbles, the edit history and search. The base draws a generic product room, and
  the host's search cannot find messages it does not store.
- A link, a QR code or a username that the user opens from the host means something to the chat product only, and the
  host has no way to give it to the product.

## Approach

The design has these parts:

- **Rules**: what every addition follows.
- **Trait**: the new methods.
- **Types**: the payloads of the new methods and the additions to the shared types.
- **Capabilities**: the feature names a host gives the core and what the core removes.
- **Room features and calls**: the rules of the new requests.
- **Room events**: the second event stream.
- **Contact rooms**: the contact of a room and how the host draws it.
- **Features**: HOP attachments, wakes, calls, opened links, the edit history and search.
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
4. **Every feature has a name the host lists.** A host lists a feature to the core only when it serves it end to end,
   and the core removes a name it cannot back. A product does not read the list: an unserved feature is stored and
   ignored, a live request for it is refused with `Unknown`, and an unserved query is never asked.
5. **The host does all network work.** Uploading a file, receiving a notification and carrying a call go through the
   core and the host's own services. Nothing here gives a product a connection to a node, a statement store or a
   chain, and no product holds an allowance or chain key.
6. **The core screens, hosts render.** The core enforces every bound of [Limits](#limits) before a host sees a value,
   as for the base.

### Trait

`Chat` keeps trait id `4` and every method of the base. The new methods take ids `23` and `25` to `30`, and every one
has a default body that answers `unavailable`.

```rust
    // ---- Product to host requests ------------------------------------------

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
    /// calls.
    #[wire(id = 26)]
    async fn room_events_subscribe(&self, cx: &CallContext, request: HostChatRoomEventsSubscribeRequest)
        -> Subscription<HostChatRoomEventsSubscribeItem, CallError<HostChatRoomEventsSubscribeError>>;

    /// Set the person a room is a conversation with.
    #[wire(id = 29)]
    async fn set_room_contact(&self, cx: &CallContext, request: HostChatSetRoomContactRequest)
        -> Result<HostChatSetRoomContactResponse, CallError<HostChatRoomError>>;

    // ---- Host to product queries -------------------------------------------

    /// The messages of a product-source room that match what the user typed,
    /// newest first. One item, then complete.
    #[wire(host_initiated, id = 27)]
    fn search_query(&self, cx: &CallContext, request: ProductChatSearchQueryRequest)
        -> Subscription<ProductChatSearchQueryItem, CallError<ProductChatQueryError>>;

    /// What the user opened from an entry point of the host. One item, then
    /// complete.
    #[wire(host_initiated, id = 28)]
    fn open_link(&self, cx: &CallContext, request: ProductChatOpenLinkRequest)
        -> Subscription<ProductChatOpenLinkItem, CallError<ProductChatQueryError>>;

    /// The texts one message of a product-source room had. One item, then
    /// complete.
    #[wire(host_initiated, id = 30)]
    fn edit_history(&self, cx: &CallContext, request: ProductChatEditHistoryQueryRequest)
        -> Subscription<ProductChatEditHistoryQueryItem, CallError<ProductChatQueryError>>;
```

Versions of each leg:

| Method                            | Leg      | Version            |
| --------------------------------- | -------- | ------------------ |
| `23`, `25`-`30`                   | All legs | `V1`, first shape  |
| Every method of the base          | All legs | Unchanged          |

The responses of methods `23`, `25` and `29` are bare `V1` unit responses, as for the base's `set_room_*` requests.
The request of `26` carries no payload, and its error is `V1` of `GenericError`.

### Types

The new types reuse `ChatMessage`, `Bytes32`, `HostChatRoomError` and `ProductChatQueryError` of the base.

```rust
/// Call buttons in a room header.
pub struct ChatRoomCalls { pub audio: bool, pub video: bool }

/// Optional features of one room. Every field at its default selects the
/// behaviour of the base.
pub struct ChatRoomFeatures {
    /// Call buttons in the header; `None` shows none.
    pub calls: Option<ChatRoomCalls>,
    /// A user photo, video or file goes to a HOP node of the person-to-person
    /// chat, and the product receives a `ChatResource::Hop`.
    pub hop_attachments: bool,
}
pub struct HostChatSetRoomFeaturesRequest { pub room_id: String, pub features: ChatRoomFeatures }

/// The call of a room.
pub enum ChatRoomCall {
    /// An incoming call the product received; the host rings.
    Ringing { call_id: String, video: bool, caller: Option<String> },
    /// A call in progress.
    Active { call_id: String, video: bool, started_at: u64 },
}
pub struct HostChatSetRoomCallRequest { pub room_id: String, pub call: Option<ChatRoomCall> }

/// What the user does with the added features of a room.
pub enum ChatRoomEvent {
    /// The user pressed a call button of the header. `call_id`: the id the host chose for the call.
    CallRequested { video: bool, call_id: Option<String> },
    /// The user answered the ringing call.
    CallAnswered { call_id: String, accept: bool },
    /// The user ended the call.
    CallEnded { call_id: String },
    /// A media signal of the host's call engine for the other side.
    CallSignal { call_id: String, signal: Vec<u8> },
}
// Request: `HostChatRoomEventsSubscribeRequest { V1 }`; error `{ V1 => GenericError }`.
pub struct HostChatRoomEventsSubscribeItem { pub room_id: String, pub event: ChatRoomEvent }

/// Where the conversation of a room with its contact stands.
pub enum ChatContactState { Invite, Outgoing, Incoming, Established, Blocked, PeerLeft }

/// The person a room is a conversation with.
pub struct ChatRoomContact {
    pub account: Bytes32,
    pub username: Option<String>,
    pub state: ChatContactState,
}
pub struct HostChatSetRoomContactRequest { pub room_id: String, pub contact: ChatRoomContact }

pub struct ProductChatOpenLinkRequest {
    /// A room of the product the host shows unless the product answers another.
    pub room_id: String,
    /// What the user opened. The host does not read it.
    pub payload: String,
}
pub struct ProductChatOpenLinkItem {
    /// The room the host shows next; `None` keeps the room of the request.
    pub room_id: Option<String>,
}

pub struct ProductChatEditHistoryQueryRequest { pub room_id: String, pub message_id: String }
/// One text a message had. The first text has the time of the message.
pub struct ChatMessageRevision { pub edited_at: u64, pub text: Option<String> }
pub struct ProductChatEditHistoryQueryItem {
    /// Oldest first.
    pub revisions: Vec<ChatMessageRevision>,
}

pub struct ProductChatSearchQueryRequest {
    pub room_id: String,
    /// What the user typed.
    pub text: String,
    /// A cursor the product returned; `None` asks for the newest matches.
    pub cursor: Option<String>,
    pub page_size: u32,
}
pub struct ProductChatSearchQueryItem {
    /// The matches, newest first.
    pub hits: Vec<ChatMessage>,
    /// Cursor for older matches; `None` ends the matches.
    pub next: Option<String>,
}
```

The shared types of the base carry the contact rooms too. These are part of the base's types, and a host that does not
list `contact_rooms` draws them with its generic strings:

```rust
pub struct ChatMessage {
    // ...the fields of the base...
    /// The message it replies to, as the host quotes it.
    pub reply_preview: Option<ChatReplyPreview>,
    /// The message is the welcome message of a chat request.
    pub request: bool,
}
pub struct ChatReplyPreview { pub origin: ChatMessageOrigin, pub body: ChatMessageBody }

pub struct ChatMediaMeta {
    // ...the fields of the base...
    /// Length of a video, in milliseconds.
    pub duration_ms: Option<u64>,
}

pub struct ChatReactionSummary {
    // ...the fields of the base...
    /// The contact placed it and the user has not seen that reaction yet.
    pub unseen: bool,
}

pub struct ChatReactionDetail {
    // ...the fields of the base...
    /// When the participant reacted, in milliseconds since the Unix epoch.
    pub timestamp: u64,
}

pub enum ChatMessageBody {
    Text { text: String },
    RichText { text: Option<String>, media: Vec<ChatMediaItem> },
    /// A line about the conversation, drawn as a row with no bubble.
    Event(ChatContactEvent),
    /// A call. The time of the message is the offer, its origin the caller.
    Call(ChatCallRecord),
    /// A payment. Its origin is the payer.
    Payment(ChatPaymentRecord),
}

pub enum ChatContactEvent { RequestAccepted, ContactAdded, LeftChat, Unavailable, Unsupported }
pub struct ChatCallClose { pub at: u64, pub by_user: bool }
pub struct ChatCallRecord { pub video: bool, pub answered_at: Option<u64>, pub closed: Option<ChatCallClose> }
pub struct ChatPaymentRecord { pub amount: u128, pub legacy: bool }

pub enum ChatFeedUpdate {
    // ...New, Edited, ReactionPlaced, ReactionRemoved, StatusChanged...
    /// The message changed for a reason that is not an edit, for example its
    /// call was answered or ended. The host draws it again in its place and
    /// does not mark it edited.
    Replaced(ChatMessage),
}

pub enum ChatUserAction {
    // ...NewMessage, EditMessage, ReactionPlaced, ReactionRemoved, Seen...
    AcceptRequest,
    DeclineRequest,
    Block,
    Unblock,
    Leave,
    /// The user paid the contact with coins the host planned, reserved and handed off.
    SendPayment { message_id: String, timestamp: u64, total_value: u128, coin_keys: Vec<Vec<u8>> },
}
```

As in the base, the variant order of every enum is part of the wire: a later addition goes after the last variant.

### Capabilities

A host gives the core stable feature names through the `chat_capabilities` callback of its chat platform. It lists a
name only when it serves the feature end to end on that build. Products do not read the list.

| Name                   | Feature                                                                                  |
| ---------------------- | ---------------------------------------------------------------------------------------- |
| `product_source_rooms` | Product-source rooms with paging, optimistic sends, `messages_query` and `user_action`   |
| `reaction_details`     | Who reacted, through `reactions_query`                                                   |
| `hop_attachments`      | User attachments uploaded to HOP ([HOP attachments](#hop-attachments))                   |
| `call_buttons`         | Header buttons, `Ringing` and the call events ([Calls](#calls))                          |
| `contact_rooms`        | Rooms with a contact drawn as the host's person-to-person chat ([Contact rooms](#contact-rooms)) |

The core keeps the names of the host's list that it knows and can back: `hop_attachments` only when it backs HOP
uploads, and `contact_rooms` only for the person chat product (the `chat` label on a dotNS network). It drops names it
does not know, and uses the rest to refuse `set_room_call` without `call_buttons` and `set_room_contact` without
`contact_rooms`.

### Room features and calls

- **`set_room_features`** needs a room the product hosts, as every `set_room_*` request does. The host stores the
  declaration whole, keyed by product and room, and keeps it across restarts. It stores a feature it does not serve and
  ignores it, so a later host build applies it.
- **`set_room_call`** needs a room the product hosts, at most two calls per second per room. `Ringing` and `Active`
  need `call_buttons`; otherwise the core answers `Unknown` with a reason that names the feature. `None` clears the
  call.

### Room events

`room_events_subscribe` is a second product stream beside `action_subscribe`, so the base's action item keeps its
version. Its items are the call events.

- When a subscription starts, the core sends the events queued while no one subscribed, then live events.
- The core queues at most 64 events per product while no subscription is open, and drops the oldest beyond that. The
  queue is per product, not per connection, so an event published while the worker is stopped reaches its next start.
- A second subscription on one connection replaces the first, as for `action_subscribe`.

### Contact rooms

A room with a contact is a conversation with one person. The person chat product keeps the messages
(`product_source`) and the host draws the room with its own person-to-person chat components and strings, so the room
looks and behaves as the native chat of that host.

- **`set_room_contact`** needs a room the product hosts and `contact_rooms`, at most five calls per second per room. It
  replaces the contact whole, and the host keeps it across restarts. The core screens `username` as display text and
  refuses an empty one.
- **The contact.** The host draws the avatar of `account` and the `username`, and the header, menu, footer and banners
  of the state. The participant id `"contact"` is the contact, beside `"user"` and `"product"`.
- **The states.**

  | State         | The host shows                                                                                             |
  | ------------- | ---------------------------------------------------------------------------------------------------------- |
  | `Invite`      | No room in the list. The input is the invite composer, and the first `NewMessage` is the request's welcome message |
  | `Outgoing`    | The room, with no input and a line that the request waits for the contact                                  |
  | `Incoming`    | The room only behind the requests entry of the list, with the accept banner (Decline, Accept)               |
  | `Established` | The room with the input and, when the room declares them, the call buttons                                 |
  | `Blocked`     | No room in the list, and the blocked banner with Unblock                                                    |
  | `PeerLeft`    | The room with the host's footer for a contact that left                                                    |

- **What the user does.** Accept, Decline, Block, Unblock and Leave reach the product as `user_action` with
  `AcceptRequest`, `DeclineRequest`, `Block`, `Unblock` and `Leave`. The host asks for the confirmations its native
  chat asks for before it sends one. The product answers `Acknowledged` and sets the new state with `set_room_contact`.
- **Bubbles.** `Event` bodies are rows with the host's own strings: the request was accepted, the contact was added,
  a participant left, messages could not be retrieved, or the message is of a kind this version of the product cannot
  show. The origin says whose event it is. A `Call` body is the call bubble; the host works out the state its native
  chat shows (calling, active, ended, cancelled, missed, declined) from `video`, `answered_at`, `closed`, the time of
  the message and its origin. A `Payment` body is the transfer bubble, the legacy one when `legacy` is set. A message
  with `request` set is the welcome message of a chat request.
- **The preview.** `ChatRoomPreview` carries what the chat list needs: `timestamp` orders the rooms, newest first;
  `badge` is the unread count; the host makes the preview line itself from `last_message`, or from `last_reaction`
  when the newest event of the room is a reaction or, with `removed`, the removal of one; `last_call` is the newest
  call message when it is not `last_message`, because a call message keeps the time of its offer while it is
  answered and closed later: native iOS shows the call as the last message once its newest event (the close, else the
  answer) is newer than `last_message`, and native Android keeps `last_message`, so each host applies its own rule;
  and `unseen_reactions` counts the contact's reactions the user has not seen, which a host shows as the reaction mark
  or adds to the unread count, as its native chat does.
- **The input.** The host draws the input of its native chat for the contact state, whatever footer the room sets: the
  invite composer, no input while a request waits, the input once the contact is established. It offers attachments
  when the room declares `hop_attachments`, and its payment button where its native chat shows one.
- **Sending.** A host may choose the id of a message the user sends, a UUID as its native chat makes them, and pass it
  as `NewMessage.message_id`, with the time the user wrote it as `NewMessage.timestamp`. The product stores and sends
  the message under that id and with that time, so the message keeps its id and time from the moment the user sent it,
  and the host can act on it before the answer arrives. The core screens the id as every message id.
- **Payments.** The host runs the coinage flow of its native chat. When the user pays, it plans and reserves the
  coins, hands them off, and sends `SendPayment` with the message id it chose (a UUID, the id its coinage records
  track the payment under), the time, the total value and the 64-byte key of each coin (1 to 1,024 keys). The product
  sends the payment message to the contact under that id and answers `MessageCreated` with a `Payment` body; the keys
  appear in no body, preview or reply quote, and an action repeated with the same id sends nothing new. When a payment
  arrives, the product hands its keys to the host with `PersonChat.paymentReceived` (room, message id, time, total
  value, keys), and the host claims the coins under the message id; a repeated call changes nothing. A host without
  coinage answers `Unsupported`, and the product keeps the keys. The protocol carries no claim state: each host draws
  the state of its own records over the `Payment` message with that id, with the labels of its native chat.
- **Replies, media and reactions.** A reply carries `reply_preview`, so the host draws the quote without loading the
  quoted message. `duration_ms` is the length of a video. A reaction summary with `unseen` set is a reaction of the
  contact the user has not seen, and each entry of the who-reacted list carries the time of its reaction.
- **Updates.** The product changes a call record with `ChatFeedUpdate::Replaced`, which redraws the message in its
  place without marking it edited.
- On a host without `contact_rooms` the core refuses `set_room_contact` with `Unknown`, and the room draws as a product
  room of the base.

### Room notifications

A notification of a product whose deeplink names a room of the same product (`/-/chat/<room_id>`) is a notification of
that room. The host shows it as it shows a notification of a person chat:

- The title is the name of the room, or the contact of a contact room. The text is the text of the notification, so a
  product sends the message alone, not the sender name before it. A notification that opens no room of the product
  that the host knows keeps the title of other product notifications.
- While the app is in the foreground, the host shows it as its in-app banner, except while that room is on screen or
  where the host also hides person chat notifications. In the background the operating system shows it.
- A tap opens the room.

So a product sends a notification for each event that the user must know about, also while its worker runs in the
foreground, and does not follow which room is on screen.

### HOP attachments

A room of the person chat product that declares `hop_attachments` sends a user photo, video or file the way the native
person-to-person chat sends it: the host gives the bytes to the core, which uploads them to a HOP node the host lists,
signed with the product's Bulletin allowance account, and the message carries a `ChatResource::Hop` with the identifier,
the ticket, the node and the size. The ticket opens the file, so a product sends the resource only through its own
encrypted channel. No wire method is added.

- **The declaration the core reads.** The upload call carries the product and the room, not the room's features: a host
  does not vouch for its own opt-in. The core reads the flag from the last `set_room_features` it accepted for the room
  in this runtime, which a `delete_room` forgets. When it has none, as after a restart, it reads the features the host
  keeps through the chat adapter of the product's connection. With neither, the room declares nothing and nothing is
  uploaded.
- **Bounds.** At most 128 MiB, the largest file the native chat sends.
- **Progress.** The core reports the bytes the node holds and the size of the file as the upload goes, for the host's
  progress ring: none before the first part, then after each part, and all of them once the file is in.
- **Opening.** A host that draws a `Hop` item claims the bytes through the core, only from a node it lists, keeps them,
  and then acks the entries so the node drops them. The core reports the bytes received and the size of the file as
  they arrive, for the host's progress ring.

### Wakes

A **wake** is a bounded hold: the host takes one worker reference for 30 s for a reason, and the product does what it
needs in that time.

| Source            | When                                                                                                       | Hold                                          | Event                                  |
| ----------------- | ---------------------------------------------------------------------------------------------------------- | --------------------------------------------- | -------------------------------------- |
| Message in flight | A user message, a user action or a room event is sent to the product while no room screen holds the worker | 30 s                                          | None: the product receives the message |
| Product start     | A chat product starts and owns no room yet                                                                 | 30 s, once                                    | None                                   |
| Notification      | A chat push for a room of the product reaches the app                                                      | 30 s; at most 12 per product per rolling hour | None                                   |

- A product has at most one wake reference; a new wake moves its deadline. The product extends past 30 s only with
  `Worker::begin_operation`, which the operating system may still end on a phone.
- During a wake the product may do anything a running worker does: read the statement store through the host, update
  its feed and preview, and send a notification for the room.
- There are no scheduled wakeups and no periodic background refresh.
- A host wakes a product with `wake_product(product_id, reason)`, where the reason is `ProductStart`,
  `MessageInFlight { room_id }` or `Notification { room_id }`. A notification wake over the hourly budget answers
  `BudgetSpent` and holds nothing.
- No source needs a feature name: wakes are how the host keeps the [Worker Lifecycle](worker-lifecycle.md) promise for
  a message in flight and a chat push.

### Calls

The chat side of a call is a room state the product sets and events the user produces; the audio and video run in the
host's call engine ([Call media](call-media.md)).

- **Buttons.** `features.calls { audio, video }` draws call buttons in the room header, in a contact room only while
  the contact is `Established`. A press sends `CallRequested { video, call_id }`, and the product starts the call its
  own way. When the host chose `call_id`, the product uses it as the id of the call and of the message that records
  it, so the host shows its outgoing call screen at once and shows it ringing once that message's status is `Read`.
- **Ringing.** `set_room_call(Some(Ringing { .. }))` rings with the host's incoming call screen. Accept or Decline sends
  `CallAnswered { call_id, accept }`. The ringing clears on `set_room_call(None)`: a call rings until the caller ends
  it or the user answers, as the native chat rings.
- **Active.** `Active { .. }` shows the host's call screen. End sends `CallEnded`.
- **Worker.** A room with a `Ringing` or `Active` call holds the worker until the call is cleared or the room is
  deleted. The core keeps this hold, so every host gets it.
- `call_buttons` covers the buttons, ringing and the events.

### Opened links

The host gives the person chat product what the user opens from an entry point of the host and only the product reads:
a contact link, a scanned code, a shared link or a username typed in the add-contact panel.

- The host asks `open_link { room_id, payload }`, where `room_id` is a room of the product it shows unless the product
  answers another, and holds the worker until the answer. The host does not read, show or store the payload.
- The product answers the room the host shows next, or `None` to keep the room of the request. A product without a
  handler answers `Unsupported`.
- The core asks for any room of the product and needs no declaration. It refuses an empty payload or one above the bound
  with `Unknown`, and refuses an answer that names an invalid room.
- The payload can come from anyone. A product treats it as untrusted input, and asks the user before it acts on it with
  an effect the user did not ask for, such as a connection.

### Edit history

The host's message menu offers the edit history of an edited message. For a product-source room it asks
`edit_history { room_id, message_id }`, and the product answers the texts the message had, oldest first, the first with
the time of the message. The core answers `NotAvailable` itself for a room that is not product-source, and holds an
answer to at most 100 texts.

### Search

The host's search covers the messages of a product-source room, which the host does not store. It asks
`search_query { room_id, text, cursor, page_size }`, and the product answers the matching messages newest first, with a
cursor for older matches. The host asks only for a room whose configuration sets `product_source`, and no feature name
is needed. The core answers `NotAvailable` itself for any other room, refuses an empty `text` or a `page_size` above
50, and holds an answer to `page_size` messages.

### Limits

The core enforces these bounds before a host sees a value, with the identifier and display-text screening of the base:

| Value                                   | Bound                                                                     |
| --------------------------------------- | ------------------------------------------------------------------------- |
| `call_id`, `caller`                     | Identifier, 256 bytes                                                     |
| `username` of a contact                 | Display text, 1 to 256 bytes                                              |
| HOP attachment                          | 128 MiB                                                                   |
| `set_room_call`                         | 2 per second per room                                                     |
| `set_room_contact`                      | 5 per second per room                                                     |
| Call `signal`                           | 1 byte to 64 KiB                                                          |
| Wake                                    | 30 s per hold; at most 12 notification wakes per product per rolling hour |
| Room events queued with no subscriber   | 64 per product, oldest dropped                                            |
| Opened link `payload`                   | 1 byte to 8 KiB of UTF-8                                                  |
| Edit history                            | 100 texts                                                                 |
| Search `page_size`                      | 1 to 50 messages                                                          |

A rate over its bound fails with `RateLimited`, and any other bound fails with `Unknown` naming the field.

How long the host waits:

| What           | Host waits                                                       | Then                                            |
| -------------- | ---------------------------------------------------------------- | ----------------------------------------------- |
| `edit_history` | 5 s                                                              | Shows that the history is not available         |
| `search_query` | 10 s                                                             | Shows no matches from the room                  |
| Wake hold      | 30 s, extended only by the product's own `begin_operation`       | Releases the reference                          |
| `Ringing`      | The product's `set_room_call(None)`                              | The ringing clears                              |
| `open_link`    | 90 s, without blocking the screen: the link can start the worker | Logs the failure; the user stays where they are |

## Trade-offs

- **Methods, not versions.** Adding fields to a request as a `V2`, or new events to the `action_subscribe` item, gives
  cleaner types, but the client always encodes the newest version and an older host cannot decode it. New methods keep
  every base call of a new product working on an older host.
- **One features value.** `ChatRoomFeatures` is a persisted declaration beside `ChatRoomConfig`, so new features need no
  change of the configuration that hosts store.
- **The host draws the person chat.** A contact room could be a product room that the product draws through the
  renderer. That would give the person chat a second look on every host. The contact and its state let each host reuse
  its own native person chat, so the chat looks the same whether the native code or the product keeps the messages.
- **Bounded wakes.** A worker kept alive while notifications are enabled, or a periodic background refresh, would
  break the Worker Lifecycle rule that a worker runs only while referenced. A 30 s hold with an hourly budget keeps a
  product from running in the background on its own.

## Open questions

- **Background reach on iOS.** A background push gives an iOS app about 30 s and the system throttles silent pushes.
  Whether a worker that is a headless web view boots and answers in that window is not measured, so a cold wake on iOS
  is best effort.

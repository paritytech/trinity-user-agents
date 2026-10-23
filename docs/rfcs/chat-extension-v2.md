---
title: "Chat Extension v2"
owner: "@valentin-parity"
status: draft
---

# RFC — Chat Extension v2

## Summary

A product's Worker owns the data of its chat rooms: which rooms exist, how they appear in the chat list, which messages
they hold and who reacted to them. The host owns the screen, draws all of it with native elements, and applies a
default for every part the product does not declare, so a product that only creates a room and posts into it gets a
complete chat. `Chat` keeps its six methods and gains seven: four requests by which the product changes its rooms and
their messages, and three host-initiated queries by which the host asks the product for pages, user actions and
reactions.

Tracking issue: [#487](https://github.com/paritytech/host-rust-core/issues/487).

## Motivation

`Chat` lets a product create a room, post into it, set its footer and read what the user does there. The host stores
every message, and a room has one fixed shape:

- A product whose messages live elsewhere, on chain, in a peer-to-peer protocol or on its own backend, must copy each
  one into the host store and keep two stores in step, and it cannot edit or delete what it copied.
- The room has a name and icon fixed at creation, and no way to show where it stands in the chat list.
- The host loads a room whole. A room cannot hold more history than the host is willing to store, and the product has no
  way to serve older pages on demand.

## Approach

The design has nine parts:

- **Model**: rooms, room modes, and who owns each concern.
- **Wire patterns**: requests, product subscriptions and host-initiated queries.
- **Trait**: the `Chat` methods and their versions.
- **Types**: the payloads of every method.
- **Room rules**: how the host applies declarations, feed updates, user actions and events.
- **Defaults**: what the host does for every aspect the product leaves undeclared.
- **Lifecycle**: what is persisted and what holds the worker.
- **Rendering**: how the host draws each body.
- **Limits**: the bounds the core enforces and the time the host waits for a query.

### Model

- A **room** is one conversation a product serves. The product names it with its own `room_id`, an identifier normalized
  and screened as every chat identifier is, and the host scopes it by product. No request names a product, so a product
  sees and changes only its own rooms.
- A **host-store room** is a room whose messages the host persists. It is the default.
- A **product-source room** is a room whose messages live in the product. The host keeps only a page cache for the
  screen and asks the product for pages.
- A **declaration** is anything the product tells the host about a room: metadata, configuration, preview, footer. The
  host persists each one until the product replaces it or deletes the room.
- A **feed update** is one change to a room's messages that the product pushes: a new message, an edit, a reaction
  placed or removed, a status change, or a message drawn again with new content.
- A **query** is a host-initiated subscription that carries one question. The product answers with one item and
  completes it.
- A **participant id** is a product-local string naming one participant of a room. The reserved id `"user"` is the
  host's user, the reserved id `"product"` is the product itself, and the reserved id `"contact"` is the contact of a
  room ([Chat Extension v2 additions](chat-extension-v2-additions.md#contact-rooms)).

Ownership of each concern:

| Concern                                         | Owner                        | How it crosses the wire                                           |
| ----------------------------------------------- | ---------------------------- | ----------------------------------------------------------------- |
| Room existence, name, description               | Product                      | `create_room`, `set_room_metadata`, `delete_room`                 |
| Configuration of the room                       | Product, with host defaults  | `set_room_config`                                                 |
| Place of the room in the chat list              | Product, with a host default | `set_room_preview`                                                |
| Messages of a host-store room                   | Host                         | `update_feed`, `post_message`; user actions on `action_subscribe` |
| Messages of a product-source room               | Product                      | `messages_query`, `update_feed`, `user_action`                    |
| Who reacted                                     | Product, with a host default | `reactions_query`                                                 |
| Footer                                          | Product, with a host default | `set_room_footer`                                                 |
| Layout, theme, fonts, accessibility, navigation | Host                         | Nothing                                                           |

### Wire patterns

Every chat call uses one of three patterns that already exist on the wire:

| Pattern                     | Direction                             | Frame legs                                    | Precedent              |
| --------------------------- | ------------------------------------- | --------------------------------------------- | ---------------------- |
| Request                     | Product asks, host answers once       | `Request`, `Response`                         | `Chat::create_room`    |
| Product subscription        | Product opens, host streams items     | `Start`, `Receive`, `Interrupt`, `Stop`       | `Chat::action_subscribe` |
| Host-initiated subscription | Host opens, product streams items     | `Start`, `Receive`, `Interrupt`, `Stop`       | `Renderer::render`     |

A query is a host-initiated subscription. The host takes the first item and stops the stream; a product that answers
nothing within the time of [Limits](#limits) is treated as unavailable. The host opens a query only when the room's
configuration declares the matching flag, and applies its default otherwise without asking the product. A query
interrupted with `NotAvailable` is not a failure: the host hides the feature for that item and applies its default.

Every payload is SCALE-encoded and travels in a versioned envelope, as every other method's does.

### Trait

`Chat` keeps trait id `4`, the `Worker` execution gate, and ids `0` to `4` and `6`; id `5` is spent. The new methods take
ids `8` to `11`, `16` to `18` and `31`.

```rust
/// Chat rooms a product serves, drawn by the host.
#[wire_trait(id = 4)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait Chat: Send + Sync {
    /// Create a room, or find it again. An existing room keeps its stored declarations.
    #[wire(id = 0)]
    async fn create_room(
        &self,
        _cx: &CallContext,
        _request: HostChatCreateRoomRequest,
    ) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
        Err(CallError::unavailable())
    }

    /// Register a chat bot.
    #[wire(id = 1)]
    async fn register_bot(
        &self,
        _cx: &CallContext,
        _request: HostChatRegisterBotRequest,
    ) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
        Err(CallError::unavailable())
    }

    /// The product's rooms, whole set on subscribe and on every change.
    #[wire(id = 2)]
    async fn list_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostChatListSubscribeRequest,
    ) -> Subscription<HostChatListSubscribeItem, CallError<HostChatListSubscribeError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Post one message into a host-store room. The host assigns its id.
    #[wire(id = 3)]
    async fn post_message(
        &self,
        _cx: &CallContext,
        _request: HostChatPostMessageRequest,
    ) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
        Err(CallError::unavailable())
    }

    /// What the user does in the product's rooms.
    #[wire(id = 4)]
    async fn action_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostChatActionSubscribeRequest,
    ) -> Subscription<HostChatActionSubscribeItem, CallError<HostChatActionSubscribeError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    // Id 5 is spent and must never be reassigned.

    /// Set what a room shows below its messages.
    #[wire(id = 6)]
    async fn set_room_footer(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomFooterRequest,
    ) -> Result<HostChatSetRoomFooterResponse, CallError<HostChatSetRoomFooterError>> {
        Err(CallError::unavailable())
    }

    /// Replace a room's name and description.
    #[wire(id = 8)]
    async fn set_room_metadata(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomMetadataRequest,
    ) -> Result<HostChatSetRoomMetadataResponse, CallError<HostChatRoomError>> {
        Err(CallError::unavailable())
    }

    /// Replace how a room appears in the chat list.
    #[wire(id = 9)]
    async fn set_room_preview(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomPreviewRequest,
    ) -> Result<HostChatSetRoomPreviewResponse, CallError<HostChatRoomError>> {
        Err(CallError::unavailable())
    }

    /// Delete a room and everything the host holds for it. Succeeds for a room that does not exist.
    #[wire(id = 10)]
    async fn delete_room(
        &self,
        _cx: &CallContext,
        _request: HostChatDeleteRoomRequest,
    ) -> Result<HostChatDeleteRoomResponse, CallError<HostChatRoomError>> {
        Err(CallError::unavailable())
    }

    /// Push one change to a room's messages.
    #[wire(id = 11)]
    async fn update_feed(
        &self,
        _cx: &CallContext,
        _request: HostChatUpdateFeedRequest,
    ) -> Result<HostChatUpdateFeedResponse, CallError<HostChatUpdateFeedError>> {
        Err(CallError::unavailable())
    }

    /// One page of messages of a product-source room. One item, then complete.
    #[wire(host_initiated, id = 16)]
    fn messages_query(
        &self,
        _cx: &CallContext,
        _request: ProductChatMessagesQueryRequest,
    ) -> Subscription<ProductChatMessagesQueryItem, CallError<ProductChatQueryError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    /// One thing the user did in a product-source room. One item, then complete.
    #[wire(host_initiated, id = 17)]
    fn user_action(
        &self,
        _cx: &CallContext,
        _request: ProductChatUserActionRequest,
    ) -> Subscription<ProductChatUserActionItem, CallError<ProductChatUserActionError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Who reacted to one message. One item, then complete.
    #[wire(host_initiated, id = 18)]
    fn reactions_query(
        &self,
        _cx: &CallContext,
        _request: ProductChatReactionsQueryRequest,
    ) -> Subscription<ProductChatReactionsQueryItem, CallError<ProductChatQueryError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Declare which parts of a room the product serves. Replaces the previous configuration.
    #[wire(id = 31)]
    async fn set_room_config(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomConfigRequest,
    ) -> Result<HostChatSetRoomConfigResponse, CallError<HostChatRoomError>> {
        Err(CallError::unavailable())
    }
}
```

The methods of the first Chat surface keep their shapes and versions. A host injects its own `@parity/truapi` client
into every product it runs, and that client encodes the newest version of each request, so a product built against the
first SDK calls these methods with the first shapes. `create_room` takes `{ room_id, name, icon }` at `V1`,
`set_room_footer` takes `ChatRoomFooter` with `TextInput` and `Empty` at `V1`, and `post_message` keeps its `V1` and
its `V2` with `alt`. `register_bot`, `list_subscribe` and `action_subscribe` keep their shapes too: an
`action_subscribe` item is `{ room_id, peer, payload: ChatActionPayload }` with `MessagePosted`, `ActionTriggered` and
`Command`.

A new room is a host-store room with the name and icon of `create_room` and the text input. The product declares the
rest with `set_room_config`, `set_room_metadata` and `set_room_preview`. Every new method has a single `V1` shape. The
responses of methods `8` to `11` and `31` carry no payload, so each envelope is the bare `V1` variant.

### Types

Resources never travel as a URL, so the host never fetches from an address the product chose. A media item names a file
in the HOP pool of the person-to-person chat, which the host claims through the core
([HOP attachments](chat-extension-v2-additions.md#hop-attachments)).

```rust
/// Bytes the host shows: a media item.
pub enum ChatResource {
    /// A file in the HOP pool. The ticket claims and opens it.
    Hop {
        /// The 32-byte identifier of the file's root entry.
        identifier: Vec<u8>,
        /// The 32-byte ticket.
        ticket: Vec<u8>,
        /// The `wss://` node that holds it.
        node: String,
        /// The size the sender states.
        size: u64,
    },
}

/// What the host draws for a media item before its bytes arrive.
pub struct ChatMediaMeta {
    /// Media type of the item.
    pub mime_type: String,
    /// Size in bytes, as the product states it.
    pub size_bytes: Option<u64>,
    /// Width in pixels.
    pub width: Option<u32>,
    /// Height in pixels.
    pub height: Option<u32>,
    /// A BlurHash the host draws until the bytes arrive.
    pub placeholder: Option<String>,
    /// Accessibility text.
    pub alt: Option<String>,
    /// Length of a video, in milliseconds.
    pub duration_ms: Option<u64>,
}

/// One media item of a message.
pub enum ChatMediaItem {
    /// An image, drawn in the message.
    Image {
        /// The bytes.
        resource: ChatResource,
        /// What the host draws first.
        meta: ChatMediaMeta,
    },
    /// A video, played in the host's player.
    Video {
        /// The bytes.
        resource: ChatResource,
        /// What the host draws first; `placeholder` is the poster.
        meta: ChatMediaMeta,
    },
    /// A file, opened through the host's share sheet.
    File {
        /// The bytes.
        resource: ChatResource,
        /// What the host draws first.
        meta: ChatMediaMeta,
        /// Name shown on the file and used when the user saves it.
        file_name: String,
    },
}
```

Rooms, their configuration and their place in the chat list:

```rust
/// What a room is called.
pub struct ChatRoomMetadata {
    /// Display name.
    pub name: String,
    /// One paragraph shown in the room's details.
    pub description: Option<String>,
}

/// Which parts of a room the product serves itself. A field left at its default selects the host default.
pub struct ChatRoomConfig {
    /// `true`: the product answers `messages_query`, `user_action` and `edit_history`, and the host persists no
    /// message.
    pub product_source: bool,
    /// `true`: the host offers "who reacted" and asks `reactions_query`.
    pub reaction_details: bool,
    /// `true`: the host reports the newest message the user saw.
    pub report_seen: bool,
}

/// The badge of a room in the chat list.
pub enum ChatRoomBadge {
    /// No badge.
    Hidden,
    /// A number; `0` shows no badge.
    UnreadCount {
        /// The number shown.
        count: u32,
    },
}

/// The newest event of a room when it is a reaction.
pub struct ChatPreviewReaction {
    /// The emoji.
    pub emoji: String,
    /// `true`: the user reacted. `false`: the contact did.
    pub by_user: bool,
    /// The message the reaction is on.
    pub message: Option<ChatMessage>,
    /// `true`: the newest event is the removal of this reaction.
    pub removed: bool,
}

/// How a room appears in the chat list. A field left at `None` keeps the host default.
pub struct ChatRoomPreview {
    /// Sort key, in milliseconds since the Unix epoch, newest first; `None` uses the newest message's time.
    pub timestamp: Option<u64>,
    /// Badge; `None` keeps the host's unread count.
    pub badge: Option<ChatRoomBadge>,
    /// The newest message the user sees; the host makes the preview line from it. `None` uses the newest message the
    /// host holds.
    pub last_message: Option<ChatMessage>,
    /// The newest event, when it is a reaction newer than `last_message`.
    pub last_reaction: Option<ChatPreviewReaction>,
    /// The number of reactions of the contact the user did not see yet.
    pub unseen_reactions: u32,
    /// The newest call message, a `Call` body with its record, when it is not already `last_message`. The host follows
    /// its native rule: iOS shows the call once its newest event (the close, else the answer) is newer than
    /// `last_message`; Android keeps `last_message`.
    pub last_call: Option<ChatMessage>,
}

/// Request to create a room, unchanged from the first Chat surface.
pub struct HostChatCreateRoomRequest {
    /// The product's identifier for the room.
    pub room_id: String,
    /// Room display name.
    pub name: String,
    /// URL or base64 image.
    pub icon: String,
}

/// Request to declare which parts of a room the product serves.
pub struct HostChatSetRoomConfigRequest {
    /// A room the product created.
    pub room_id: String,
    /// The configuration. It replaces the previous one.
    pub config: ChatRoomConfig,
}

/// Request to replace a room's metadata.
pub struct HostChatSetRoomMetadataRequest {
    /// A room the product created.
    pub room_id: String,
    /// The new metadata.
    pub metadata: ChatRoomMetadata,
}

/// Request to replace a room's chat-list preview.
pub struct HostChatSetRoomPreviewRequest {
    /// A room the product created.
    pub room_id: String,
    /// The new preview.
    pub preview: ChatRoomPreview,
}

/// Request to delete a room.
pub struct HostChatDeleteRoomRequest {
    /// A room the product created.
    pub room_id: String,
}

/// Failure of a request that changes a room.
pub enum HostChatRoomError {
    /// The product created no room with this id.
    UnknownRoom,
    /// The product takes part in the room as a bot, not as its host.
    NotRoomHost,
    /// The product sent this request for this room too often.
    RateLimited,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
```

`ChatRoom`, `ChatRoomParticipation`, `HostChatCreateRoomResponse` and `HostChatCreateRoomError` keep their shapes.

Messages and feed updates:

```rust
/// Who wrote a message.
pub struct ChatMessageOrigin {
    /// A participant id of the room: `"user"`, `"product"` or `"contact"`.
    pub participant_id: String,
}

/// Where a message is on its way to the room.
pub enum ChatMessageStatus {
    /// Visible to its sender, not yet sent, such as while media uploads.
    Preparing,
    /// Sent.
    Sent,
    /// Delivered to its recipients.
    Delivered,
    /// Read by its recipients.
    Read,
    /// Not sent.
    Failed,
}

/// What a message says.
pub enum ChatMessageBody {
    /// Plain text.
    Text {
        /// The text, drawn without Markdown.
        text: String,
    },
    /// Plain text with media under it.
    RichText {
        /// The caption, drawn without Markdown.
        text: Option<String>,
        /// Media drawn under the text.
        media: Vec<ChatMediaItem>,
    },
    /// A line about the conversation, drawn as a row with the host's strings.
    Event(ChatContactEvent),
    /// A call, drawn as the host's call bubble.
    Call(ChatCallRecord),
    /// A payment, drawn as the host's transfer bubble.
    Payment(ChatPaymentRecord),
}

/// The reactions of one emoji on one message.
pub struct ChatReactionSummary {
    /// The emoji.
    pub emoji: String,
    /// How many participants placed it.
    pub count: u32,
    /// `true`: the host's user is one of them.
    pub reacted_by_user: bool,
    /// `true`: the contact placed it and the user has not seen that reaction yet.
    pub unseen: bool,
}

/// The message a reply quotes, so that the host draws the quote without loading that message.
pub struct ChatReplyPreview {
    /// Who wrote the quoted message.
    pub origin: ChatMessageOrigin,
    /// What the quoted message says.
    pub body: ChatMessageBody,
}

/// One message of a room.
pub struct ChatMessage {
    /// Identifier, unique in the room.
    pub message_id: String,
    /// Who wrote it.
    pub origin: ChatMessageOrigin,
    /// When it was written, in milliseconds since the Unix epoch.
    pub timestamp: u64,
    /// What it says.
    pub body: ChatMessageBody,
    /// Where it is on its way.
    pub status: ChatMessageStatus,
    /// The message it replies to.
    pub reply_to: Option<String>,
    /// The message it replies to, as the host quotes it; `None` without `reply_to` or when the product no longer has
    /// that message.
    pub reply_preview: Option<ChatReplyPreview>,
    /// When it was last edited, in milliseconds since the Unix epoch.
    pub edited_at: Option<u64>,
    /// Its reactions, one entry per emoji.
    pub reactions: Vec<ChatReactionSummary>,
    /// `true`: the welcome message of a chat request.
    pub request: bool,
}

/// One change to a room's messages.
pub enum ChatFeedUpdate {
    /// A new message.
    New(ChatMessage),
    /// A message's body replaced.
    Edited {
        /// The message.
        message_id: String,
        /// The new body.
        body: ChatMessageBody,
        /// When, in milliseconds since the Unix epoch.
        edited_at: u64,
    },
    /// A participant placed a reaction.
    ReactionPlaced {
        /// The message.
        message_id: String,
        /// The participant.
        participant_id: String,
        /// The emoji.
        emoji: String,
    },
    /// A participant removed a reaction.
    ReactionRemoved {
        /// The message.
        message_id: String,
        /// The participant.
        participant_id: String,
        /// The emoji.
        emoji: String,
    },
    /// A message's status changed.
    StatusChanged {
        /// The message.
        message_id: String,
        /// The new status.
        status: ChatMessageStatus,
    },
    /// A message changed for a reason that is not an edit, such as an answered call; drawn again in its place and
    /// not marked edited.
    Replaced(ChatMessage),
}

/// Request to push one change to a room's messages.
pub struct HostChatUpdateFeedRequest {
    /// A room the product created.
    pub room_id: String,
    /// The change.
    pub update: ChatFeedUpdate,
}

/// Failure of a feed update.
pub enum HostChatUpdateFeedError {
    /// The product created no room with this id.
    UnknownRoom,
    /// The product takes part in the room as a bot, not as its host.
    NotRoomHost,
    /// The update names a message the room does not hold.
    UnknownMessage,
    /// `New` names a message the room already holds.
    DuplicateMessage,
    /// A text is over its bound.
    MessageTooLarge,
    /// In a host-store room: `New` from `"user"`, or `Edited` of a message the product did not write.
    NotOwner,
    /// The product pushed updates to this room too often.
    RateLimited,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
```

`ChatContactEvent`, `ChatCallRecord` and `ChatPaymentRecord` are the types of
[Contact rooms](chat-extension-v2-additions.md#contact-rooms). A product streams a long answer as `New` with
`status: Preparing`, then `Edited` as text arrives, then `StatusChanged` to `Sent`.

Paging and user actions, for product-source rooms:

```rust
/// Where a page starts.
pub enum ChatPageAnchor {
    /// Newer messages than the cursor; `None` starts at the oldest.
    After {
        /// Cursor of the previous page in this direction.
        cursor: Option<String>,
    },
    /// Older messages than the cursor; `None` starts at the newest.
    Before {
        /// Cursor of the previous page in this direction.
        cursor: Option<String>,
    },
}

/// The host asks for one page of messages.
pub struct ProductChatMessagesQueryRequest {
    /// The room.
    pub room_id: String,
    /// Where the page starts.
    pub anchor: ChatPageAnchor,
    /// How many messages the host wants at most.
    pub page_size: u32,
}

/// One page of messages.
pub struct ProductChatMessagesQueryItem {
    /// The messages, oldest first.
    pub messages: Vec<ChatMessage>,
    /// Cursor of the next page in the same direction; `None` ends that direction.
    pub next: Option<String>,
}

/// Failure of a query.
pub enum ProductChatQueryError {
    /// The product serves no room with this id.
    UnknownRoom,
    /// The product cannot answer this question, such as for a room too large to track; the host applies its default.
    NotAvailable,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Something the user did in a product-source room.
pub enum ChatUserAction {
    /// The user sent a message.
    NewMessage {
        /// What the user wrote.
        body: ChatMessageBody,
        /// The message the user replied to.
        reply_to: Option<String>,
        /// The id the host chose for the message; the product stores and sends the message under it. `None` lets
        /// the product choose.
        message_id: Option<String>,
        /// When the user wrote the message, in milliseconds since the Unix epoch; the product sends the message with
        /// this time. `None` lets the product use its own clock.
        timestamp: Option<u64>,
    },
    /// The user edited a message of their own.
    EditMessage {
        /// The message.
        message_id: String,
        /// The new body.
        body: ChatMessageBody,
    },
    /// The user placed a reaction.
    ReactionPlaced {
        /// The message.
        message_id: String,
        /// The emoji.
        emoji: String,
    },
    /// The user removed a reaction.
    ReactionRemoved {
        /// The message.
        message_id: String,
        /// The emoji.
        emoji: String,
    },
    /// The user saw this message, the newest one on screen.
    Seen {
        /// The message.
        message_id: String,
    },
    /// What the user did with the contact of the room: see
    /// [Contact rooms](chat-extension-v2-additions.md#contact-rooms).
    AcceptRequest,
    DeclineRequest,
    Block,
    Unblock,
    Leave,
    /// The user paid the contact: see [Contact rooms](chat-extension-v2-additions.md#contact-rooms).
    SendPayment {
        /// The id the host chose for the payment message.
        message_id: String,
        /// When the user paid, in milliseconds since the Unix epoch.
        timestamp: u64,
        /// The sum of the coins, in the smallest unit.
        total_value: u128,
        /// The 64-byte sr25519 secret key of each coin.
        coin_keys: Vec<Vec<u8>>,
    },
}

/// The host reports one user action.
pub struct ProductChatUserActionRequest {
    /// The room.
    pub room_id: String,
    /// The action.
    pub action: ChatUserAction,
}

/// The product's answer to a user action.
pub enum ProductChatUserActionItem {
    /// For `NewMessage` and `SendPayment`: the message as the product stored it, with its id (the one the host chose,
    /// when it chose one), timestamp and status.
    MessageCreated(ChatMessage),
    /// For every other action: the product applied it.
    Acknowledged,
}

/// Failure of a user action.
pub enum ProductChatUserActionError {
    /// The product serves no room with this id.
    UnknownRoom,
    /// The action names a message the room does not hold.
    UnknownMessage,
    /// The product refuses the action; the host shows the reason to the user.
    Rejected {
        /// Text shown to the user.
        reason: String,
    },
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
```

Reactions:

```rust
/// The host asks who reacted to one message.
pub struct ProductChatReactionsQueryRequest {
    /// The room.
    pub room_id: String,
    /// The message.
    pub message_id: String,
}

/// One reaction by one participant.
pub struct ChatReactionDetail {
    /// The participant.
    pub participant_id: String,
    /// The emoji.
    pub emoji: String,
    /// When it was placed, in milliseconds since the Unix epoch.
    pub timestamp: u64,
}

/// Every reaction on the message.
pub struct ProductChatReactionsQueryItem {
    /// The reactions.
    pub reactions: Vec<ChatReactionDetail>,
}
```

The footer, unchanged from the first Chat surface. The host draws its own input: in a room with a contact it draws the
input of the contact state, with attachments when the room declares `hop_attachments`
([Chat Extension v2 additions](chat-extension-v2-additions.md#contact-rooms)).

```rust
/// What a room shows below its messages.
pub enum ChatRoomFooter {
    /// The text field the user types messages into.
    TextInput,
    /// Nothing, for a room the product drives through actions alone.
    Empty,
}

/// Request to set what a room shows below its messages.
pub struct HostChatSetRoomFooterRequest {
    /// A room the product created.
    pub room_id: String,
    /// The footer.
    pub footer: ChatRoomFooter,
}
```

### Room rules

- Every request except `create_room` needs a room the product created as its host. The core checks it, and the limits
  and rate limits, before a host sees the call.
- `create_room` on a room that exists answers `Exists` and keeps the stored declarations. The product changes them with
  the `set_room_*` requests. A room the product declared no configuration for keeps the behaviour of a host-store room,
  as on a host of the first Chat surface; the product declares the configuration right after `create_room`.
- In a host-store room the host persists `update_feed` and `post_message`. A message from `post_message` has the origin
  `"product"`. The product cannot write as `"user"` or edit a message it did not write.
- Where a query or an event carries a message from `post_message`, its body converts to `ChatMessageBody`. A `Reaction`
  or `ReactionRemoved` body applies as a reaction of `"product"`. URL media, which `ChatMediaItem` cannot carry, the
  buttons of `Actions` and the payload of `Custom` are left out of the converted body.
- In a product-source room the host persists no message. It applies `update_feed` to the pages of an open room screen
  and to the chat-list preview. When no screen shows the room, it applies the update to the cached newest page of the
  room, if the host has one, so the next open shows the change before the newest page arrives. The core refuses
  `post_message` with `Unknown`.
- The app icon badge, where the host has one, counts the `UnreadCount` badge of each product room as it counts the
  unread messages of a stored chat. A room whose product sets its badge counts that badge, not its stored messages.
- In a product-source room the host sends every user action to `user_action`. It draws a sent message as `Preparing`
  at once and replaces it with the `MessageCreated` answer. On `Rejected` it removes the message and shows the reason,
  and on time-out it marks the message `Failed`. It reverts any other action that fails and shows the reason.
- `Seen` is reported only when `report_seen` is `true`, at most once per newest message on screen.
- In a host-store room a message the user sends reaches `action_subscribe` as `MessagePosted`. The user's edits,
  reactions and seen messages there reach no product.
- The core answers a query whose flag the configuration does not set with `NotAvailable`, and a `user_action` in a
  host-store room with `Unknown`, without starting the worker.

Events, by room mode:

| Event                        | Host-store room | Product-source room                |
| ---------------------------- | --------------- | ---------------------------------- |
| `MessagePosted`              | Sent            | Not sent; `user_action` carries it |
| `ActionTriggered`, `Command` | Sent            | Sent                               |

### Defaults

What the host does for an aspect the product does not declare:

| Aspect                     | Default                                                                                             | Declared through                                      |
| -------------------------- | --------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Message store              | Host-store                                                                                          | `product_source` of `set_room_config`                 |
| Chat-list preview          | A line the host makes from the newest message; sorted by its time                                  | `set_room_preview`                                    |
| Badge, host-store room     | Unread count after the newest message the user saw                                                  | `set_room_preview`                                    |
| Badge, product-source room | None                                                                                                | `set_room_preview`                                    |
| Message menu               | Copy, Reply, React, and Edit on the user's own text; the edit history of an edited message in a product-source room | Not declared                         |
| Who reacted                | Not offered                                                                                         | `reaction_details` of `set_room_config`               |
| Footer                     | `TextInput`                                                                                         | `set_room_footer`                                     |

### Lifecycle

The host persists every declaration, keyed by product and room, so the chat list draws the product's rooms without
starting its worker. A worker that starts re-declares nothing; what it declared last stands until it declares again. A
room opened before any declaration arrived uses the defaults and the metadata of `create_room`.

The host delivers a host-initiated `Start` to a worker only after the worker's bundle finished its top-level
evaluation, so a handler registered at start never misses a query. A query interrupted with `unavailable` is asked once
more after one second before the host applies its default.

Chat reference holders for the [Worker Lifecycle](worker-lifecycle.md):

| Holder                                           | Held while                                                  |
| ------------------------------------------------ | ----------------------------------------------------------- |
| Room screen                                      | The room is on screen                                       |
| Query                                            | From its `Start` to its first item, interrupt or time-out   |
| Chat list                                        | Never                                                       |

A product-source room's screen opens with the cached pages, if any, and `messages_query` with `Before { cursor: None }`.
It asks `Before` with the returned cursor as the user scrolls up and `After` when a jump to a message needs newer pages,
and it applies feed updates while open. The host keeps at most 500 messages of a room.

`delete_room` removes the room, its messages, its cache and its declarations, and closes its screen. Uninstalling the
product removes all its rooms.

### Rendering

The host draws everything in its own theme; a product sends no colours, fonts or layout.

- `Text` and the caption of `RichText` are plain, with their web links detected: `https` and `http` addresses as
  written, and a bare domain as `https`.
- Media draws at its stated size with the BlurHash until the bytes arrive. A file that cannot be claimed draws the
  placeholder with a retry.
- `Event`, `Call` and `Payment` draw as the host draws them in its person chat: an event as a row with no bubble, a call
  as the call bubble, a payment as the transfer bubble ([Contact rooms](chat-extension-v2-additions.md#contact-rooms)).
- The user's own messages carry a status mark per `ChatMessageStatus`. A reply quotes the first line or the media kind of
  its `reply_preview`. Reactions draw as chips that toggle the user's reaction. An edited message is marked "edited".

### Limits

The core enforces these bounds before a host sees a value:

| Value                                                                                           | Bound                                                    |
| ----------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `room_id`, `message_id`, `participant_id`, `action_id`, `message_type`                          | Identifier: normalized, screened, 256 bytes              |
| `name`, `alt`, `file_name`, `mime_type`, media `placeholder`, emoji, input hint                 | Display text: screened, 256 bytes                        |
| `description` of a room                                                                         | Screened with line breaks kept, 1 KiB                    |
| Message text                                                                                    | 16 KiB                                                   |
| `Custom` payload of `post_message`                                                              | 256 KiB                                                  |
| HOP file                                                                                        | 32-byte identifier and ticket, a `wss://` node, 128 MiB  |
| Media per message, buttons per `post_message` `Actions`                                         | 32                                                       |
| Reaction summaries per message                                                                  | 64                                                       |
| `page_size` the host asks                                                                       | 1 to 100 messages                                        |
| Cursor                                                                                          | 1 KiB                                                    |
| `update_feed`                                                                                   | 20 per second per room; the screen coalesces `Edited`    |
| `set_room_preview`                                                                              | 5 per second per room                                    |

No other request of the base has a rate bound. The ids of the buttons of an `Actions` content are unique in their list,
and so are the emoji of a message's reactions. The answers of a query pass the same bounds: a page of messages holds at
most the `page_size` asked.

In `update_feed` and `post_message`, a message text or `Custom` payload over its bound fails with `MessageTooLarge`. A
rate over its bound fails with `RateLimited`, and any other bound fails with `Unknown` naming the field.

How long the host waits for a query:

| Query                                                                      | Wait | Then                                                       |
| -------------------------------------------------------------------------- | ---- | ---------------------------------------------------------- |
| `messages_query`                                                           | 10 s | Keeps what it shows and offers a retry                     |
| `user_action` with `NewMessage` or `SendPayment`                           | 30 s | Marks the message `Failed`                                 |
| `user_action`, any other                                                   | 10 s | Reverts the change and says so                             |
| `reactions_query`                                                          | 5 s  | Applies its default for the item                           |

The wait starts at the first frame of a worker the query had to start. A worker that cannot start leaves the room with
its cached pages and an input that says the product is unavailable.

## Trade-offs

- A product-source room's preview and badge are as fresh as the product's last run, because the chat list never starts
  the worker.
- Every query costs a stream of four frames for one answer. A host-initiated request leg would cost two, but needs a new
  frame leg, codegen and dispatch in every client.
- Declarations are pushed and persisted rather than pulled, so a product changes its footer or badge by calling again,
  and a stopped product cannot be asked.
- Resources cannot be URLs. Media travels as a file in the HOP pool, which the host claims through the core.
- Room ids are the product's own strings, scoped by the host. A host-derived hash was dropped: every existing call
  addresses a room by its id, and the host knows the caller.
- Considered and dropped: a separate trait for rooms, a host-initiated request leg, product-declared timeouts, and a
  worker kept alive while the chat list is on screen.

## Open questions

- **Notification wake.** The Worker Lifecycle counts a message addressed to the product as a reference holder, but no
  host wakes a worker for one, so a product-source room changes only while something holds the worker.
- **Init barrier mechanics.** The worker runtime can hold host-initiated `Start` frames until the bundle evaluated, or
  the client can answer `unavailable` for a handler not yet registered and rely on the host's one retry.

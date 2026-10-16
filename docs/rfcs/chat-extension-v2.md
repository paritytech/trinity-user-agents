---
title: "Chat Extension v2"
owner: "@valentin-parity"
status: draft
---

# RFC — Chat Extension v2

## Summary

A product's Worker owns the data of its chat rooms: which rooms exist, how they appear in the chat list, which messages
they hold, who takes part, and what the user can do to a message or a participant. The host owns the screen, draws all
of it with native elements, and applies a default for every part the product does not declare, so a product that only
creates a room and posts into it gets a complete chat. `Chat` keeps its six methods and gains fifteen: nine requests by
which the product declares and changes its rooms, and six host-initiated queries by which the host asks the product for
pages, user actions, reactions, participants and menus.

Tracking issue: [#487](https://github.com/paritytech/host-rust-core/issues/487).

## Motivation

`Chat` lets a product create a room, post into it, set its footer and read what the user does there. The host stores
every message, and a room has one fixed shape:

- A product whose messages live elsewhere, on chain, in a peer-to-peer protocol or on its own backend, must copy each
  one into the host store and keep two stores in step, and it cannot edit or delete what it copied.
- The room has no product entries in the message menu, no toolbar, no placeholder, no quick replies, and a name and icon
  fixed at creation.
- The host loads a room whole. A room cannot hold more history than the host is willing to store, and the product has no
  way to serve older pages on demand.
- Every message comes from the user or the product. A room cannot name its participants, so a group room cannot show who
  wrote what.

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
- A **declaration** is anything the product tells the host about a room: metadata, configuration, preview, toolbar,
  footer, suggestions, commands. The host persists each one until the product replaces it or deletes the room.
- A **feed update** is one change to a room's messages that the product pushes: a new message, an edit, a deletion, a
  reaction placed or removed, a status change.
- A **query** is a host-initiated subscription that carries one question. The product answers with one item and
  completes it.
- A **participant id** is a product-local string naming one participant of a room. The reserved id `"user"` is the
  host's user and the reserved id `"product"` is the product itself; neither needs a query to resolve.

Ownership of each concern:

| Concern                                                            | Owner                        | How it crosses the wire                                         |
| ------------------------------------------------------------------ | ---------------------------- | --------------------------------------------------------------- |
| Room existence, name, avatar, description                          | Product                      | `create_room`, `set_room_metadata`, `delete_room`               |
| Place of the room in the chat list                                 | Product, with a host default | `set_room_preview`                                              |
| Messages of a host-store room                                      | Host                         | `update_feed`, `post_message`; user actions on `action_subscribe` |
| Messages of a product-source room                                  | Product                      | `messages_query`, `update_feed`, `user_action`                  |
| Who reacted, participants, message menus, participant menus        | Product, with host defaults  | Queries                                                         |
| Toolbar, footer, placeholder, suggestions, commands, activity line | Product, with host defaults  | Requests                                                        |
| `Custom` message bodies and a custom footer                        | Product                      | `Renderer::render`                                              |
| Layout, theme, fonts, accessibility, navigation                    | Host                         | Nothing                                                         |

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
ids `7` to `21`.

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

    /// Replace which parts of a room the product serves itself.
    #[wire(id = 7)]
    async fn set_room_config(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomConfigRequest,
    ) -> Result<HostChatSetRoomConfigResponse, CallError<HostChatRoomError>> {
        Err(CallError::unavailable())
    }

    /// Replace a room's name, avatar and description.
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

    /// Replace the buttons of a room's toolbar.
    #[wire(id = 12)]
    async fn set_room_toolbar(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomToolbarRequest,
    ) -> Result<HostChatSetRoomToolbarResponse, CallError<HostChatRoomError>> {
        Err(CallError::unavailable())
    }

    /// Replace the quick replies shown above a room's input.
    #[wire(id = 13)]
    async fn set_room_suggestions(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomSuggestionsRequest,
    ) -> Result<HostChatSetRoomSuggestionsResponse, CallError<HostChatRoomError>> {
        Err(CallError::unavailable())
    }

    /// Replace the slash commands a room's input completes.
    #[wire(id = 14)]
    async fn set_room_commands(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomCommandsRequest,
    ) -> Result<HostChatSetRoomCommandsResponse, CallError<HostChatRoomError>> {
        Err(CallError::unavailable())
    }

    /// Show or clear a room's activity line.
    #[wire(id = 15)]
    async fn set_room_activity(
        &self,
        _cx: &CallContext,
        _request: HostChatSetRoomActivityRequest,
    ) -> Result<HostChatSetRoomActivityResponse, CallError<HostChatRoomError>> {
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

    /// The participants of a room: a count, a page, or some by id. One item, then complete.
    #[wire(host_initiated, id = 19)]
    fn participants_query(
        &self,
        _cx: &CallContext,
        _request: ProductChatParticipantsQueryRequest,
    ) -> Subscription<ProductChatParticipantsQueryItem, CallError<ProductChatQueryError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    /// The menu for one message. One item, then complete.
    #[wire(host_initiated, id = 20)]
    fn message_actions_query(
        &self,
        _cx: &CallContext,
        _request: ProductChatMessageActionsQueryRequest,
    ) -> Subscription<ProductChatMessageActionsQueryItem, CallError<ProductChatQueryError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    /// The menu for one participant. One item, then complete.
    #[wire(host_initiated, id = 21)]
    fn participant_actions_query(
        &self,
        _cx: &CallContext,
        _request: ProductChatParticipantActionsQueryRequest,
    ) -> Subscription<ProductChatParticipantActionsQueryItem, CallError<ProductChatQueryError>> {
        Subscription::interrupted(CallError::unavailable())
    }
}
```

Three legs gain a `V2` shape. The host upgrades each `V1` caller and answers it in its own version:

| Leg                     | `V2` shape                                    | A `V1` caller                                                                                     |
| ----------------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `create_room` request   | `{ room_id, metadata, config, preview }`      | `{ room_id, name, icon }` becomes metadata with an inline `icon` as the avatar, and the default configuration; an `https` icon keeps the `V1` rules |
| `set_room_footer` request | `ChatRoomFooter` with `Input`, `Empty`, `Custom` | `TextInput` becomes `Input` with the default input, `Empty` stays `Empty`                       |
| `action_subscribe` item | `{ room_id, peer, event: ChatEvent }`         | receives `MessagePosted`, `ActionTriggered` and `Command` only                                    |

A `V1` subscriber receives a posted `Text` or `Custom` body as it is, any other body as `Text` with the body's text, and
nothing for a body without text. The response and error of `create_room` and of `set_room_footer`, and the request and
error of `action_subscribe`, gain a `V2` with the `V1` shape, so a caller's version reaches every leg of its call. Every
new method has a single `V1` shape. The responses of methods `7` to `15` carry no payload, so each envelope is the bare
`V1` variant. `register_bot`, `list_subscribe` and `post_message` keep their shapes.

### Types

Resources travel inline or as a preimage key, never as a URL, so the host never fetches from an address the product
chose.

```rust
/// Bytes the host shows: an avatar, an icon, a badge or a media item.
pub enum ChatResource {
    /// The bytes themselves.
    Inline {
        /// Media type of `bytes`, such as `image/png`.
        mime_type: String,
        /// The content.
        bytes: Vec<u8>,
    },
    /// A preimage the host fetches through `Preimage::lookup_subscribe`.
    Preimage {
        /// The preimage key, as `Preimage::submit` returns it.
        key: Vec<u8>,
    },
}

/// What the host draws for a media item before its bytes arrive.
pub struct ChatMediaMeta {
    /// Media type of the item.
    pub mime_type: String,
    /// Size in bytes, as the product states it. The host checks it against a preimage it fetched.
    pub size_bytes: Option<u64>,
    /// Width in pixels.
    pub width: Option<u32>,
    /// Height in pixels.
    pub height: Option<u32>,
    /// A BlurHash the host draws until the bytes arrive.
    pub placeholder: Option<String>,
    /// Accessibility text, and the chat-list preview of a message with media only.
    pub alt: Option<String>,
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
/// What a room is called and how it looks in its header.
pub struct ChatRoomMetadata {
    /// Display name.
    pub name: String,
    /// Avatar; `None` draws the product's icon.
    pub avatar: Option<ChatResource>,
    /// One paragraph shown in the room's details.
    pub description: Option<String>,
}

/// Which parts of a room the product serves itself. A field left at its default selects the host default.
pub struct ChatRoomConfig {
    /// `true`: the product answers `messages_query` and `user_action`, and the host persists no message.
    pub product_source: bool,
    /// `true`: the host offers "who reacted" and asks `reactions_query`.
    pub reaction_details: bool,
    /// `true`: the host asks `participants_query`.
    pub participants: bool,
    /// `true`: the host asks `message_actions_query` before it opens a message menu.
    pub message_actions: bool,
    /// `true`: the host asks `participant_actions_query` before it opens a participant menu.
    pub participant_actions: bool,
    /// Emoji the reaction picker offers; `None` offers all.
    pub allowed_reactions: Option<Vec<String>>,
    /// Text shown while the room has no messages; `None` shows the host's text.
    pub placeholder: Option<String>,
    /// `true`: the host reports the newest message the user saw.
    pub report_seen: bool,
}

/// How a room sorts in the chat list.
pub enum ChatRoomOrder {
    /// By the preview's timestamp, among the rooms that are not pinned.
    Timestamp,
    /// Above every room that is not pinned, by timestamp among the pinned.
    PinToTop,
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
    /// An image.
    Image {
        /// The image.
        resource: ChatResource,
    },
}

/// The line of text under a room's name in the chat list.
pub struct ChatRoomPreviewText {
    /// The text.
    pub text: String,
    /// `true`: drawn as an ongoing activity, such as "Alice is typing", with its animation.
    pub ongoing: bool,
}

/// How a room appears in the chat list. A field left at `None` keeps the host default.
pub struct ChatRoomPreview {
    /// Where the room sorts.
    pub order: ChatRoomOrder,
    /// Sort key, in milliseconds since the Unix epoch; `None` uses the newest message's time.
    pub timestamp: Option<u64>,
    /// Badge; `None` keeps the host's unread count.
    pub badge: Option<ChatRoomBadge>,
    /// Text; `None` shows the newest message.
    pub text: Option<ChatRoomPreviewText>,
}

/// Request to create a room.
pub struct HostChatCreateRoomRequest {
    /// The product's identifier for the room.
    pub room_id: String,
    /// Name, avatar and description.
    pub metadata: ChatRoomMetadata,
    /// Which parts the product serves.
    pub config: ChatRoomConfig,
    /// Place in the chat list; `None` keeps the host default.
    pub preview: Option<ChatRoomPreview>,
}

/// Request to replace a room's configuration whole.
pub struct HostChatSetRoomConfigRequest {
    /// A room the product created.
    pub room_id: String,
    /// The new configuration.
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
    /// A participant id of the room, or `"user"` or `"product"`.
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
    /// Not sent; the host offers retry and delete.
    Failed,
}

/// What a message says.
pub enum ChatMessageBody {
    /// Plain text.
    Text {
        /// The text, drawn without Markdown.
        text: String,
    },
    /// Markdown text with media under it.
    RichText {
        /// Markdown text.
        text: Option<String>,
        /// Media drawn under the text.
        media: Vec<ChatMediaItem>,
    },
    /// Buttons the host draws under an optional text.
    Actions(ChatActions),
    /// A body the product draws through `Renderer::render`.
    Custom {
        /// Product-defined discriminator, passed to the render request.
        message_type: String,
        /// Product-defined payload, passed to the render request.
        payload: Vec<u8>,
        /// One line the host shows wherever it does not draw the body, such as the chat list.
        preview_text: Option<String>,
    },
}

/// The reactions of one emoji on one message.
pub struct ChatReactionSummary {
    /// The emoji.
    pub emoji: String,
    /// How many participants placed it.
    pub count: u32,
    /// `true`: the host's user is one of them.
    pub reacted_by_user: bool,
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
    /// When it was last edited, in milliseconds since the Unix epoch.
    pub edited_at: Option<u64>,
    /// Its reactions, one entry per emoji.
    pub reactions: Vec<ChatReactionSummary>,
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
    /// A message removed.
    Deleted {
        /// The message.
        message_id: String,
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
    /// A text or payload is over its bound.
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

`ChatActions` is the type `post_message` already takes. A product streams a long answer as `New` with
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
    },
    /// The user edited a message of their own.
    EditMessage {
        /// The message.
        message_id: String,
        /// The new body.
        body: ChatMessageBody,
    },
    /// The user deleted a message of their own.
    DeleteMessage {
        /// The message.
        message_id: String,
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
    /// The user asked to send a `Failed` message again.
    Retry {
        /// The message.
        message_id: String,
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
    /// For `NewMessage`: the message as the product stored it, with its id, timestamp and status.
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

Participants and reactions:

```rust
/// Who a participant is.
pub enum ChatParticipant {
    /// A verified person of the People chain; the host draws the person.
    Person {
        /// The person's id on the People chain.
        person_id: u32,
    },
    /// A registered account that is not yet a person.
    LitePerson {
        /// The account.
        account: Bytes32,
    },
    /// A contact the user picked through the Contacts API; the host draws its name and avatar.
    Contact {
        /// The handle the Contacts API gave the product.
        handle: ContactHandle,
    },
    /// Another product; the host draws its manifest name and icon.
    Product {
        /// The product's identifier.
        product_id: String,
    },
    /// Anyone else; the product supplies the appearance.
    Custom {
        /// Display name.
        nickname: String,
        /// Avatar; `None` draws a generic one.
        avatar: Option<ChatResource>,
    },
}

/// One participant of a room.
pub struct ChatParticipantEntry {
    /// The participant id messages and reactions use.
    pub participant_id: String,
    /// Who the participant is.
    pub participant: ChatParticipant,
}

/// What the host asks about a room's participants.
pub enum ChatParticipantsQuery {
    /// How many there are.
    Count,
    /// One page of them.
    Page {
        /// Cursor of the previous page; `None` starts at the first.
        cursor: Option<String>,
        /// How many the host wants at most.
        page_size: u32,
    },
    /// The ones with these ids.
    ByIds {
        /// The participant ids.
        participant_ids: Vec<String>,
    },
}

/// The host asks about a room's participants.
pub struct ProductChatParticipantsQueryRequest {
    /// The room.
    pub room_id: String,
    /// The question.
    pub query: ChatParticipantsQuery,
}

/// The answer, of the kind the question asked for.
pub enum ProductChatParticipantsQueryItem {
    /// Answer to `Count`.
    Count {
        /// How many participants the room has.
        count: u32,
    },
    /// Answer to `Page`.
    Page {
        /// The participants.
        participants: Vec<ChatParticipantEntry>,
        /// Cursor of the next page; `None` ends the list.
        next: Option<String>,
    },
    /// Answer to `ByIds`; an id the product does not know is left out.
    Resolved {
        /// The participants.
        participants: Vec<ChatParticipantEntry>,
    },
}

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
}

/// Every reaction on the message.
pub struct ProductChatReactionsQueryItem {
    /// The reactions.
    pub reactions: Vec<ChatReactionDetail>,
}
```

Menus, toolbar, footer, suggestions, commands and the activity line:

```rust
/// What happens when the user picks a product action.
pub enum ChatActionHandler {
    /// The host opens the link; the worker is not involved.
    Deeplink {
        /// A link the host may open through `system.navigate_to`.
        url: String,
    },
    /// The host sends the matching `ChatEvent` on `action_subscribe`.
    Callback,
}

/// An entry the product adds to a menu.
pub struct ChatProductAction {
    /// Identifier sent back in the event.
    pub action_id: String,
    /// Label.
    pub label: String,
    /// Icon drawn beside the label.
    pub icon: Option<ChatResource>,
    /// `true`: drawn as destructive, last in its section.
    pub destructive: bool,
    /// What a pick does.
    pub handler: ChatActionHandler,
}

/// An entry the host implements itself in a message menu.
pub enum ChatBuiltInMessageAction {
    /// Copy the text.
    Copy,
    /// Reply to the message.
    Reply,
    /// Edit a message of the user's own.
    Edit,
    /// Delete a message of the user's own.
    Delete,
    /// Open the reaction picker.
    React,
    /// Send a `Failed` message again.
    Retry,
}

/// The menu of one message.
pub enum ChatMessageMenu {
    /// Only the product's entries.
    Custom {
        /// The entries.
        actions: Vec<ChatProductAction>,
    },
    /// The host's entries minus `hidden`, then the product's in a section of their own.
    ExtendBuiltIn {
        /// The product's entries.
        actions: Vec<ChatProductAction>,
        /// Host entries left out.
        hidden: Vec<ChatBuiltInMessageAction>,
    },
}

/// The menu of one participant.
pub enum ChatParticipantMenu {
    /// Only the product's entries.
    Custom {
        /// The entries.
        actions: Vec<ChatProductAction>,
    },
    /// The host's entry for the participant's kind, then the product's in a section of their own.
    ExtendBuiltIn {
        /// The product's entries.
        actions: Vec<ChatProductAction>,
    },
}

/// The host asks for the menu of one message.
pub struct ProductChatMessageActionsQueryRequest {
    /// The room.
    pub room_id: String,
    /// The message; a `Custom` body arrives with an empty `payload`.
    pub message: ChatMessage,
}

/// The menu of the message.
pub struct ProductChatMessageActionsQueryItem {
    /// The menu.
    pub menu: ChatMessageMenu,
}

/// The host asks for the menu of one participant.
pub struct ProductChatParticipantActionsQueryRequest {
    /// The room.
    pub room_id: String,
    /// The participant.
    pub entry: ChatParticipantEntry,
}

/// The menu of the participant.
pub struct ProductChatParticipantActionsQueryItem {
    /// The menu.
    pub menu: ChatParticipantMenu,
}

/// A button of a room's toolbar.
pub struct ChatToolbarAction {
    /// Identifier sent back in the event.
    pub action_id: String,
    /// Label, shown when the button sits in the overflow menu.
    pub label: String,
    /// Icon.
    pub icon: ChatResource,
    /// What a press does.
    pub handler: ChatActionHandler,
}

/// Request to replace a room's toolbar.
pub struct HostChatSetRoomToolbarRequest {
    /// A room the product created.
    pub room_id: String,
    /// The buttons, in order; empty removes the toolbar.
    pub actions: Vec<ChatToolbarAction>,
}

/// The host's message input.
pub struct ChatInputConfig {
    /// Hint text in the empty input; `None` shows the host's.
    pub placeholder: Option<String>,
    /// `true`: the user can attach photos.
    pub photo: bool,
    /// `true`: the user can attach videos.
    pub video: bool,
    /// `true`: the user can attach files.
    pub file: bool,
    /// A payment button; `None` shows none.
    pub payment: Option<ChatInputPayment>,
}

/// The payment button of the input.
pub enum ChatInputPayment {
    /// Payments go to one account.
    Enabled {
        /// The account that receives the payment.
        destination: Bytes32,
    },
}

/// What a room shows below its messages.
pub enum ChatRoomFooter {
    /// The host's message input.
    Input(ChatInputConfig),
    /// Nothing, for a room the product drives through actions alone.
    Empty,
    /// A body the product draws through `Renderer::render` with a `ChatFooter` context.
    Custom,
}

/// Request to set what a room shows below its messages.
pub struct HostChatSetRoomFooterRequest {
    /// A room the product created.
    pub room_id: String,
    /// The footer.
    pub footer: ChatRoomFooter,
}

/// A quick reply shown above the input.
pub struct ChatSuggestion {
    /// Identifier sent back in the event.
    pub suggestion_id: String,
    /// Text of the chip.
    pub text: String,
    /// `true`: a press sends `text` as the user's message. `false`: a press sends `SuggestionSelected` only.
    pub sends_text: bool,
}

/// Request to replace a room's quick replies.
pub struct HostChatSetRoomSuggestionsRequest {
    /// A room the product created.
    pub room_id: String,
    /// The chips, in order; empty removes them.
    pub suggestions: Vec<ChatSuggestion>,
}

/// A slash command the input completes.
pub struct ChatCommandSpec {
    /// Name, typed after `/`.
    pub name: String,
    /// One line shown in the catalogue.
    pub description: String,
    /// Hint for the arguments, such as `<amount>`.
    pub argument_hint: Option<String>,
}

/// Request to replace a room's commands.
pub struct HostChatSetRoomCommandsRequest {
    /// A room the product created.
    pub room_id: String,
    /// The commands; empty removes the catalogue.
    pub commands: Vec<ChatCommandSpec>,
}

/// The activity line of a room, such as "Bot is typing".
pub struct ChatActivity {
    /// The text.
    pub text: String,
    /// The participant the line is about; `None` is the product.
    pub participant_id: Option<String>,
}

/// Request to show or clear a room's activity line.
pub struct HostChatSetRoomActivityRequest {
    /// A room the product created.
    pub room_id: String,
    /// The line; `None` clears it.
    pub activity: Option<ChatActivity>,
}
```

`ChatRoomFooter::Custom` adds a context to the [Unified Renderer](unified-renderer.md)'s `RenderContext`, appended after
the existing variants. The render request of a custom footer carries an empty payload.

```rust
/// Where a product-drawn body lives.
pub enum RenderContext {
    /// A message in a chat room.
    ChatMessage {
        /// Room the message is in.
        room_id: String,
        /// The message's id.
        message_id: String,
        /// Product-defined discriminator, as in the `Custom` body.
        message_type: String,
    },
    /// A candidate answered to an input query.
    InputWidget {
        /// Candidate id, as the product answered it.
        candidate_id: String,
    },
    /// A card face in the host's Pocket collection.
    PocketCard {
        /// Card id, as declared in the product's worker manifest.
        card_id: String,
    },
    /// The custom footer of a chat room.
    ChatFooter {
        /// The room.
        room_id: String,
    },
}
```

Events, the `V2` item of `action_subscribe`:

```rust
/// Something the user did in one of the product's rooms.
pub enum ChatEvent {
    /// The user sent a message into a host-store room.
    MessagePosted {
        /// The id the host gave the message.
        message_id: String,
        /// What the user wrote.
        body: ChatMessageBody,
        /// The message the user replied to.
        reply_to: Option<String>,
    },
    /// The user pressed a button of an `Actions` message.
    ActionTriggered(ActionTrigger),
    /// The user sent a declared slash command.
    Command(ChatCommand),
    /// The user edited a message of their own in a host-store room.
    UserEdited {
        /// The message.
        message_id: String,
        /// The new body.
        body: ChatMessageBody,
    },
    /// The user deleted a message of their own in a host-store room.
    UserDeleted {
        /// The message.
        message_id: String,
    },
    /// The user placed or removed a reaction in a host-store room.
    UserReacted {
        /// The message.
        message_id: String,
        /// The emoji.
        emoji: String,
        /// `true`: placed. `false`: removed.
        placed: bool,
    },
    /// The user saw this message, the newest one on screen, in a host-store room.
    Seen {
        /// The message.
        message_id: String,
    },
    /// The user picked a `Callback` entry of a message menu.
    MessageActionTriggered {
        /// The message.
        message_id: String,
        /// The entry.
        action_id: String,
    },
    /// The user picked a `Callback` entry of a participant menu.
    ParticipantActionTriggered {
        /// The participant.
        participant_id: String,
        /// The entry.
        action_id: String,
    },
    /// The user pressed a `Callback` button of the toolbar.
    ToolbarActionTriggered {
        /// The button.
        action_id: String,
    },
    /// The user pressed a quick reply whose `sends_text` is `false`.
    SuggestionSelected {
        /// The chip.
        suggestion_id: String,
    },
    /// The room screen opened.
    RoomOpened,
    /// The room screen closed.
    RoomClosed,
}

/// One event in one room.
pub struct HostChatActionSubscribeItem {
    /// The room.
    pub room_id: String,
    /// Who acted; `"native"` for the host's user.
    pub peer: String,
    /// What happened.
    pub event: ChatEvent,
}
```

`ActionTrigger` and `ChatCommand` are the existing types.

### Room rules

- Every request except `create_room` needs a room the product created as its host. The core checks it, and the limits
  and rate limits, before a host sees the call.
- `create_room` on a room that exists answers `Exists` and keeps the stored declarations. The product changes them with
  the `set_room_*` requests.
- `set_room_config` replaces the configuration whole. A change of `product_source` discards the room's stored messages
  or cached pages.
- In a host-store room the host persists `update_feed` and `post_message`. A message from `post_message` has the origin
  `"product"`. The product cannot write as `"user"` or edit a message it did not write, and it can delete any message.
- Where a query or an event carries a message from `post_message`, its body converts to `ChatMessageBody`. A `Reaction`
  or `ReactionRemoved` body applies as a reaction of `"product"`, and URL media, which `ChatMediaItem` cannot carry,
  is left out of the converted body.
- In a product-source room the host persists no message. It applies `update_feed` to the pages of an open room screen
  and to the chat-list preview. When no screen shows the room, it applies the update to the cached newest page of the
  room, if the host has one, so the next open shows the change before the newest page arrives. The core refuses
  `post_message` with `Unknown`.
- The app icon badge, where the host has one, counts the `UnreadCount` badge of each product room as it counts the
  unread messages of a stored chat. A room whose product sets its badge counts that badge, not its stored messages, and a
  room that the user muted counts nothing.
- In a product-source room the host sends every user action to `user_action`. It draws a sent message as `Preparing`
  at once and replaces it with the `MessageCreated` answer. On `Rejected` it removes the message and shows the reason,
  and on time-out it marks the message `Failed`. It reverts any other action that fails and shows the reason.
- `Seen` is reported only when `report_seen` is `true`, at most once per newest message on screen. On `action_subscribe`
  a room's `Seen` events reach the product at most once per second: the core holds the newest one and sends it when the
  second ends.
- The core answers a query whose flag the configuration does not set with `NotAvailable`, and a `user_action` in a
  host-store room with `Unknown`, without starting the worker.
- `Retry` is offered on a `Failed` message in a product-source room. A `Failed` message in a host-store room offers
  delete only.
- A message whose first word is `/` and a declared command name is sent as `Command`, with the rest of the text as its
  payload, and posts no message. Any other text is a message.
- A quick reply with `sends_text` sends its text as the user's message. The chips stay until the product replaces them.
- `message_actions_query` carries the whole message, except that a `Custom` body arrives with an empty payload.
- A `Deeplink` handler and the room link go through the host's navigation policy. A room is addressed as
  `polkadot://<product_id>.<tld>/-/chat/<room_id>`, following the reserved `-` segment of
  [Pocket modality](pocket-modality.md); a host without the room opens the chat list.

Events, by room mode:

| Event                                                                                                                      | Host-store room          | Product-source room               |
| -------------------------------------------------------------------------------------------------------------------------- | ------------------------ | --------------------------------- |
| `MessagePosted`, `UserEdited`, `UserDeleted`, `UserReacted`                                                                | Sent                     | Not sent; `user_action` carries it |
| `Seen`                                                                                                                     | Sent with `report_seen`  | Not sent; `user_action` carries it |
| `ActionTriggered`, `Command`, `MessageActionTriggered`, `ParticipantActionTriggered`, `ToolbarActionTriggered`, `SuggestionSelected`, `RoomOpened`, `RoomClosed` | Sent | Sent                       |

### Defaults

What the host does for an aspect the product does not declare:

| Aspect                     | Default                                                                                             | Declared through                                      |
| -------------------------- | --------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Message store              | Host-store                                                                                          | `product_source`                                      |
| Chat-list preview          | Newest message: its text, `preview_text`, `alt`, or "Sent a message"; sorted by its time           | `set_room_preview`                                    |
| Badge, host-store room     | Unread count after the newest message the user saw                                                  | `set_room_preview`                                    |
| Badge, product-source room | None                                                                                                | `set_room_preview`                                    |
| Message menu               | Copy, Reply, Edit and Delete on the user's own messages, React with every emoji, Retry when `Failed` in a product-source room | `message_actions`, `allowed_reactions`               |
| Participants               | The user and the product, drawn with the product's manifest name and icon                           | `participants`                                        |
| Participant menu           | Person: open the profile. LitePerson: copy the address. Contact: open the contact. Product: open the product. Custom: none | `participant_actions`         |
| Who reacted                | Not offered                                                                                         | `reaction_details`                                    |
| Toolbar                    | None                                                                                                | `set_room_toolbar`                                    |
| Footer                     | `Input` with photo and video on, file and payment off, the host's hint text                         | `set_room_footer`                                     |
| Placeholder                | The host's text                                                                                     | `placeholder`                                         |
| Suggestions, commands, activity line | None                                                                                      | `set_room_suggestions`, `set_room_commands`, `set_room_activity` |

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
| `Renderer::render` stream of a body or a footer  | The body is on screen                                       |
| Chat list                                        | Never                                                       |

A product-source room's screen opens with the cached pages, if any, and `messages_query` with `Before { cursor: None }`.
It asks `Before` with the returned cursor as the user scrolls up and `After` when a jump to a message needs newer pages,
and it applies feed updates while open. The host keeps at most 500 messages of a room. `RoomOpened` and `RoomClosed`
frame each visit, and the activity line clears on `RoomClosed`.

`delete_room` removes the room, its messages, its cache and its declarations, and closes its screen. Uninstalling the
product removes all its rooms.

### Rendering

The host draws everything in its own theme; a product sends no colours, fonts or layout.

- `Text` is plain, with its `https` links detected. `RichText` text is CommonMark limited to paragraphs, bold, italic,
  inline and fenced code, lists, block quotes and `https` links; headings draw as bold paragraphs, and anything else
  draws as its text. A line ending inside a paragraph draws as a line break, as in a chat message. The host asks
  before it opens a link.
- Media draws at its stated size with the BlurHash until the bytes arrive. A preimage that cannot be fetched draws the
  placeholder with a retry.
- `Actions` draws its buttons in a column or a grid. The host disables none; the product removes them by editing the
  message.
- `Custom` bodies and the `Custom` footer draw through `Renderer::render`; a host without `Renderer` draws
  `preview_text`.
- The user's own messages carry a status mark per `ChatMessageStatus`. A reply quotes the first line or the media kind of
  the message it answers. Reactions draw as chips that toggle the user's reaction. An edited message is marked "edited".
- The toolbar shows two buttons and puts the rest in an overflow menu. Quick replies draw as chips above the input and
  hide while the user types. Typing `/` opens the command catalogue. The activity line sits above the input.

### Limits

The core enforces these bounds before a host sees a value:

| Value                                                                                           | Bound                                                    |
| ----------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `room_id`, `message_id`, `participant_id`, `action_id`, `suggestion_id`, `message_type`, `product_id`, command `name` | Identifier: normalized, screened, 256 bytes |
| `name`, `label`, `nickname`, `alt`, `file_name`, `mime_type`, `argument_hint`, media `placeholder`, emoji, input hint, preview text, suggestion and activity text | Display text: screened, 256 bytes |
| `description` of a room or a command                                                            | Screened with line breaks kept, 1 KiB                    |
| Message text, `preview_text`, room placeholder                                                  | 16 KiB                                                   |
| `Custom` payload                                                                                | 256 KiB                                                  |
| Inline avatar, icon or badge image                                                              | 64 KiB, raster image                                     |
| Inline media item                                                                               | 1 MiB                                                    |
| Preimage key                                                                                    | Exactly 32 bytes                                         |
| Media per message, buttons per `Actions` body                                                   | 32                                                       |
| Reaction summaries per message, emoji in `allowed_reactions`                                    | 64                                                       |
| Menu entries, toolbar buttons, quick replies, commands                                          | 16, 6, 8, 32                                             |
| `page_size` the host asks                                                                       | 1 to 100 messages, 1 to 200 participants                 |
| `ByIds`                                                                                         | 100 ids                                                  |
| Cursor, deeplink                                                                                | 1 KiB; a deeplink also passes `system.navigate_to`       |
| `update_feed`                                                                                   | 20 per second per room; the screen coalesces `Edited`    |
| `set_room_preview` and `set_room_activity` together                                             | 5 per second per room                                    |

No other request has a rate bound. The ids of the entries of a menu, of the toolbar buttons, of the quick replies and
of the commands are unique in their list, and so are the emoji of `allowed_reactions` and of a message's reactions. The
answers of a query pass the same bounds: a page of messages holds at most the `page_size` asked, and a page or list of
participants at most 200.

In `update_feed` and `post_message`, a message text or `Custom` payload over its bound fails with `MessageTooLarge`. A
rate over its bound fails with `RateLimited`, and any other bound fails with `Unknown` naming the field.

How long the host waits for a query:

| Query                                                                      | Wait | Then                                                       |
| -------------------------------------------------------------------------- | ---- | ---------------------------------------------------------- |
| `messages_query`                                                           | 10 s | Keeps what it shows and offers a retry                     |
| `user_action` with `NewMessage`                                            | 30 s | Marks the message `Failed`                                 |
| `user_action`, any other                                                   | 10 s | Reverts the change and says so                             |
| `reactions_query`, `participants_query`, `message_actions_query`, `participant_actions_query` | 5 s | Applies its default for the item              |

The wait starts at the first frame of a worker the query had to start. A worker that cannot start leaves the room with
its cached pages and an input that says the product is unavailable.

## Trade-offs

- A product-source room's preview and badge are as fresh as the product's last run, because the chat list never starts
  the worker.
- Every query costs a stream of four frames for one answer. A host-initiated request leg would cost two, but needs a new
  frame leg, codegen and dispatch in every client.
- Declarations are pushed and persisted rather than pulled, so a product changes its toolbar or badge by calling again,
  and a stopped product cannot be asked.
- Resources cannot be URLs. A product with media on its own server re-hosts it inline or as a preimage.
- `Actions` and `Custom` are two ways to put a button in a message. `Actions` stays because every shipped chat product
  uses it and it needs no renderer.
- Room ids are the product's own strings, scoped by the host. A host-derived hash was dropped: every existing call
  addresses a room by its id, and the host knows the caller.
- Considered and dropped: a separate trait for rooms, a host-initiated request leg, product-declared timeouts, and a
  worker kept alive while the chat list is on screen.

## Open questions

- **Payments in the footer.** `ChatInputPayment` needs a payment path the product can address, such as the receivables
  of [RFC 0017](0017-coinage-payment.md). Until one exists, hosts do not draw the button.
- **Who pays for preimage media.** An attachment over 1 MiB needs a `Preimage::submit` paid from an allowance: the
  product's, or a host purse. Until settled, user attachments go inline only.
- **Notification wake.** The Worker Lifecycle counts a message addressed to the product as a reference holder, but no
  host wakes a worker for one, so a product-source room changes only while something holds the worker.
- **Init barrier mechanics.** The worker runtime can hold host-initiated `Start` frames until the bundle evaluated, or
  the client can answer `unavailable` for a handler not yet registered and rely on the host's one retry.

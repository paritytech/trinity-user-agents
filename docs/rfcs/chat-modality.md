---
title: "Chat Extension v2 — Extendable Chat System for Host API"
type: rfc
status: draft
owner: "@valentin-parity"
pr:
---

# RFC 0002 — Chat Extension v2: Extendable Chat System for Host API

|                 |                                                            |
| --------------- | ---------------------------------------------------------- |
| **Start Date**  | 2026-03-12                                                 |
| **Description** | Extendable chat system controlled by Product scripts       |
| **Authors**     | Valentin Sergeev                                                        |

## Summary

This RFC replaces the Chat Extension v1 of the Host API with a new architecture where **Products control the chat experience** while the **Host provides the UI shell**.

In v1, the Host owned the message store and the Product could only post and receive messages. In v2, the Product becomes the **data source** for messages, participants, metadata, and actions. The Host renders the chat UI, handles pagination, and delegates all data operations back to the Product via registered handlers.

The key design principle is: **be generic on the API level, but define reasonable defaults**. If a Product registers no custom handlers, the Host falls back to built-in behavior (two participants, standard message actions, default input). If a Product wants full control, it can override any aspect of the chat experience — message fetching, participant lists, toolbar actions, input UI, and per-message action menus.

This enables Products to build arbitrarily complex chat experiences (group chats, on-chain messaging, custom protocols) while reusing the Host's native chat UI components.

## Motivation

Chat Extension v1 had several limitations:

1. **Host-owned message store**: All messages lived in the Host's database. Products could not bring their own message source (e.g. on-chain messages, P2P protocols, external backends). This made it impossible to build Products with custom messaging protocols.

2. **Fixed chat structure**: v1 assumed a simple room-and-messages model with no customization of actions, participants, or input UI. Products that needed richer interactions (reactions, media, payments, custom actions) had no path forward.

3. **No scalability path**: With the Host storing all messages, there was no way to support large chats (10k+ members) or large numbers of rooms without overwhelming the Host's local storage.

4. **Rigid participant model**: v1 had no concept of participant types. The new Polkadot identity model (Person, LitePerson, Contact) requires a richer participant abstraction.

v2 solves these problems by inverting control: the Product decides how messages are stored, fetched, and acted upon. The Host becomes a rendering engine with sensible defaults.

## Detailed Design

### 1. Initialization Model

Products are JS scripts that the Host executes from scratch on each app start. During script initialization, the Product:

1. Creates chat rooms via `chat_room_create`
2. Registers handlers via `handle_*` functions (message source, actions, participants, etc.)
3. Pushes updates via `on_*` functions (metadata changes, new messages, etc.)

**The Host MUST delay rendering decisions for a chat room until the Product script has finished initialization.** This ensures the Host knows which handlers were registered before it attempts to display any chat UI.

If a `handle_*` function is never called for a room, the Host uses its built-in default behavior for that aspect.

### 2. Room Lifecycle

#### 2.1 Types

```rust
RoomId = String
ProductId = DotNsIdentifier

enum ResourceUri {
  Inline(Base64String),
  Preimage(CID)
}

struct CreateChatRoomRequest {
    // Host computes the actual room ID to guarantee scoping.
    // e.g. blake2b("product" ++ productId ++ room_id_source)
    room_id_source: String,
    // Initial state for the room metadata
    initial_metadata: {
        avatar: ResourceUri?,
        name: String,
        description: String?,
        kind: RoomKind,
        parent: Option<RoomId>,
    },
    // Optional. Initial state for the room preview that will define appearance in the chat list. If not provided - default rendering is used
    initial_preview?: {
        order: ChatRoomPreviewOrder,
        timestamp: Timestamp,
        badge: ChatRoomPreviewBadge,
    },
}

struct CreateChatRoomResponse {
    status: ChatRoomRegistrationStatus,
    room_id: RoomId,
}

enum ChatRoomRegistrationStatus {
    New,
    Exists,
}

struct ChatRoom {
    id: RoomId,
}
```

#### 2.2 Methods

```rust
// Create a new chat room. Host deduplicates by room_id_source per product.
// If a room with the same room_id_source already exists, returns Exists status.
fn chat_room_create(
    request: CreateChatRoomRequest
) -> Result<CreateChatRoomResponse, ChatRoomRegistrationErr>;

// Subscribe to the list of rooms created by this Product.
fn chat_room_list_subscribe(
    callback: fn(Vec<ChatRoom>)
) -> Result<Subscriber, GenericErr>;

// Delete a room and all associated data.
fn chat_room_delete(
    room_id: RoomId
) -> Result<(), GenericErr>;
```

#### 2.3 Errors

```rust
enum ChatRoomRegistrationErr {
    PermissionDenied,
    Unknown(GenericErr),
}
```

#### 2.4 Links and Invites

A chat link is a `Deeplink` that the Host opens in the Product that owns the room.

```rust
enum ChatLink {
    Room(RoomId),
    Message(RoomId, MessageId),
    Call(RoomId, MessageId),
    Invite(Invite),
}

struct Invite {
    room_id: RoomId,
    // Proves the invite to the members that manage the room.
    token: Vec<u8>,
    expires_at: Option<Timestamp>,
    max_uses: Option<u32>,
}
```

- A user who opens an invite sends `JoinRequest`. When the room approves joins, a member that manages it answers with `AnswerJoin`. A request waits until such a member is online.
- A user sets an `Audience` (8.1) for who may add it to a room. An add by anyone else is a request (`RoomRequest::Incoming`) that the user answers.

### 3. Room Metadata & Preview

Products control how rooms appear in the chat list and inside the chat screen.

#### 3.1 Types

```rust
struct RoomMetadata {
    id: RoomId,
    avatar: ResourceUri?,
    name: String,
    description: String?,
    kind: RoomKind,
    // The Space, Category or Forum that holds the room, and its place there.
    parent: Option<RoomId>,
    position: u32,
    settings: RoomSettings,
    pinned: Vec<PinnedMessage>,
}

struct PinnedMessage {
    message_id: MessageId,
    by: ChatParticipantId,
    until: Option<Timestamp>,
}

enum RoomKind {
    // Its parent can be a Channel: then it is the inbox of one reader to
    // the admins of the Channel.
    Direct,
    Group,
    // A Group of posts with `only_admins_post`. The comments of a post are
    // a Thread on it.
    Channel,
    Space,
    Category,
    // Each child room is a topic.
    Forum,
    Voice,
    // The replies to one message of the parent room.
    Thread { root: MessageId },
    // The stories of one user: posts that expire. Its members are the
    // audience, and only the author lists who read a post. A reply goes to
    // the direct room with the author, with a `forward` that names the post.
    Stories,
}

// Settings that every member sees. A change shows as `SettingsChanged`.
struct RoomSettings {
    // Seconds between two messages of one member. 0: no wait.
    slow_mode: u32,
    // A text that a member can read after the join.
    welcome: Option<String>,
    // A join by invite waits for `AnswerJoin`.
    approve_joins: bool,
    // A sender sets `expires_at` this many milliseconds after the send.
    message_timer: Option<u64>,
    // A reader drops a message from a participant who is not an admin.
    only_admins_post: bool,
    // Asks every Host to hide save and forward. A reader holds the
    // content, so no Product shows this as a guarantee. Same for
    // `ChatMedia.view_once`.
    protected: bool,
}

enum ChatRoomPreviewOrder {
    Timestamp,
    // Pinned rooms are ordered above non-pinned.
    // Multiple pinned rooms are secondarily ordered by timestamp.
    PinToTop,
}

enum ChatRoomPreviewBadge {
    // Derived from message read status (default) or from
    // the Product's subscribeUnreadCount handler.
    UnreadCount,
    Image(ResourceUri),
}

struct ChatRoomPreviewText {
    preview_text: String,
    // true to show animation, e.g. "User is typing..."
    ongoing: bool,
}

struct ChatRoomPreview {
    room_id: RoomId,
    order: ChatRoomPreviewOrder,
    timestamp: Timestamp,
    badge: ChatRoomPreviewBadge,
    // The user's own state of the room, from the room actions (4.3).
    archived: bool,
    muted_until: Option<Timestamp>,
    marked_unread: bool,
    request: Option<RoomRequest>,
}

// A direct room that waits for an answer.
enum RoomRequest {
    Incoming { since: Timestamp },
    Outgoing { since: Timestamp },
    Declined,
}
```

#### 3.2 Methods

```rust
// Notify the Host of updated room metadata.
// Host shows cached value from the last call.
// If never called, Host uses initial_metadata from CreateChatRoomRequest.
fn on_chat_room_metadata_change(new_metadata: RoomMetadata);

// Notify the Host of updated room preview.
// Same caching behavior as metadata.
fn on_chat_room_preview_change(preview: ChatRoomPreview);
```

### 4. Messages

This is the core of v2. The Product can either rely on the Host's built-in message store, or take over as the message data source.

#### 4.1 Types

```rust
// Unique in the room.
MessageId = String
// The id "room" is the room itself, for a post of a Channel with no author.
ChatParticipantId = String
Timestamp = u64

struct MessageOrigin {
    participant_id: ChatParticipantId,
}

CustomMessageType = Vec<u8>
CustomMessagePayload = Vec<u8>

enum MessageContent {
    // Particular case of RichText, for optimization.
    Text(String),

    // General purpose text message with optional media.
    RichText {
        text: Option<String>,  // Markdown-enabled
        media: Vec<ChatMedia>,
        // The text goes above the media.
        text_above: bool,
        entities: Vec<TextEntity>,
        // Made by the sender. A Host does not fetch a link of a room that a
        // Product serves: the site would learn who reads the message.
        link_preview: Option<LinkPreview>,
    },

    // preview_text: Human-readable summary for chat list preview
    // (e.g. "Payment received", "Shared a location").
    // If None, Host displays "Sent a message".
    Custom {
        message_type: CustomMessageType,
        payload: CustomMessagePayload,
        preview_text: Option<String>,
    },

    // A deleted message that keeps its place, so that replies still point
    // at it. The Host draws "Message deleted", and names an admin who
    // deleted it.
    Deleted { by: ChatParticipantId },

    // A line about the room that the Host draws with its own strings. The
    // origin is the participant who acted: "Ana added Ben".
    RoomEvent(RoomEvent),

    // A call. The origin is the caller and the time is the time of the
    // offer. Only the `CallSignal`s of its participants change it.
    Call(Call),

    // Each participant answers with `Answer`.
    Poll(Poll),

    Location(Location),

    // A person that the receiver can open a chat with.
    ContactCard { participant_id: ChatParticipantId, name: String },

    // A long post. Its Markdown also has headings and tables.
    Article { title: String, blocks: Vec<ArticleBlock> },

    // Each participant answers with `Answer`: 0 going, 1 maybe, 2 not going.
    CalendarEvent(CalendarEvent),

    // A payment that the Host made. The origin is the payer. The Host of a
    // reader checks `reference` and draws the state that it reads
    // (pending, sent, claimed, failed), not one that the sender wrote.
    Payment(Payment),
}

struct Payment {
    asset: PaymentAsset,
    amount: u128,
    reference: PaymentReference,
}

enum PaymentAsset {
    Id(u32),
    Token { symbol: String, decimals: u8 },
}

enum PaymentReference {
    CoinCheque(Vec<u8>),
    Transfer { chain: String, block_hash: [u8; 32], extrinsic_hash: [u8; 32] },
}

struct CalendarEvent {
    name: String,
    description: Option<String>,
    starts_at: Timestamp,
    ends_at: Option<Timestamp>,
    location: Option<String>,
    allow_guests: bool,
    cancelled: bool,
}

enum ArticleBlock {
    Text(String),
    Media(ChatMedia),
}

struct Location {
    // Degrees times 10^7.
    latitude: i32,
    longitude: i32,
    title: Option<String>,
    // A live location, which its origin moves with edits until this time.
    live_until: Option<Timestamp>,
}

struct Poll {
    question: String,
    options: Vec<String>,
    multiple: bool,
    // No Product shows who answered. Between Products the record of an
    // answer still names its origin.
    anonymous: bool,
    // A quiz: the index of the correct option.
    correct: Option<u32>,
    closes_at: Option<Timestamp>,
}

struct Call {
    video: bool,
    // Only admins send media; the others listen.
    broadcast: bool,
    // Who is in the call now.
    participants: Vec<ChatParticipantId>,
    answered_at: Option<Timestamp>,
    ended_at: Option<Timestamp>,
    ended_by: Option<ChatParticipantId>,
}

enum RoomEvent {
    Created,
    MemberAdded(ChatParticipantId),
    MemberRemoved(ChatParticipantId),
    MemberJoined,
    MemberLeft,
    RequestAccepted,
    Renamed(String),
    AvatarChanged,
    DescriptionChanged,
    SettingsChanged,
    MessagePinned(MessageId),
    RoleChanged { participant_id: ChatParticipantId, role: Role },
    // Each Product writes it when it sees the change. A user may hide it
    // on a device.
    SecurityKeyChanged,
}

struct ChatMedia {
    // The Host draws the kind that the sender chose: a `File` with an image
    // type stays a file.
    kind: ChatMediaKind,
    // The same bytes from one or more sources, in the order to try.
    sources: Vec<ResourceUri>,
    // What the Host draws before the bytes arrive:
    file_name: Option<String>,
    mime_type: String,
    size: Option<u64>,
    // The size as shown, after any rotation.
    width: Option<u32>,
    height: Option<u32>,
    // A BlurHash of the image or of the first video frame.
    placeholder: Option<String>,
    duration_ms: Option<u64>,
    // Audio: the peaks that the Host draws.
    waveform: Option<Vec<u8>>,
    caption: Option<String>,
    // Hidden until the user taps it.
    spoiler: bool,
    // The receiver opens it once, then the Host drops it. See `Opened`.
    view_once: bool,
    // The status of this item while it differs from the message.
    status: Option<MessageStatus>,
}

enum ChatMediaKind {
    Image,
    Video,
    File,
    // Drawn with no bubble.
    Sticker,
    Audio,
    // A recorded voice note.
    Voice,
    // A recorded video note, drawn round.
    RoundVideo,
}

struct LinkPreview {
    url: String,
    title: Option<String>,
    description: Option<String>,
    site: Option<String>,
    // An `Inline` picture with its size and BlurHash.
    image: Option<ChatMedia>,
}

enum Emoji {
    // One extended grapheme cluster.
    Unicode(String),
    Custom(CustomEmoji),
}

struct CustomEmoji {
    id: String,
    name: String,
    image: ResourceUri,
}

// A range of the text, in UTF-8 bytes, with a meaning that the Host draws.
struct TextEntity {
    start: u32,
    end: u32,
    kind: TextEntityKind,
}

enum TextEntityKind {
    Mention(MentionTarget),
    CustomEmoji(CustomEmoji),
    Spoiler,
}

enum MentionTarget {
    Participant(ChatParticipantId),
    Role(Role),
    Everyone,
    Room(RoomId),
}

struct MessageReaction {
    origin: MessageOrigin,
    reaction: Emoji,
}

struct AggregatedReaction {
    count: u32,
    reacted_by_me: bool,
    // The contact reacted with this emoji and the user has not seen that
    // reaction yet.
    unseen: bool,
    // The user's own reaction has not left yet.
    pending: bool,
}

// In a room with more than one other participant, `Delivered` and `Read`
// mean every recipient. `Message.receipts` gives the counts.
enum MessageStatus {
    // Message is visible to sender but not yet ready for delivery.
    // e.g. media is still uploading, with its progress in percent.
    Preparing { progress: Option<u8> },
    ReadyForDelivery,
    Delivered,
    Read,
    // The Host offers a retry only when `retryable`.
    Failed { reason: Option<String>, retryable: bool },
}

struct Message {
    id: MessageId,
    origin: MessageOrigin,
    room_id: RoomId,
    timestamp: Timestamp,
    content: MessageContent,
    status: MessageStatus,
    reply_to: Option<MessageId>,
    silent: bool,
    expires_at: Option<Timestamp>,
    forward: Option<ForwardSource>,
    // Rows of buttons under the message (5.3).
    buttons: Vec<Vec<MessageButton>>,
    last_edited: Option<Timestamp>,
    edit_count: u32,
    // The status of the newest own edit while it differs from `status`.
    edit_status: Option<MessageStatus>,
    reactions: Map<Emoji, AggregatedReaction>,
    receipts: Option<ReceiptSummary>,
    // The thread whose root is this message.
    thread: Option<ThreadSummary>,
    answers: Option<AnswerSummary>,
    // The readers of a post of a Channel.
    views: Option<u32>,
}

struct AnswerSummary {
    // The number of participants for each option.
    counts: Vec<u32>,
    mine: Vec<u32>,
}

// A forward keeps the resources of the media, so a receiver fetches the
// same bytes. A Host marks a message that came through many forwards.
struct ForwardSource {
    // None: the forwarder hides the author.
    origin: Option<ChatParticipantId>,
    room_id: Option<RoomId>,
    room_name: Option<String>,
    message_id: Option<MessageId>,
    timestamp: Timestamp,
    // The forwards that brought the message here, this one included.
    count: u32,
}

struct ThreadSummary {
    room_id: RoomId,
    replies: u32,
}

struct ReceiptSummary {
    recipients: u32,
    delivered: u32,
    read: u32,
}
```

The text of `Text` and `RichText` is Markdown that every Host draws in the same way: CommonMark with the GFM strikethrough, limited to emphasis, strong text (`**x**` and `__x__`), strikethrough, inline code, links (`http`, `https` and `mailto`), fenced code blocks, quote blocks (`> `) and lists of two levels. Every line break is a line break. Any other construct, raw HTML and images show as typed. There is no underline. Mentions, custom emoji and spoilers are `entities`, not marks.

#### 4.2 Message Feed Updates

The Product notifies the Host when the message feed changes. This is the **primary method** for pushing message data to the Host.

```rust
enum MessageFeedUpdate {
    // With the page cursor of the message, so that a Host that does not
    // hold the newest page can place it.
    NewMessage(Message, Option<PaginationCursor>),
    MessageEdit(MessageId, MessageContent, Timestamp),
    ReactionPlaced(MessageId, MessageReaction),
    ReactionRemoved(MessageId, MessageReaction),
    // Removes the row. To keep a "Message deleted" row, the Product sends
    // `MessageEdit` with `MessageContent::Deleted`.
    MessageDeleted(MessageId),
}

// Notify the Host of a message feed update.
//
// Behavior depends on whether handle_chat_messages_source was called:
// - If Product registered a message source: Host updates in-memory screen
//   state only (no persistent writes).
// - If Product did NOT register a message source: Host persists the update
//   to its local message store.
fn on_message_feed_update(
    room_id: RoomId,
    update: MessageFeedUpdate
);
```

#### 4.3 Product as Message Source

When a Product registers as the message source, it takes full control over message fetching and unread tracking. The Host MAY cache pages for performance but MUST NOT attempt to store or request the entire history.

```rust
PaginationCursor = String

struct Page<T> {
    next: Option<PaginationCursor>,
    items: T,
}

type MessagesPage = Page<Vec<Message>>

enum PaginationAnchor {
    // Fetch the next page. If cursor is None, fetches from the beginning.
    NextPage(Option<PaginationCursor>),
    // Fetch the previous page. If cursor is None, fetches from the end.
    PreviousPage(Option<PaginationCursor>),
    // Fetch a page around a message, for a jump to a quoted message or to
    // the first unread one.
    Around(MessageId),
}

type UpdateUnreadCountFn = fn(u32);

// User-initiated action in the chat.
enum UserChatAction {
    NewMessage {
        content: MessageContent,
        reply_to: Option<MessageId>,
        // Notify the author of `reply_to`.
        reply_ping: bool,
        // No notification for the receivers.
        silent: bool,
        expires_at: Option<Timestamp>,
        forward: Option<ForwardSource>,
        buttons: Vec<Vec<MessageButton>>,
    },
    // Changes only the text of `Text` or `RichText` (and keeps the media),
    // the point of a live `Location`, or a `CalendarEvent` (also to cancel
    // it). A reader drops any other edit, and an edit by another origin.
    EditMessage {
        message_id: MessageId,
        new_content: MessageContent,
    },
    ReactionPlaced {
        message_id: MessageId,
        reaction: MessageReaction,
    },
    ReactionRemoved {
        message_id: MessageId,
        reaction: MessageReaction,
    },
    MessageDeleted {
        message_id: MessageId,
        // false: hide the message on the user's own devices only.
        for_everyone: bool,
    },
    // The user opened the view-once media of a message.
    Opened { message_id: MessageId },
    // A pin that every member sees, until a time or the unpin.
    PinMessage { message_id: MessageId, until: Option<Timestamp> },
    UnpinMessage { message_id: MessageId },
    // The user changed the metadata or the settings of the room.
    EditRoom(RoomMetadata),
    // Room actions. The Product keeps the result in the room preview, so
    // it follows the user to the user's other devices.
    PinRoom(bool),
    ArchiveRoom(bool),
    // None unmutes. Timestamp::MAX mutes until the user unmutes.
    MuteRoom(Option<Timestamp>),
    MarkUnread(bool),
    // Removes the history on the user's own devices.
    ClearRoom,
    LeaveRoom,
    AnswerRequest { accept: bool },
    // Blocks (true) or unblocks the other participant of a direct room.
    Block(bool),

    // A step of the call of a `Call` message. `payload` is for the call
    // transport, and the Host does not read it.
    CallSignal {
        message_id: MessageId,
        signal: CallSignalKind,
        payload: Vec<u8>,
    },

    // Links and invites (2.4). A reset is a revoke, then a create.
    CreateInvite { expires_at: Option<Timestamp>, max_uses: Option<u32> },
    RevokeInvite { token: Vec<u8> },
    JoinRequest(Invite),
    AnswerJoin { participant_id: ChatParticipantId, accept: bool },

    // Only an admin (8.1).
    SetRole { participant_id: ChatParticipantId, role: Role },

    // The indexes of the chosen options. It replaces an earlier answer of
    // the same participant.
    Answer { message_id: MessageId, choices: Vec<u32> },

    // Answered with `Toast` or `Acknowledged`.
    PressButton { message_id: MessageId, button_id: String },
}

enum CallSignalKind {
    Answer,
    Join,
    Leave,
    // Also a decline.
    End,
}

// Return type varies by action:
// - NewMessage → the finalized Message (with ID, timestamp, status assigned by Product)
// - EditMessage, ReactionPlaced, ReactionRemoved → void
enum UserChatActionResult {
    MessageCreated(Message),
    InviteCreated(Invite),
    // A short text that the Host shows for a moment.
    Toast(String),
    Acknowledged,
}

// Register the Product as the message data source for a specific room.
// Per-room: must be called separately for each room the Product wants to control.
fn handle_chat_messages_source(
    room_id: RoomId,

    // Product provides a function to fetch a page of messages.
    fetch_message_page: fn(
        anchor: Option<PaginationAnchor>,
        page_size: u32
    ) -> MessagesPage,

    // Product takes over unread count tracking.
    subscribe_unread_count: fn(
        update_count: UpdateUnreadCountFn
    ),

    // Host calls this when the user performs a chat action (send, edit, react).
    on_user_action: fn(UserChatAction) -> UserChatActionResult,

    // Host calls this when the user has seen a message, and with it every
    // earlier message. Product can update its unread counter accordingly.
    on_user_seen_message: fn(Message),
);
```

- `LeaveRoom`: the member that leaves sends it, and the member or Host that manages the room removes the member. In a direct room the user's devices clear the room, and the other participant sees `MemberLeft` and keeps it. A later message from either side opens the room again.
- Every reader removes a message at its `expires_at`, also one that arrives late. A scheduled send is a Product matter: the Product sends the message at its time.
- A decline and a block are not shown to the other participant. A reader applies them, and tells its user nothing: a decline closes the request on both sides, a block stops delivery (or the reader drops the records of the blocked participant in that room), and an unblock ends that.

#### 4.4 Message Details

When a user asks who reacted, who read a message, how it read before an edit, or who answered, the Host requests the full list from the Product. This is **not paginated**. For large chats where tracking individual participants is impractical, the Product returns `NotAvailable`.

```rust
enum MessageDetailsKind {
    Reactions,
    Receipts,
    Edits,
    Answers,
}

enum MessageDetails {
    Reactions(Vec<MessageReaction>),
    Receipts(Vec<MessageReceipt>),
    // The earlier versions, oldest first.
    Edits(Vec<MessageVersion>),
    Answers(Vec<ParticipantAnswer>),
}

struct ParticipantAnswer {
    participant_id: ChatParticipantId,
    choices: Vec<u32>,
}

struct MessageVersion {
    content: MessageContent,
    edited_at: Timestamp,
}

struct MessageReceipt {
    participant_id: ChatParticipantId,
    delivered_at: Option<Timestamp>,
    read_at: Option<Timestamp>,
}

enum MessageDetailsErr {
    // The Product cannot provide individual details
    // (e.g. chat is too large).
    NotAvailable,
    Unknown(GenericErr),
}

// Register a handler to load the details of a message.
// If not registered, the Host offers none of these views.
fn handle_message_details(
    room_id: RoomId,
    get_details: fn(MessageId, MessageDetailsKind) -> Result<MessageDetails, MessageDetailsErr>,
);
```

#### 4.5 Chat Placeholder

Shown when a chat room has no messages yet.

```rust
struct ChatPlaceholder {
    text: String,
}

fn handle_chat_placeholder(
    get_placeholder: fn() -> ChatPlaceholder
);
```

#### 4.6 Well-known Custom Types

Every Host draws the `Custom` types of this list. A Product may propose a type for it.

| `message_type` | Payload | Host behavior |
|---|---|---|
| `attention` | None | A "nudge" line, with a sound or an effect. At most one for each sender and room every 15 seconds. No sound when the room is muted or the user is `Busy`. |

### 5. Message Actions

Products can customize the actions available on each message (long-press menu). By default, the Host provides built-in actions (copy, reply, edit own messages, react).

#### 5.1 Types

```rust
Deeplink = String

enum MessageActions {
    // Product takes full control. Only product-defined actions are shown.
    Custom(Vec<ProductMessageAction>),

    // Host shows built-in actions plus additional product-defined actions
    // in a separate section.
    ExtendBuiltIn(Vec<ProductMessageAction>),
}

enum BuiltInMessageAction {
    Copy(String),
    Edit(EditableMessageContent),
    Reply,
    // If allowed_emoji_set is None, all emoji are allowed.
    Reaction(Option<Vec<Emoji>>),
}

enum EditableMessageContent {
    Text(String),
    RichText { text: Option<String>, media: Vec<ChatMedia> },
}

struct ProductMessageAction {
    label: String,
    handler: ProductMessageActionHandler,
}

enum ProductMessageActionHandler {
    Deeplink(Deeplink),
    Callback(fn(Message)),
}
```

#### 5.2 Methods

```rust
// Optional. Register a handler that determines available actions per message.
// If not called, Host uses default built-in actions.
fn handle_message_actions(
    get_message_actions: fn(Message) -> MessageActions
);
```

#### 5.3 Buttons and Commands

A message can carry buttons, for example from a bot. A press goes to the Product as `PressButton`. A room can offer commands, which the Host lists when the user types `/`.

```rust
struct MessageButton {
    id: String,
    label: String,
}

struct ChatCommand {
    name: String,
    description: String,
}

fn handle_chat_commands(
    room_id: RoomId,
    get_commands: fn() -> Vec<ChatCommand>,
);
```

### 6. Toolbar Actions

Products can add action buttons to the top-right area of the chat screen.

```rust
ToolbarActionId = String

struct ToolbarAction {
    id: ToolbarActionId,
    icon: ResourceUri,
    // Shown as label if Host needs to collapse actions into a dropdown.
    label: String,
    // Called when the user taps this action.
    on_triggered: fn(),
}

// Register a handler that provides toolbar actions for this room.
fn handle_toolbar_actions(
    get_toolbar_actions: fn() -> Vec<ToolbarAction>
);
```

### 7. Chat Footer / Input

Products can customize the chat input area at the bottom of the screen.

#### 7.1 Types

```rust
type AccountId = [u8; 32];

enum ChatFooter {
    // Standard message input with optional payment and attachment buttons.
    DefaultChatMessageInput(DefaultChatMessageInputConfig),
    // No input area.
    None,
    // Custom rendering using the existing SerializedCustomChatMessage system.
    Custom(fn(SerializedCustomChatMessage)),
}

struct DefaultChatMessageInputConfig {
    payment: ChatInputPayment,
    attachments: ChatInputAttachments,
}

enum ChatInputPayment {
    Disabled,
    // The Host makes the payment that the user approves, then sends the
    // Product a `NewMessage` with its `Payment`.
    Enabled {
        // The account that receives the payment.
        destination: AccountId,
    },
}

struct ChatInputAttachments {
    photo: bool,
    video: bool,
}
```

#### 7.2 Methods

```rust
// Register a custom chat footer configuration.
// If not called, Host uses DefaultChatMessageInput with payment disabled
// and default attachment settings.
fn handle_chat_footer(footer: ChatFooter);
```

### 8. Participants

Products can customize the participant list. The default is two participants: the user and the Product.

#### 8.1 Types

```rust
PersonId = u32

struct ChatParticipant {
    id: ChatParticipantId,
    kind: ChatParticipantKind,
    role: Role,
    // Shown over what the Host infers from `kind`.
    name: Option<String>,
    avatar: Option<ResourceUri>,
    about: Option<String>,
    birthday: Option<Date>,
    links: Vec<String>,
    // A name color and a profile color that every member sees.
    colors: Option<ParticipantColors>,
    presence: Option<Presence>,
    // A short code of the security key, which two users can compare.
    security_code: Option<String>,
}

enum Role {
    Member,
    Admin,
    Owner,
}

struct Presence {
    status: PresenceStatus,
    // When the status last changed: the "last seen" time of `Offline`.
    since: Timestamp,
    text: Option<String>,
}

enum PresenceStatus {
    Online,
    Busy,
    Away,
    // Also a user who appears offline.
    Offline,
}

struct ParticipantColors {
    name: u32,     // RGB
    profile: u32,  // RGB
}

struct Date {
    year: Option<u16>,
    month: u8,
    day: u8,
}

// Who may see a field of a participant.
enum Audience {
    Everyone,
    Contacts,
    Nobody,
}

enum ChatParticipantKind {
    // Verified person with a numeric ID from the People Registry.
    // Has access to Ring VRF proofs.
    Person(PersonId),

    // Registered app user identified by AccountId.
    // Not yet verified through the People Registry.
    LitePerson(AccountId),

    // Entry in the user's address book. Host infers
    // appearance (nickname, avatar) from the contact book.
    Contact(AccountId),

    // Another Product acting as a participant.
    Product(ProductId),

    // Arbitrary participant with explicit appearance.
    Custom {
        nickname: String,
        avatar: ResourceUri,
    },
}
```

Only an `Admin` or the `Owner` adds and removes members, answers joins, deletes the messages of others, posts when `only_admins_post` is set, changes the room metadata and settings, manages the rooms of a Space, mentions everyone, and sets the role of another participant (not of the Owner). A reader drops such an action from a `Member`. The role of a participant in a Space applies in each room of the Space.

#### 8.2 Methods

```rust
// Optional. Register handlers for participant data.
// Default: two participants — the user and the Product.
fn handle_chat_participants(
    get_participants_count: fn() -> u32,
    get_participants_page: fn(
        anchor: Option<PaginationAnchor>,
        page_size: u32
    ) -> Page<Vec<ChatParticipant>>,
    get_participants_by_ids: fn(
        ids: Vec<ChatParticipantId>
    ) -> Vec<ChatParticipant>,
);
```

### 9. Participant Actions

Products can customize actions available when tapping on a participant.

```rust
enum ParticipantActions {
    // Product takes full control.
    Custom(Vec<ProductParticipantAction>),
    // Host shows built-in actions plus product-defined actions.
    ExtendBuiltIn(Vec<ProductParticipantAction>),
}

enum BuiltInParticipantAction {
    OpenContact(AccountId),
    OpenProductInfo(ProductId),
}

struct ProductParticipantAction {
    label: String,
    handler: ProductParticipantActionHandler,
}

enum ProductParticipantActionHandler {
    Deeplink(Deeplink),
    Callback(fn(ChatParticipant)),
}

// Optional. Register a handler for participant actions.
// Default: Host shows built-in actions based on participant type.
fn handle_participant_actions(
    get_participant_actions: fn(ChatParticipant) -> ParticipantActions
);
```

### 10. Default Behaviors Summary

When a Product does **not** register a handler, the Host applies these defaults:

| Aspect | Default Behavior |
|---|---|
| Room preview | Ordered by timestamp, unread count badge, last message as preview text. Custom messages use `preview_text` if provided, otherwise "Sent a message" |
| Message source | Host stores and fetches messages from its local DB |
| Unread count | Derived from read status vs total message count |
| Message actions | Copy, Reply, Edit (own messages only), Reaction (all emoji) |
| Toolbar actions | None |
| Commands | None |
| Chat footer | Default input, payment disabled, default attachments |
| Participants | Two participants: user and Product |
| Participant actions | Based on participant type (open contact, open product info) |
| Message details | Not available: no view of who reacted, read or answered, or of earlier versions |
| Placeholder | None |

### 11. Transport & Serialization

This RFC follows the same transport and serialization rules as the Host API protocol:

- Messages are serialized using JAM codec.
- Request/response and subscription patterns follow the rules defined in the Host API design document (Section: Transport).
- All `handle_*` functions follow the subscription pattern: `_start`, `_stop`, `_interrupt`, `_receive`.
- All `on_*` functions follow the request pattern: `_request`, `_response`.
- `chat_room_create`, `chat_room_delete` follow the request pattern.
- `chat_room_list_subscribe` follows the subscription pattern.

### Requirements

#### Functional Requirements

1. **Room management**: Create, delete, and list chat rooms. Each room is scoped to a Product.
2. **Room metadata**: Name, avatar, description — updatable at any time by the Product.
3. **Room preview**: Configurable appearance in the chat list — ordering (timestamp, pin-to-top), badge (unread count or custom image), preview text with typing indicator support.
4. **Text messaging**: Plain text and rich text (Markdown) messages.
5. **Media attachments**: Image, file, and video attachments within messages.
6. **Custom message types**: Arbitrary binary message types for Product-specific content.
7. **Message editing**: Edit message content after sending.
8. **Message deletion**: Delete messages.
9. **Reply threads**: Reply to a specific message.
10. **Emoji reactions**: Place and remove emoji reactions on messages. Aggregated counts displayed inline; detailed reaction lists available on demand.
11. **Unread counter**: Track and display unread message count per room — either Host-derived or Product-controlled.
12. **Message status**: Track message lifecycle (preparing, delivered, read), with read receipts in groups.
13. **Typing indicator**: Preview text with `ongoing` flag for "User is typing..." style animations.
14. **Participants**: List, paginate, and resolve participants. Support multiple identity types (Person, LitePerson, Contact, Product, Custom).
15. **Per-message actions**: Configurable long-press menu — built-in actions (copy, reply, edit, react) extensible with Product-defined actions.
16. **Toolbar actions**: Product-defined action buttons in the chat screen header.
17. **Customizable input**: Configurable chat footer — default input with optional payment/attachment buttons, no input, or fully custom.
18. **Participant actions**: Configurable actions when tapping on a participant.
19. **Chat placeholder**: Custom empty-state content for rooms with no messages.
20. **Room events**: Lines such as "Ana added Ben" or "Ana renamed the group", with the participants they name.
21. **Room actions**: Pin, archive, mute, mark unread, clear and leave a room, and answer or block a request, with the same state on all the user's devices.
22. **Spaces and threads**: A tree of rooms (space, category, forum, thread) with an order and shared settings, and a reply count on the root of a thread.
23. **Calls**: A call row that its participants update, for one-to-one calls, room calls and live streams.
24. **Links and invites**: Links to a room, a message, a call or an invite, join requests, and consent before an add.
25. **Polls and events**: Questions with options, quizzes and anonymous polls, and calendar events with RSVP, with one tally for every Product.
26. **Buttons and commands**: Buttons on a message with an answer to a press, and the commands of a room.
27. **Articles**: Long posts with a title, headings, tables and media between paragraphs.
28. **Stories**: Posts that expire, for an audience, with views and replies.
29. **Payments**: A payment row that points at a payment of the Host, with a state that the reader's Host checks.

#### Non-Functional Requirements

1. **Scalability**: Must support large chats with 10,000+ members without degrading performance.
2. **Pagination**: Messages and participants are paginated — the Host never attempts to load or store entire histories.
3. **Cacheability**: Host MAY cache pages of messages for performance but MUST NOT treat the cache as authoritative.
4. **Responsiveness**: Incremental feed updates (`on_message_feed_update`) allow the Host to update the UI without re-fetching entire pages.
5. **Graceful defaults**: A Product with zero handler registrations gets a fully functional basic chat out of the box.

#### Technical Constraints

1. **Serialization**: All types are serialized using JAM codec.
2. **Transport patterns**: All `handle_*` functions follow the subscription pattern (`_start`, `_stop`, `_interrupt`, `_receive`). All `on_*` functions and `chat_room_create`/`chat_room_delete` follow the request pattern (`_request`, `_response`).
3. **Product script lifecycle**: Products are JS scripts executed from scratch on each app start. The Host MUST delay rendering until initialization completes.
4. **Room ID scoping**: Room IDs are computed by the Host via `blake2b("product" ++ productId ++ room_id_source)` to guarantee cross-Product uniqueness.
5. **Resource URIs**: Binary resources are referenced via inline Base64 or Preimage CID — no direct URL references.

#### Limits

A Product refuses an action over a limit, and a reader drops a record over it.

| Item | Limit |
|---|---|
| One `Emoji::Unicode` | 64 bytes |
| Distinct reactions on one message | 64 |
| The text of a message | 16 KiB |
| The text of an `Article` | 64 KiB |
| The image of a `LinkPreview` | 64 KiB |
| A post of a `Stories` room, from the send to `expires_at` | 24 hours |

## Drawbacks

1. **Increased complexity for Product developers**: Products that want custom chat behavior must implement multiple handlers. The defaults mitigate this for simple use cases, but the API surface is large.

2. **Host caching ambiguity**: When the Product is the message source, the Host MAY cache but has no strict contract for cache invalidation. This could lead to stale data in edge cases.

3. **Initialization ordering**: The requirement to delay rendering until script initialization completes adds latency to the first chat screen render.

## Alternatives

- **Chat Extension v1** (Host API v0.4–v0.5): Product could create rooms and post messages, but the Host owned the message store. Bot registration was supported but is deferred to a future RFC.
- **Telegram Bot API**: Similar pattern where bots define commands, inline keyboards, and callback handlers. This RFC's action system (built-in + product actions) is analogous.
- **Matrix protocol**: Federated messaging with extensible event types. The `MessageContent` enum with `Custom` variant follows a similar extensibility model.

## Unresolved Questions

1. **Full message editing**: Should it be possible to edit all fields of a message (origin, timestamp), or only content? Current design only allows text edits that keep the media.

2. **Bot extension**: v1 defined bot registration (`host_chat_register_bot`). The bot concept needs to be revisited in the context of v2's handler model. Buttons and commands (5.3) give a bot its messages; bot registration is deferred to a future RFC.

3. **Visible decline**: This RFC hides a decline from the requester. Should every Product show it instead?

4. **Pinned messages**: How many pinned messages may one room hold? Chat apps range from three to no limit.

5. **Event guests**: When a `CalendarEvent` allows guests, is the number of guests part of the answer?

6. **Admins**: How many admins may one room have?

## Related Discussions

1. **Payment messages**: The payment button in chat input and `MessageContent::Payment` are in scope. Making a payment and checking a reference belong to the Payment Host API ([RFC 0006](0006-payments.md)).

2. **Call transport**: A call needs a signalling and a media path in the Host. This RFC defines only the call row and its signals.

3. **Custom message rendering**: `Custom(CustomMessageType, CustomMessagePayload)` messages use the existing `SerializedCustomChatMessage` rendering system. The details of that system are outside the scope of this RFC.

## References

- [Host API Design Document v0.5](https://docs.google.com/document/d/1AxKjF15y7gmdl-a6twc5wd8R5xcxKxMO8Ahp2l20v0g/edit?usp=sharing)
- [Triangle JS SDKs](https://github.com/Polkadot-Community-Foundation/triangle-js-sdks/tree/main/packages/host-api)
- [Chat Extension v1 Issue #41](https://github.com/paritytech/triangle-js-sdks/issues/41)

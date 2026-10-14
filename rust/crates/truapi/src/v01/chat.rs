//! Chat room, message and query types.
//!
//! The variant order of each enum here is part of the wire. Add a new variant
//! after the last one, and never reorder or remove a variant.

use crate::{Bytes32, v01::ContactHandle};
use parity_scale_codec::{Decode, Encode};

/// Request to create a chat room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateRoomRequest {
	/// Unique room identifier.
	pub room_id: String,
	/// Room display name.
	pub name: String,
	/// URL or base64 image.
	pub icon: String,
}

/// Whether the room was newly created or already existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatRoomRegistrationStatus {
	/// The room was created.
	New,
	/// A room with this ID already existed.
	Exists,
}

/// Result of a room registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateRoomResponse {
	/// `New` or `Exists`.
	pub status: ChatRoomRegistrationStatus,
}

/// Chat room registration error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatCreateRoomError {
	/// Not allowed.
	PermissionDenied,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// What a room shows below its messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ChatRoomFooter {
	/// The text field the user types messages into.
	TextInput,
	/// Nothing, for a room the product drives through actions alone.
	Empty,
}

/// Request to set what a room shows below its messages.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomFooterRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// What the room shows below its messages.
	pub footer: ChatRoomFooter,
}

/// Room footer failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatSetRoomFooterError {
	/// The calling product created no room with this id.
	UnknownRoom,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Request to register a chat bot.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatRegisterBotRequest {
	/// Unique bot identifier.
	pub bot_id: String,
	/// Bot display name.
	pub name: String,
	/// URL or base64 image.
	pub icon: String,
}

/// Whether the bot was newly registered or already existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatBotRegistrationStatus {
	/// The bot was registered.
	New,
	/// A bot with this ID already existed.
	Exists,
}

/// Result of a bot registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HostChatRegisterBotResponse {
	/// `New` or `Exists`.
	pub status: ChatBotRegistrationStatus,
}

/// Chat bot registration error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatRegisterBotError {
	/// Not allowed.
	PermissionDenied,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// How the product participates in a chat room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatRoomParticipation {
	/// The product owns and hosts the room.
	RoomHost,
	/// The product participates as a registered bot.
	Bot,
}

/// A chat room the product participates in.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoom {
	/// Room identifier.
	pub room_id: String,
	/// `RoomHost` or `Bot`.
	pub participating_as: ChatRoomParticipation,
}

/// A clickable action button in a chat message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatAction {
	/// Action identifier.
	pub action_id: String,
	/// Button label.
	pub title: String,
}

/// Layout for action buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatActionLayout {
	/// Buttons stacked vertically.
	Column,
	/// Buttons arranged in a grid.
	Grid,
}

/// A set of action buttons with optional text.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatActions {
	/// Optional message text.
	pub text: Option<String>,
	/// List of action buttons.
	pub actions: Vec<ChatAction>,
	/// `Column` or `Grid` layout.
	pub layout: ChatActionLayout,
}

/// A media attachment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatMedia {
	/// Media URL.
	pub url: String,
}

/// Rich text message with optional media.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRichText {
	/// Optional text content.
	pub text: Option<String>,
	/// Attached media items.
	pub media: Vec<ChatMedia>,
}

/// A file attachment in a chat message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatFile {
	/// File download URL.
	pub url: String,
	/// File name.
	pub file_name: String,
	/// MIME type.
	pub mime_type: String,
	/// File size in bytes.
	pub size_bytes: u64,
	/// Optional caption text.
	pub text: Option<String>,
}

/// A reaction to a chat message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatReaction {
	/// Message being reacted to.
	pub message_id: String,
	/// Emoji reaction.
	pub emoji: String,
}

/// A custom message with application-defined type and binary payload. The
/// host draws it through `Renderer::render`, with a `ChatMessage` context
/// carrying `message_type` and `payload` as the render payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatCustomMessage {
	/// Application-defined type key.
	pub message_type: String,
	/// Binary payload.
	pub payload: Vec<u8>,
}

/// Content of a chat message -- one of several types.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatMessageContent {
	/// Plain text message.
	Text {
		/// Message text.
		text: String,
	},
	/// Rich text with media.
	RichText(ChatRichText),
	/// Action button set.
	Actions(ChatActions),
	/// File attachment.
	File(ChatFile),
	/// Emoji reaction.
	Reaction(ChatReaction),
	/// Reaction removal.
	ReactionRemoved(ChatReaction),
	/// Custom message.
	Custom(ChatCustomMessage),
}

/// Request to post a message to a chat room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatPostMessageRequest {
	/// Room to post to.
	pub room_id: String,
	/// Message content.
	pub payload: ChatMessageContent,
}

/// Result of posting a message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatPostMessageResponse {
	/// Host-assigned message id, and the correlation key for any action the
	/// message carries: a trigger names it in [`ActionTrigger::message_id`].
	pub message_id: String,
}

/// Chat message posting error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatPostMessageError {
	/// Message exceeded size limit.
	MessageTooLarge,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// A press on a button the host draws for a `ChatMessageContent::Actions`
/// message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ActionTrigger {
	/// Message containing the action, as returned by `Chat::post_message` in
	/// [`HostChatPostMessageResponse::message_id`].
	pub message_id: String,
	/// Which action was triggered.
	pub action_id: String,
	/// Optional additional data.
	pub payload: Option<Vec<u8>>,
}

/// A slash command from a chat user.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatCommand {
	/// Command name.
	pub command: String,
	/// Command arguments.
	pub payload: String,
}

/// Payload of a received chat action.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatActionPayload {
	/// A peer posted a message.
	MessagePosted(ChatMessageContent),
	/// A user pressed a host-drawn `Actions` button.
	ActionTriggered(ActionTrigger),
	/// A user issued a command.
	Command(ChatCommand),
}

/// A chat action received from the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostChatActionSubscribeItem {
	/// Room where the action occurred.
	pub room_id: String,
	/// Peer who initiated the action.
	pub peer: String,
	/// The action payload.
	pub payload: ChatActionPayload,
}

/// Item containing the current chat rooms.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatListSubscribeItem {
	/// Chat rooms the product participates in.
	pub rooms: Vec<ChatRoom>,
}

/// Bytes the host shows: an avatar, an icon or a media item.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatResource {
	/// The bytes, with their media type.
	Inline {
		/// Media type of `bytes`, for example `image/png`.
		mime_type: String,
		/// The resource itself.
		bytes: Vec<u8>,
	},
	/// A preimage the host gets through `Preimage::lookup_subscribe`.
	Preimage {
		/// The key `Preimage::lookup_subscribe` takes.
		key: Vec<u8>,
	},
}

/// What a timeline needs to draw a media item before its bytes arrive.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatMediaMeta {
	/// Media type of the item, for example `image/jpeg`.
	pub mime_type: String,
	/// Size the product states. The host checks it for a preimage it gets.
	pub size_bytes: Option<u64>,
	/// Width in pixels.
	pub width: Option<u32>,
	/// Height in pixels.
	pub height: Option<u32>,
	/// BlurHash string the host draws until the bytes arrive.
	pub placeholder: Option<String>,
	/// Accessibility text, and the chat list preview of a message with only
	/// media.
	pub alt: Option<String>,
}

/// One media item of a message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatMediaItem {
	/// A still image.
	Image {
		/// The image bytes.
		resource: ChatResource,
		/// What the host draws before the bytes arrive.
		meta: ChatMediaMeta,
	},
	/// A video the host plays in its own player.
	Video {
		/// The video bytes.
		resource: ChatResource,
		/// What the host draws before the bytes arrive.
		meta: ChatMediaMeta,
	},
	/// A file the user can open or share.
	File {
		/// The file bytes.
		resource: ChatResource,
		/// What the host draws before the bytes arrive.
		meta: ChatMediaMeta,
		/// Name the host shows and saves the file under.
		file_name: String,
	},
}

/// Name, avatar and description of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomMetadata {
	/// Room display name.
	pub name: String,
	/// Room avatar. `None` draws the icon of the product.
	pub avatar: Option<ChatResource>,
	/// One or more lines the host shows in the room header.
	pub description: Option<String>,
}

/// How the chat list orders a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatRoomOrder {
	/// By `ChatRoomPreview::timestamp`, among the other rooms.
	Timestamp,
	/// Above every room that is not pinned, then by timestamp.
	PinToTop,
}

/// The badge the chat list shows on a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatRoomBadge {
	/// No badge.
	None,
	/// A number. The value `0` removes the badge.
	UnreadCount {
		/// The number the host shows.
		count: u32,
	},
	/// An image.
	Image {
		/// The image the host shows.
		resource: ChatResource,
	},
}

/// The line the chat list shows under a room name.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomPreviewText {
	/// The line itself.
	pub text: String,
	/// `true`: the host draws the line as an activity that continues, for
	/// example "Alice is typing", and not as the last message.
	pub ongoing: bool,
}

/// How a room shows in the chat list. A field at `None` keeps the host
/// default.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomPreview {
	/// The order group of the room.
	pub order: ChatRoomOrder,
	/// Milliseconds since the Unix epoch. The sort key in the order group.
	/// `None` uses the time of the newest message.
	pub timestamp: Option<u64>,
	/// The badge on the room. `None` keeps the unread count of the host.
	pub badge: Option<ChatRoomBadge>,
	/// `None` lets the host make the line from the newest message.
	pub text: Option<ChatRoomPreviewText>,
}

/// The parts of a room the product serves itself. A field at its default
/// value keeps the host behaviour for that part.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomConfig {
	/// `true`: the product keeps the messages and answers `messages_query` and
	/// `user_action`, and the host keeps only a page cache. `false`: the host
	/// stores the messages.
	pub product_source: bool,
	/// `true`: the host shows who reacted, and asks `reactions_query`.
	pub reaction_details: bool,
	/// `true`: the host asks `participants_query`. `false`: the room has two
	/// participants, the user and the product.
	pub participants: bool,
	/// `true`: the host asks `message_actions_query` before it opens a message
	/// menu.
	pub message_actions: bool,
	/// `true`: the host asks `participant_actions_query` before it opens a
	/// participant menu.
	pub participant_actions: bool,
	/// Emoji the reaction picker offers. `None` offers all emoji.
	pub allowed_reactions: Option<Vec<String>>,
	/// Text the room shows while it has no messages. `None` shows the host
	/// text.
	pub placeholder: Option<String>,
	/// `true`: the host sends a `Seen` event or user action when the user sees
	/// the newest message.
	pub report_seen: bool,
}

/// Request to replace the configuration of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomConfigRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The new configuration. It replaces the previous one completely.
	pub config: ChatRoomConfig,
}

/// Request to replace the name, avatar and description of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomMetadataRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The new metadata.
	pub metadata: ChatRoomMetadata,
}

/// Request to replace how a room shows in the chat list.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomPreviewRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The new preview.
	pub preview: ChatRoomPreview,
}

/// Request to delete a room and all the host keeps for it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatDeleteRoomRequest {
	/// A room the calling product created.
	pub room_id: String,
}

/// Failure of a request that changes a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatRoomError {
	/// The calling product created no room with this id.
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

/// Who wrote a message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatMessageOrigin {
	/// A participant id of the room. The id `"user"` is the user of the host,
	/// and the id `"product"` is the product.
	pub participant_id: String,
}

/// Where a message is on its way to the other participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatMessageStatus {
	/// Only the sender sees the message, for example while media uploads.
	Preparing,
	/// The message left the sender.
	Sent,
	/// The message reached the other participants.
	Delivered,
	/// Another participant read the message.
	Read,
	/// The product could not send the message. The host offers retry and
	/// delete.
	Failed,
}

/// What a message says. A reaction is not content; see `ChatFeedUpdate`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatMessageBody {
	/// Plain text. The host detects links but applies no Markdown.
	Text {
		/// Message text.
		text: String,
	},
	/// Markdown text with media below it.
	RichText {
		/// Markdown text, in the subset the host draws.
		text: Option<String>,
		/// Media the host draws below the text.
		media: Vec<ChatMediaItem>,
	},
	/// Buttons the host draws below an optional text. A press sends
	/// `ChatEvent::ActionTriggered`.
	Actions(ChatActions),
	/// A body the product draws through `Renderer::render` with a
	/// `ChatMessage` context.
	Custom {
		/// Product-defined type key.
		message_type: String,
		/// Product-defined payload, opaque to the host.
		payload: Vec<u8>,
		/// One line the host shows where it lists the message without drawing
		/// it.
		preview_text: Option<String>,
	},
}

/// How many participants reacted to a message with one emoji.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatReactionSummary {
	/// The emoji.
	pub emoji: String,
	/// Number of participants that reacted with this emoji.
	pub count: u32,
	/// `true` when the user is one of them.
	pub reacted_by_user: bool,
}

/// One message of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatMessage {
	/// Identifier the product chose, unique in the room.
	pub message_id: String,
	/// Who wrote the message.
	pub origin: ChatMessageOrigin,
	/// Milliseconds since the Unix epoch.
	pub timestamp: u64,
	/// What the message says.
	pub body: ChatMessageBody,
	/// Where the message is on its way.
	pub status: ChatMessageStatus,
	/// The message this one replies to.
	pub reply_to: Option<String>,
	/// Milliseconds since the Unix epoch of the last edit.
	pub edited_at: Option<u64>,
	/// Reactions, one entry for each emoji.
	pub reactions: Vec<ChatReactionSummary>,
}

/// One change to the messages of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatFeedUpdate {
	/// A new message.
	New(ChatMessage),
	/// A new body for a message.
	Edited {
		/// The message.
		message_id: String,
		/// The body that replaces the previous one.
		body: ChatMessageBody,
		/// Milliseconds since the Unix epoch.
		edited_at: u64,
	},
	/// A message is gone.
	Deleted {
		/// The message.
		message_id: String,
	},
	/// A participant reacted to a message.
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
	/// A message has a new status.
	StatusChanged {
		/// The message.
		message_id: String,
		/// The new status.
		status: ChatMessageStatus,
	},
}

/// Request to push one change to the messages of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatUpdateFeedRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The change.
	pub update: ChatFeedUpdate,
}

/// Failure to push a change to the messages of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatUpdateFeedError {
	/// The calling product created no room with this id.
	UnknownRoom,
	/// The product takes part in the room as a bot, not as its host.
	NotRoomHost,
	/// The update names a message the room does not hold.
	UnknownMessage,
	/// `New` names a message the room holds already.
	DuplicateMessage,
	/// The text or the custom payload is over its limit.
	MessageTooLarge,
	/// In a room the host stores: `New` from `"user"`, or `Edited` of a message
	/// the product did not write.
	NotOwner,
	/// The product pushed updates to this room too often.
	RateLimited,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Where a page of messages starts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatPageAnchor {
	/// Messages newer than the cursor. `None` starts at the oldest message.
	After {
		/// A cursor the product returned, or `None`.
		cursor: Option<String>,
	},
	/// Messages older than the cursor. `None` starts at the newest message.
	Before {
		/// A cursor the product returned, or `None`.
		cursor: Option<String>,
	},
}

/// Host question for one page of messages of a room the product keeps.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatMessagesQueryRequest {
	/// The room.
	pub room_id: String,
	/// Where the page starts.
	pub anchor: ChatPageAnchor,
	/// Largest number of messages the host wants.
	pub page_size: u32,
}

/// One page of messages.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatMessagesQueryItem {
	/// The messages, oldest first.
	pub messages: Vec<ChatMessage>,
	/// Cursor for the next page in the same direction. `None` ends the
	/// direction.
	pub next: Option<String>,
}

/// Failure of a query the host sends to the product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ProductChatQueryError {
	/// The product has no room with this id.
	UnknownRoom,
	/// The product cannot answer this question. The host uses its default.
	NotAvailable,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Something the user did in a room the product keeps.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatUserAction {
	/// The user sent a message.
	NewMessage {
		/// What the user wrote.
		body: ChatMessageBody,
		/// The message the user replied to.
		reply_to: Option<String>,
	},
	/// The user edited a message.
	EditMessage {
		/// The message.
		message_id: String,
		/// The new body.
		body: ChatMessageBody,
	},
	/// The user deleted a message.
	DeleteMessage {
		/// The message.
		message_id: String,
	},
	/// The user reacted to a message.
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
	/// The user saw this message, the newest one on the screen.
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

/// Host request that tells the product what the user did.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatUserActionRequest {
	/// The room.
	pub room_id: String,
	/// What the user did.
	pub action: ChatUserAction,
}

/// The answer of the product to a user action.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ProductChatUserActionItem {
	/// For `NewMessage`: the message as the product stored it.
	MessageCreated(ChatMessage),
	/// The product applied the action.
	Acknowledged,
}

/// Failure of a user action.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ProductChatUserActionError {
	/// The product has no room with this id.
	UnknownRoom,
	/// The room has no message with this id.
	UnknownMessage,
	/// The product refuses the action. The host shows the reason to the user.
	Rejected {
		/// Human-readable reason.
		reason: String,
	},
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// A participant of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatParticipant {
	/// A verified person of the People chain.
	Person {
		/// The person id on the People chain.
		person_id: u32,
	},
	/// A registered account that is not a person yet.
	LitePerson {
		/// The account id.
		account: Bytes32,
	},
	/// A contact the user picked through the Contacts API. The host draws its
	/// name and avatar.
	Contact {
		/// The handle the Contacts API gave the product.
		handle: ContactHandle,
	},
	/// Another product. The host draws its manifest name and icon.
	Product {
		/// The product id.
		product_id: String,
	},
	/// A participant of another kind. The product supplies the appearance.
	Custom {
		/// Display name.
		nickname: String,
		/// Avatar. `None` lets the host draw its default.
		avatar: Option<ChatResource>,
	},
}

/// A participant with its id in the room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatParticipantEntry {
	/// Identifier of the participant in the room.
	pub participant_id: String,
	/// Who the participant is.
	pub participant: ChatParticipant,
}

/// What the host asks about the participants of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatParticipantsQuery {
	/// The number of participants.
	Count,
	/// One page of participants.
	Page {
		/// A cursor the product returned, or `None` for the first page.
		cursor: Option<String>,
		/// Largest number of participants the host wants.
		page_size: u32,
	},
	/// The participants with these ids.
	ByIds {
		/// Participant ids.
		participant_ids: Vec<String>,
	},
}

/// Host question about the participants of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatParticipantsQueryRequest {
	/// The room.
	pub room_id: String,
	/// The question.
	pub query: ChatParticipantsQuery,
}

/// The answer to a participants query, in the kind of the question.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ProductChatParticipantsQueryItem {
	/// The answer to `Count`.
	Count {
		/// The number of participants.
		count: u32,
	},
	/// The answer to `Page`.
	Page {
		/// The participants of the page.
		participants: Vec<ChatParticipantEntry>,
		/// Cursor for the next page. `None` ends the list.
		next: Option<String>,
	},
	/// The answer to `ByIds`. An id the product does not know is absent.
	Resolved {
		/// The participants found.
		participants: Vec<ChatParticipantEntry>,
	},
}

/// Host question about who reacted to one message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatReactionsQueryRequest {
	/// The room.
	pub room_id: String,
	/// The message.
	pub message_id: String,
}

/// One reaction of one participant.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatReactionDetail {
	/// The participant.
	pub participant_id: String,
	/// The emoji.
	pub emoji: String,
}

/// Who reacted to one message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatReactionsQueryItem {
	/// One entry for each participant and emoji.
	pub reactions: Vec<ChatReactionDetail>,
}

/// What happens when the user selects a product action.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatActionHandler {
	/// The host opens the link. The worker does not take part.
	Deeplink {
		/// The link.
		url: String,
	},
	/// The host sends the matching `ChatEvent` to the product.
	Callback,
}

/// A menu entry the product defines.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatProductAction {
	/// Identifier of the action, unique in its menu.
	pub action_id: String,
	/// Text of the entry.
	pub label: String,
	/// Icon of the entry.
	pub icon: Option<ChatResource>,
	/// `true`: the host draws the entry as destructive.
	pub destructive: bool,
	/// What happens when the user selects the entry.
	pub handler: ChatActionHandler,
}

/// A message menu entry the host makes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatBuiltInMessageAction {
	/// Copy the text of the message.
	Copy,
	/// Reply to the message.
	Reply,
	/// Edit a message of the user.
	Edit,
	/// Delete a message of the user.
	Delete,
	/// React to the message.
	React,
	/// Send a `Failed` message again, in a room the product keeps.
	Retry,
}

/// The menu of one message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatMessageMenu {
	/// Only the entries of the product.
	Custom {
		/// The entries.
		actions: Vec<ChatProductAction>,
	},
	/// The host entries without `hidden`, then the entries of the product in
	/// their own section.
	ExtendBuiltIn {
		/// The entries of the product.
		actions: Vec<ChatProductAction>,
		/// Host entries the menu does not show.
		hidden: Vec<ChatBuiltInMessageAction>,
	},
}

/// The menu of one participant.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatParticipantMenu {
	/// Only the entries of the product.
	Custom {
		/// The entries.
		actions: Vec<ChatProductAction>,
	},
	/// The host entry for the kind of the participant, then the entries of the
	/// product in their own section.
	ExtendBuiltIn {
		/// The entries of the product.
		actions: Vec<ChatProductAction>,
	},
}

/// Host question for the menu of one message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatMessageActionsQueryRequest {
	/// The room.
	pub room_id: String,
	/// The message. The host sends a `Custom` body with an empty payload.
	pub message: ChatMessage,
}

/// The menu for one message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatMessageActionsQueryItem {
	/// The menu.
	pub menu: ChatMessageMenu,
}

/// Host question for the menu of one participant.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatParticipantActionsQueryRequest {
	/// The room.
	pub room_id: String,
	/// The participant.
	pub entry: ChatParticipantEntry,
}

/// The menu for one participant.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatParticipantActionsQueryItem {
	/// The menu.
	pub menu: ChatParticipantMenu,
}

/// A button in the toolbar of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatToolbarAction {
	/// Identifier of the action, unique in the toolbar.
	pub action_id: String,
	/// Text of the button, and of its entry in the overflow menu.
	pub label: String,
	/// Icon of the button.
	pub icon: ChatResource,
	/// What happens when the user presses the button.
	pub handler: ChatActionHandler,
}

/// Request to replace the toolbar buttons of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomToolbarRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The buttons, in order. An empty list removes the toolbar.
	pub actions: Vec<ChatToolbarAction>,
}

/// A payment button in the input of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatInputPayment {
	/// The button pays to this account.
	Enabled {
		/// The account that receives the payment.
		destination: Bytes32,
	},
}

/// The input the host shows below the messages.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatInputConfig {
	/// Hint text in the empty input. `None` shows the host text.
	pub placeholder: Option<String>,
	/// `true`: the input offers to attach a photo.
	pub photo: bool,
	/// `true`: the input offers to attach a video.
	pub video: bool,
	/// `true`: the input offers to attach a file.
	pub file: bool,
	/// A payment button, or `None`.
	pub payment: Option<ChatInputPayment>,
}

impl Default for ChatInputConfig {
	/// The input of a room that declares nothing: photo and video on, file and
	/// payment off, and the host hint text.
	fn default() -> Self {
		Self { placeholder: None, photo: true, video: true, file: false, payment: None }
	}
}

/// A quick reply above the input.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatSuggestion {
	/// Identifier of the suggestion, unique in the room.
	pub suggestion_id: String,
	/// Text of the suggestion.
	pub text: String,
	/// `true`: a press sends `text` as a message of the user. `false`: a press
	/// sends only `ChatEvent::SuggestionSelected`.
	pub sends_text: bool,
}

/// Request to replace the quick replies of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomSuggestionsRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The quick replies, in order. An empty list removes them.
	pub suggestions: Vec<ChatSuggestion>,
}

/// A slash command the input completes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatCommandSpec {
	/// The command name, without the slash.
	pub name: String,
	/// What the command does.
	pub description: String,
	/// Hint for the arguments, for example `<amount>`.
	pub argument_hint: Option<String>,
}

/// Request to replace the slash commands of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomCommandsRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The commands. An empty list removes the catalogue.
	pub commands: Vec<ChatCommandSpec>,
}

/// The activity line above the input.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatActivity {
	/// The line, for example "Bot is typing".
	pub text: String,
	/// The participant the line is about. `None` is the product.
	pub participant_id: Option<String>,
}

/// Request to show or remove the activity line of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomActivityRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The line, or `None` to remove it.
	pub activity: Option<ChatActivity>,
}

/// What a user did in a room of the product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatEvent {
	/// The user posted into a room the host stores. The host gave the id.
	MessagePosted {
		/// The id the host gave the message.
		message_id: String,
		/// What the user wrote.
		body: ChatMessageBody,
		/// The message the user replied to.
		reply_to: Option<String>,
	},
	/// A press on a button of an `Actions` message.
	ActionTriggered(ActionTrigger),
	/// The user sent a slash command the room declares.
	Command(ChatCommand),
	/// The user edited a message in a room the host stores.
	UserEdited {
		/// The message.
		message_id: String,
		/// The new body.
		body: ChatMessageBody,
	},
	/// The user deleted a message in a room the host stores.
	UserDeleted {
		/// The message.
		message_id: String,
	},
	/// The user reacted to a message, or removed a reaction, in a room the
	/// host stores.
	UserReacted {
		/// The message.
		message_id: String,
		/// The emoji.
		emoji: String,
		/// `true` for a new reaction, `false` for a removed one.
		placed: bool,
	},
	/// The user saw this message, the newest one on the screen, in a room the
	/// host stores.
	Seen {
		/// The message.
		message_id: String,
	},
	/// The user selected a product entry of a message menu.
	MessageActionTriggered {
		/// The message.
		message_id: String,
		/// The entry.
		action_id: String,
	},
	/// The user selected a product entry of a participant menu.
	ParticipantActionTriggered {
		/// The participant.
		participant_id: String,
		/// The entry.
		action_id: String,
	},
	/// The user pressed a toolbar button.
	ToolbarActionTriggered {
		/// The button.
		action_id: String,
	},
	/// The user pressed a quick reply.
	SuggestionSelected {
		/// The quick reply.
		suggestion_id: String,
	},
	/// The room screen opened.
	RoomOpened,
	/// The room screen closed.
	RoomClosed,
}

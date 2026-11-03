//! Chat room, message and query types.
//!
//! The variant order of each enum here is part of the wire. Add a new variant
//! after the last one, and never reorder or remove a variant.

use crate::v01::ChatMessageContent;
use parity_scale_codec::{Decode, Encode};

/// Request to create a chat room.
///
/// v0.2 lets the host make the room id. The product names the room with its
/// own source, and the host derives the id from the product and that source.
/// It also declares the parts of the room that the product serves.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateRoomRequest {
	/// The product's own name for the room. The same source resolves the same
	/// room again.
	pub room_id_source: String,
	/// Room display name.
	pub name: String,
	/// URL or base64 image.
	pub icon: String,
	/// The parts of the room that the product serves. It replaces the
	/// configuration that the host keeps for the room, also when the room
	/// exists. The host keeps it across restarts.
	pub config: ChatRoomConfig,
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
///
/// v0.2 adds `room_id`. Every later call names the room with this id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateRoomResponse {
	/// `New` or `Exists`.
	pub status: ChatRoomRegistrationStatus,
	/// The id the host derived for the room.
	pub room_id: String,
}

/// Chat room registration error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, derive_more::From)]
pub enum HostChatCreateRoomError {
	/// Not allowed.
	PermissionDenied,
	/// Catch-all.
	#[from]
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Request to post a message to a chat room.
///
/// v0.2 adds `alt`, the text a host shows wherever it lists a custom message
/// without drawing it, such as a chat list preview.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatPostMessageRequest {
	/// Room to post to.
	pub room_id: String,
	/// Message content.
	pub payload: ChatMessageContent,
	/// One line describing a custom message. `None` leaves the host its own
	/// placeholder. Every other kind previews from its own content, so its
	/// `alt` is dropped.
	pub alt: Option<String>,
}

/// Bytes the host shows: a media item of a message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatResource {
	/// A file in the HOP pool of the person-to-person chat, as its
	/// `P2PMixnet` attachments name it. Only a holder of the ticket reads the
	/// bytes, so a product sends the resource only through an encrypted
	/// channel.
	Hop {
		/// The identifier of the file's root entry.
		identifier: [u8; 32],
		/// The ticket that claims and opens the file.
		ticket: [u8; 32],
		/// The `wss://` URL of the node that holds it.
		node: String,
		/// The size of the file the sender states, in bytes.
		size: u64,
	},
}

/// What a timeline needs to draw a media item before its bytes arrive.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatMediaMeta {
	/// Media type of the item, for example `image/jpeg`.
	pub mime_type: String,
	/// Size the product states.
	pub size_bytes: Option<u64>,
	/// Width in pixels.
	pub width: Option<u32>,
	/// Height in pixels.
	pub height: Option<u32>,
	/// BlurHash string the host draws until the bytes arrive.
	pub placeholder: Option<String>,
	/// Accessibility text.
	pub alt: Option<String>,
	/// Length of a video, in milliseconds.
	pub duration_ms: Option<u64>,
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

/// The name, description, icon and empty-room text of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomMetadata {
	/// Room display name.
	pub name: String,
	/// One or more lines the host shows in the room header.
	pub description: Option<String>,
	/// An icon that replaces the icon of `create_room`, with the same rules.
	/// An empty string removes the icon. `None` keeps the current icon.
	pub icon: Option<String>,
	/// The text that the room shows while it has no messages. `None` shows
	/// nothing.
	pub placeholder: Option<String>,
}

/// The badge the chat list shows on a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatRoomBadge {
	/// A number. The value `0` shows no badge.
	UnreadCount {
		/// The number the host shows.
		count: u32,
	},
	/// An image, with the rules of the icon of `create_room`.
	Image {
		/// The image the host shows.
		icon: String,
	},
}

/// The newest event of a room when it is a reaction.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatPreviewReaction {
	/// The emoji.
	pub emoji: String,
	/// `true`: the user reacted. `false`: the contact of the room did.
	pub by_user: bool,
	/// The message the reaction is on, when the product still has it.
	pub message: Option<ChatPreviewMessage>,
	/// `true`: the newest event is the removal of this reaction.
	pub removed: bool,
}

/// A message as the chat list shows it in the preview line of a room. It has
/// no reactions: the preview shows them through `last_reaction` and
/// `unseen_reactions`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatPreviewMessage {
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
	/// The id of the message this one replies to.
	pub reply_to: Option<String>,
	/// Milliseconds since the Unix epoch of the last edit.
	pub edited_at: Option<u64>,
	/// `true`: the message is the welcome message of a chat request.
	pub request: bool,
}

/// How a room shows in the chat list. A field at `None` keeps the host
/// default.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomPreview {
	/// Milliseconds since the Unix epoch. The chat list orders the rooms by
	/// it, newest first. `None` uses the time of the newest message.
	pub timestamp: Option<u64>,
	/// The badge on the room. `None` keeps the unread count of the host.
	pub badge: Option<ChatRoomBadge>,
	/// The newest message the user sees in the room. The host makes the
	/// preview line from it. `None` lets the host use the newest message it
	/// holds.
	pub last_message: Option<ChatPreviewMessage>,
	/// The newest event of the room, when it is a reaction newer than
	/// `last_message`.
	pub last_reaction: Option<ChatPreviewReaction>,
	/// The number of reactions of the contact of the room that the user did
	/// not see yet.
	pub unseen_reactions: u32,
	/// The chat list shows the room above the rooms that are not pinned.
	/// Pinned rooms keep the order of `timestamp` among themselves.
	pub pinned: bool,
	/// A line that replaces the preview line, for example "Ben is typing".
	/// `None` shows the preview line of the newest message.
	pub activity: Option<ChatRoomActivity>,
}

/// A line that the chat list shows for a room in place of its newest message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomActivity {
	/// The text of the line.
	pub text: String,
	/// The activity continues, so the host animates the line.
	pub ongoing: bool,
}

/// The parts of a room the product serves itself. A field at its default
/// value keeps the host behaviour for that part.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomConfig {
	/// `true`: the product keeps the messages and answers `messages_query` and
	/// `user_action`, and the host keeps only a page cache.
	/// `false`: the host stores the messages.
	pub product_source: bool,
	/// `true`: the host shows who reacted, and asks `reactions_query`.
	pub reaction_details: bool,
	/// `true`: the host sends a `Seen` user action when the user sees the
	/// newest message.
	pub report_seen: bool,
}

/// Request to replace the name, description, icon and empty-room text of a
/// room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomMetadataRequest {
	/// A room that this product created.
	pub room_id: String,
	/// The new metadata.
	pub metadata: ChatRoomMetadata,
}

/// Request to replace how a room shows in the chat list.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomPreviewRequest {
	/// A room that this product created.
	pub room_id: String,
	/// The new preview.
	pub preview: ChatRoomPreview,
}

/// Request to delete a room and all the host keeps for it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatDeleteRoomRequest {
	/// A room that this product created.
	pub room_id: String,
}

/// Failure to replace the metadata of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatSetRoomMetadataError {
	/// This product created no room with this id.
	UnknownRoom,
	/// The product takes part in the room as a bot, not as its host.
	NotRoomHost,
}

/// Failure to replace how a room shows in the chat list.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatSetRoomPreviewError {
	/// This product created no room with this id.
	UnknownRoom,
	/// The product takes part in the room as a bot, not as its host.
	NotRoomHost,
	/// In a room the host stores: the preview shows a message or a reaction of
	/// the user.
	NotOwner,
	/// The product replaced the preview of this room too often.
	RateLimited,
}

/// Failure to delete a room. A delete of a room that does not exist succeeds.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatDeleteRoomError {
	/// The product takes part in the room as a bot, not as its host.
	NotRoomHost,
}

/// Who wrote a message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatMessageOrigin {
	/// A participant id of the room. The id `"user"` is the user of the host,
	/// the id `"product"` is the product, and the id `"contact"` is the
	/// contact of the room.
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
	/// The product did not send the message.
	Failed,
}

/// A line about the conversation that the host draws with its own strings.
/// The origin of the message says whose event it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatContactEvent {
	/// The participant accepted the chat request.
	RequestAccepted,
	/// The participant added the other participant as a contact.
	ContactAdded,
	/// The participant left the chat.
	LeftChat,
	/// Messages that are not available.
	Unavailable,
	/// A message this version of the product cannot show.
	Unsupported,
}

/// How a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatCallClose {
	/// Milliseconds since the Unix epoch.
	pub at: u64,
	/// `true`: the user ended or declined the call. `false`: the contact did.
	pub by_user: bool,
}

/// A call of the room. The time of the message is the time of the offer, and
/// its origin is the caller. The host draws the call bubble in the state that
/// these give.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatCallRecord {
	/// The call has video.
	pub video: bool,
	/// Milliseconds since the Unix epoch when a participant answered the call.
	/// `None`: nobody answered.
	pub answered_at: Option<u64>,
	/// How the call ended. `None`: the call has not ended.
	pub closed: Option<ChatCallClose>,
}

/// A payment between the user and the contact. The origin of the message is
/// the payer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatPaymentRecord {
	/// The amount, in the smallest unit.
	pub amount: u128,
	/// `true`: a transfer of the earlier chat, which the host draws as its
	/// legacy transfer bubble.
	pub legacy: bool,
}

/// A custom message body with an application-defined type and binary payload.
/// The host draws it through `Renderer::render`, with a `ChatMessage` context
/// carrying `message_type` and `payload` as the render payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatCustomBody {
	/// Application-defined type key.
	pub message_type: String,
	/// Binary payload.
	pub payload: Vec<u8>,
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
	/// Plain text with media below it.
	RichText {
		/// The caption, plain like `Text`.
		text: Option<String>,
		/// Media the host draws below the text.
		media: Vec<ChatMediaItem>,
	},
	/// A line about the conversation. The host shows it as a row with no bubble
	/// and no menu.
	Event(ChatContactEvent),
	/// A call.
	Call(ChatCallRecord),
	/// A payment.
	Payment(ChatPaymentRecord),
	/// A message the product draws through `Renderer::render`, with a
	/// `ChatMessage` render context.
	Custom {
		/// The type and payload that the renderer receives.
		message: ChatCustomBody,
		/// One line that describes the message in previews, reply quotes and
		/// notifications. `None` leaves the host its own placeholder.
		alt: Option<String>,
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
	/// `true`: the contact reacted with this emoji and the user has not seen
	/// that reaction yet.
	pub unseen: bool,
}

/// The message that a reply quotes. With it, the host draws the quote and does
/// not load that message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatReplyPreview {
	/// Who wrote the quoted message.
	pub origin: ChatMessageOrigin,
	/// What the quoted message says.
	pub body: ChatMessageBody,
}

/// The message that a message replies to.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatReply {
	/// The id of the quoted message.
	pub message_id: String,
	/// The quoted message, as the host shows it. `None` when the product no
	/// longer has that message.
	pub preview: Option<ChatReplyPreview>,
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
	pub reply: Option<ChatReply>,
	/// Milliseconds since the Unix epoch of the last edit.
	pub edited_at: Option<u64>,
	/// Reactions, one entry for each emoji.
	pub reactions: Vec<ChatReactionSummary>,
	/// `true`: the message is the welcome message of a chat request.
	pub request: bool,
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
	/// A message changed, but not by an edit. For example, a participant answered
	/// or ended its call. The host draws it again with this content in the same
	/// place, and does not mark it edited.
	Replaced(ChatMessage),
	/// A message is gone. The host removes it from the room.
	Deleted {
		/// The message.
		message_id: String,
	},
}

/// Request to push one change to the messages of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatUpdateFeedRequest {
	/// A room that this product created.
	pub room_id: String,
	/// The change.
	pub update: ChatFeedUpdate,
}

/// Failure to push a change to the messages of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostChatUpdateFeedError {
	/// This product created no room with this id.
	UnknownRoom,
	/// The product takes part in the room as a bot, not as its host.
	NotRoomHost,
	/// The update names a message the room does not hold.
	UnknownMessage,
	/// `New` names a message the room holds already.
	DuplicateMessage,
	/// The text is over its limit.
	MessageTooLarge,
	/// In a room the host stores: `New`, `Replaced` or a reaction from
	/// `"user"`, or `Edited`, `Replaced` or `Deleted` of a message the product
	/// did not write.
	NotOwner,
	/// The product pushed updates to this room too often.
	RateLimited,
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
pub struct ProductChatMessagesQueryResponse {
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
		/// The id the host chose for the message. The product stores and
		/// sends the message under it, so the message keeps its id from the
		/// moment the user sent it. `None` lets the product choose.
		message_id: Option<String>,
		/// Milliseconds since the Unix epoch when the user wrote the message.
		/// The product sends the message with this time, so the message keeps
		/// the time the user sent it. `None` lets the product use its own
		/// clock.
		timestamp: Option<u64>,
	},
	/// The user edited a message.
	EditMessage {
		/// The message.
		message_id: String,
		/// The new body.
		body: ChatMessageBody,
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
	/// The user accepted the chat request of the contact.
	AcceptRequest,
	/// The user declined the chat request of the contact.
	DeclineRequest,
	/// The user blocked the contact.
	Block,
	/// The user unblocked the contact.
	Unblock,
	/// The user left the chat.
	Leave,
	/// The user paid the contact. This version carries no payment: the core
	/// answers this action with `Unsupported` and does not send it to the
	/// product. A later change gives it the payment as a `CoinPaymentCheque`.
	SendPayment,
	/// The user deleted a message.
	DeleteMessage {
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
// A uniffi `Enum` cannot box a variant.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ProductChatUserActionResponse {
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
	/// The room does not keep its messages in the product, so the product
	/// takes no user action for it.
	NotAvailable,
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
	/// Milliseconds since the Unix epoch when the participant reacted.
	pub timestamp: u64,
}

/// Who reacted to one message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatReactionsQueryResponse {
	/// One entry for each participant and emoji.
	pub reactions: Vec<ChatReactionDetail>,
}

//! Chat room, message and query types.
//!
//! The variant order of each enum here is part of the wire. Add a new variant
//! after the last one, and never reorder or remove a variant.

use crate::Bytes32;
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
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
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

/// Bytes the host shows: a media item of a message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatResource {
	/// A file in the HOP pool of the person-to-person chat, as its
	/// `P2PMixnet` attachments name it. The host gets the bytes through the
	/// core, which claims them only from a node the host lists. Only a holder
	/// of the ticket reads the bytes, so a product sends the resource only
	/// through an encrypted channel.
	Hop {
		/// The 32-byte identifier of the file's root entry.
		identifier: Vec<u8>,
		/// The 32-byte ticket that claims and opens the file.
		ticket: Vec<u8>,
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

/// Name and description of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomMetadata {
	/// Room display name.
	pub name: String,
	/// One or more lines the host shows in the room header.
	pub description: Option<String>,
}

/// The badge the chat list shows on a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatRoomBadge {
	/// No badge.
	Hidden,
	/// A number. The value `0` removes the badge.
	UnreadCount {
		/// The number the host shows.
		count: u32,
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
	pub message: Option<ChatMessage>,
	/// `true`: the newest event is the removal of this reaction.
	pub removed: bool,
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
	pub last_message: Option<ChatMessage>,
	/// The newest event of the room, when it is a reaction newer than
	/// `last_message`.
	pub last_reaction: Option<ChatPreviewReaction>,
	/// The number of reactions of the contact of the room that the user did
	/// not see yet.
	pub unseen_reactions: u32,
	/// The newest call message of the room, a `Call` body with its record,
	/// when it is not already `last_message`. `None`: the room has no call,
	/// or its newest call is `last_message`. The host follows its own native
	/// rule: iOS shows the call as the last message once its newest event (the
	/// close, else the answer) is newer than `last_message`, and Android keeps
	/// `last_message`.
	#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), uniffi(default))]
	pub last_call: Option<ChatMessage>,
}

/// The parts of a room the product serves itself. A field at its default
/// value keeps the host behaviour for that part, and a room the product
/// declared no configuration for has the default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomConfig {
	/// `true`: the product keeps the messages and answers `messages_query`,
	/// `user_action` and `edit_history`, and the host keeps only a page cache.
	/// `false`: the host stores the messages.
	pub product_source: bool,
	/// `true`: the host shows who reacted, and asks `reactions_query`.
	pub reaction_details: bool,
	/// `true`: the host sends a `Seen` event or user action when the user sees
	/// the newest message.
	pub report_seen: bool,
}

/// Request to declare the parts of a room the product serves itself.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomConfigRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The configuration. It replaces the previous one completely.
	pub config: ChatRoomConfig,
}

/// Request to replace the name and description of a room.
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

/// Where the conversation of a room with its contact stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatContactState {
	/// No request yet. The input is the invite composer, and the first
	/// `NewMessage` is the welcome message of the request. The host does not
	/// list the room.
	Invite,
	/// The request of the user waits for the contact. The room has no input.
	Outgoing,
	/// The request of the contact waits for the user. The host lists the room
	/// under the requests and shows the accept banner.
	Incoming,
	/// Both sides accepted.
	Established,
	/// The user blocked the contact. The host does not list the room, and
	/// shows the blocked banner with Unblock.
	Blocked,
	/// The contact left the chat.
	PeerLeft,
}

/// The person a room is a conversation with. The host draws the room as its
/// own person-to-person chat: the avatar and name of the account, and the
/// header, menu, footer and banners of the state.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomContact {
	/// The account of the contact.
	pub account: Bytes32,
	/// The username of the contact, when it has one.
	pub username: Option<String>,
	/// Where the conversation stands.
	pub state: ChatContactState,
}

/// Request to set the contact of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomContactRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The contact. It replaces the previous one.
	pub contact: ChatRoomContact,
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
	/// The product could not send the message.
	Failed,
}

/// A line about the conversation that the host draws with its own strings.
/// The origin of the message says whose event it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatContactEvent {
	/// The chat request was accepted.
	RequestAccepted,
	/// The contact was added.
	ContactAdded,
	/// The participant left the chat.
	LeftChat,
	/// Messages that could not be retrieved.
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
	/// Milliseconds since the Unix epoch when the call was answered. `None`:
	/// nobody answered.
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
	/// A line about the conversation, drawn as a row with no bubble and no
	/// menu.
	Event(ChatContactEvent),
	/// A call.
	Call(ChatCallRecord),
	/// A payment.
	Payment(ChatPaymentRecord),
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

/// The message a reply quotes, so that the host draws the quote without
/// loading that message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatReplyPreview {
	/// Who wrote the quoted message.
	pub origin: ChatMessageOrigin,
	/// What the quoted message says.
	pub body: ChatMessageBody,
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
	/// The message this one replies to, as the host quotes it. `None` when
	/// `reply_to` is `None` or the product no longer has that message.
	pub reply_preview: Option<ChatReplyPreview>,
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
	/// A message changed for a reason that is not an edit, for example its
	/// call was answered or ended. The host draws it again with this content
	/// in the same place, and does not mark it edited.
	Replaced(ChatMessage),
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
	/// The text is over its limit.
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
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatUserAction {
	/// The user sent a message. In a room whose contact is `Invite`, it is the
	/// welcome message of the chat request.
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
	/// The user paid the contact. The host planned and reserved the coins and
	/// handed their keys off before it sends the action; the product sends
	/// them to the contact as one payment message under `message_id`, and
	/// answers `MessageCreated` with a `Payment` body. The keys appear in no
	/// body, preview or reply quote. Sending the action again with the same
	/// `message_id` sends nothing new.
	SendPayment {
		/// The id the host chose for the message: the id its own coinage
		/// records track the payment under.
		message_id: String,
		/// Milliseconds since the Unix epoch when the user paid.
		timestamp: u64,
		/// The sum of the coins, in the smallest unit.
		total_value: u128,
		/// The 64-byte sr25519 secret key of each coin.
		#[debug("{} keys", coin_keys.len())]
		coin_keys: Vec<Vec<u8>>,
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
pub enum ProductChatUserActionItem {
	/// For `NewMessage` and `SendPayment`: the message as the product stored
	/// it.
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
pub struct ProductChatReactionsQueryItem {
	/// One entry for each participant and emoji.
	pub reactions: Vec<ChatReactionDetail>,
}

/// Host question for the earlier texts of one message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatEditHistoryQueryRequest {
	/// The room.
	pub room_id: String,
	/// The message.
	pub message_id: String,
}

/// Host question for the messages of a room the product keeps that match
/// what the user typed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatSearchQueryRequest {
	/// The room.
	pub room_id: String,
	/// What the user typed.
	pub text: String,
	/// A cursor the product returned. `None` asks for the newest matches.
	pub cursor: Option<String>,
	/// Largest number of matches the host wants.
	pub page_size: u32,
}

/// One page of the messages that match a search.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatSearchQueryItem {
	/// The matches, newest first.
	pub hits: Vec<ChatMessage>,
	/// Cursor for older matches. `None` ends the matches.
	pub next: Option<String>,
}

/// One text a message had.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatMessageRevision {
	/// Milliseconds since the Unix epoch when the message took this text. The
	/// first text has the time of the message.
	pub edited_at: u64,
	/// The text, or the caption of a message with media. `None`: no text.
	pub text: Option<String>,
}

/// The texts one message had.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatEditHistoryQueryItem {
	/// The texts, oldest first.
	pub revisions: Vec<ChatMessageRevision>,
}

/// The call buttons in the header of a room.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomCalls {
	/// The host shows the audio call button.
	pub audio: bool,
	/// The host shows the video call button.
	pub video: bool,
}

/// The optional features of one room. Each field at its default value keeps
/// the behaviour of a room that declares no features.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatRoomFeatures {
	/// The call buttons in the header. `None` shows no call button.
	pub calls: Option<ChatRoomCalls>,
	/// The host gives a user photo, video or file to the core, which uploads
	/// it to a HOP node of the person-to-person chat, and gives the product a
	/// `ChatResource::Hop`. Only the chat product declares it, so its contacts
	/// on the native chat receive the file the way the native chat sends it.
	pub hop_attachments: bool,
}

/// Request to declare the optional features of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomFeaturesRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The features. They replace the previous declaration completely.
	pub features: ChatRoomFeatures,
}

/// The call of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatRoomCall {
	/// An incoming call that the product received. The host rings.
	Ringing {
		/// Identifier the product chose for the call.
		call_id: String,
		/// The call has video.
		video: bool,
		/// The participant that calls. `None` shows the product.
		caller: Option<String>,
	},
	/// A call that continues.
	Active {
		/// Identifier the product chose for the call.
		call_id: String,
		/// The call has video.
		video: bool,
		/// Milliseconds since the Unix epoch when the call started.
		started_at: u64,
	},
}

/// Request to show, change or remove the call of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomCallRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// The call. `None` removes the call.
	pub call: Option<ChatRoomCall>,
}

/// What the user did with the optional features of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum ChatRoomEvent {
	/// The user pressed a call button in the header.
	CallRequested {
		/// The user pressed the video call button.
		video: bool,
		/// The id the host chose for the call. The product uses it as the id
		/// of the call and of the message that records it, so the host shows
		/// the outgoing call at once and follows the status of that message.
		/// `None` lets the product choose.
		call_id: Option<String>,
	},
	/// The user answered the call that rings.
	CallAnswered {
		/// The call.
		call_id: String,
		/// The user accepted the call. `false`: the user declined it.
		accept: bool,
	},
	/// The user ended the call.
	CallEnded {
		/// The call.
		call_id: String,
	},
	/// The host has a media signal of the call for the other side: the
	/// product sends it there, where the product gives it to `callMedia.signal`.
	CallSignal {
		/// The call.
		call_id: String,
		/// Opaque bytes of the host.
		signal: Vec<u8>,
	},
}

/// One thing the user did with the optional features of a room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatRoomEventsSubscribeItem {
	/// The room.
	pub room_id: String,
	/// What the user did.
	pub event: ChatRoomEvent,
}

/// Host question: the user opened something that the product reads, such as
/// a contact link, a scanned code or a username, from an entry point of the
/// host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatOpenLinkRequest {
	/// A room of the product that the host shows unless the product answers
	/// another.
	pub room_id: String,
	/// What the user opened. The host does not read it.
	pub payload: String,
}

/// What the product did with an opened link.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductChatOpenLinkItem {
	/// The room the host shows next. `None` keeps the room of the request.
	pub room_id: Option<String>,
}

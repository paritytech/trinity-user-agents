use crate::v01::{
	ChatEvent, ChatInputConfig, ChatMessageContent, ChatRoomConfig, ChatRoomMetadata,
	ChatRoomPreview,
};
use parity_scale_codec::{Decode, Encode};

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

/// Request to create a chat room, or to find it again.
///
/// v0.2 carries the metadata, the configuration and the chat list preview of
/// the room.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateRoomRequest {
	/// Identifier the product chose for the room.
	pub room_id: String,
	/// Name, avatar and description of the room.
	pub metadata: ChatRoomMetadata,
	/// The parts of the room the product serves itself.
	pub config: ChatRoomConfig,
	/// How the room shows in the chat list. `None` lets the host make it.
	pub preview: Option<ChatRoomPreview>,
}

/// What a room shows below its messages.
///
/// v0.2 lets the product configure the input, and draw the footer itself.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ChatRoomFooter {
	/// The input the user types messages into.
	Input(ChatInputConfig),
	/// Nothing, for a room the product drives through actions alone.
	Empty,
	/// A footer the product draws through `Renderer::render` with a
	/// `ChatFooter` context.
	Custom,
}

/// Request to set what a room shows below its messages.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomFooterRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// What the room shows below its messages.
	pub footer: ChatRoomFooter,
}

/// What a user did in a room of the product.
///
/// v0.2 carries a `ChatEvent`, which gives a posted message its id and adds
/// the events of edits, deletes, reactions, menus, the toolbar, quick replies
/// and the room screen.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatActionSubscribeItem {
	/// Room where the user acted.
	pub room_id: String,
	/// Who acted. `"native"` is the user of the host.
	pub peer: String,
	/// What the user did.
	pub event: ChatEvent,
}

use crate::v01::{ChatCustomMessage, ChatMessageContent};
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

/// What a room shows below its messages.
///
/// `Card` pins a product-drawn body under the feed, rendered with
/// `RenderContext::ChatRoomFooter` and never shown as a message. `Web` shows a
/// page from the product's own web content there instead.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ChatRoomFooter {
	/// The text field the user types messages into.
	TextInput,
	/// Nothing, for a room the product drives through actions alone.
	Empty,
	/// One of the product's own cards, drawn by its renderer.
	Card(ChatCustomMessage),
	/// A page of the product's own web content, shown in a webview where the
	/// text field would be.
	Web(ChatRoomWebFooter),
}

/// A page of the product's own web content that a room shows below its
/// messages.
///
/// The host loads `path` from the product's own origin, the same way it loads
/// the product's pages, and never from anywhere else. The page talks to the
/// host through one message channel: a press it reports reaches the product
/// as a renderer action with `RenderContext::ChatRoomFooter` and an empty
/// `message_type`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ChatRoomWebFooter {
	/// Path of the page relative to the product's own web root, such as
	/// `footer.html?label=Go`. It may carry a query string and a fragment, but
	/// no scheme, host or parent reference.
	pub path: String,
	/// Height of the footer in logical points. The host caps it.
	pub height: u32,
}

/// Request to set what a room shows below its messages.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatSetRoomFooterRequest {
	/// A room the calling product created.
	pub room_id: String,
	/// What the room shows below its messages.
	pub footer: ChatRoomFooter,
}

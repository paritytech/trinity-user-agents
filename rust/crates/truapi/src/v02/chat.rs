use crate::v01::{ChatMessageContent, ChatRoomRegistrationStatus};
use parity_scale_codec::{Decode, Encode};

/// Request to create a chat room.
///
/// v0.2 lets the host make the room id. The product names the room with its
/// own source, and the host derives the id from the product and that source.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateRoomRequest {
	/// The product's own name for the room. The same source resolves the same
	/// room again.
	pub room_id_source: String,
	/// Room display name.
	pub name: String,
	/// URL or base64 image.
	pub icon: String,
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

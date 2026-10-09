use crate::v01::ChatMessageContent;
use parity_scale_codec::{Decode, Encode};

/// Request to post a message to a chat room.
///
/// v0.2 adds `alt`, the text a host shows wherever it lists the message
/// without drawing it, such as a chat list preview of a custom card.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatPostMessageRequest {
    /// Room to post to.
    pub room_id: String,
    /// Message content.
    pub payload: ChatMessageContent,
    /// One line describing the message. `None` leaves the host its own
    /// placeholder. A text message previews as itself, so its `alt` is dropped.
    pub alt: Option<String>,
}

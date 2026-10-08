use parity_scale_codec::{Decode, Encode, OptionBool};

/// Request to create a chat room.
///
/// A host creates the room once and answers `Exists` afterwards, but it still
/// applies `hide_text_input` on every call, so a product can change it for a
/// room that already exists.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateRoomRequest {
    /// Unique room identifier.
    pub room_id: String,
    /// Room display name.
    pub name: String,
    /// URL or base64 image.
    pub icon: String,
    /// Hide the text input, for a room the product drives through actions
    /// alone. Absent means the input is shown.
    pub hide_text_input: OptionBool,
}

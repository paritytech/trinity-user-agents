use parity_scale_codec::{Decode, Encode};

/// Where a product-rendered body lives, and the id that names it there.
///
/// v0.2 adds the room footer. The three v0.1 contexts come first, so they keep
/// their wire indexes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum RenderContext {
	/// A message in a chat room.
	ChatMessage {
		/// Room the message was posted in.
		room_id: String,
		/// Message id, as returned by `Chat::post_message`.
		message_id: String,
		/// Product-defined discriminator, as stored in
		/// `ChatCustomMessage::message_type`.
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
	/// The card a product docked under the messages of a room it created.
	ChatRoomFooter {
		/// Room the card is docked in.
		room_id: String,
		/// The `message_type` of the `ChatCustomMessage` the product set
		/// through `Chat::set_room_footer`.
		message_type: String,
	},
}

/// A body the host needs drawn.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProductRendererRenderRequest {
	/// Where the body lives.
	pub context: RenderContext,
	/// Product-defined payload, opaque to the host.
	pub payload: Vec<u8>,
}

/// An action triggered inside a product-rendered body.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostRendererActionSubscribeItem {
	/// Where the body lives.
	pub context: RenderContext,
	/// Which action was triggered, as named in the renderer tree.
	pub action_id: String,
	/// Data the node attached to the action. A `Button` press carries an
	/// empty payload, and a `TextField` value change carries the UTF-8 bytes
	/// of the new value, with no length prefix.
	pub payload: Vec<u8>,
}

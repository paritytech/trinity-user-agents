//! Versioned wrappers for [`Renderer`](crate::api::Renderer) methods.
//!
//! v0.2 adds the room footer as a place a body can live. The three v0.1
//! contexts keep their wire indexes, so a v0.1 product sees no change, and a
//! footer context is only ever sent to the product that docked it, which spoke
//! v0.2 to do so.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum ProductRendererRenderRequest {
		V1 => v01::ProductRendererRenderRequest,
		V2 => v02::ProductRendererRenderRequest,
	}
	pub enum ProductRendererRenderItem { V1 => v01::RendererNode, V2 => v01::RendererNode }
	pub enum ProductRendererRenderError { V1 => v01::GenericError, V2 => v01::GenericError }
	pub enum HostRendererActionSubscribeRequest { V1, V2 }
	pub enum HostRendererActionSubscribeItem {
		V1 => v01::HostRendererActionSubscribeItem,
		V2 => v02::HostRendererActionSubscribeItem,
	}
	pub enum HostRendererActionSubscribeError { V1 => v01::GenericError, V2 => v01::GenericError }
}

fn upgrade(context: v01::RenderContext) -> v02::RenderContext {
	match context {
		v01::RenderContext::ChatMessage { room_id, message_id, message_type } => {
			v02::RenderContext::ChatMessage { room_id, message_id, message_type }
		},
		v01::RenderContext::InputWidget { candidate_id } => {
			v02::RenderContext::InputWidget { candidate_id }
		},
		v01::RenderContext::PocketCard { card_id } => v02::RenderContext::PocketCard { card_id },
	}
}

fn downgrade(context: v02::RenderContext) -> Result<v01::RenderContext, v02::RenderContext> {
	match context {
		v02::RenderContext::ChatMessage { room_id, message_id, message_type } => {
			Ok(v01::RenderContext::ChatMessage { room_id, message_id, message_type })
		},
		v02::RenderContext::InputWidget { candidate_id } => {
			Ok(v01::RenderContext::InputWidget { candidate_id })
		},
		v02::RenderContext::PocketCard { card_id } => {
			Ok(v01::RenderContext::PocketCard { card_id })
		},
		footer @ v02::RenderContext::ChatRoomFooter { .. } => Err(footer),
	}
}

impl IntoLatest for ProductRendererRenderRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::ProductRendererRenderRequest { context, payload }) => {
				v02::ProductRendererRenderRequest { context: upgrade(context), payload }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for ProductRendererRenderRequest {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		let v02::ProductRendererRenderRequest { context, payload } = latest;
		if target >= 2 {
			return Self::V2(v02::ProductRendererRenderRequest { context, payload });
		}
		match downgrade(context) {
			Ok(context) => Self::V1(v01::ProductRendererRenderRequest { context, payload }),
			Err(context) => Self::V2(v02::ProductRendererRenderRequest { context, payload }),
		}
	}
}

impl IntoLatest for HostRendererActionSubscribeItem {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostRendererActionSubscribeItem { context, action_id, payload }) => {
				v02::HostRendererActionSubscribeItem {
					context: upgrade(context),
					action_id,
					payload,
				}
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for HostRendererActionSubscribeItem {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		let v02::HostRendererActionSubscribeItem { context, action_id, payload } = latest;
		if target >= 2 {
			return Self::V2(v02::HostRendererActionSubscribeItem { context, action_id, payload });
		}
		match downgrade(context) {
			Ok(context) => {
				Self::V1(v01::HostRendererActionSubscribeItem { context, action_id, payload })
			},
			Err(context) => {
				Self::V2(v02::HostRendererActionSubscribeItem { context, action_id, payload })
			},
		}
	}
}

impl IntoLatest for HostRendererActionSubscribeRequest {
	fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostRendererActionSubscribeRequest {
	fn from_latest((): Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2 } else { Self::V1 }
	}
}

impl IntoLatest for ProductRendererRenderItem {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for ProductRendererRenderItem {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for ProductRendererRenderError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for ProductRendererRenderError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for HostRendererActionSubscribeError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostRendererActionSubscribeError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		v02,
		versioned::{FromLatest, Versioned},
	};
	use parity_scale_codec::{Decode, Encode};

	#[test]
	fn render_item_string_matches_the_wire_fixture() {
		let item =
			ProductRendererRenderItem::V1(v01::RendererNode::String { text: "Votes: 1".into() });
		assert_eq!(hex::encode(item.encode()), "000120566f7465733a2031");
	}

	#[test]
	fn render_request_carries_the_chat_context() {
		let request = ProductRendererRenderRequest::V1(v01::ProductRendererRenderRequest {
			context: v01::RenderContext::ChatMessage {
				room_id: "room".into(),
				message_id: "message-1".into(),
				message_type: "vote".into(),
			},
			payload: vec![1, 2],
		});
		let bytes = request.encode();
		// V1 envelope, ChatMessage variant, then the three strings and payload.
		assert_eq!(hex::encode(&bytes), "000010726f6f6d246d6573736167652d3110766f7465080102");
		assert_eq!(ProductRendererRenderRequest::decode(&mut bytes.as_slice()).unwrap(), request);
	}

	#[test]
	fn action_item_button_payload_is_empty() {
		let item = HostRendererActionSubscribeItem::V1(v01::HostRendererActionSubscribeItem {
			context: v01::RenderContext::PocketCard { card_id: "loyalty".into() },
			action_id: "vote".into(),
			payload: Vec::new(),
		});
		let bytes = item.encode();
		// V1 envelope, PocketCard variant, the card and action ids, then the
		// empty payload's length prefix.
		assert_eq!(hex::encode(&bytes), "00021c6c6f79616c747910766f746500");
		assert_eq!(HostRendererActionSubscribeItem::decode(&mut bytes.as_slice()).unwrap(), item);
	}

	#[test]
	fn a_chat_message_context_still_renders_as_v1() {
		let request = v02::ProductRendererRenderRequest {
			context: v02::RenderContext::ChatMessage {
				room_id: "room".into(),
				message_id: "message-1".into(),
				message_type: "vote".into(),
			},
			payload: vec![1, 2],
		};

		assert_eq!(ProductRendererRenderRequest::from_latest(request, 1).version(), 1);
	}

	#[test]
	fn a_footer_context_renders_as_v2() {
		let request = v02::ProductRendererRenderRequest {
			context: v02::RenderContext::ChatRoomFooter {
				room_id: "room".into(),
				message_type: "register".into(),
			},
			payload: vec![1],
		};

		assert_eq!(ProductRendererRenderRequest::from_latest(request, 1).version(), 2);
	}

	#[test]
	fn footer_render_request_matches_the_wire_fixture() {
		let request = ProductRendererRenderRequest::V2(v02::ProductRendererRenderRequest {
			context: v02::RenderContext::ChatRoomFooter {
				room_id: "room".into(),
				message_type: "register".into(),
			},
			payload: vec![1],
		});
		let bytes = request.encode();
		assert_eq!(hex::encode(&bytes), "010310726f6f6d2072656769737465720401");
		assert_eq!(ProductRendererRenderRequest::decode(&mut bytes.as_slice()).unwrap(), request);
	}
}

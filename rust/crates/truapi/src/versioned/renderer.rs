//! Versioned wrappers for [`Renderer`](crate::api::Renderer) methods.
//!
//! v0.2 adds the room footer as a place a body can live. The v0.1
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
		v01::RenderContext::PocketCardBack { card_id } => {
			v02::RenderContext::PocketCardBack { card_id }
		},
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
		v02::RenderContext::PocketCardBack { card_id } => {
			Ok(v01::RenderContext::PocketCardBack { card_id })
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
mod tests;

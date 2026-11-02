//! Versioned wrappers for [`Chat`](crate::api::Chat) methods.
//!
//! v0.2 lets a product describe a posted message in one line with `alt`. A
//! v0.1 caller had no such text, so upgrading its request fills `alt` with
//! `None` and the host keeps its own placeholder. It also lets a product dock
//! one of its own cards in a room footer. A v0.1 footer upgrades to the same
//! `TextInput` or `Empty`.
//!
//! v0.2 also lets the host make the room id. The `room_id` of a v0.1 room
//! request becomes the source of the id, and a v0.1 answer carries no id.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostChatCreateRoomRequest {
		V1 => v01::HostChatCreateRoomRequest,
		V2 => v02::HostChatCreateRoomRequest,
	}
	pub enum HostChatCreateRoomResponse {
		V1 => v01::HostChatCreateRoomResponse,
		V2 => v02::HostChatCreateRoomResponse,
	}
	pub enum HostChatCreateRoomError {
		V1 => v01::HostChatCreateRoomError,
		V2 => v01::HostChatCreateRoomError,
	}
	pub enum HostChatRegisterBotRequest { V1 => v01::HostChatRegisterBotRequest }
	pub enum HostChatRegisterBotResponse { V1 => v01::HostChatRegisterBotResponse }
	pub enum HostChatRegisterBotError { V1 => v01::HostChatRegisterBotError }
	pub enum HostChatPostMessageRequest {
		V1 => v01::HostChatPostMessageRequest,
		V2 => v02::HostChatPostMessageRequest,
	}
	pub enum HostChatPostMessageResponse {
		V1 => v01::HostChatPostMessageResponse,
		V2 => v01::HostChatPostMessageResponse,
	}
	pub enum HostChatPostMessageError {
		V1 => v01::HostChatPostMessageError,
		V2 => v01::HostChatPostMessageError,
	}
	pub enum HostChatListSubscribeRequest { V1 }
	pub enum HostChatListSubscribeItem { V1 => v01::HostChatListSubscribeItem }
	pub enum HostChatListSubscribeError { V1 => v01::GenericError }
	pub enum HostChatActionSubscribeRequest { V1 }
	pub enum HostChatActionSubscribeItem { V1 => v01::HostChatActionSubscribeItem }
	pub enum HostChatActionSubscribeError { V1 => v01::GenericError }
	pub enum HostChatSetRoomFooterRequest {
		V1 => v01::HostChatSetRoomFooterRequest,
		V2 => v02::HostChatSetRoomFooterRequest,
	}
	pub enum HostChatSetRoomFooterResponse { V1, V2 }
	pub enum HostChatSetRoomFooterError {
		V1 => v01::HostChatSetRoomFooterError,
		V2 => v01::HostChatSetRoomFooterError,
	}
	pub enum ProductChatQueryError { V1 => v01::ProductChatQueryError }
	pub enum HostChatSetRoomMetadataRequest { V1 => v01::HostChatSetRoomMetadataRequest }
	pub enum HostChatSetRoomMetadataResponse { V1 }
	pub enum HostChatSetRoomMetadataError { V1 => v01::HostChatSetRoomMetadataError }
	pub enum HostChatSetRoomPreviewRequest { V1 => v01::HostChatSetRoomPreviewRequest }
	pub enum HostChatSetRoomPreviewResponse { V1 }
	pub enum HostChatSetRoomPreviewError { V1 => v01::HostChatSetRoomPreviewError }
	pub enum HostChatDeleteRoomRequest { V1 => v01::HostChatDeleteRoomRequest }
	pub enum HostChatDeleteRoomResponse { V1 }
	pub enum HostChatDeleteRoomError { V1 => v01::HostChatDeleteRoomError }
	pub enum HostChatUpdateFeedRequest { V1 => v01::HostChatUpdateFeedRequest }
	pub enum HostChatUpdateFeedResponse { V1 }
	pub enum HostChatUpdateFeedError { V1 => v01::HostChatUpdateFeedError }
	pub enum ProductChatMessagesQueryRequest { V1 => v01::ProductChatMessagesQueryRequest }
	pub enum ProductChatMessagesQueryResponse { V1 => v01::ProductChatMessagesQueryResponse }
	pub enum ProductChatUserActionRequest { V1 => v01::ProductChatUserActionRequest }
	pub enum ProductChatUserActionResponse { V1 => v01::ProductChatUserActionResponse }
	pub enum ProductChatUserActionError { V1 => v01::ProductChatUserActionError }
	pub enum ProductChatReactionsQueryRequest { V1 => v01::ProductChatReactionsQueryRequest }
	pub enum ProductChatReactionsQueryResponse { V1 => v01::ProductChatReactionsQueryResponse }
	pub enum HostChatSetRoomConfigRequest { V1 => v01::HostChatSetRoomConfigRequest }
	pub enum HostChatSetRoomConfigResponse { V1 }
	pub enum HostChatSetRoomConfigError { V1 => v01::HostChatSetRoomConfigError }
}

impl IntoLatest for HostChatCreateRoomRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostChatCreateRoomRequest { room_id, name, icon }) => {
				v02::HostChatCreateRoomRequest { room_id_source: room_id, name, icon }
			},
			Self::V2(latest) => latest,
		}
	}
}

// A v0.1 answer names no room. The dispatcher upgrades an answer only to
// downgrade it again for the same caller, so the empty id never reaches a
// product.
impl IntoLatest for HostChatCreateRoomResponse {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostChatCreateRoomResponse { status }) => {
				v02::HostChatCreateRoomResponse { status, room_id: String::new() }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for HostChatCreateRoomResponse {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 {
			return Self::V2(latest);
		}
		Self::V1(v01::HostChatCreateRoomResponse { status: latest.status })
	}
}

impl IntoLatest for HostChatCreateRoomError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostChatCreateRoomError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for HostChatPostMessageRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostChatPostMessageRequest { room_id, payload }) => {
				v02::HostChatPostMessageRequest { room_id, payload, alt: None }
			},
			Self::V2(latest) => latest,
		}
	}
}

// The response and error keep their v0.1 shape. They still gain a V2 variant,
// because a method's version is uniform across its request, response and error.

impl IntoLatest for HostChatPostMessageResponse {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostChatPostMessageResponse {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for HostChatPostMessageError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostChatPostMessageError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}


impl IntoLatest for HostChatSetRoomFooterRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostChatSetRoomFooterRequest { room_id, footer }) => {
				let footer = match footer {
					v01::ChatRoomFooter::TextInput => v02::ChatRoomFooter::TextInput,
					v01::ChatRoomFooter::Empty => v02::ChatRoomFooter::Empty,
				};
				v02::HostChatSetRoomFooterRequest { room_id, footer }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl IntoLatest for HostChatSetRoomFooterResponse {
	fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostChatSetRoomFooterResponse {
	fn from_latest((): Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2 } else { Self::V1 }
	}
}

impl IntoLatest for HostChatSetRoomFooterError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostChatSetRoomFooterError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

#[cfg(test)]
mod tests;

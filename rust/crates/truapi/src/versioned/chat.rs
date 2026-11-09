//! Versioned wrappers for [`Chat`](crate::api::Chat) methods.
//!
//! v0.2 lets a product describe a posted message in one line with `alt`. A
//! v0.1 caller had no such text, so upgrading its request fills `alt` with
//! `None` and the host keeps its own placeholder. It also lets a product dock
//! one of its own cards in a room footer. A v0.1 footer upgrades to the same
//! `TextInput` or `Empty`.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostChatCreateRoomRequest { V1 => v01::HostChatCreateRoomRequest }
	pub enum HostChatCreateRoomResponse { V1 => v01::HostChatCreateRoomResponse }
	pub enum HostChatCreateRoomError { V1 => v01::HostChatCreateRoomError }
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

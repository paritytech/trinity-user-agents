//! Versioned wrappers for [`ExpandedCard`](crate::api::ExpandedCard) methods.
//!
//! v0.2 names what each call does. Covering takes `covered`, the opposite of
//! the v0.1 `shown`, so a v0.1 request upgrades with its flag inverted. A turn
//! names the side to show, and a v0.1 `shown` upgrades to `Back` when true and
//! `Front` when false. Responses and errors keep their v0.1 shape.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostExpandedCardSetCardCoveredRequest {
		V1 => v01::HostExpandedCardSetCardCoveredRequest,
		V2 => v02::HostExpandedCardSetCardCoveredRequest,
	}
	pub enum HostExpandedCardSetCardCoveredResponse { V1, V2 }
	pub enum HostExpandedCardSetCardCoveredError {
		V1 => v01::HostExpandedCardSetCardCoveredError,
		V2 => v01::HostExpandedCardSetCardCoveredError,
	}
	pub enum HostExpandedCardTurnCardRequest {
		V1 => v01::HostExpandedCardTurnCardRequest,
		V2 => v02::HostExpandedCardTurnCardRequest,
	}
	pub enum HostExpandedCardTurnCardResponse { V1, V2 }
	pub enum HostExpandedCardTurnCardError {
		V1 => v01::HostExpandedCardTurnCardError,
		V2 => v01::HostExpandedCardTurnCardError,
	}
}

impl IntoLatest for HostExpandedCardSetCardCoveredRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostExpandedCardSetCardCoveredRequest { shown }) => {
				v02::HostExpandedCardSetCardCoveredRequest { covered: !shown }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl IntoLatest for HostExpandedCardTurnCardRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostExpandedCardTurnCardRequest { shown }) => {
				let side =
					if shown { v02::PocketCardSide::Back } else { v02::PocketCardSide::Front };
				v02::HostExpandedCardTurnCardRequest { side }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl IntoLatest for HostExpandedCardSetCardCoveredResponse {
	fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostExpandedCardSetCardCoveredResponse {
	fn from_latest((): Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2 } else { Self::V1 }
	}
}

impl IntoLatest for HostExpandedCardTurnCardResponse {
	fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostExpandedCardTurnCardResponse {
	fn from_latest((): Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2 } else { Self::V1 }
	}
}

impl IntoLatest for HostExpandedCardSetCardCoveredError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostExpandedCardSetCardCoveredError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for HostExpandedCardTurnCardError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostExpandedCardTurnCardError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

#[cfg(test)]
mod tests;

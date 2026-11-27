use parity_scale_codec::{Decode, Encode};

use super::PocketCardSide;

/// Request to cover or uncover the card under the calling Widget.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostExpandedCardSetCardCoveredRequest {
	/// `true` covers the card so touches over it go to the Widget, `false`
	/// gives them back to the card.
	pub covered: bool,
}

/// Card cover change failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum HostExpandedCardSetCardCoveredError {
	/// The Widget is not shown under its card right now.
	NotPresented,
	/// The user is moving or turning the card; the request had no effect.
	UserMoving,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// Request to turn the card under the calling Widget to one side.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostExpandedCardTurnCardRequest {
	/// The side to turn up.
	pub side: PocketCardSide,
}

/// Card turn failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum HostExpandedCardTurnCardError {
	/// The Widget is not shown under its card right now, such as one the host
	/// keeps loaded after the card closed.
	NotPresented,
	/// The user is moving or turning the card; the request had no effect.
	UserMoving,
	/// The card has no back to show right now.
	NoBack,
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

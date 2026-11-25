use parity_scale_codec::{Decode, Encode};

/// Request to cover or uncover the card under the calling Widget.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostExpandedCardSetCardCoveredRequest {
	/// `false` covers the card so touches over it go to the Widget, `true`
	/// gives them back to the card.
	pub shown: bool,
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
pub struct HostExpandedCardTurnCardRequest {
	/// `true` turns the back up, `false` turns the front up.
	pub shown: bool,
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

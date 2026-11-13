use parity_scale_codec::{Decode, Encode};

/// Request to cover or uncover the face under the calling Widget.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostExpandedCardSetFaceShownRequest {
	/// `false` covers the face so its touches go to the Widget, `true` gives
	/// them back to the face.
	pub shown: bool,
}

/// Face cover change failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum HostExpandedCardSetFaceShownError {
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
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostExpandedCardSetBackShownRequest {
	/// `true` turns the back up, `false` turns the front up.
	pub shown: bool,
}

/// Card turn failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum HostExpandedCardSetBackShownError {
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

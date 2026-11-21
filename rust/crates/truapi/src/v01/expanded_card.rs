use parity_scale_codec::{Decode, Encode};

/// Request to cover or uncover the face under the calling Widget.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostExpandedCardSetFaceShownRequest {
	/// `false` covers the face so its touches go to the Widget, `true` gives
	/// them back to the face.
	pub shown: bool,
}

/// Face cover change failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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

use parity_scale_codec::{Decode, Encode};

/// Request to show or hide the face above the calling Widget.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostExpandedCardSetFaceShownRequest {
    /// `true` brings the face back, `false` moves it out of the way.
    pub shown: bool,
}

/// Face visibility change failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum HostExpandedCardSetFaceShownError {
    /// The Widget is not shown under its card right now.
    NotPresented,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

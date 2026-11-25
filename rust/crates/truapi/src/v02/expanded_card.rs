//! Expanded card messages v0.2 redefines: a Widget says whether it covers the
//! card, and names the side a turn shows.

use parity_scale_codec::{Decode, Encode};

/// Request to cover or uncover the card under the calling Widget.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostExpandedCardSetCardCoveredRequest {
	/// `true` covers the card so touches over it go to the Widget, `false`
	/// gives them back to the card.
	pub covered: bool,
}

/// Request to turn the card under the calling Widget to one side.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostExpandedCardTurnCardRequest {
	/// The side to turn up.
	pub side: PocketCardSide,
}

/// A side of a Pocket card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PocketCardSide {
	/// The side the card shows unless it is turned over, drawn for
	/// [`RenderContext::PocketCard`](crate::v02::RenderContext::PocketCard).
	Front,
	/// The side drawn for
	/// [`RenderContext::PocketCardBack`](crate::v02::RenderContext::PocketCardBack), shown
	/// when the card is turned over.
	Back,
}

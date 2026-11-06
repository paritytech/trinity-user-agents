//! Versioned wrappers for [`ExpandedCard`](crate::api::ExpandedCard) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostExpandedCardSetFaceShownRequest { V1 => v01::HostExpandedCardSetFaceShownRequest }
	pub enum HostExpandedCardSetFaceShownResponse { V1 }
	pub enum HostExpandedCardSetFaceShownError { V1 => v01::HostExpandedCardSetFaceShownError }
	pub enum HostExpandedCardSetBackShownRequest { V1 => v01::HostExpandedCardSetBackShownRequest }
	pub enum HostExpandedCardSetBackShownResponse { V1 }
	pub enum HostExpandedCardSetBackShownError { V1 => v01::HostExpandedCardSetBackShownError }
}

#[cfg(test)]
mod tests;

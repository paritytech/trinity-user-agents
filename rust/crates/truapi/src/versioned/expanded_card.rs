//! Versioned wrappers for [`ExpandedCard`](crate::api::ExpandedCard) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostExpandedCardSetCardCoveredRequest { V1 => v01::HostExpandedCardSetCardCoveredRequest }
	pub enum HostExpandedCardSetCardCoveredResponse { V1 }
	pub enum HostExpandedCardSetCardCoveredError { V1 => v01::HostExpandedCardSetCardCoveredError }
	pub enum HostExpandedCardTurnCardRequest { V1 => v01::HostExpandedCardTurnCardRequest }
	pub enum HostExpandedCardTurnCardResponse { V1 }
	pub enum HostExpandedCardTurnCardError { V1 => v01::HostExpandedCardTurnCardError }
}

#[cfg(test)]
mod tests;

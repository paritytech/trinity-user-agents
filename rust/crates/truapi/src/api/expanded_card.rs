//! Unified [`ExpandedCard`] trait.

use crate::{
	CallContext, CallError,
	versioned::expanded_card::{
		HostExpandedCardSetCardCoveredError, HostExpandedCardSetCardCoveredRequest,
		HostExpandedCardSetCardCoveredResponse, HostExpandedCardTurnCardError,
		HostExpandedCardTurnCardRequest, HostExpandedCardTurnCardResponse,
	},
	wire, wire_trait,
};

/// The card a Widget is shown under.
///
/// Only a Widget execution may call it.
#[wire_trait(id = 23)]
#[crate::service(required_execution = Widget)]
#[async_trait::async_trait]
pub trait ExpandedCard: Send + Sync {
	/// Cover or uncover the card under the calling Widget.
	///
	/// A covered card stays drawn where it is and touches over it go to the
	/// Widget; neither state moves the card or resizes the Widget. Succeeds when
	/// the card is already in that state. Fails with `NotPresented` when the Widget
	/// is not shown under its card, and `UserMoving` while the user moves or
	/// turns the card.
	///
	/// ```ts
	/// const result = await truapi.expandedCard.setCardCovered({ covered: true });
	/// assert(result.isOk(), "setCardCovered failed:", result);
	/// console.log("card covered");
	/// ```
	#[wire(id = 0)]
	async fn set_card_covered(
		&self,
		_cx: &CallContext,
		_request: HostExpandedCardSetCardCoveredRequest,
	) -> Result<
		HostExpandedCardSetCardCoveredResponse,
		CallError<HostExpandedCardSetCardCoveredError>,
	> {
		Err(CallError::unavailable())
	}

	/// Turn the card under the calling Widget so its back or its front is up.
	///
	/// Succeeds when that side is already up. Fails with `NotPresented` when
	/// the Widget is not shown under its card, `UserMoving` while the user moves
	/// or turns the card, `NoBack` when the card has no back to show
	/// right now, and `Unsupported` on a host that cannot turn cards.
	///
	/// ```ts
	/// const result = await truapi.expandedCard.turnCard({ side: "Back" });
	/// assert(result.isOk(), "turnCard failed:", result);
	/// console.log("card turned to its back");
	/// ```
	#[wire(id = 1)]
	async fn turn_card(
		&self,
		_cx: &CallContext,
		_request: HostExpandedCardTurnCardRequest,
	) -> Result<HostExpandedCardTurnCardResponse, CallError<HostExpandedCardTurnCardError>> {
		Err(CallError::unavailable())
	}
}

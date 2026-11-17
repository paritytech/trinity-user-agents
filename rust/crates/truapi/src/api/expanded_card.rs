//! Unified [`ExpandedCard`] trait.

use crate::{
	CallContext, CallError,
	versioned::expanded_card::{
		HostExpandedCardSetBackShownError, HostExpandedCardSetBackShownRequest,
		HostExpandedCardSetBackShownResponse, HostExpandedCardSetFaceShownError,
		HostExpandedCardSetFaceShownRequest, HostExpandedCardSetFaceShownResponse,
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
	/// Cover or uncover the face under the calling Widget.
	///
	/// A covered face stays where it is and its touches go to the Widget;
	/// neither state moves the face or resizes the Widget. Succeeds when the
	/// face is already in that state. Fails with `NotPresented` when the Widget
	/// is not shown under its card, and `UserMoving` while the user moves or
	/// turns the card.
	///
	/// ```ts
	/// const result = await truapi.expandedCard.setFaceShown({ shown: false });
	/// assert(result.isOk(), "setFaceShown failed:", result);
	/// console.log("face covered");
	/// ```
	#[wire(id = 0)]
	async fn set_face_shown(
		&self,
		_cx: &CallContext,
		_request: HostExpandedCardSetFaceShownRequest,
	) -> Result<HostExpandedCardSetFaceShownResponse, CallError<HostExpandedCardSetFaceShownError>>
	{
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
	/// const result = await truapi.expandedCard.setBackShown({ shown: true });
	/// assert(result.isOk(), "setBackShown failed:", result);
	/// console.log("back shown");
	/// ```
	#[wire(id = 1)]
	async fn set_back_shown(
		&self,
		_cx: &CallContext,
		_request: HostExpandedCardSetBackShownRequest,
	) -> Result<HostExpandedCardSetBackShownResponse, CallError<HostExpandedCardSetBackShownError>>
	{
		Err(CallError::unavailable())
	}
}

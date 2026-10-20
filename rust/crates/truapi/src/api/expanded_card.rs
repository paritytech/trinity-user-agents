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
#[crate::async_trait]
pub trait ExpandedCard: Send + Sync {
	/// Show or hide the face above the calling Widget.
	///
	/// Succeeds when the face is already in that state. Fails with
	/// `NotPresented` when the Widget is not shown under its card.
	///
	/// ```ts
	/// const result = await truapi.expandedCard.setFaceShown({ shown: false });
	/// assert(result.isOk(), "setFaceShown failed:", result);
	/// console.log("face hidden");
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

	/// Turn the card above the calling Widget so its back or its front is up.
	///
	/// Succeeds when that side is already up. Fails with `NotPresented` when
	/// the Widget is not shown under its card, `UserMoving` while the user turns
	/// the card or moves its face, and `NoBack` when the card has no back to
	/// show right now.
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

//! Unified [`ExpandedCard`] trait.

use crate::{
	CallContext, CallError,
	versioned::expanded_card::{
		HostExpandedCardSetFaceShownError, HostExpandedCardSetFaceShownRequest,
		HostExpandedCardSetFaceShownResponse,
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
}

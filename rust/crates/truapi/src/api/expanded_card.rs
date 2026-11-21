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
}

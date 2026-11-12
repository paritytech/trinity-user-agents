//! Product-facing expanded card capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::ExpandedCard,
	latest::HostExpandedCardSetFaceShownError as FaceShownError,
	versioned::expanded_card::{
		HostExpandedCardSetBackShownError, HostExpandedCardSetBackShownRequest,
		HostExpandedCardSetBackShownResponse, HostExpandedCardSetFaceShownError,
		HostExpandedCardSetFaceShownRequest, HostExpandedCardSetFaceShownResponse,
	},
};

use crate::{
	platform::{ExecutableKind, ExpandedCardFaceOutcome},
	runtime::{ProductRuntimeHost, versioning::version},
};

#[async_trait::async_trait]
impl ExpandedCard for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "expanded_card.set_face_shown"))]
	async fn set_face_shown(
		&self,
		_cx: &CallContext,
		request: HostExpandedCardSetFaceShownRequest,
	) -> Result<HostExpandedCardSetFaceShownResponse, CallError<HostExpandedCardSetFaceShownError>>
	{
		version(request)
			.exec(|request| async move {
				self.require_widget()?;
				let Some(card) = &self.connection.expanded_card else {
					return Err(CallError::Unsupported);
				};
				match card.set_expanded_card_face_shown(request.shown).await {
					Ok(ExpandedCardFaceOutcome::Applied) => Ok(()),
					Ok(ExpandedCardFaceOutcome::NotPresented) => {
						Err(CallError::Domain(FaceShownError::NotPresented))
					},
					Ok(ExpandedCardFaceOutcome::UserMoving) => {
						Err(CallError::Domain(FaceShownError::UserMoving))
					},
					Ok(ExpandedCardFaceOutcome::Unsupported) => Err(CallError::Unsupported),
					Err(error) => {
						Err(CallError::Domain(FaceShownError::Unknown { reason: error.reason }))
					},
				}
			})
			.await
	}

	// This runtime has no way to turn a card, so a Widget reads "not supported" rather than a
	// failure.
	#[instrument(skip_all, fields(runtime.method = "expanded_card.set_back_shown"))]
	async fn set_back_shown(
		&self,
		_cx: &CallContext,
		request: HostExpandedCardSetBackShownRequest,
	) -> Result<HostExpandedCardSetBackShownResponse, CallError<HostExpandedCardSetBackShownError>>
	{
		version(request)
			.exec(|_request| async move {
				self.require_widget()?;
				Err(CallError::Unsupported)
			})
			.await
	}
}

impl ProductRuntimeHost {
	// Admin connections call the trait without the dispatcher's kind filter.
	fn require_widget<E>(&self) -> Result<(), CallError<E>> {
		if self.connection.product.execution_kind != ExecutableKind::Widget {
			return Err(CallError::Denied);
		}
		Ok(())
	}
}

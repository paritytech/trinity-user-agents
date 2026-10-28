//! Product-facing expanded card capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::ExpandedCard,
	latest::HostExpandedCardSetFaceShownError as FaceShownError,
	versioned::expanded_card::{
		HostExpandedCardSetFaceShownError, HostExpandedCardSetFaceShownRequest,
		HostExpandedCardSetFaceShownResponse,
	},
};

use crate::{
	platform::{ExecutableKind, ExpandedCardFaceOutcome},
	runtime::{ProductRuntimeHost, versioning::version},
};

#[truapi::async_trait]
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
				// Admin connections call the trait without the dispatcher's kind filter.
				if self.connection.product.execution_kind != ExecutableKind::Widget {
					return Err(CallError::Denied);
				}
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
}

//! Product-facing expanded card capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::ExpandedCard,
	latest::HostExpandedCardSetFaceShownError as FaceShownError,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		expanded_card::{
			HostExpandedCardSetFaceShownError, HostExpandedCardSetFaceShownRequest,
			HostExpandedCardSetFaceShownResponse,
		},
	},
};

use super::from_latest_error;
use crate::{
	platform::{ExpandedCardFaceOutcome, ProductExecutionKind},
	runtime::ProductRuntimeHost,
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
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			// Admin connections call the trait without the dispatcher's kind filter.
			if self.connection.product.execution_kind != ProductExecutionKind::Widget {
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
		}
		.await;
		result
			.map(|response| HostExpandedCardSetFaceShownResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}

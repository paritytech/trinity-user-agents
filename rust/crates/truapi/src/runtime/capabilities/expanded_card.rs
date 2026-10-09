//! Product-facing expanded card capability adapter.

use tracing::instrument;
use truapi::api::ExpandedCard;
use truapi::latest::HostExpandedCardSetFaceShownError as FaceShownError;
use truapi::versioned::expanded_card::{
    HostExpandedCardSetFaceShownError, HostExpandedCardSetFaceShownRequest,
    HostExpandedCardSetFaceShownResponse,
};
use truapi::{CallContext, CallError};

use crate::platform::{ExpandedCardFaceOutcome, ProductExecutionKind};
use crate::runtime::ProductRuntimeHost;

#[truapi::async_trait]
impl ExpandedCard for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "expanded_card.set_face_shown"))]
    async fn set_face_shown(
        &self,
        _cx: &CallContext,
        request: HostExpandedCardSetFaceShownRequest,
    ) -> Result<HostExpandedCardSetFaceShownResponse, CallError<HostExpandedCardSetFaceShownError>>
    {
        // Admin connections call the trait without the dispatcher's kind filter.
        if self.product.execution_kind != ProductExecutionKind::Widget {
            return Err(CallError::Denied);
        }
        let Some(card) = &self.expanded_card else {
            return Err(CallError::Unsupported);
        };
        let HostExpandedCardSetFaceShownRequest::V1(request) = request;
        let domain_error = |error| CallError::Domain(HostExpandedCardSetFaceShownError::V1(error));
        match card.set_expanded_card_face_shown(request.shown).await {
            Ok(ExpandedCardFaceOutcome::Applied) => Ok(HostExpandedCardSetFaceShownResponse::V1),
            Ok(ExpandedCardFaceOutcome::NotPresented) => {
                Err(domain_error(FaceShownError::NotPresented))
            }
            Ok(ExpandedCardFaceOutcome::UserMoving) => {
                Err(domain_error(FaceShownError::UserMoving))
            }
            Ok(ExpandedCardFaceOutcome::Unsupported) => Err(CallError::Unsupported),
            Err(error) => {
                Err(domain_error(FaceShownError::Unknown {
                    reason: error.reason,
                }))
            }
        }
    }
}

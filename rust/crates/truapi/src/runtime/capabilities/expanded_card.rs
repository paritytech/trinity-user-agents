//! Product-facing expanded card capability adapter.

use tracing::instrument;
use truapi::api::ExpandedCard;
use truapi::versioned::expanded_card::{
    HostExpandedCardSetFaceShownError, HostExpandedCardSetFaceShownRequest,
    HostExpandedCardSetFaceShownResponse,
};
use truapi::{CallContext, CallError};

use crate::runtime::ProductRuntimeHost;

#[truapi::async_trait]
impl ExpandedCard for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "expanded_card.set_face_shown"))]
    async fn set_face_shown(
        &self,
        _cx: &CallContext,
        _request: HostExpandedCardSetFaceShownRequest,
    ) -> Result<HostExpandedCardSetFaceShownResponse, CallError<HostExpandedCardSetFaceShownError>>
    {
        Err(CallError::Unsupported)
    }
}

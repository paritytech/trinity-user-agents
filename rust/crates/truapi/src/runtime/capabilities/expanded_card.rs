//! Product-facing expanded card capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::ExpandedCard,
	latest::{
		HostExpandedCardSetCardCoveredError as CoverError,
		HostExpandedCardTurnCardError as TurnError,
	},
	versioned::expanded_card::{
		HostExpandedCardSetCardCoveredError, HostExpandedCardSetCardCoveredRequest,
		HostExpandedCardSetCardCoveredResponse, HostExpandedCardTurnCardError,
		HostExpandedCardTurnCardRequest, HostExpandedCardTurnCardResponse,
	},
};

use crate::{
	platform::{ExecutableKind, ExpandedCardCoverOutcome, ExpandedCardTurnOutcome},
	runtime::{AccountHolder, ProductRuntimeHost, versioning::version},
};

#[async_trait::async_trait]
impl<H: AccountHolder> ExpandedCard for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "expanded_card.set_card_covered"))]
	async fn set_card_covered(
		&self,
		_cx: &CallContext,
		request: HostExpandedCardSetCardCoveredRequest,
	) -> Result<
		HostExpandedCardSetCardCoveredResponse,
		CallError<HostExpandedCardSetCardCoveredError>,
	> {
		version(request)
			.exec(|request| async move {
				self.require_widget()?;
				let Some(card) = &self.connection.expanded_card else {
					return Err(CallError::Unsupported);
				};
				match card.set_expanded_card_covered(request.covered).await {
					Ok(ExpandedCardCoverOutcome::Applied) => Ok(()),
					Ok(ExpandedCardCoverOutcome::NotPresented) => {
						Err(CallError::Domain(CoverError::NotPresented))
					},
					Ok(ExpandedCardCoverOutcome::UserMoving) => {
						Err(CallError::Domain(CoverError::UserMoving))
					},
					Ok(ExpandedCardCoverOutcome::Unsupported) => Err(CallError::Unsupported),
					Err(error) => {
						Err(CallError::Domain(CoverError::Unknown { reason: error.reason }))
					},
				}
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "expanded_card.turn_card"))]
	async fn turn_card(
		&self,
		_cx: &CallContext,
		request: HostExpandedCardTurnCardRequest,
	) -> Result<HostExpandedCardTurnCardResponse, CallError<HostExpandedCardTurnCardError>> {
		version(request)
			.exec(|request| async move {
				self.require_widget()?;
				let Some(card) = &self.connection.expanded_card else {
					return Err(CallError::Unsupported);
				};
				match card.turn_expanded_card(request.side).await {
					Ok(ExpandedCardTurnOutcome::Applied) => Ok(()),
					Ok(ExpandedCardTurnOutcome::NotPresented) => {
						Err(CallError::Domain(TurnError::NotPresented))
					},
					Ok(ExpandedCardTurnOutcome::UserMoving) => {
						Err(CallError::Domain(TurnError::UserMoving))
					},
					Ok(ExpandedCardTurnOutcome::NoBack) => {
						Err(CallError::Domain(TurnError::NoBack))
					},
					Ok(ExpandedCardTurnOutcome::Unsupported) => Err(CallError::Unsupported),
					Err(error) => {
						Err(CallError::Domain(TurnError::Unknown { reason: error.reason }))
					},
				}
			})
			.await
	}
}

impl<H: AccountHolder> ProductRuntimeHost<H> {
	// Admin connections call the trait without the dispatcher's kind filter.
	fn require_widget<E>(&self) -> Result<(), CallError<E>> {
		if self.connection.product.execution_kind != ExecutableKind::Widget {
			return Err(CallError::Denied);
		}
		Ok(())
	}
}

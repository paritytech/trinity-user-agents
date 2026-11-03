//! Summaries stored in format 1, read into the current shape: a summary
//! kept how the session ended, and nothing of its provider, quote or
//! references.

use parity_scale_codec::Decode;
use truapi::latest::{FundingDirection, FundingFailure};

use crate::{
	host_logic::funding::{FundingSessionSummary, FundingStage, FundingStatus},
	platform::ProductId,
};

/// A summary as format 1 stored it.
#[derive(Decode)]
pub(super) struct Summary {
	intent: String,
	owner_product_id: Option<ProductId>,
	direction: FundingDirection,
	amount: Option<u128>,
	stage: FundingStage,
	opened_at_ms: u64,
}

impl From<Summary> for FundingSessionSummary {
	fn from(summary: Summary) -> Self {
		// Only an ended session was summarised, and format 1 knew no payout
		// or refund after it ended.
		let status = match &summary.stage {
			FundingStage::Delivered { .. } => FundingStatus::ToppedUp,
			FundingStage::Released { .. } => FundingStatus::Sent,
			FundingStage::Failed { reason, .. } => FundingStatus::Failed { reason: reason.clone() },
			FundingStage::Open => FundingStatus::Failed { reason: FundingFailure::Expired },
		};
		Self {
			intent: summary.intent,
			owner_product_id: summary.owner_product_id,
			direction: summary.direction,
			amount: summary.amount,
			stage: summary.stage,
			opened_at_ms: summary.opened_at_ms,
			provider_id: None,
			choice: None,
			status,
			payout: None,
			refunded: None,
			transaction_id: None,
			reference: None,
		}
	}
}

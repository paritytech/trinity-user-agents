//! Sessions stored in format 4, read into the current shape: a chosen quote
//! named no network or asset decimals, a session had no refund or progress
//! detail, and started funds moved the session's own way.

use parity_scale_codec::Decode;
use truapi::latest::{FundingDirection, FundingQuote, FundingRail};

use super::format3::Started;
use crate::{
	host_logic::funding::{
		FundingCancel, FundingChoice, FundingSession, FundingStage, FundingUpdateRecord,
	},
	platform::ProductId,
};

/// A session as format 4 stored it.
#[derive(Decode)]
pub struct Session {
	intent: String,
	owner_product_id: Option<ProductId>,
	direction: FundingDirection,
	amount: Option<u128>,
	stage: FundingStage,
	opened_at_ms: u64,
	deadline_ms: u64,
	acknowledged: bool,
	provider_id: Option<ProductId>,
	assigned_at_ms: Option<u64>,
	cancel: Option<FundingCancel>,
	updates: Vec<FundingUpdateRecord>,
	started: Vec<Started>,
	choice: Option<Choice>,
}

#[derive(Decode)]
struct Choice {
	quote: Quote,
	rail: FundingRail,
	asset: String,
}

#[derive(Decode)]
struct Quote {
	quote_id: String,
	send_amount: u128,
	receive_amount: u128,
	provider_fee: u128,
	network_fee: u128,
	eta_secs: Option<u64>,
	expires_at: Option<u64>,
}

impl From<Session> for FundingSession {
	fn from(session: Session) -> Self {
		Self {
			intent: session.intent,
			owner_product_id: session.owner_product_id,
			direction: session.direction,
			amount: session.amount,
			stage: session.stage,
			opened_at_ms: session.opened_at_ms,
			deadline_ms: session.deadline_ms,
			acknowledged: session.acknowledged,
			provider_id: session.provider_id,
			assigned_at_ms: session.assigned_at_ms,
			cancel: session.cancel,
			updates: session.updates,
			started: session
				.started
				.into_iter()
				.map(|started| started.moving(session.direction))
				.collect(),
			choice: session.choice.map(Into::into),
			refund: None,
			detail: None,
		}
	}
}

impl From<Choice> for FundingChoice {
	fn from(choice: Choice) -> Self {
		let Quote {
			quote_id,
			send_amount,
			receive_amount,
			provider_fee,
			network_fee,
			eta_secs,
			expires_at,
		} = choice.quote;
		Self {
			// Format 4 did not keep the asset's decimals, so they read as
			// none; the quote is no longer ranked against others.
			quote: FundingQuote {
				quote_id,
				send_amount,
				receive_amount,
				provider_fee,
				network_fee,
				eta_secs,
				expires_at,
				asset_decimals: 0,
			},
			rail: choice.rail,
			asset: choice.asset,
			network: None,
		}
	}
}

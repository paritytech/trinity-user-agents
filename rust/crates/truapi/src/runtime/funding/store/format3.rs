//! Sessions stored in format 3, read into the current shape: a session had
//! no chosen quote, refund or progress detail, a received payment named no
//! mismatch, and started funds moved the session's own way.

use parity_scale_codec::Decode;
use truapi::latest::{
	FundingDirection, FundingFailure, FundingInboundUpdate, FundingOutboundUpdate, FundingUpdate,
};

use crate::{
	host_logic::funding::{
		FundingCancel, FundingSession, FundingStage, FundingUpdateRecord, StartedFunds,
	},
	platform::ProductId,
};

/// A session as format 3 stored it.
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
	updates: Vec<UpdateRecord>,
	started: Vec<Started>,
}

/// Started funds as formats 3 and 4 stored them, before they named their
/// direction.
#[derive(Decode)]
pub struct Started {
	host_id: [u8; 32],
	amount: u128,
	terms: [u8; 32],
	recorded_at_ms: u64,
	accepted: bool,
	named_at_ms: Option<u64>,
}

impl Started {
	/// These funds, moving the way of the session they were started for:
	/// before refunds, a session only started funds its own way.
	pub fn moving(self, direction: FundingDirection) -> StartedFunds {
		StartedFunds {
			direction,
			host_id: self.host_id,
			amount: self.amount,
			terms: self.terms,
			recorded_at_ms: self.recorded_at_ms,
			accepted: self.accepted,
			named_at_ms: self.named_at_ms,
		}
	}
}

#[derive(Decode)]
struct UpdateRecord {
	update: Update,
	at_ms: u64,
}

#[derive(Decode)]
enum Update {
	#[codec(index = 0)]
	In(Inbound),
	#[codec(index = 1)]
	Out(FundingOutboundUpdate),
	#[codec(index = 2)]
	Failed { reason: FundingFailure },
}

#[derive(Decode)]
enum Inbound {
	#[codec(index = 0)]
	AwaitingPayment,
	#[codec(index = 1)]
	PaymentReceived { finalized: bool },
	#[codec(index = 2)]
	Converting,
	#[codec(index = 3)]
	Crediting { top_up_id: [u8; 32], amount: u128 },
	#[codec(index = 4)]
	Delivered,
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
			updates: session.updates.into_iter().map(Into::into).collect(),
			started: session
				.started
				.into_iter()
				.map(|started| started.moving(session.direction))
				.collect(),
			choice: None,
			refund: None,
			detail: None,
		}
	}
}

impl From<UpdateRecord> for FundingUpdateRecord {
	fn from(record: UpdateRecord) -> Self {
		let update = match record.update {
			Update::In(inbound) => FundingUpdate::In(match inbound {
				Inbound::AwaitingPayment => FundingInboundUpdate::AwaitingPayment,
				Inbound::PaymentReceived { finalized } => {
					FundingInboundUpdate::PaymentReceived { finalized, mismatch: None }
				},
				Inbound::Converting => FundingInboundUpdate::Converting,
				Inbound::Crediting { top_up_id, amount } => {
					FundingInboundUpdate::Crediting { top_up_id, amount }
				},
				Inbound::Delivered => FundingInboundUpdate::Delivered,
			}),
			Update::Out(outbound) => FundingUpdate::Out(outbound),
			Update::Failed { reason } => FundingUpdate::Failed { reason },
		};
		Self { update, at_ms: record.at_ms }
	}
}

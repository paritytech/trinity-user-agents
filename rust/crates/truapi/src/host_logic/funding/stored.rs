//! How funding sessions are stored, and every earlier layout the core still
//! reads.
//!
//! The stored blob leads with a `u16` format. A change to how
//! [`FundingSession`] encodes bumps [`SESSIONS_FORMAT`] and keeps the layout it
//! replaces here, converted forward, so an upgrade keeps the sessions in
//! flight.

use parity_scale_codec::{Decode, Encode};
use truapi::latest::{FundingDirection, FundingFailure, FundingQuote, FundingRail, FundingUpdate};

use super::{FundingChoice, FundingSession, FundingStage, FundingUpdateRecord, StartedFunds};

/// Format of the stored sessions blob this core writes.
pub(super) const SESSIONS_FORMAT: u16 = 4;

/// Decode a stored sessions blob of any format this core reads, or `None`
/// when it is not one.
pub(super) fn decode_sessions(blob: &[u8]) -> Option<Vec<FundingSession>> {
	let input = &mut &blob[..];
	let sessions = match u16::decode(input).ok()? {
		1 => upgrade(Vec::<SessionV1>::decode(input).ok()?),
		2 => upgrade(Vec::<SessionV2>::decode(input).ok()?),
		3 => upgrade(Vec::<SessionV3>::decode(input).ok()?),
		SESSIONS_FORMAT => Vec::<FundingSession>::decode(input).ok()?,
		_ => return None,
	};
	input.is_empty().then_some(sessions)
}

fn upgrade<T: Into<FundingSession>>(sessions: Vec<T>) -> Vec<FundingSession> {
	sessions.into_iter().map(Into::into).collect()
}

/// A session as stored in format 1, before providers served sessions.
#[derive(Encode, Decode)]
pub(super) struct SessionV1 {
	pub(super) intent: String,
	pub(super) owner_product_id: Option<String>,
	pub(super) direction: FundingDirection,
	pub(super) amount: Option<u128>,
	pub(super) stage: FundingStage,
	pub(super) opened_at_ms: u64,
	pub(super) deadline_ms: u64,
	pub(super) acknowledged: bool,
}

/// A session as stored in format 2, before a session kept the provider's
/// quote and saved state.
#[derive(Encode, Decode)]
pub(super) struct SessionV2 {
	pub(super) intent: String,
	pub(super) owner_product_id: Option<String>,
	pub(super) direction: FundingDirection,
	pub(super) amount: Option<u128>,
	pub(super) stage: FundingStage,
	pub(super) opened_at_ms: u64,
	pub(super) deadline_ms: u64,
	pub(super) acknowledged: bool,
	pub(super) provider_id: Option<String>,
	pub(super) assigned_at_ms: Option<u64>,
	pub(super) cancel_requested: bool,
	pub(super) updates: Vec<UpdateRecordV2>,
	pub(super) started: Vec<StartedFunds>,
}

/// A provider update as stored in format 2.
#[derive(Encode, Decode)]
pub(super) struct UpdateRecordV2 {
	pub(super) update: UpdateV2,
	pub(super) at_ms: u64,
}

/// [`FundingUpdate`] as stored in format 2, before a received payment could
/// carry a mismatch.
#[derive(Encode, Decode)]
pub(super) enum UpdateV2 {
	AwaitingPayment,
	PaymentReceived { finalized: bool },
	Converting,
	Crediting { top_up_id: [u8; 32], amount: u128 },
	Delivered,
	Collecting { payment_id: [u8; 32], amount: u128 },
	Failed { reason: FundingFailure },
}

/// A session as stored in format 3, before a session kept a refund or the
/// network its quote was asked for. Its updates are read as they are now:
/// later formats only added variants after the ones it used.
#[derive(Encode, Decode)]
pub(super) struct SessionV3 {
	pub(super) intent: String,
	pub(super) owner_product_id: Option<String>,
	pub(super) direction: FundingDirection,
	pub(super) amount: Option<u128>,
	pub(super) stage: FundingStage,
	pub(super) opened_at_ms: u64,
	pub(super) deadline_ms: u64,
	pub(super) acknowledged: bool,
	pub(super) provider_id: Option<String>,
	pub(super) assigned_at_ms: Option<u64>,
	pub(super) cancel_requested: bool,
	pub(super) updates: Vec<FundingUpdateRecord>,
	pub(super) started: Vec<StartedFunds>,
	pub(super) choice: Option<ChoiceV3>,
	pub(super) saved: Option<Vec<u8>>,
}

/// [`FundingChoice`] as stored in format 3, without a network.
#[derive(Encode, Decode)]
pub(super) struct ChoiceV3 {
	pub(super) quote: FundingQuote,
	pub(super) rail: FundingRail,
	pub(super) asset: String,
}

impl From<SessionV1> for FundingSession {
	fn from(session: SessionV1) -> Self {
		SessionV2 {
			intent: session.intent,
			owner_product_id: session.owner_product_id,
			direction: session.direction,
			amount: session.amount,
			stage: session.stage,
			opened_at_ms: session.opened_at_ms,
			deadline_ms: session.deadline_ms,
			acknowledged: session.acknowledged,
			provider_id: None,
			assigned_at_ms: None,
			cancel_requested: false,
			updates: Vec::new(),
			started: Vec::new(),
		}
		.into()
	}
}

impl From<SessionV2> for FundingSession {
	fn from(session: SessionV2) -> Self {
		SessionV3 {
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
			cancel_requested: session.cancel_requested,
			updates: session.updates.into_iter().map(Into::into).collect(),
			started: session.started,
			choice: None,
			saved: None,
		}
		.into()
	}
}

impl From<SessionV3> for FundingSession {
	fn from(session: SessionV3) -> Self {
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
			cancel_requested: session.cancel_requested,
			updates: session.updates,
			started: session.started,
			choice: session.choice.map(|choice| FundingChoice {
				quote: choice.quote,
				rail: choice.rail,
				asset: choice.asset,
				network: None,
			}),
			saved: session.saved,
			refund: None,
		}
	}
}

impl From<UpdateRecordV2> for FundingUpdateRecord {
	fn from(record: UpdateRecordV2) -> Self {
		Self { update: record.update.into(), at_ms: record.at_ms }
	}
}

impl From<UpdateV2> for FundingUpdate {
	fn from(update: UpdateV2) -> Self {
		match update {
			UpdateV2::AwaitingPayment => Self::AwaitingPayment,
			UpdateV2::PaymentReceived { finalized } => {
				Self::PaymentReceived { finalized, mismatch: None }
			},
			UpdateV2::Converting => Self::Converting,
			UpdateV2::Crediting { top_up_id, amount } => Self::Crediting { top_up_id, amount },
			UpdateV2::Delivered => Self::Delivered,
			UpdateV2::Collecting { payment_id, amount } => Self::Collecting { payment_id, amount },
			UpdateV2::Failed { reason } => Self::Failed { reason },
		}
	}
}

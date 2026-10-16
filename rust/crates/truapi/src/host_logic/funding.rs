//! Core-owned funding sessions and their status machine.
//!
//! A session is one funding intent in flight. The core owns it, not the host
//! overlay that opened it, so dismissing the overlay after starting is not
//! cancelling: the session keeps running, persists across app death through
//! [`CoreStorageKey::FundingSessions`], and is re-attached by
//! `Funding::status_subscribe`. It expires on the core's own clock, whether or
//! not anyone watches it: 24 hours after it opened while no provider has taken
//! it, and 7 days after its provider's last update while no money moved. Once
//! money moved, only the provider ends the session, by settling it, or by
//! cancelling it on terms the user accepted.

use parity_scale_codec::{Decode, Encode};
use tracing::error;
use truapi::latest::{
	FundingAssignment, FundingDirection, FundingFailure, FundingFrameTarget, FundingUpdate,
	GenericError, HostFundingReportError, HostFundingServeSubscribeItem,
	HostFundingStatusSubscribeItem,
};
use url::Url;

use crate::platform::{CoreStorage, CoreStorageKey};

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has not recorded yet the core keeps,
/// newest first, so a host that never records them cannot grow the store
/// without bound.
const UNACKNOWLEDGED_LIMIT: usize = 200;
/// Most top-ups a provider may start for one session, as getcash claims in
/// up to three attempts.
const TOP_UP_LIMIT: usize = 3;
/// How long an assigned session with no funds moving waits for its
/// provider's next update before it expires, so a provider that went silent
/// cannot hold the session and its worker for good. A bank transfer can take
/// days, so this is longer than the unassigned window.
const PROVIDER_SILENCE_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has recorded the core keeps in full per
/// owner, newest first. Older ones are compacted into the session pack.
pub(crate) const RECORDED_PER_OWNER: usize = 64;
/// Format of the stored sessions blob, which leads it as a `u16`. The stored
/// enums pin their SCALE indexes, so variants added later decode alongside
/// the ones a blob was written with. A blob that leads with another format is
/// set aside rather than read.
const SESSIONS_FORMAT: u16 = 1;
/// Format of the stored session pack, which leads it as a `u16`, read like
/// the sessions blob.
const PACK_FORMAT: u16 = 1;
/// How many unreadable blobs [`CoreStorageKey::FundingSessionsUnreadable`]
/// keeps, oldest dropped first, so repeated bad writes cannot grow it without
/// bound. The core never reads them back: they are kept for diagnosis only.
const UNREADABLE_LIMIT: usize = 8;

/// An ended session the host recorded, compacted once newer recorded sessions
/// of its owner pushed it out of the full record. It keeps what history needs,
/// so its owner can still read how it ended and the host can still list it;
/// the pack is never trimmed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingSessionSummary {
	/// The session's identifier.
	pub intent: String,
	/// Product that opened the session, `None` when the host did.
	pub owner_product_id: Option<String>,
	/// Which way value crossed the boundary.
	pub direction: FundingDirection,
	/// Amount sought, in the user's payment balance units.
	pub amount: Option<u128>,
	/// How the session ended, with what moved and when.
	pub stage: FundingStage,
	/// When the session was opened, in Unix milliseconds.
	pub opened_at_ms: u64,
}

impl From<FundingSession> for FundingSessionSummary {
	fn from(session: FundingSession) -> Self {
		Self {
			intent: session.intent,
			owner_product_id: session.owner_product_id,
			direction: session.direction,
			amount: session.amount,
			stage: session.stage,
			opened_at_ms: session.opened_at_ms,
		}
	}
}

impl FundingSession {
	/// The session as history keeps it, leaving the session as it is.
	pub fn summary(&self) -> FundingSessionSummary {
		FundingSessionSummary {
			intent: self.intent.clone(),
			owner_product_id: self.owner_product_id.clone(),
			direction: self.direction,
			amount: self.amount,
			stage: self.stage.clone(),
			opened_at_ms: self.opened_at_ms,
		}
	}
}

impl FundingSessionSummary {
	/// The terminal item a subscriber receives.
	pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
		self.stage.wire_item(None)
	}

	/// When the session ended.
	pub fn settled_at_ms(&self) -> Option<u64> {
		self.stage.settled_at_ms()
	}
}

/// What the core knows about one session, independent of any host surface.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingSession {
	/// Identifier handed back to the caller and used to re-attach.
	pub intent: String,
	/// Product that opened the session and the only one that may watch it.
	/// `None` when the host opened it, as the Balance card does.
	pub owner_product_id: Option<String>,
	/// Which way value crosses the boundary.
	pub direction: FundingDirection,
	/// Amount sought, in the user's payment balance units. `None` until the
	/// user chooses one.
	pub amount: Option<u128>,
	/// Current stage.
	pub stage: FundingStage,
	/// When the session was opened, in Unix milliseconds.
	pub opened_at_ms: u64,
	/// When the session expires if no provider takes it, in Unix
	/// milliseconds.
	pub deadline_ms: u64,
	/// Whether the host has recorded the session's outcome in its own
	/// history. An ended session is handed to the host until it has.
	pub acknowledged: bool,
	/// Product id of the provider the user chose. `None` until they choose.
	pub provider_id: Option<String>,
	/// When the provider was assigned, in Unix milliseconds.
	pub assigned_at_ms: Option<u64>,
	/// Where the user's cancel stands, while one is pending or answered.
	pub cancel: Option<FundingCancel>,
	/// Every update the provider reported, oldest first.
	pub updates: Vec<FundingUpdateRecord>,
	/// The top-ups or payment requests the provider started while serving
	/// the session, oldest first. A `Crediting` or `Collecting` report must
	/// name one of them.
	pub started: Vec<StartedFunds>,
}

/// One update a provider reported, and when the core stored it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingUpdateRecord {
	/// The update.
	pub update: FundingUpdate,
	/// When it was stored, in Unix milliseconds.
	pub at_ms: u64,
}

/// A top-up or payment request the provider started with `payment.topUp` or
/// `payment.request` while serving a session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct StartedFunds {
	/// The id the host received, with the provider's product hashed in.
	pub host_id: [u8; 32],
	/// Amount it asks to move, which the session settles from.
	pub amount: u128,
	/// When a report named it, in Unix milliseconds. A record is named at
	/// most once.
	pub named_at_ms: Option<u64>,
}

/// Where a user's cancel of a served session stands.
///
/// Before money moved, a cancel asks the provider to stop and a second one
/// ends the session. After, a cancel is a request the provider answers with
/// an offer or a refusal, and only the provider ends the session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingCancel {
	/// The user asked to cancel and the provider has not answered.
	#[codec(index = 0)]
	Requested,
	/// The provider will not cancel. The session carries on.
	#[codec(index = 1)]
	Refused {
		/// Why, as the provider put it.
		reason: String,
	},
	/// The provider can cancel on these terms and waits for the user.
	#[codec(index = 2)]
	Offered {
		/// What the provider keeps, in payment balance units.
		fee: u128,
		/// What the user gets back, in payment balance units.
		refund: u128,
		/// When the offer lapses, in Unix milliseconds.
		expires_at: Option<u64>,
	},
	/// The user accepted the offer. The provider settles the cancel.
	#[codec(index = 3)]
	Accepted {
		/// What the provider keeps, in payment balance units.
		fee: u128,
		/// What the user gets back, in payment balance units.
		refund: u128,
	},
	/// The user declined the offer, or it lapsed. The session carries on.
	#[codec(index = 4)]
	Declined,
}

/// What a user's cancel did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum CancelOutcome {
	/// The session ended as cancelled: no provider serves it, or no money
	/// moved and its provider was asked once already.
	Ended,
	/// The provider serving it was asked to cancel.
	Requested,
	/// A cancel is already with the provider, or accepted and being settled.
	Pending,
	/// Too late: the provider already started crediting the user's balance.
	TooLate,
	/// Not cancellable here: the session ended, or its overlay is still up.
	Refused,
}

/// What the user's answer to a cancel offer did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum CancelConfirmation {
	/// The offer was accepted and passed to the provider to settle.
	Accepted,
	/// The offer was declined. The session carries on.
	Declined,
	/// The offer had lapsed, so it was declined.
	Lapsed,
	/// The session has no offer to answer.
	NoOffer,
	/// The offer changed since the user saw it, so nothing was accepted. The
	/// host shows the current offer and asks again.
	Changed,
	/// Too late: the provider started crediting the user, so the offer is
	/// declined and the session carries on.
	TooLate,
}

/// The terms of a cancel offer the user accepts, as the host showed them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct CancelTerms {
	/// What the provider keeps, in payment balance units.
	pub fee: u128,
	/// What the user gets back, in payment balance units.
	pub refund: u128,
}

/// What the core follows to decide a session's outcome once funds move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settlement {
	/// The top-ups an inbound session credits through.
	TopUps(Vec<StartedFunds>),
	/// The payment an outbound session collects through.
	Payment(StartedFunds),
}

/// Stage of a session, as the core persists it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingStage {
	/// In flight.
	#[codec(index = 0)]
	Open,
	/// Ended without success.
	#[codec(index = 1)]
	Failed {
		/// Why it ended.
		reason: FundingFailure,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
	/// Inbound success.
	#[codec(index = 2)]
	Delivered {
		/// Amount credited, which may differ from the amount requested.
		credited: u128,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
	/// Outbound success.
	#[codec(index = 3)]
	Released {
		/// Amount debited.
		debited: u128,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
}

impl FundingStage {
	/// When the session ended, if it has.
	fn settled_at_ms(&self) -> Option<u64> {
		match self {
			Self::Open => None,
			Self::Failed { settled_at_ms, .. } |
			Self::Delivered { settled_at_ms, .. } |
			Self::Released { settled_at_ms, .. } => Some(*settled_at_ms),
		}
	}

	/// The wire item subscribers receive for this stage; an open stage
	/// reports `expires_at`.
	fn wire_item(&self, expires_at: Option<u64>) -> HostFundingStatusSubscribeItem {
		match self {
			Self::Open => HostFundingStatusSubscribeItem::InProgress { expires_at },
			Self::Failed { reason, .. } => {
				HostFundingStatusSubscribeItem::Failed { reason: reason.clone(), moved: 0 }
			},
			Self::Delivered { credited, .. } => {
				HostFundingStatusSubscribeItem::Delivered { credited: *credited }
			},
			Self::Released { debited, .. } => {
				HostFundingStatusSubscribeItem::Released { debited: *debited }
			},
		}
	}
}

impl FundingSession {
	/// Open a session that expires one window from `now_ms` unless a
	/// provider takes it.
	pub fn new(
		intent: String,
		owner_product_id: Option<String>,
		direction: FundingDirection,
		amount: Option<u128>,
		now_ms: u64,
	) -> Self {
		Self {
			intent,
			owner_product_id,
			direction,
			amount,
			stage: FundingStage::Open,
			opened_at_ms: now_ms,
			deadline_ms: now_ms.saturating_add(SESSION_WINDOW_MS),
			acknowledged: false,
			provider_id: None,
			assigned_at_ms: None,
			cancel: None,
			updates: Vec::new(),
			started: Vec::new(),
		}
	}

	/// Assign the session to the provider the user chose. Returns whether it
	/// was open and not yet assigned.
	pub fn assign(&mut self, provider_id: &str, now_ms: u64) -> bool {
		if self.is_terminal() || self.provider_id.is_some() {
			return false;
		}
		self.provider_id = Some(provider_id.to_string());
		self.assigned_at_ms = Some(now_ms);
		true
	}

	/// The session as the provider serving it receives it.
	pub fn assignment(&self) -> FundingAssignment {
		FundingAssignment {
			intent: self.intent.clone(),
			direction: self.direction,
			amount: self.amount,
			expires_at: self.expires_at(),
			last_update: self.last_step().cloned(),
		}
	}

	/// The provider's most recent update.
	pub fn last_update(&self) -> Option<&FundingUpdate> {
		self.updates.last().map(|record| &record.update)
	}

	/// The provider's most recent update that moves the session on, which an
	/// answer to a cancel does not.
	pub fn last_step(&self) -> Option<&FundingUpdate> {
		self.updates
			.iter()
			.rev()
			.map(|record| &record.update)
			.find(|update| update_rank(update).is_some())
	}

	/// Store `update` from `provider_id`. A `Failed` update ends the session.
	///
	/// `host_id` turns a top-up or payment id the provider chose into the id
	/// the host received; a `Crediting` or `Collecting` update must name, by
	/// that id and amount, funds the provider started for this session and no
	/// report named yet.
	pub fn report(
		&mut self,
		provider_id: &str,
		update: FundingUpdate,
		host_id: impl Fn(&[u8; 32]) -> [u8; 32],
		now_ms: u64,
	) -> Result<(), HostFundingReportError> {
		if self.is_terminal() || self.provider_id.as_deref() != Some(provider_id) {
			return Err(HostFundingReportError::NotFound);
		}
		if let Some(answer) = cancel_answer(&update) {
			if !matches!(
				self.cancel,
				Some(FundingCancel::Requested | FundingCancel::Offered { .. })
			) {
				return Err(HostFundingReportError::OutOfOrder);
			}
			if let FundingCancel::Offered { fee, refund, .. } = answer {
				self.check_offer(fee, refund)?;
			}
			self.cancel = Some(answer);
			self.updates.push(FundingUpdateRecord { update, at_ms: now_ms });
			return Ok(());
		}
		if !self.follows(&update) {
			return Err(HostFundingReportError::OutOfOrder);
		}
		if let Some((id, amount)) = named_funds(&update) {
			let named = host_id(id);
			let record = self
				.started
				.iter_mut()
				.find(|record| record.host_id == named && record.named_at_ms.is_none())
				.filter(|record| record.amount == amount)
				.ok_or(HostFundingReportError::NotStarted)?;
			record.named_at_ms = Some(now_ms);
		}
		if let FundingUpdate::Failed { reason } = &update {
			self.stage = FundingStage::Failed { reason: reason.clone(), settled_at_ms: now_ms };
		}
		self.updates.push(FundingUpdateRecord { update, at_ms: now_ms });
		// A provider that reports its next step has carried on, so the decline
		// it was owed reached it and is not repeated.
		if self.is_terminal() || matches!(self.cancel, Some(FundingCancel::Declined)) {
			self.cancel = None;
		}
		Ok(())
	}

	/// Whether the provider may offer to cancel on these terms. An inbound
	/// session whose credit started can no longer be cancelled, and an offer
	/// cannot hand back more than the session moves. An outbound offer waits
	/// for the refund a released withdrawal settles through.
	fn check_offer(&self, fee: u128, refund: u128) -> Result<(), HostFundingReportError> {
		if self.direction == FundingDirection::Out ||
			(self.direction == FundingDirection::In && self.funds_started())
		{
			return Err(HostFundingReportError::OutOfOrder);
		}
		match self.amount {
			Some(amount) if fee.saturating_add(refund) > amount => {
				Err(HostFundingReportError::Unknown {
					reason: "the offer exceeds the session amount".to_string(),
				})
			},
			_ => Ok(()),
		}
	}

	/// Whether `update` may come next. Updates only move forward, a session
	/// may credit through a few top-ups, and once funds move the provider
	/// can no longer fail it: the top-ups or payment decide.
	fn follows(&self, update: &FundingUpdate) -> bool {
		let fits_direction = match update {
			FundingUpdate::Collecting { .. } => self.direction == FundingDirection::Out,
			FundingUpdate::Failed { .. } => true,
			_ => self.direction == FundingDirection::In,
		};
		let last = self.last_step().and_then(update_rank);
		let top_ups = self.top_up_ids();
		fits_direction &&
			match update {
				FundingUpdate::Failed { .. } => !self.funds_started(),
				FundingUpdate::Crediting { top_up_id, .. } => {
					last <= update_rank(update) &&
						top_ups.len() < TOP_UP_LIMIT &&
						!top_ups.contains(&top_up_id)
				},
				FundingUpdate::Delivered => last < update_rank(update) && !top_ups.is_empty(),
				_ => last < update_rank(update),
			}
	}

	/// The ids of the top-ups the provider named, in order.
	fn top_up_ids(&self) -> Vec<&[u8; 32]> {
		self.updates
			.iter()
			.filter_map(|record| match &record.update {
				FundingUpdate::Crediting { top_up_id, .. } => Some(top_up_id),
				_ => None,
			})
			.collect()
	}

	/// Record funds the serving provider started under `host_id`, unless
	/// they are recorded already. Returns whether this recorded them.
	pub fn record_start(&mut self, host_id: [u8; 32], amount: u128) -> bool {
		if self.is_terminal() ||
			self.provider_id.is_none() ||
			self.started.iter().any(|record| record.host_id == host_id)
		{
			return false;
		}
		self.started.push(StartedFunds { host_id, amount, named_at_ms: None });
		true
	}

	/// Drop the record of funds under `host_id` that no report named, as the
	/// host refused them or another session named them.
	pub fn forget_start(&mut self, host_id: &[u8; 32]) {
		self.started
			.retain(|record| record.host_id != *host_id || record.named_at_ms.is_some());
	}

	/// Whether funds are moving: the provider named a top-up or a payment
	/// request it started. The named funds then decide the outcome.
	pub fn funds_moving(&self) -> bool {
		self.updates.iter().any(|record| named_funds(&record.update).is_some())
	}

	/// Whether the provider started a top-up or payment request for the
	/// session, named or not. A worker can stop between starting funds and
	/// naming them, so from then on the provider cannot fail the session.
	fn funds_started(&self) -> bool {
		!self.started.is_empty() || self.funds_moving()
	}

	/// Whether money moved for the session: the provider started funds for
	/// it or saw the user's payment. From then on nothing but the provider
	/// ends the session: a cancel is a request it answers, and no deadline
	/// expires it.
	pub fn money_moved(&self) -> bool {
		self.funds_started() ||
			self.updates
				.iter()
				.any(|record| matches!(record.update, FundingUpdate::PaymentReceived { .. }))
	}

	/// What the core must follow to end the session, once the provider has
	/// handed the outcome over.
	pub fn settlement_due(&self) -> Option<Settlement> {
		if self.is_terminal() {
			return None;
		}
		let mut named = self.started.iter().filter(|record| record.named_at_ms.is_some());
		// The last step, not the last update: an answer to a cancel or a
		// detail must not hide the outcome the provider handed over.
		match self.last_step()? {
			FundingUpdate::Delivered => Some(Settlement::TopUps(named.cloned().collect())),
			FundingUpdate::Collecting { .. } => named.next().cloned().map(Settlement::Payment),
			_ => None,
		}
	}

	/// End the session with what its top-ups claimed or its payment moved.
	/// Nothing moved fails it. Returns whether it changed.
	pub fn settle(&mut self, moved: u128, now_ms: u64) -> bool {
		if self.is_terminal() {
			return false;
		}
		self.stage = match (moved, self.direction) {
			(0, FundingDirection::In) => FundingStage::Failed {
				reason: FundingFailure::Other {
					code: "not_claimed".to_string(),
					message: "Nothing reached your balance".to_string(),
				},
				settled_at_ms: now_ms,
			},
			(0, FundingDirection::Out) => FundingStage::Failed {
				reason: FundingFailure::Other {
					code: "payment_failed".to_string(),
					message: "Your payment did not go through".to_string(),
				},
				settled_at_ms: now_ms,
			},
			(credited, FundingDirection::In) => {
				FundingStage::Delivered { credited, settled_at_ms: now_ms }
			},
			(debited, FundingDirection::Out) => {
				FundingStage::Released { debited, settled_at_ms: now_ms }
			},
		};
		self.cancel = None;
		true
	}

	/// Whether the host still has to hear of the session when funding
	/// resumes: it is in flight, or it ended and the host has not recorded
	/// the outcome.
	pub fn needs_handoff(&self) -> bool {
		!self.is_terminal() || !self.acknowledged
	}

	/// Cancel at the user's request.
	///
	/// With no provider serving it, the session ends. Before money moved, the
	/// provider is asked to stop, and a second cancel ends the session, so a
	/// provider that went silent cannot hold it. Once money moved, a cancel
	/// is a request the provider answers with an offer or a refusal, and it
	/// never ends the session itself. An inbound session whose credit
	/// started is too late to cancel.
	pub fn cancel(&mut self, now_ms: u64) -> CancelOutcome {
		if self.is_terminal() {
			return CancelOutcome::Refused;
		}
		if self.provider_id.is_none() {
			self.fail(FundingFailure::Cancelled, now_ms);
			return CancelOutcome::Ended;
		}
		if !self.money_moved() {
			if self.cancel.is_some() {
				self.fail(FundingFailure::Cancelled, now_ms);
				return CancelOutcome::Ended;
			}
			self.cancel = Some(FundingCancel::Requested);
			return CancelOutcome::Requested;
		}
		if self.direction == FundingDirection::In && self.funds_started() {
			return CancelOutcome::TooLate;
		}
		match self.cancel {
			Some(
				FundingCancel::Requested |
				FundingCancel::Offered { .. } |
				FundingCancel::Accepted { .. },
			) => CancelOutcome::Pending,
			Some(FundingCancel::Refused { .. } | FundingCancel::Declined) | None => {
				self.cancel = Some(FundingCancel::Requested);
				CancelOutcome::Requested
			},
		}
	}

	/// Answer the provider's cancel offer: accept it on `terms`, the terms
	/// the host showed, so the provider settles the cancel, or decline it with
	/// `None`, so the session carries on. An offer past its expiry is declined
	/// whatever the answer, an offer whose terms changed since is not
	/// accepted, and an inbound session whose credit started is too late.
	pub fn confirm_cancel(
		&mut self,
		terms: Option<CancelTerms>,
		now_ms: u64,
	) -> CancelConfirmation {
		if self.is_terminal() {
			return CancelConfirmation::NoOffer;
		}
		let Some(FundingCancel::Offered { fee, refund, expires_at }) = self.cancel else {
			return CancelConfirmation::NoOffer;
		};
		if expires_at.is_some_and(|expires_at| now_ms >= expires_at) {
			self.cancel = Some(FundingCancel::Declined);
			return CancelConfirmation::Lapsed;
		}
		let Some(terms) = terms else {
			self.cancel = Some(FundingCancel::Declined);
			return CancelConfirmation::Declined;
		};
		if self.direction == FundingDirection::In && self.funds_started() {
			self.cancel = Some(FundingCancel::Declined);
			return CancelConfirmation::TooLate;
		}
		if terms != (CancelTerms { fee, refund }) {
			return CancelConfirmation::Changed;
		}
		self.cancel = Some(FundingCancel::Accepted { fee, refund });
		CancelConfirmation::Accepted
	}

	/// Decline the provider's cancel offer once it lapsed. Returns whether it
	/// did.
	pub fn lapse_cancel_offer(&mut self, now_ms: u64) -> bool {
		let lapsed = self.cancel_offer_expires_at().is_some_and(|expires_at| now_ms >= expires_at);
		if lapsed {
			self.cancel = Some(FundingCancel::Declined);
		}
		lapsed
	}

	/// When the provider's open cancel offer lapses, if it has one that does.
	fn cancel_offer_expires_at(&self) -> Option<u64> {
		match self.cancel {
			Some(FundingCancel::Offered { expires_at, .. }) if !self.is_terminal() => expires_at,
			_ => None,
		}
	}

	/// The next time the session changes on its own: it expires, or its
	/// cancel offer lapses.
	pub fn next_alarm_ms(&self) -> Option<u64> {
		match (self.expires_at(), self.cancel_offer_expires_at()) {
			(Some(expires), Some(lapses)) => Some(expires.min(lapses)),
			(expires, lapses) => expires.or(lapses),
		}
	}

	/// What the provider still has to hear about the user's cancel when its
	/// serve stream opens: a request it has not answered, or an answer to its
	/// offer. A decline is repeated on every open until the provider reports
	/// its next step, as a worker that stopped before handling it would
	/// otherwise wait for an answer that never comes.
	pub fn pending_cancel_item(&self) -> Option<HostFundingServeSubscribeItem> {
		let intent = self.intent.clone();
		match self.cancel.as_ref()? {
			FundingCancel::Requested => Some(HostFundingServeSubscribeItem::Cancel { intent }),
			// The provider answered already; asking again would invite a new
			// offer while the user looks at this one.
			FundingCancel::Offered { .. } => None,
			FundingCancel::Accepted { .. } => {
				Some(HostFundingServeSubscribeItem::CancelAccepted { intent })
			},
			FundingCancel::Declined => {
				Some(HostFundingServeSubscribeItem::CancelDeclined { intent })
			},
			FundingCancel::Refused { .. } => None,
		}
	}

	/// Whether the session has ended.
	pub fn is_terminal(&self) -> bool {
		self.settled_at_ms().is_some()
	}

	/// When the session ended, if it has.
	pub fn settled_at_ms(&self) -> Option<u64> {
		self.stage.settled_at_ms()
	}

	/// Project the current stage onto the wire item subscribers receive.
	pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
		self.stage.wire_item(self.expires_at())
	}

	/// End the session with `reason`, unless it already ended. Returns whether
	/// it changed.
	pub fn fail(&mut self, reason: FundingFailure, now_ms: u64) -> bool {
		if self.is_terminal() {
			return false;
		}
		self.stage = FundingStage::Failed { reason, settled_at_ms: now_ms };
		self.cancel = None;
		true
	}

	/// Expire the session if it is still open at [`Self::expires_at`].
	/// Returns whether it expired.
	pub fn expire_if_due(&mut self, now_ms: u64) -> bool {
		self.expires_at().is_some_and(|expires_at| now_ms >= expires_at) &&
			self.fail(FundingFailure::Expired, now_ms)
	}

	/// When the open session expires if nothing changes: 24 hours after it
	/// opened while no provider has taken it, then 7 days after its
	/// provider's last update until money moves. `None` once it ended or
	/// money moved: expiring it then would write off money in flight.
	pub fn expires_at(&self) -> Option<u64> {
		if self.is_terminal() || self.money_moved() {
			return None;
		}
		let Some(assigned_at_ms) = self.assigned_at_ms else {
			return Some(self.deadline_ms);
		};
		let heard_at_ms = self.updates.last().map_or(assigned_at_ms, |record| record.at_ms);
		Some(heard_at_ms.max(assigned_at_ms).saturating_add(PROVIDER_SILENCE_MS))
	}
}

/// The provider's id and amount of the funds `update` names, if any.
pub fn named_funds(update: &FundingUpdate) -> Option<(&[u8; 32], u128)> {
	match update {
		FundingUpdate::Crediting { top_up_id, amount } => Some((top_up_id, *amount)),
		FundingUpdate::Collecting { payment_id, amount } => Some((payment_id, *amount)),
		_ => None,
	}
}

/// Order of an update within a session: a later update ranks higher, and a
/// top-up and a payment request rank where funds start moving. An answer to
/// a cancel has no place in the order.
fn update_rank(update: &FundingUpdate) -> Option<u8> {
	Some(match update {
		FundingUpdate::AwaitingPayment => 0,
		FundingUpdate::PaymentReceived { finalized: false } => 1,
		FundingUpdate::PaymentReceived { finalized: true } => 2,
		FundingUpdate::Converting => 3,
		FundingUpdate::Crediting { .. } | FundingUpdate::Collecting { .. } => 4,
		FundingUpdate::Delivered => 5,
		FundingUpdate::Failed { .. } => 6,
		FundingUpdate::CancelRefused { .. } | FundingUpdate::CancelOffer { .. } => return None,
	})
}

/// Where the user's cancel stands after `update`, when it answers one.
fn cancel_answer(update: &FundingUpdate) -> Option<FundingCancel> {
	match update {
		FundingUpdate::CancelRefused { reason } => {
			Some(FundingCancel::Refused { reason: reason.clone() })
		},
		FundingUpdate::CancelOffer { fee, refund, expires_at } => {
			Some(FundingCancel::Offered { fee: *fee, refund: *refund, expires_at: *expires_at })
		},
		_ => None,
	}
}

/// Why a session operation failed.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum FundingSessionError {
	/// The host's storage callback failed.
	#[display("funding session storage failed: {reason}")]
	Storage {
		/// Failure detail.
		reason: String,
	},
	/// The chosen provider is not a valid product id.
	#[display("invalid funding provider: {reason}")]
	InvalidProvider {
		/// Why the id was refused.
		reason: String,
	},
}

/// `target` as the host may show it: an App route must stay inside the
/// provider's App, and a page and its return URL must be https URLs with a
/// host and no credentials. A page is passed on as parsed, so the host opens
/// exactly what was checked.
pub fn checked_frame_target(target: FundingFrameTarget) -> Option<FundingFrameTarget> {
	match target {
		FundingFrameTarget::App { route } => {
			app_route_stays_inside(&route).then_some(FundingFrameTarget::App { route })
		},
		FundingFrameTarget::Page { url, return_url } => Some(FundingFrameTarget::Page {
			url: checked_page_url(&url)?,
			return_url: match return_url {
				Some(return_url) => Some(checked_page_url(&return_url)?),
				None => None,
			},
		}),
	}
}

/// Whether `route`, resolved against the provider's App, stays on it: no
/// scheme, no host and no scheme-relative `//`.
fn app_route_stays_inside(route: &str) -> bool {
	const APP: &str = "https://app.invalid/";
	let Ok(base) = Url::parse(APP) else {
		return false;
	};
	base.join(route)
		.is_ok_and(|joined| joined.scheme() == "https" && joined.host_str() == Some("app.invalid"))
}

fn checked_page_url(raw: &str) -> Option<String> {
	let url = Url::parse(raw).ok()?;
	let plain = url.scheme() == "https" &&
		url.host().is_some() &&
		url.username().is_empty() &&
		url.password().is_none();
	plain.then(|| url.into())
}

impl From<FundingSessionError> for GenericError {
	fn from(error: FundingSessionError) -> Self {
		Self { reason: error.to_string() }
	}
}

impl<D> From<FundingSessionError> for crate::CallError<D> {
	fn from(error: FundingSessionError) -> Self {
		Self::HostFailure { reason: error.to_string() }
	}
}

/// Split `sessions` into those kept in full and the recorded ones to compact.
/// Kept in full: every open one, the ended ones the host has not recorded yet,
/// newest first up to a fixed bound, and the newest ended ones it has recorded,
/// up to a fixed bound per owner. Older recorded ones go to the pack.
pub fn retained(
	sessions: impl IntoIterator<Item = FundingSession>,
) -> (Vec<FundingSession>, Vec<FundingSessionSummary>) {
	let (mut open, ended): (Vec<_>, Vec<_>) =
		sessions.into_iter().partition(|session| !session.is_terminal());
	let (mut unrecorded, mut recorded): (Vec<_>, Vec<_>) =
		ended.into_iter().partition(|session| !session.acknowledged);
	unrecorded.sort_by_key(|session| core::cmp::Reverse(session.settled_at_ms()));
	unrecorded.truncate(UNACKNOWLEDGED_LIMIT);
	// Grouped by owner, newest first within each, so each owner's newest run is
	// what survives the per-owner bound.
	recorded.sort_by(|a, b| {
		a.owner_product_id
			.cmp(&b.owner_product_id)
			.then_with(|| b.settled_at_ms().cmp(&a.settled_at_ms()))
	});
	open.append(&mut unrecorded);
	let mut owner: Option<Option<String>> = None;
	let mut owner_run = 0;
	let (newest, older): (Vec<_>, Vec<_>) = recorded.into_iter().partition(|session| {
		if owner.as_ref() == Some(&session.owner_product_id) {
			owner_run += 1;
		} else {
			owner = Some(session.owner_product_id.clone());
			owner_run = 1;
		}
		owner_run <= RECORDED_PER_OWNER
	});
	open.extend(newest);
	(open, older.into_iter().map(FundingSessionSummary::from).collect())
}

/// Read every persisted session.
///
/// A blob that does not decode is set aside and reads as no sessions; see
/// [`load_versioned`].
pub async fn load_sessions(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSession>, FundingSessionError> {
	load_versioned(storage, CoreStorageKey::FundingSessions, SESSIONS_FORMAT).await
}

/// Read the session pack, in no particular order.
///
/// A blob that does not decode is set aside and reads as an empty pack; see
/// [`load_versioned`].
pub async fn load_pack(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSessionSummary>, FundingSessionError> {
	load_versioned(storage, CoreStorageKey::FundingSessionPack, PACK_FORMAT).await
}

/// Read the list stored under `key`, which leads with `format`.
///
/// A blob that does not decode is appended to the list kept under
/// [`CoreStorageKey::FundingSessionsUnreadable`] and reads as an empty list,
/// so one bad write cannot disable funding for good, the next store cannot
/// overwrite records it never read, and a later bad blob does not replace an
/// earlier one. If it cannot be set aside, loading fails and the blob stays
/// where it is.
async fn load_versioned<T: Decode>(
	storage: &(impl CoreStorage + ?Sized),
	key: CoreStorageKey,
	format: u16,
) -> Result<Vec<T>, FundingSessionError> {
	let storage_error = |err: GenericError| FundingSessionError::Storage { reason: err.reason };
	let Some(blob) = storage.read_core_storage(key.clone()).await.map_err(storage_error)? else {
		return Ok(Vec::new());
	};
	if let Some(records) = decode_versioned(&blob, format) {
		return Ok(records);
	}
	error!(?key, bytes = blob.len(), "funding records do not decode; keeping them aside");
	let mut aside = match storage
		.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)
		.await
		.map_err(storage_error)?
	{
		Some(kept) => Vec::<Vec<u8>>::decode(&mut &kept[..]).unwrap_or_else(|_| vec![kept]),
		None => Vec::new(),
	};
	aside.push(blob);
	if aside.len() > UNREADABLE_LIMIT {
		aside.drain(..aside.len() - UNREADABLE_LIMIT);
	}
	storage
		.write_core_storage(CoreStorageKey::FundingSessionsUnreadable, aside.encode())
		.await
		.map_err(storage_error)?;
	Ok(Vec::new())
}

/// Decode a stored list that leads with `format`, or `None` when the blob is
/// not one.
fn decode_versioned<T: Decode>(blob: &[u8], format: u16) -> Option<Vec<T>> {
	let input = &mut &blob[..];
	if u16::decode(input).ok()? != format {
		return None;
	}
	let records = Vec::<T>::decode(input).ok()?;
	input.is_empty().then_some(records)
}

/// Replace the persisted set with `sessions`, clearing the slot when empty.
pub async fn store_sessions(
	storage: &(impl CoreStorage + ?Sized),
	sessions: &[&FundingSession],
) -> Result<(), FundingSessionError> {
	store_versioned(storage, CoreStorageKey::FundingSessions, SESSIONS_FORMAT, sessions).await
}

/// Encode `pack` as [`store_pack`] writes it.
pub fn pack_blob<'a>(pack: impl IntoIterator<Item = &'a FundingSessionSummary>) -> Vec<u8> {
	(PACK_FORMAT, pack.into_iter().collect::<Vec<_>>()).encode()
}

/// Replace the persisted pack with `blob`, built by [`pack_blob`].
pub async fn store_pack(
	storage: &(impl CoreStorage + ?Sized),
	blob: Vec<u8>,
) -> Result<(), FundingSessionError> {
	storage
		.write_core_storage(CoreStorageKey::FundingSessionPack, blob)
		.await
		.map_err(|err| FundingSessionError::Storage { reason: err.reason })
}

/// Write `records` under `key`, led by `format`, clearing the slot when empty.
async fn store_versioned<T: Encode>(
	storage: &(impl CoreStorage + ?Sized),
	key: CoreStorageKey,
	format: u16,
	records: &[&T],
) -> Result<(), FundingSessionError> {
	let written = if records.is_empty() {
		storage.clear_core_storage(key).await
	} else {
		storage.write_core_storage(key, (format, records).encode()).await
	};
	written.map_err(|err| FundingSessionError::Storage { reason: err.reason })
}

#[cfg(test)]
mod tests {
	use super::*;

	use futures::executor::block_on;

	use crate::test_support::stub_platform;

	const NOW: u64 = 1_700_000_000_000;
	const PROVIDER: &str = "ramp.dot";

	// The host opens a provider page in its own frame, so only an https page
	// it can show the origin of reaches it, never one carrying credentials
	// or another scheme, and the return URL is held to the same rule.
	#[test]
	fn a_frame_opens_only_plain_https_pages() {
		let page = |url: &str, return_url: Option<&str>| {
			checked_frame_target(FundingFrameTarget::Page {
				url: url.to_string(),
				return_url: return_url.map(str::to_string),
			})
		};
		let widget = "https://widget.example.com/buy?amount=10&token=a%2Fb";

		assert_eq!(
			(
				page(widget, Some("https://onramp.example.com/meld/return?flow=sell")),
				page("http://widget.example.com/", None),
				page("https://user:pass@widget.example.com/", None),
				page("javascript:alert(1)", None),
				page(widget, Some("polkadotapp://done")),
				checked_frame_target(FundingFrameTarget::App { route: "/kyc".to_string() }),
			),
			(
				Some(FundingFrameTarget::Page {
					url: widget.to_string(),
					return_url: Some(
						"https://onramp.example.com/meld/return?flow=sell".to_string()
					),
				}),
				None,
				None,
				None,
				None,
				Some(FundingFrameTarget::App { route: "/kyc".to_string() }),
			)
		);
	}

	#[test]
	fn an_app_route_stays_inside_the_provider_app() {
		let route = |route: &str| {
			checked_frame_target(FundingFrameTarget::App { route: route.to_string() }).is_some()
		};

		assert_eq!(
			[
				route("/kyc"),
				route("deposit?step=2"),
				route("//evil.example/x"),
				route("https://evil.example/"),
				route("javascript:alert(1)"),
				route("\\\\evil.example"),
			],
			[true, true, false, false, false, false]
		);
	}

	fn session(direction: FundingDirection) -> FundingSession {
		FundingSession::new(
			"fs_1".to_string(),
			Some("wallet.dot".to_string()),
			direction,
			Some(100),
			NOW,
		)
	}

	fn expired(intent: &str, settled_at_ms: u64) -> FundingSession {
		FundingSession {
			intent: intent.to_string(),
			stage: FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms },
			..session(FundingDirection::In)
		}
	}

	// Each owner keeps its newest recorded sessions in full, so no owner can
	// crowd out another, and its older ones are compacted rather than dropped.
	#[test]
	fn recorded_sessions_keep_the_newest_per_owner_and_compact_the_rest() {
		let recorded = |intent: String, owner: &str, settled_at_ms: u64| FundingSession {
			acknowledged: true,
			owner_product_id: Some(owner.to_string()),
			..expired(&intent, settled_at_ms)
		};
		let mut sessions: Vec<FundingSession> = (0..RECORDED_PER_OWNER as u64 + 2)
			.map(|n| recorded(format!("a_{n}"), "a.dot", NOW + n))
			.collect();
		sessions.push(recorded("b_0".to_string(), "b.dot", NOW));
		sessions.push(session(FundingDirection::In));

		let (kept, compacted) = retained(sessions);
		let count = |owner: &str| {
			kept.iter()
				.filter(|session| session.owner_product_id.as_deref() == Some(owner))
				.count()
		};

		assert_eq!(
			(
				count("a.dot"),
				count("b.dot"),
				kept.iter().any(|session| session.intent == "a_0" || session.intent == "a_1"),
				kept.iter().any(|session| !session.is_terminal()),
				compacted,
			),
			(
				RECORDED_PER_OWNER,
				1,
				false,
				true,
				vec![
					FundingSessionSummary::from(recorded("a_1".to_string(), "a.dot", NOW + 1)),
					FundingSessionSummary::from(recorded("a_0".to_string(), "a.dot", NOW)),
				]
			)
		);
	}

	// A product only waits for the outcome; the host draws the steps, so an
	// open session reads the same whichever way value moves.
	#[test]
	fn an_open_session_reads_in_progress_until_its_deadline() {
		let in_progress = HostFundingStatusSubscribeItem::InProgress {
			expires_at: Some(NOW + SESSION_WINDOW_MS),
		};
		assert_eq!(
			(session(FundingDirection::In).wire_item(), session(FundingDirection::Out).wire_item(),),
			(in_progress.clone(), in_progress)
		);
	}

	// Nothing else ends a session that hears nothing, so expiry is what
	// guarantees every subscriber eventually gets a terminal item.
	#[test]
	fn a_session_expires_at_its_deadline_and_not_before() {
		let mut session = session(FundingDirection::In);

		assert!(!session.expire_if_due(NOW + SESSION_WINDOW_MS - 1));
		assert!(session.expire_if_due(NOW + SESSION_WINDOW_MS));
		assert_eq!(
			session.wire_item(),
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired, moved: 0 }
		);
	}

	// A bank transfer can take days, so a provider that keeps reporting holds
	// the session past the first day; one that went silent for a week loses
	// it, so it cannot hold the session and its worker for good.
	#[test]
	fn an_assigned_session_expires_a_week_after_its_provider_last_reported() {
		let mut session = served(FundingDirection::In);
		let heard_at = NOW + SESSION_WINDOW_MS;
		session
			.report(PROVIDER, FundingUpdate::AwaitingPayment, |id| *id, heard_at)
			.expect("reported");

		let before = session.expire_if_due(heard_at + PROVIDER_SILENCE_MS - 1);
		let shown = session.wire_item();
		let at = session.expire_if_due(heard_at + PROVIDER_SILENCE_MS);

		assert_eq!(
			(before, shown, at, session.wire_item()),
			(
				false,
				HostFundingStatusSubscribeItem::InProgress {
					expires_at: Some(heard_at + PROVIDER_SILENCE_MS)
				},
				true,
				HostFundingStatusSubscribeItem::Failed {
					reason: FundingFailure::Expired,
					moved: 0
				},
			)
		);
	}

	// Once the provider holds the user's payment, expiring the session would
	// write off money still owed to the user, so only its outcome ends it.
	#[test]
	fn a_session_whose_provider_holds_the_payment_never_expires() {
		let mut session = served(FundingDirection::In);
		session
			.report(PROVIDER, FundingUpdate::PaymentReceived { finalized: true }, |id| *id, NOW)
			.expect("reported");

		let expired = session.expire_if_due(NOW + 10 * PROVIDER_SILENCE_MS);

		assert_eq!(
			(expired, session.wire_item()),
			(false, HostFundingStatusSubscribeItem::InProgress { expires_at: None })
		);
	}

	#[test]
	fn an_ended_session_keeps_its_first_outcome() {
		let mut session = expired("fs_1", NOW);

		assert!(!session.fail(FundingFailure::Cancelled, NOW + 1));
		assert_eq!(session, expired("fs_1", NOW));
	}

	// The host owns the history: open sessions come first, then the ones it
	// has not recorded, newest first and not without bound, then the recorded
	// ones kept only so their owner can read how they ended.
	#[test]
	fn the_core_keeps_open_then_unrecorded_then_recorded_sessions() {
		let ended = |index: usize, acknowledged| FundingSession {
			acknowledged,
			..expired(&format!("fs_s{index}"), NOW + index as u64)
		};
		let open = session(FundingDirection::Out);
		let recorded = (0..3).map(|index| ended(index, true));
		let unrecorded = (100..101 + UNACKNOWLEDGED_LIMIT).map(|index| ended(index, false));

		let kept: Vec<String> = retained(recorded.chain(unrecorded).chain([open]))
			.0
			.into_iter()
			.map(|session| session.intent)
			.collect();

		let newest_unrecorded =
			(101..101 + UNACKNOWLEDGED_LIMIT).rev().map(|index| format!("fs_s{index}"));
		let recorded_newest_first = (0..3).rev().map(|index| format!("fs_s{index}"));
		assert_eq!(
			kept,
			std::iter::once("fs_1".to_string())
				.chain(newest_unrecorded)
				.chain(recorded_newest_first)
				.collect::<Vec<_>>()
		);
	}

	fn served(direction: FundingDirection) -> FundingSession {
		let mut session = session(direction);
		assert!(session.assign(PROVIDER, NOW));
		session
	}

	/// Report `update` from the provider. Ids are compared as given, as the
	/// session only checks them against what was recorded.
	fn report(
		session: &mut FundingSession,
		update: FundingUpdate,
	) -> Result<(), HostFundingReportError> {
		session.report(PROVIDER, update, |id| *id, NOW)
	}

	fn reported(session: &mut FundingSession, updates: &[FundingUpdate]) {
		for update in updates {
			if let Some((id, amount)) = named_funds(update) {
				session.record_start(*id, amount);
			}
			report(session, update.clone())
				.unwrap_or_else(|refusal| panic!("{update:?} refused: {refusal:?}"));
		}
	}

	fn named(host_id: [u8; 32], amount: u128) -> StartedFunds {
		StartedFunds { host_id, amount, named_at_ms: Some(NOW) }
	}

	// With no provider to ask, a cancel ends the session itself; once it
	// ended, its outcome stands.
	#[test]
	fn an_unserved_session_is_cancelled_once() {
		let mut open = session(FundingDirection::In);
		let mut ended = expired("fs_2", NOW);

		assert_eq!(
			(open.cancel(NOW + 1), open.wire_item(), ended.cancel(NOW + 1)),
			(
				CancelOutcome::Ended,
				HostFundingStatusSubscribeItem::Failed {
					reason: FundingFailure::Cancelled,
					moved: 0,
				},
				CancelOutcome::Refused
			)
		);
	}

	fn cancelled(session: &mut FundingSession) -> CancelOutcome {
		session.cancel(NOW)
	}

	fn offer(fee: u128, refund: u128, expires_at: Option<u64>) -> FundingUpdate {
		FundingUpdate::CancelOffer { fee, refund, expires_at }
	}

	// Once the user's payment reached the provider, a cancel is a request:
	// asking again while it is pending changes nothing and never ends the
	// session, and a refusal leaves it running until the user asks anew.
	#[test]
	fn after_money_moved_a_cancel_is_a_request_the_provider_answers() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::PaymentReceived { finalized: false }]);

		let asked = cancelled(&mut session);
		let again = cancelled(&mut session);
		reported(&mut session, &[FundingUpdate::CancelRefused { reason: "card settled".into() }]);
		let refused = session.cancel.clone();
		let asked_anew = cancelled(&mut session);

		assert_eq!(
			(asked, again, refused, asked_anew, session.is_terminal()),
			(
				CancelOutcome::Requested,
				CancelOutcome::Pending,
				Some(FundingCancel::Refused { reason: "card settled".into() }),
				CancelOutcome::Requested,
				false,
			)
		);
	}

	// The user sees the provider's terms and declines them: the session runs
	// on, and the provider hears the answer whenever its stream reopens.
	#[test]
	fn a_declined_offer_lets_the_session_carry_on() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::PaymentReceived { finalized: true }]);
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None)]);
		let offered = session.cancel.clone();

		let answer = session.confirm_cancel(None, NOW);

		assert_eq!(
			(offered, answer, session.is_terminal(), session.pending_cancel_item()),
			(
				Some(FundingCancel::Offered { fee: 5, refund: 95, expires_at: None }),
				CancelConfirmation::Declined,
				false,
				Some(HostFundingServeSubscribeItem::CancelDeclined { intent: "fs_1".into() }),
			)
		);
	}

	// A provider that reports its next step after a decline has carried on,
	// so the decline is not handed to it again on every reopen.
	#[test]
	fn a_decline_stops_repeating_once_the_provider_carries_on() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::PaymentReceived { finalized: true }]);
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None)]);
		session.confirm_cancel(None, NOW);
		let before = session.pending_cancel_item();

		reported(&mut session, &[FundingUpdate::Converting]);

		assert_eq!(
			(before, session.pending_cancel_item(), session.cancel.clone()),
			(
				Some(HostFundingServeSubscribeItem::CancelDeclined { intent: "fs_1".into() }),
				None,
				None,
			)
		);
	}

	// An accepted offer is the provider's to settle: before any credit it
	// returns the user's payment and fails the session, which it may do
	// although it holds the payment.
	#[test]
	fn an_accepted_offer_is_settled_by_the_provider() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::PaymentReceived { finalized: true }]);
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None)]);

		let answer = session.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW);
		let replayed = session.pending_cancel_item();
		let settled =
			report(&mut session, FundingUpdate::Failed { reason: FundingFailure::Cancelled });

		assert_eq!(
			(answer, replayed, settled, session.wire_item(), session.cancel),
			(
				CancelConfirmation::Accepted,
				Some(HostFundingServeSubscribeItem::CancelAccepted { intent: "fs_1".into() }),
				Ok(()),
				HostFundingStatusSubscribeItem::Failed {
					reason: FundingFailure::Cancelled,
					moved: 0,
				},
				None,
			)
		);
	}

	// An offer the user leaves unanswered lapses on the core's clock, which
	// keeps time for it although nothing else can expire the session; an
	// answer after that cannot accept it.
	#[test]
	fn an_offer_lapses_at_its_expiry() {
		let mut lapsing = served(FundingDirection::In);
		reported(&mut lapsing, &[FundingUpdate::PaymentReceived { finalized: true }]);
		cancelled(&mut lapsing);
		reported(&mut lapsing, &[offer(5, 95, Some(NOW + 10))]);
		let mut late = lapsing.clone();

		let alarm = lapsing.next_alarm_ms();
		let early = lapsing.lapse_cancel_offer(NOW + 9);
		let due = lapsing.lapse_cancel_offer(NOW + 10);

		assert_eq!(
			(
				alarm,
				early,
				due,
				lapsing.cancel.clone(),
				lapsing.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 11),
				late.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 10),
				late.cancel,
			),
			(
				Some(NOW + 10),
				false,
				true,
				Some(FundingCancel::Declined),
				CancelConfirmation::NoOffer,
				CancelConfirmation::Lapsed,
				Some(FundingCancel::Declined),
			)
		);
	}

	// Once the provider started crediting the user's balance, there is
	// nothing left to cancel: the host tells the user it is too late, and
	// the provider is not asked.
	#[test]
	fn an_inbound_session_whose_credit_started_is_too_late_to_cancel() {
		let mut recorded = served(FundingDirection::In);
		recorded.record_start([1; 32], 100);
		let mut named = served(FundingDirection::In);
		reported(&mut named, &[FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 }]);

		assert_eq!(
			(cancelled(&mut recorded), recorded.cancel, cancelled(&mut named)),
			(CancelOutcome::TooLate, None, CancelOutcome::TooLate)
		);
	}

	// A worker can stop between starting a payment and naming it. The
	// payment it recorded locks the session as named funds do: asking twice
	// does not end it, no deadline does, and the provider cannot fail it, so
	// a debited user is never told nothing moved.
	#[test]
	fn recorded_funds_lock_the_session_before_they_are_named() {
		let mut session = served(FundingDirection::Out);
		session.record_start([3; 32], 40);

		let first = cancelled(&mut session);
		let second = cancelled(&mut session);
		let expired = session.expire_if_due(u64::MAX);
		let failed =
			report(&mut session, FundingUpdate::Failed { reason: FundingFailure::Cancelled });

		assert_eq!(
			(first, second, expired, session.expires_at(), failed, session.is_terminal()),
			(
				CancelOutcome::Requested,
				CancelOutcome::Pending,
				false,
				None,
				Err(HostFundingReportError::OutOfOrder),
				false,
			)
		);
	}

	// A provider may replace its offer while the user looks at the first
	// one. Accepting the terms the user saw must not accept the new ones,
	// and the provider is not asked again while an offer is open.
	#[test]
	fn an_offer_that_changed_since_the_user_saw_it_is_not_accepted() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::PaymentReceived { finalized: true }]);
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None), offer(20, 80, None)]);

		let stale = session.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW);
		let replayed = session.pending_cancel_item();
		let current = session.confirm_cancel(Some(CancelTerms { fee: 20, refund: 80 }), NOW);

		assert_eq!(
			(stale, replayed, current, session.cancel),
			(
				CancelConfirmation::Changed,
				None,
				CancelConfirmation::Accepted,
				Some(FundingCancel::Accepted { fee: 20, refund: 80 }),
			)
		);
	}

	// An inbound offer accepted after the provider started crediting comes
	// too late: the provider could not honour it, so it is declined.
	#[test]
	fn accepting_an_inbound_offer_after_the_credit_started_is_too_late() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::PaymentReceived { finalized: true }]);
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None)]);
		session.record_start([1; 32], 100);

		let answer = session.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW);

		assert_eq!(
			(answer, session.cancel.clone(), session.pending_cancel_item()),
			(
				CancelConfirmation::TooLate,
				Some(FundingCancel::Declined),
				Some(HostFundingServeSubscribeItem::CancelDeclined { intent: "fs_1".into() }),
			)
		);
	}

	// An offer cannot hand back more than the session moves, an inbound
	// session whose credit started cannot be offered, and an outbound offer
	// waits for the refund path of a released withdrawal.
	#[test]
	fn an_offer_must_fit_the_session() {
		let mut over = served(FundingDirection::In);
		reported(&mut over, &[FundingUpdate::PaymentReceived { finalized: true }]);
		cancelled(&mut over);
		let mut crediting = over.clone();
		crediting.record_start([1; 32], 100);
		let mut outbound = served(FundingDirection::Out);
		cancelled(&mut outbound);

		assert_eq!(
			(
				report(&mut over, offer(5, 100, None)),
				report(&mut crediting, offer(5, 95, None)),
				report(&mut outbound, offer(5, 95, None)),
			),
			(
				Err(HostFundingReportError::Unknown {
					reason: "the offer exceeds the session amount".into(),
				}),
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::OutOfOrder),
			)
		);
	}

	// A refusal reported after the payment was named is the latest update,
	// not the latest step: the payment still settles the session, before and
	// after a restart.
	#[test]
	fn a_cancel_answer_after_collecting_does_not_hide_the_settlement() {
		let mut session = served(FundingDirection::Out);
		reported(&mut session, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 40 }]);
		cancelled(&mut session);
		reported(&mut session, &[FundingUpdate::CancelRefused { reason: "paid out".into() }]);
		let restarted =
			FundingSession::decode(&mut &session.encode()[..]).expect("stored session decodes");

		let due = |session: &FundingSession| match session.settlement_due() {
			Some(Settlement::Payment(funds)) => Some(funds.amount),
			_ => None,
		};
		assert_eq!((due(&session), due(&restarted)), (Some(40), Some(40)));
	}

	// A provider answers only a cancel the user asked for, and an accepted
	// offer is not taken back: it is the provider's to settle.
	#[test]
	fn a_cancel_answer_needs_a_pending_cancel() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::PaymentReceived { finalized: true }]);
		let unasked = report(&mut session, offer(5, 95, None));
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None), offer(2, 98, None)]);
		session.confirm_cancel(Some(CancelTerms { fee: 2, refund: 98 }), NOW);

		assert_eq!(
			(
				unasked,
				report(&mut session, FundingUpdate::CancelRefused { reason: "late".into() }),
				session.cancel,
			),
			(
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::OutOfOrder),
				Some(FundingCancel::Accepted { fee: 2, refund: 98 }),
			)
		);
	}

	// An answer to a cancel is not a step: a restarted worker resumes from
	// the step before it, and the next step still follows.
	#[test]
	fn cancel_answers_sit_outside_the_order_of_steps() {
		let mut session = served(FundingDirection::In);
		let received = FundingUpdate::PaymentReceived { finalized: true };
		reported(&mut session, std::slice::from_ref(&received));
		cancelled(&mut session);
		reported(&mut session, &[FundingUpdate::CancelRefused { reason: "no".into() }]);

		assert_eq!(
			(session.assignment().last_update, report(&mut session, FundingUpdate::Converting)),
			(Some(received), Ok(()))
		);
	}

	// A provider that never answers the first cancel cannot hold the session:
	// asking again ends it.
	#[test]
	fn a_second_cancel_ends_a_session_its_provider_left_unanswered() {
		let mut session = served(FundingDirection::Out);

		assert_eq!(
			(session.cancel(NOW), session.cancel(NOW + 1), session.wire_item()),
			(
				CancelOutcome::Requested,
				CancelOutcome::Ended,
				HostFundingStatusSubscribeItem::Failed {
					reason: FundingFailure::Cancelled,
					moved: 0,
				},
			)
		);
	}

	#[test]
	fn a_session_is_assigned_once() {
		let mut session = served(FundingDirection::In);

		assert!(!session.assign("other.dot", NOW));
		assert_eq!(session.provider_id.as_deref(), Some(PROVIDER));
	}

	// Reports come only from the assigned provider and only move forward, so
	// the stored timeline is one the host can draw in order.
	#[test]
	fn reports_come_from_the_provider_and_only_move_forward() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::Converting]);

		assert_eq!(
			(
				session.report("other.dot", FundingUpdate::Converting, |id| *id, NOW),
				report(&mut session, FundingUpdate::AwaitingPayment),
				report(&mut session, FundingUpdate::Converting),
				report(&mut session, FundingUpdate::Collecting { payment_id: [1; 32], amount: 5 }),
				report(&mut session, FundingUpdate::Delivered),
			),
			(
				Err(HostFundingReportError::NotFound),
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::OutOfOrder),
			)
		);
	}

	// The core settles from what it saw the provider start, so a provider
	// cannot claim a top-up it never started for this session, inflate one,
	// or name one twice.
	#[test]
	fn a_report_names_only_funds_started_for_the_session_at_their_amount() {
		let mut unassigned = session(FundingDirection::In);
		let mut session = served(FundingDirection::In);
		let recorded_unassigned = unassigned.record_start([1; 32], 70);
		session.record_start([1; 32], 70);

		let never_started =
			report(&mut session, FundingUpdate::Crediting { top_up_id: [9; 32], amount: 70 });
		let inflated =
			report(&mut session, FundingUpdate::Crediting { top_up_id: [1; 32], amount: 700 });
		let credited =
			report(&mut session, FundingUpdate::Crediting { top_up_id: [1; 32], amount: 70 });

		assert_eq!(
			(recorded_unassigned, never_started, inflated, credited, session.started),
			(
				false,
				Err(HostFundingReportError::NotStarted),
				Err(HostFundingReportError::NotStarted),
				Ok(()),
				vec![named([1; 32], 70)],
			)
		);
	}

	// A partial claim is retried with a new top-up, as getcash does in up to
	// three attempts; the host follows all of them once the provider is done.
	#[test]
	fn an_inbound_session_settles_from_every_top_up_it_names() {
		let mut session = served(FundingDirection::In);
		reported(
			&mut session,
			&[
				FundingUpdate::Crediting { top_up_id: [1; 32], amount: 70 },
				FundingUpdate::Crediting { top_up_id: [2; 32], amount: 30 },
			],
		);
		let repeated =
			report(&mut session, FundingUpdate::Crediting { top_up_id: [1; 32], amount: 30 });
		let due_before_done = session.settlement_due();
		reported(&mut session, &[FundingUpdate::Delivered]);

		assert_eq!(
			(repeated, due_before_done, session.settlement_due()),
			(
				Err(HostFundingReportError::OutOfOrder),
				None,
				Some(Settlement::TopUps(vec![named([1; 32], 70), named([2; 32], 30)]))
			)
		);
	}

	// Once a top-up or payment started, the money is in motion: the provider
	// cannot fail the session and no deadline ends it.
	#[test]
	fn moving_funds_are_settled_by_the_host_not_the_provider_or_the_clock() {
		let mut session = served(FundingDirection::Out);
		reported(&mut session, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 40 }]);

		let failed =
			report(&mut session, FundingUpdate::Failed { reason: FundingFailure::ProviderTimeout });
		let expired = session.expire_if_due(u64::MAX);

		assert_eq!(
			(failed, expired, session.expires_at(), session.settlement_due()),
			(
				Err(HostFundingReportError::OutOfOrder),
				false,
				None,
				Some(Settlement::Payment(named([3; 32], 40)))
			)
		);
	}

	#[test]
	fn settling_records_what_moved_and_nothing_fails_the_session() {
		let mut credited = served(FundingDirection::In);
		let mut debited = served(FundingDirection::Out);
		let mut unclaimed = served(FundingDirection::In);

		credited.settle(90, NOW);
		debited.settle(40, NOW);
		unclaimed.settle(0, NOW);

		assert_eq!(
			(credited.wire_item(), debited.wire_item(), unclaimed.wire_item()),
			(
				HostFundingStatusSubscribeItem::Delivered { credited: 90 },
				HostFundingStatusSubscribeItem::Released { debited: 40 },
				HostFundingStatusSubscribeItem::Failed {
					reason: FundingFailure::Other {
						code: "not_claimed".to_string(),
						message: "Nothing reached your balance".to_string(),
					},
					moved: 0,
				},
			)
		);
	}

	#[test]
	fn a_provider_failure_ends_the_session_and_is_kept() {
		let mut session = served(FundingDirection::In);
		let failure = FundingUpdate::Failed { reason: FundingFailure::VerificationRefused };

		reported(&mut session, &[FundingUpdate::AwaitingPayment, failure.clone()]);

		assert_eq!(
			(session.wire_item(), session.last_update()),
			(
				HostFundingStatusSubscribeItem::Failed {
					reason: FundingFailure::VerificationRefused,
					moved: 0,
				},
				Some(&failure)
			)
		);
	}

	#[test]
	fn sessions_round_trip_through_core_storage() {
		let storage = stub_platform();
		let sessions = vec![session(FundingDirection::In), expired("fs_2", NOW)];

		block_on(store_sessions(storage.as_ref(), &sessions.iter().collect::<Vec<_>>()))
			.expect("stored");

		assert_eq!(block_on(load_sessions(storage.as_ref())).expect("loaded"), sessions);
	}

	#[test]
	fn an_undecodable_blob_is_kept_aside_and_reads_as_no_sessions() {
		let storage = stub_platform();
		let mut blob = (SESSIONS_FORMAT, vec![session(FundingDirection::In)]).encode();
		blob.push(0xff);
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
			.expect("written");

		assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
		assert_eq!(
			block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			Ok(Some(vec![blob].encode()))
		);
	}

	#[test]
	fn a_second_unreadable_blob_is_kept_alongside_the_first() {
		let storage = stub_platform();
		let first = vec![0xff, 0x01];
		let second = vec![0xff, 0x02];
		for blob in [&first, &second] {
			block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
				.expect("written");
			assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
		}

		assert_eq!(
			block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			Ok(Some(vec![first, second].encode()))
		);
	}

	#[test]
	fn a_blob_of_an_unknown_version_is_kept_aside() {
		let storage = stub_platform();
		let blob = (SESSIONS_FORMAT + 1, vec![session(FundingDirection::In)]).encode();
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
			.expect("written");

		assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(Vec::new()));
		assert_eq!(
			block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			Ok(Some(vec![blob].encode()))
		);
	}

	// A compacted session answers with the same terminal item it had in full.
	#[test]
	fn a_summary_reads_as_the_session_ended() {
		let delivered = FundingSession {
			stage: FundingStage::Delivered { credited: 90, settled_at_ms: NOW + 1 },
			..session(FundingDirection::In)
		};
		let summary = FundingSessionSummary::from(delivered.clone());

		assert_eq!(
			(summary.wire_item(), summary.settled_at_ms()),
			(delivered.wire_item(), Some(NOW + 1))
		);
	}

	#[test]
	fn the_pack_round_trips_through_core_storage() {
		let storage = stub_platform();
		let pack = vec![FundingSessionSummary::from(expired("fs_2", NOW))];

		block_on(store_pack(storage.as_ref(), pack_blob(&pack))).expect("stored");

		assert_eq!(block_on(load_pack(storage.as_ref())).expect("loaded"), pack);
	}

	#[test]
	fn an_undecodable_pack_is_kept_aside_and_reads_as_empty() {
		let storage = stub_platform();
		let blob = vec![0xff, 0x01];
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessionPack, blob.clone()))
			.expect("written");

		assert_eq!(block_on(load_pack(storage.as_ref())), Ok(Vec::new()));
		assert_eq!(
			block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			Ok(Some(vec![blob].encode()))
		);
	}

	#[test]
	fn storing_nothing_clears_the_slot() {
		let storage = stub_platform();
		block_on(store_sessions(storage.as_ref(), &[&session(FundingDirection::In)]))
			.expect("stored");

		block_on(store_sessions(storage.as_ref(), &[])).expect("cleared");

		assert_eq!(block_on(storage.read_core_storage(CoreStorageKey::FundingSessions)), Ok(None));
	}
}

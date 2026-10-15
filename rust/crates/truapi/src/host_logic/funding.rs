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
	FundingAssignment, FundingDeposit, FundingDirection, FundingFailure, FundingFrameTarget,
	FundingQuote, FundingRail, FundingReceived, FundingUpdate, GenericError,
	HostFundingReportError, HostFundingServeSubscribeItem, HostFundingStatusSubscribeItem,
};
use url::Url;

use self::stored::{SESSIONS_FORMAT, decode_sessions};
use crate::platform::{CoreStorage, CoreStorageKey};

mod stored;

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
	/// What the user chose the provider on: its quote, and the rail and
	/// asset it priced. `None` until chosen, or when chosen unquoted.
	pub choice: Option<FundingChoice>,
	/// The provider's own state for the session, opaque to the host. Kept
	/// only while the session is open.
	pub saved: Option<Vec<u8>>,
}

/// The most bytes of provider state a session keeps.
pub const MAX_SAVED_BYTES: u32 = 4096;

/// The quote a session's provider was chosen on, with the rail and asset it
/// priced.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingChoice {
	/// The quote.
	pub quote: FundingQuote,
	/// The rail the user pays or is paid by.
	pub rail: FundingRail,
	/// The asset symbol the user pays with or receives.
	pub asset: String,
}

impl FundingChoice {
	/// The balance amount the quote moves in `direction`: what it credits
	/// inbound, what it debits outbound.
	pub fn balance_amount(&self, direction: FundingDirection) -> u128 {
		match direction {
			FundingDirection::In => self.quote.receive_amount,
			FundingDirection::Out => self.quote.send_amount,
		}
	}
}

/// A session's progress as a host draws it: the steps for its direction and
/// rail, each with when it was reached.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingProgress {
	/// The steps in order; a step reached later fills in any skipped
	/// before it with its time.
	pub steps: Vec<FundingProgressStep>,
	/// When the session failed, if it did.
	pub failed_at_ms: Option<u64>,
	/// The provider's transaction id, from its latest `Details`.
	pub transaction_id: Option<String>,
	/// The provider's reference, from its latest `Details`.
	pub reference: Option<String>,
	/// Where and what the user pays, from the provider's latest `Deposit`.
	pub deposit: Option<FundingDeposit>,
	/// What arrived when it differs from what was asked, from the latest
	/// `PaymentReceived`.
	pub mismatch: Option<FundingReceived>,
}

/// One step of a session's progress.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingProgressStep {
	/// The step.
	pub step: FundingStep,
	/// When it was reached, in Unix milliseconds.
	pub reached_at_ms: Option<u64>,
}

/// A step a session goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingStep {
	/// The session was opened.
	Started,
	/// The provider saw the user's payment, or asked the user to pay it.
	Payment,
	/// The payment can no longer be reversed. Bank and crypto only.
	Approved,
	/// The provider is converting to or from the balance asset.
	Conversion,
	/// The funds reached the user's balance.
	Added,
	/// The funds left the user's balance.
	Sent,
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

/// Why a provider's state was not saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRefusal {
	/// No open session with this id is assigned to the provider.
	NotFound,
	/// Larger than [`MAX_SAVED_BYTES`].
	TooLarge,
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
	Requested,
	/// The provider will not cancel. The session carries on.
	Refused {
		/// Why, as the provider put it.
		reason: String,
	},
	/// The provider can cancel on these terms and waits for the user.
	Offered {
		/// What the provider keeps, in payment balance units.
		fee: u128,
		/// What the user gets back, in payment balance units.
		refund: u128,
		/// When the offer lapses, in Unix milliseconds.
		expires_at: Option<u64>,
	},
	/// The user accepted the offer. The provider settles the cancel.
	Accepted {
		/// What the provider keeps, in payment balance units.
		fee: u128,
		/// What the user gets back, in payment balance units.
		refund: u128,
	},
	/// The user declined the offer, or it lapsed. The session carries on.
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
	/// Too late: the user's balance was already credited.
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
	Open,
	/// Ended without success.
	Failed {
		/// Why it ended.
		reason: FundingFailure,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
	/// Inbound success.
	Delivered {
		/// Amount credited, which may differ from the amount requested.
		credited: u128,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
	/// Outbound success.
	Released {
		/// Amount debited.
		debited: u128,
		/// When it ended, in Unix milliseconds.
		settled_at_ms: u64,
	},
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
			choice: None,
			saved: None,
		}
	}

	/// Assign the session to the provider the user chose, on the quote they
	/// chose it by. A session without an amount takes the amount the quote
	/// prices. Returns whether it was open and not yet assigned.
	pub fn assign(
		&mut self,
		provider_id: &str,
		choice: Option<FundingChoice>,
		now_ms: u64,
	) -> bool {
		if self.is_terminal() || self.provider_id.is_some() {
			return false;
		}
		self.provider_id = Some(provider_id.to_string());
		self.assigned_at_ms = Some(now_ms);
		if self.amount.is_none() {
			self.amount = choice.as_ref().map(|choice| choice.balance_amount(self.direction));
		}
		self.choice = choice;
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
			quote: self.choice.as_ref().map(|choice| choice.quote.clone()),
			saved: self.saved.clone(),
		}
	}

	/// The provider's most recent update.
	pub fn last_update(&self) -> Option<&FundingUpdate> {
		self.updates.last().map(|record| &record.update)
	}

	/// The provider's most recent update that moves the session on, which
	/// `Details` does not.
	pub fn last_step(&self) -> Option<&FundingUpdate> {
		self.updates
			.iter()
			.rev()
			.map(|record| &record.update)
			.find(|update| update_rank(update).is_some())
	}

	/// The session's progress for its direction and rail.
	pub fn progress(&self) -> FundingProgress {
		let steps: &[FundingStep] =
			match (self.direction, self.choice.as_ref().map(|choice| choice.rail)) {
				(FundingDirection::Out, _) => {
					&[FundingStep::Started, FundingStep::Payment, FundingStep::Sent]
				},
				(FundingDirection::In, Some(FundingRail::Card) | None) => &[
					FundingStep::Started,
					FundingStep::Payment,
					FundingStep::Conversion,
					FundingStep::Added,
				],
				(FundingDirection::In, Some(FundingRail::Bank | FundingRail::Crypto)) => &[
					FundingStep::Started,
					FundingStep::Payment,
					FundingStep::Approved,
					FundingStep::Conversion,
					FundingStep::Added,
				],
			};
		let first = |matches: fn(&FundingUpdate) -> bool| {
			self.updates
				.iter()
				.find(|record| matches(&record.update))
				.map(|record| record.at_ms)
		};
		let reached = |step: FundingStep| match step {
			FundingStep::Started => Some(self.opened_at_ms),
			FundingStep::Payment => first(|update| {
				matches!(
					update,
					FundingUpdate::PaymentReceived { .. } | FundingUpdate::Collecting { .. }
				)
			}),
			FundingStep::Approved => first(|update| {
				matches!(update, FundingUpdate::PaymentReceived { finalized: true, .. })
			}),
			FundingStep::Conversion => first(|update| {
				matches!(update, FundingUpdate::Converting | FundingUpdate::Crediting { .. })
			}),
			FundingStep::Added => match self.stage {
				FundingStage::Delivered { settled_at_ms, .. } => Some(settled_at_ms),
				_ => None,
			},
			FundingStep::Sent => match self.stage {
				FundingStage::Released { settled_at_ms, .. } => Some(settled_at_ms),
				_ => None,
			},
		};
		let mut times: Vec<Option<u64>> = steps.iter().map(|step| reached(*step)).collect();
		// A provider may skip a step; one reached later says the earlier
		// ones were passed by then.
		for index in (0..times.len().saturating_sub(1)).rev() {
			if times[index].is_none() {
				times[index] = times[index + 1];
			}
		}
		FundingProgress {
			steps: steps
				.iter()
				.zip(times)
				.map(|(step, reached_at_ms)| FundingProgressStep { step: *step, reached_at_ms })
				.collect(),
			failed_at_ms: match self.stage {
				FundingStage::Failed { settled_at_ms, .. } => Some(settled_at_ms),
				_ => None,
			},
			transaction_id: self.latest(|update| match update {
				FundingUpdate::Details { transaction_id, .. } => transaction_id.as_ref(),
				_ => None,
			}),
			reference: self.latest(|update| match update {
				FundingUpdate::Details { reference, .. } => reference.as_ref(),
				_ => None,
			}),
			deposit: self.latest(|update| match update {
				FundingUpdate::Deposit { deposit } => Some(deposit),
				_ => None,
			}),
			mismatch: self
				.latest(|update| match update {
					FundingUpdate::PaymentReceived { mismatch, .. } => Some(mismatch),
					_ => None,
				})
				.flatten(),
		}
	}

	/// What `field` reads from the provider's most recent update it reads
	/// anything from.
	fn latest<T: Clone>(&self, field: impl Fn(&FundingUpdate) -> Option<&T>) -> Option<T> {
		self.updates.iter().rev().find_map(|record| field(&record.update)).cloned()
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
		if self.is_terminal() {
			self.cancel = None;
		}
		Ok(())
	}

	/// Keep `state` from `provider_id` in place of what it saved before.
	pub fn save(&mut self, provider_id: &str, state: Vec<u8>) -> Result<(), SaveRefusal> {
		if self.is_terminal() || self.provider_id.as_deref() != Some(provider_id) {
			return Err(SaveRefusal::NotFound);
		}
		if state.len() > MAX_SAVED_BYTES as usize {
			return Err(SaveRefusal::TooLarge);
		}
		self.saved = Some(state);
		Ok(())
	}

	/// Whether `update` may come next. Updates only move forward, a session
	/// may credit through a few top-ups, and once funds move the provider
	/// can no longer fail it: the top-ups or payment decide.
	fn follows(&self, update: &FundingUpdate) -> bool {
		let fits_direction = match update {
			FundingUpdate::Collecting { .. } => self.direction == FundingDirection::Out,
			FundingUpdate::Failed { .. } | FundingUpdate::Details { .. } => true,
			_ => self.direction == FundingDirection::In,
		};
		let last = self.last_step().and_then(update_rank);
		let top_ups = self.top_up_ids();
		fits_direction &&
			match update {
				FundingUpdate::Details { .. } => true,
				FundingUpdate::Failed { .. } => !self.funds_started(),
				FundingUpdate::Deposit { .. } => !self.funds_moving(),
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

	/// Answer the provider's cancel offer: accept it, so the provider settles
	/// the cancel, or decline it, so the session carries on. An offer past its
	/// expiry is declined whatever the answer.
	pub fn confirm_cancel(&mut self, accept: bool, now_ms: u64) -> CancelConfirmation {
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
		if accept {
			self.cancel = Some(FundingCancel::Accepted { fee, refund });
			CancelConfirmation::Accepted
		} else {
			self.cancel = Some(FundingCancel::Declined);
			CancelConfirmation::Declined
		}
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
	/// offer.
	pub fn pending_cancel_item(&self) -> Option<HostFundingServeSubscribeItem> {
		let intent = self.intent.clone();
		match self.cancel.as_ref()? {
			FundingCancel::Requested | FundingCancel::Offered { .. } => {
				Some(HostFundingServeSubscribeItem::Cancel { intent })
			},
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
		match self.stage {
			FundingStage::Open => None,
			FundingStage::Failed { settled_at_ms, .. } |
			FundingStage::Delivered { settled_at_ms, .. } |
			FundingStage::Released { settled_at_ms, .. } => Some(settled_at_ms),
		}
	}

	/// Project the current stage onto the wire item subscribers receive.
	pub fn wire_item(&self) -> HostFundingStatusSubscribeItem {
		match &self.stage {
			FundingStage::Open => {
				HostFundingStatusSubscribeItem::InProgress { expires_at: self.expires_at() }
			},
			FundingStage::Failed { reason, .. } => {
				HostFundingStatusSubscribeItem::Failed { reason: reason.clone(), moved: 0 }
			},
			FundingStage::Delivered { credited, .. } => {
				HostFundingStatusSubscribeItem::Delivered { credited: *credited }
			},
			FundingStage::Released { debited, .. } => {
				HostFundingStatusSubscribeItem::Released { debited: *debited }
			},
		}
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
/// top-up and a payment request rank where funds start moving. `Deposit`,
/// `Details` and the answers to a cancel have no place in the order.
fn update_rank(update: &FundingUpdate) -> Option<u8> {
	Some(match update {
		FundingUpdate::AwaitingPayment => 0,
		FundingUpdate::PaymentReceived { finalized: false, .. } => 1,
		FundingUpdate::PaymentReceived { finalized: true, .. } => 2,
		FundingUpdate::Converting => 3,
		FundingUpdate::Crediting { .. } | FundingUpdate::Collecting { .. } => 4,
		FundingUpdate::Delivered => 5,
		FundingUpdate::Failed { .. } => 6,
		FundingUpdate::Deposit { .. } |
		FundingUpdate::Details { .. } |
		FundingUpdate::CancelRefused { .. } |
		FundingUpdate::CancelOffer { .. } => return None,
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
	/// The chosen quote is not one the core offered for this session, or it
	/// expired.
	#[display("unknown or expired funding quote {quote_id}")]
	UnknownQuote {
		/// The quote id the host named.
		quote_id: String,
	},
	/// The chosen quote prices a different amount than the session asks to
	/// move.
	#[display("funding quote {quote_id} prices {priced}, the session asks for {amount}")]
	QuoteAmountMismatch {
		/// The quote id the host named.
		quote_id: String,
		/// The balance amount the quote prices.
		priced: u128,
		/// The session's amount.
		amount: u128,
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

/// The sessions worth keeping: every open one, then the ended ones the host
/// has not recorded yet, newest first up to a fixed bound. The host owns the
/// history; a session it has recorded is no longer kept.
pub fn retained(sessions: impl IntoIterator<Item = FundingSession>) -> Vec<FundingSession> {
	let (mut open, mut unrecorded): (Vec<_>, Vec<_>) = sessions
		.into_iter()
		.filter(FundingSession::needs_handoff)
		.partition(|session| !session.is_terminal());
	unrecorded.sort_by_key(|session| core::cmp::Reverse(session.settled_at_ms()));
	unrecorded.truncate(UNACKNOWLEDGED_LIMIT);
	open.append(&mut unrecorded);
	open
}

/// Read every persisted session.
///
/// A blob that does not decode is moved to
/// [`CoreStorageKey::FundingSessionsUnreadable`] and reads as no sessions, so
/// one bad write cannot disable funding for good and the next store cannot
/// overwrite sessions it never read. If it cannot be moved, loading fails and
/// the blob stays where it is.
pub async fn load_sessions(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSession>, FundingSessionError> {
	let storage_error = |err: GenericError| FundingSessionError::Storage { reason: err.reason };
	let Some(blob) = storage
		.read_core_storage(CoreStorageKey::FundingSessions)
		.await
		.map_err(storage_error)?
	else {
		return Ok(Vec::new());
	};
	if let Some(sessions) = decode_sessions(&blob) {
		return Ok(sessions);
	}
	error!(bytes = blob.len(), "funding sessions do not decode; keeping them aside");
	storage
		.write_core_storage(CoreStorageKey::FundingSessionsUnreadable, blob)
		.await
		.map_err(storage_error)?;
	Ok(Vec::new())
}

/// Replace the persisted set with `sessions`, clearing the slot when empty.
pub async fn store_sessions(
	storage: &(impl CoreStorage + ?Sized),
	sessions: &[FundingSession],
) -> Result<(), FundingSessionError> {
	let written = if sessions.is_empty() {
		storage.clear_core_storage(CoreStorageKey::FundingSessions).await
	} else {
		storage
			.write_core_storage(
				CoreStorageKey::FundingSessions,
				(SESSIONS_FORMAT, sessions).encode(),
			)
			.await
	};
	written.map_err(|err| FundingSessionError::Storage { reason: err.reason })
}

#[cfg(test)]
mod tests {
	use super::{
		stored::{SessionV1, SessionV2, SessionV3, UpdateRecordV2, UpdateV2},
		*,
	};

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
			.report(
				PROVIDER,
				FundingUpdate::PaymentReceived { finalized: true, mismatch: None },
				|id| *id,
				NOW,
			)
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

	// The host owns the history: a session it recorded is dropped, one it has
	// not recorded is kept for it, newest first and not without bound, and
	// open sessions always come first.
	#[test]
	fn the_core_keeps_only_open_and_unrecorded_sessions() {
		let ended = |index: usize, acknowledged| FundingSession {
			acknowledged,
			..expired(&format!("fs_s{index}"), NOW + index as u64)
		};
		let open = session(FundingDirection::Out);
		let recorded = (0..3).map(|index| ended(index, true));
		let unrecorded = (100..101 + UNACKNOWLEDGED_LIMIT).map(|index| ended(index, false));

		let kept: Vec<String> = retained(recorded.chain(unrecorded).chain([open]))
			.into_iter()
			.map(|session| session.intent)
			.collect();

		let newest_unrecorded =
			(101..101 + UNACKNOWLEDGED_LIMIT).rev().map(|index| format!("fs_s{index}"));
		assert_eq!(
			kept,
			std::iter::once("fs_1".to_string()).chain(newest_unrecorded).collect::<Vec<_>>()
		);
	}

	fn served(direction: FundingDirection) -> FundingSession {
		let mut session = session(direction);
		assert!(session.assign(PROVIDER, None, NOW));
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
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: false, mismatch: None }],
		);

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
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None)]);
		let offered = session.cancel.clone();

		let answer = session.confirm_cancel(false, NOW);

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

	// An accepted offer is the provider's to settle: before any credit it
	// returns the user's payment and fails the session, which it may do
	// although it holds the payment.
	#[test]
	fn an_accepted_offer_is_settled_by_the_provider() {
		let mut session = served(FundingDirection::In);
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None)]);

		let answer = session.confirm_cancel(true, NOW);
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
		reported(
			&mut lapsing,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
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
				lapsing.confirm_cancel(true, NOW + 11),
				late.confirm_cancel(true, NOW + 10),
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

	// A provider answers only a cancel the user asked for, and an accepted
	// offer is not taken back: it is the provider's to settle.
	#[test]
	fn a_cancel_answer_needs_a_pending_cancel() {
		let mut session = served(FundingDirection::In);
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
		let unasked = report(&mut session, offer(5, 95, None));
		cancelled(&mut session);
		reported(&mut session, &[offer(5, 95, None), offer(2, 98, None)]);
		session.confirm_cancel(true, NOW);

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
		let received = FundingUpdate::PaymentReceived { finalized: true, mismatch: None };
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

		assert!(!session.assign("other.dot", None, NOW));
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

	fn chosen(direction: FundingDirection, rail: FundingRail) -> FundingSession {
		let mut session = session(direction);
		let quote = FundingQuote {
			quote_id: "q1".to_string(),
			send_amount: 100,
			receive_amount: 99,
			provider_fee: 1,
			network_fee: 0,
			eta_secs: None,
			expires_at: None,
		};
		let choice = FundingChoice { quote, rail, asset: "EUR".to_string() };
		assert!(session.assign(PROVIDER, Some(choice), NOW));
		session
	}

	fn report_at(session: &mut FundingSession, update: FundingUpdate, at_ms: u64) {
		if let Some((id, amount)) = named_funds(&update) {
			session.record_start(*id, amount);
		}
		session.report(PROVIDER, update, |id| *id, at_ms).expect("reported");
	}

	fn steps(progress: &FundingProgress) -> Vec<(FundingStep, Option<u64>)> {
		progress.steps.iter().map(|step| (step.step, step.reached_at_ms)).collect()
	}

	// A bank payment is approved once it can no longer be reversed, a card
	// payment has no such step, and a step the provider skipped takes the
	// time of the first later one, so the bar never shows a gap behind it.
	#[test]
	fn progress_follows_the_rail_and_fills_skipped_steps() {
		let mut bank = chosen(FundingDirection::In, FundingRail::Bank);
		report_at(
			&mut bank,
			FundingUpdate::PaymentReceived { finalized: false, mismatch: None },
			NOW + 1,
		);
		report_at(&mut bank, FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 }, NOW + 3);
		let mut card = chosen(FundingDirection::In, FundingRail::Card);
		report_at(&mut card, FundingUpdate::Converting, NOW + 2);
		let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
		report_at(
			&mut out,
			FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 },
			NOW + 1,
		);
		assert!(out.settle(100, NOW + 4));

		assert_eq!(
			(steps(&bank.progress()), steps(&card.progress()), steps(&out.progress())),
			(
				vec![
					(FundingStep::Started, Some(NOW)),
					(FundingStep::Payment, Some(NOW + 1)),
					(FundingStep::Approved, Some(NOW + 3)),
					(FundingStep::Conversion, Some(NOW + 3)),
					(FundingStep::Added, None),
				],
				vec![
					(FundingStep::Started, Some(NOW)),
					(FundingStep::Payment, Some(NOW + 2)),
					(FundingStep::Conversion, Some(NOW + 2)),
					(FundingStep::Added, None),
				],
				vec![
					(FundingStep::Started, Some(NOW)),
					(FundingStep::Payment, Some(NOW + 1)),
					(FundingStep::Sent, Some(NOW + 4)),
				],
			)
		);
	}

	// The provider's references can come at any point, even while funds
	// move, without changing which step comes next or what settles the
	// session; the latest of each is shown.
	#[test]
	fn details_sit_outside_the_order_of_steps() {
		let mut session = chosen(FundingDirection::In, FundingRail::Bank);
		let crediting = FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 };
		reported(
			&mut session,
			&[
				FundingUpdate::Details {
					transaction_id: Some("tx-1".into()),
					reference: Some("REF".into()),
				},
				crediting.clone(),
				FundingUpdate::Details { transaction_id: Some("tx-2".into()), reference: None },
				FundingUpdate::Delivered,
			],
		);
		let progress = session.progress();

		assert_eq!(
			(
				session.assignment().last_update,
				session.settlement_due(),
				progress.transaction_id,
				progress.reference,
			),
			(
				Some(FundingUpdate::Delivered),
				Some(Settlement::TopUps(vec![named([1; 32], 100)])),
				Some("tx-2".to_string()),
				Some("REF".to_string()),
			)
		);
	}

	// The RFC lets a provider send its references at any point while the
	// session is open, in either direction: before any step, once funds move,
	// and after it is done but before the core settled the session. Once the
	// session ended, nothing more is taken.
	#[test]
	fn details_are_taken_while_the_session_is_open() {
		let details =
			|| FundingUpdate::Details { transaction_id: Some("tx".into()), reference: None };
		let mut inbound = served(FundingDirection::In);
		let fresh = report(&mut inbound, details());
		reported(
			&mut inbound,
			&[
				FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 },
				FundingUpdate::Delivered,
			],
		);
		let done = report(&mut inbound, details());
		let mut outbound = served(FundingDirection::Out);
		reported(&mut outbound, &[FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 }]);
		let collecting = report(&mut outbound, details());
		assert!(outbound.settle(100, NOW));
		let ended = report(&mut outbound, details());

		assert_eq!(
			(fresh, done, collecting, ended),
			(Ok(()), Ok(()), Ok(()), Err(HostFundingReportError::NotFound))
		);
	}

	fn usdt_deposit(amount: u128) -> FundingDeposit {
		FundingDeposit::Crypto {
			address: "0xdeposit".to_string(),
			network: "Ethereum".to_string(),
			asset: "USDT".to_string(),
			amount,
			decimals: 6,
			exact: true,
			uri: None,
			expires_at: Some(NOW + 1_000),
		}
	}

	// A short payment, as getcash handles it: the provider says what arrived
	// and asks for the rest at the same address; the host shows the latest
	// instructions. Once funds move, or for a withdrawal, there is nothing
	// left for the user to pay.
	#[test]
	fn a_short_payment_asks_for_the_rest_until_funds_move() {
		let mut session = served(FundingDirection::In);
		let short = FundingReceived { asset: "USDT".to_string(), amount: 60 };
		reported(
			&mut session,
			&[
				FundingUpdate::Deposit { deposit: usdt_deposit(100) },
				FundingUpdate::PaymentReceived { finalized: false, mismatch: Some(short.clone()) },
				FundingUpdate::Deposit { deposit: usdt_deposit(40) },
			],
		);
		let progress = session.progress();
		reported(&mut session, &[FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 }]);
		let after_funds_move =
			report(&mut session, FundingUpdate::Deposit { deposit: usdt_deposit(1) });
		let withdrawal = report(
			&mut served(FundingDirection::Out),
			FundingUpdate::Deposit { deposit: usdt_deposit(1) },
		);

		assert_eq!(
			(progress.deposit, progress.mismatch, after_funds_move, withdrawal),
			(
				Some(usdt_deposit(40)),
				Some(short),
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::OutOfOrder),
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

		block_on(store_sessions(storage.as_ref(), &sessions)).expect("stored");

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
			Ok(Some(blob))
		);
	}

	#[test]
	fn sessions_stored_before_providers_served_them_still_load() {
		let storage = stub_platform();
		let stored = SessionV1 {
			intent: "fs_1".to_string(),
			owner_product_id: Some("wallet.dot".to_string()),
			direction: FundingDirection::In,
			amount: Some(100),
			stage: FundingStage::Open,
			opened_at_ms: NOW,
			deadline_ms: NOW + SESSION_WINDOW_MS,
			acknowledged: false,
		};
		block_on(
			storage
				.write_core_storage(CoreStorageKey::FundingSessions, (1u16, vec![stored]).encode()),
		)
		.expect("written");

		let loaded = block_on(load_sessions(storage.as_ref())).expect("loaded");

		assert_eq!(
			loaded,
			vec![FundingSession::new(
				"fs_1".to_string(),
				Some("wallet.dot".to_string()),
				FundingDirection::In,
				Some(100),
				NOW,
			)]
		);
	}

	#[test]
	fn sessions_stored_before_a_session_kept_its_quote_still_load() {
		let storage = stub_platform();
		let started = named([7; 32], 100);
		let stored = SessionV2 {
			intent: "fs_1".to_string(),
			owner_product_id: Some("wallet.dot".to_string()),
			direction: FundingDirection::In,
			amount: Some(100),
			stage: FundingStage::Open,
			opened_at_ms: NOW,
			deadline_ms: NOW + SESSION_WINDOW_MS,
			acknowledged: false,
			provider_id: Some(PROVIDER.to_string()),
			assigned_at_ms: Some(NOW),
			cancel_requested: false,
			updates: vec![UpdateRecordV2 {
				update: UpdateV2::PaymentReceived { finalized: true },
				at_ms: NOW,
			}],
			started: vec![started.clone()],
		};
		block_on(
			storage
				.write_core_storage(CoreStorageKey::FundingSessions, (2u16, vec![stored]).encode()),
		)
		.expect("written");

		let loaded = block_on(load_sessions(storage.as_ref())).expect("loaded");

		let mut expected = served(FundingDirection::In);
		expected.updates = vec![FundingUpdateRecord {
			update: FundingUpdate::PaymentReceived { finalized: true, mismatch: None },
			at_ms: NOW,
		}];
		expected.started = vec![started];
		assert_eq!(loaded, vec![expected]);
	}

	#[test]
	fn a_cancel_pending_in_format_3_is_still_with_the_provider() {
		let storage = stub_platform();
		let stored = SessionV3 {
			intent: "fs_1".to_string(),
			owner_product_id: None,
			direction: FundingDirection::In,
			amount: Some(100),
			stage: FundingStage::Open,
			opened_at_ms: NOW,
			deadline_ms: NOW + SESSION_WINDOW_MS,
			acknowledged: false,
			provider_id: Some(PROVIDER.to_string()),
			assigned_at_ms: Some(NOW),
			cancel_requested: true,
			updates: Vec::new(),
			started: Vec::new(),
			choice: None,
			saved: Some(vec![1, 2]),
		};
		block_on(
			storage
				.write_core_storage(CoreStorageKey::FundingSessions, (3u16, vec![stored]).encode()),
		)
		.expect("written");

		let loaded = block_on(load_sessions(storage.as_ref())).expect("loaded");

		assert_eq!(
			loaded
				.iter()
				.map(|session| (session.cancel.clone(), session.saved.clone()))
				.collect::<Vec<_>>(),
			vec![(Some(FundingCancel::Requested), Some(vec![1, 2]))]
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
			Ok(Some(blob))
		);
	}

	#[test]
	fn storing_nothing_clears_the_slot() {
		let storage = stub_platform();
		block_on(store_sessions(storage.as_ref(), &[session(FundingDirection::In)]))
			.expect("stored");

		block_on(store_sessions(storage.as_ref(), &[])).expect("cleared");

		assert_eq!(block_on(storage.read_core_storage(CoreStorageKey::FundingSessions)), Ok(None));
	}
}

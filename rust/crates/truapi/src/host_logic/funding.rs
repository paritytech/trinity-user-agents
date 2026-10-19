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
	FundingPayout, FundingQuote, FundingRail, FundingReceived, FundingUpdate, GenericError,
	HostFundingReportError, HostFundingServeSubscribeItem, HostFundingStatusSubscribeItem,
};
use url::Url;

use crate::platform::{CoreStorage, CoreStorageKey};

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has not recorded yet the core keeps in
/// full, newest first, so a host that never records them cannot grow the
/// sessions blob without bound. Older ones are compacted into the session
/// pack.
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
	/// What the user chose the provider on: its quote, and the rail and
	/// asset it priced. `None` until chosen, or when chosen unquoted.
	pub choice: Option<FundingChoice>,
	/// The provider's own state for the session, opaque to the host. Kept
	/// while the provider has work on the session.
	pub saved: Option<Vec<u8>>,
	/// The refund of a failed payout, once the provider reported starting
	/// it.
	pub refund: Option<FundingRefund>,
}

/// The top-up a provider gives a failed payout back with, and what it
/// returned.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingRefund {
	/// The top-up.
	pub top_up: StartedFunds,
	/// What its claims returned to the user's balance once they were final.
	/// `None` while the core follows them.
	pub refunded: Option<u128>,
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
	/// The network a crypto quote was asked for.
	pub network: Option<String>,
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
	/// Whether the provider is retrying: it named another top-up after a
	/// first one claimed only part of the amount.
	pub retrying: bool,
	/// How the payout went, once the provider reported it on a released
	/// outbound session.
	pub payout: Option<FundingPayout>,
	/// What a failed payout's refund returned to the user's balance, once
	/// its claims are final.
	pub refunded: Option<u128>,
	/// Where the session stands, in one line the host words.
	pub status: FundingStatus,
	/// When the status turns to [`FundingStatus::TakingLonger`] unless the
	/// provider reports first, in Unix milliseconds, so the host reads the
	/// progress again then. `None` while a top-up waits on the user's
	/// payment.
	pub longer_at_ms: Option<u64>,
	/// Whether money moved: the provider started or named a top-up or
	/// payment, or holds the user's payment. Until then a second cancel ends
	/// the session; from then on a cancel is a request the provider answers,
	/// and only the provider can end the session.
	pub money_moved: bool,
}

/// Where a session stands, one line for the host to word: in flight by
/// rail, then how it ended.
///
/// Each variant names the row hosts show for it in their activity list,
/// title then subtitle, so both apps word a session the same way. In
/// flight, the title is "Topping up" (in) or "Sending" (out). An ended
/// row's subtitle is "{Rail} · {moment}": the rail is "Card", "Bank" or
/// "Crypto", and the moment is "Today at 08:25", "Yesterday at 08:25",
/// "4 Oct at 08:25" or, in another year, "4 Oct 2025". Amounts read
/// "+$50 CASH" in, "-$50 CASH" out, and unsigned once nothing moved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingStatus {
	/// Inbound bank: waiting for the user's transfer.
	///
	/// Shown as Topping up · "Waiting for transfer".
	WaitingForTransfer,
	/// Inbound card: waiting for the user to pay on the provider's screens.
	///
	/// Shown as Topping up · "Upcoming receiving".
	WaitingForPayment,
	/// Inbound crypto: waiting for the user's deposit.
	///
	/// Shown as Topping up · "Upcoming receiving".
	WaitingForDeposit,
	/// Inbound: the provider has the payment and is turning it into the
	/// user's balance asset.
	///
	/// Shown as Topping up · "Converting to Cash".
	ConvertingToCash,
	/// Outbound: the user started the withdrawal and pays the provider.
	///
	/// Shown as Sending · "Transaction initiated".
	TransactionInitiated,
	/// Outbound crypto, after release: the provider converts what it
	/// collected to `asset`.
	///
	/// Shown as Sending · "Converting to {asset}".
	ConvertingTo {
		/// The asset the user receives.
		asset: String,
	},
	/// Outbound card or bank, after release: the provider prepares the
	/// payout.
	///
	/// Shown as Sending · "Preparing payout".
	PreparingPayout,
	/// Outbound, after release: a payout attempt failed and the provider
	/// is trying again.
	///
	/// Shown as Sending · "Provider is retrying the payout", in amber.
	RetryingPayout,
	/// Inbound: the provider is retrying, with another top-up after one
	/// claimed only part of the amount.
	///
	/// Shown as Topping up · "Provider is retrying your payment", in amber.
	Retrying,
	/// Nothing came from the provider for longer than its quote said the
	/// session takes; on a top-up, only once the provider holds the user's
	/// payment.
	///
	/// Shown as Topping up or Sending · "Taking a little longer", in amber.
	TakingLonger,
	/// Inbound, ended: the balance was credited.
	///
	/// Shown as "Topped up" · "{Rail} · {moment}".
	ToppedUp,
	/// Outbound, ended: the provider paid the user out.
	///
	/// Shown as "Sent" · "{Rail} · {moment}".
	Sent,
	/// Inbound, ended: the provider gave the user's payment back instead of
	/// crediting it.
	///
	/// Shown as "Refunded" · "{Rail} · {moment}".
	Refunded,
	/// Outbound: the payout failed after the balance was debited. `refunded`
	/// is what the provider's refund returned, once final.
	///
	/// Shown as "Payout failed" · "{Rail} · {moment}"; the amount is unsigned.
	PayoutFailed {
		/// What came back to the balance. `None` until the refund's claims
		/// are final.
		refunded: Option<u128>,
	},
	/// Ended without moving value.
	///
	/// Shown as "Top-up failed" or "Withdrawal failed" · the reason.
	Failed {
		/// Why.
		reason: FundingFailure,
	},
	/// The user asked to cancel and the provider has not answered.
	///
	/// Shown as Topping up or Sending · "Cancel requested, waiting for the provider".
	CancelRequested,
	/// The provider can cancel on these terms and waits for the user, who
	/// answers with `confirm_funding_cancel`. The offer's expiry is on the
	/// session's `cancel`.
	///
	/// Shown as Topping up or Sending · "Cancel offer waiting for you".
	CancelOffered {
		/// What the provider keeps, in payment balance units.
		fee: u128,
		/// What the user gets back, in payment balance units.
		refund: u128,
	},
	/// The provider will not cancel, and has reported nothing since. The
	/// session carries on.
	///
	/// Shown as Topping up or Sending · "The provider can't cancel".
	CancelRefused {
		/// Why, as the provider put it.
		reason: String,
	},
	/// The user accepted the provider's offer and the provider is refunding
	/// the user.
	///
	/// Shown as Topping up or Sending · "Cancelling…".
	Cancelling,
	/// Outbound, ended: cancelled on the provider's terms after the balance
	/// was debited. `refunded` is what came back, once final.
	///
	/// Shown as "Cancelled" · "{Rail} · {moment}".
	Cancelled {
		/// What came back to the balance. `None` until the refund's claims
		/// are final.
		refunded: Option<u128>,
	},
	/// Outbound, ended: the user accepted a cancel, but the payout landed
	/// first, so the money reached its destination.
	///
	/// Shown as "Sent" · "{Rail} · {moment}".
	PaidOutAfterCancel,
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
	/// The payment can no longer be reversed. Card and crypto only.
	Approved,
	/// The provider is converting to or from the balance asset.
	Conversion,
	/// The funds reached the user's balance.
	Added,
	/// The funds left the user's balance.
	Sent,
	/// The provider paid the user out of a withdrawal, or reported that the
	/// payout failed.
	Payout,
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
	/// The user accepted the offer, but the payout landed first: the
	/// withdrawal ended as paid out, on none of these terms.
	#[codec(index = 5)]
	Overtaken {
		/// The fee the user accepted.
		fee: u128,
		/// The refund the user accepted.
		refund: u128,
	},
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
	/// The top-up a released withdrawal's failed payout is refunded with.
	Refund(StartedFunds),
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
				HostFundingStatusSubscribeItem::Failed { reason: reason.clone() }
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
			choice: None,
			saved: None,
			refund: None,
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

	/// The session's progress for its direction and rail at `now_ms`.
	pub fn progress(&self, now_ms: u64) -> FundingProgress {
		let steps: &[FundingStep] =
			match (self.direction, self.choice.as_ref().map(|choice| choice.rail)) {
				(FundingDirection::Out, _) => &[
					FundingStep::Started,
					FundingStep::Payment,
					FundingStep::Sent,
					FundingStep::Conversion,
					FundingStep::Payout,
				],
				(FundingDirection::In, Some(FundingRail::Bank)) => {
					&[FundingStep::Started, FundingStep::Payment, FundingStep::Added]
				},
				(FundingDirection::In, Some(FundingRail::Card | FundingRail::Crypto) | None) => &[
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
			FundingStep::Payout => first(|update| matches!(update, FundingUpdate::Payout { .. })),
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
			retrying: self.progress_retrying(),
			payout: self.payout().cloned(),
			refunded: self.refund.as_ref().and_then(|refund| refund.refunded),
			status: self.status(now_ms),
			longer_at_ms: self.longer_at_ms().filter(|at_ms| *at_ms > now_ms),
			money_moved: self.money_moved(),
		}
	}

	/// Where the session stands at `now_ms`.
	pub fn status(&self, now_ms: u64) -> FundingStatus {
		let rail = self.choice.as_ref().map(|choice| choice.rail);
		match &self.stage {
			FundingStage::Delivered { .. } => return FundingStatus::ToppedUp,
			FundingStage::Failed { reason: FundingFailure::Refunded, .. } => {
				return FundingStatus::Refunded;
			},
			FundingStage::Failed { reason, .. } => {
				return FundingStatus::Failed { reason: reason.clone() };
			},
			FundingStage::Released { .. } if self.cancel_accepted() => {
				let refunded = self.refund.as_ref().and_then(|refund| refund.refunded);
				return match refunded {
					Some(_) => FundingStatus::Cancelled { refunded },
					None => FundingStatus::Cancelling,
				};
			},
			FundingStage::Released { .. } => match self.payout() {
				Some(FundingPayout::PaidOut)
					if matches!(self.cancel, Some(FundingCancel::Overtaken { .. })) =>
				{
					return FundingStatus::PaidOutAfterCancel;
				},
				Some(FundingPayout::PaidOut) => return FundingStatus::Sent,
				Some(FundingPayout::Failed { .. }) => {
					return FundingStatus::PayoutFailed {
						refunded: self.refund.as_ref().and_then(|refund| refund.refunded),
					};
				},
				None => {},
			},
			FundingStage::Open => {},
		}
		match &self.cancel {
			Some(FundingCancel::Requested) => return FundingStatus::CancelRequested,
			Some(FundingCancel::Offered { fee, refund, .. }) => {
				return FundingStatus::CancelOffered { fee: *fee, refund: *refund };
			},
			Some(FundingCancel::Refused { reason }) if self.refusal_is_latest() => {
				return FundingStatus::CancelRefused { reason: reason.clone() };
			},
			Some(FundingCancel::Accepted { .. }) => return FundingStatus::Cancelling,
			Some(
				FundingCancel::Refused { .. } |
				FundingCancel::Declined |
				FundingCancel::Overtaken { .. },
			) |
			None => {},
		}
		if self.progress_retrying() {
			return FundingStatus::Retrying;
		}
		if self.payout_retrying() {
			return FundingStatus::RetryingPayout;
		}
		if self.longer_at_ms().is_some_and(|at_ms| now_ms >= at_ms) {
			return FundingStatus::TakingLonger;
		}
		match (self.direction, &self.stage) {
			(FundingDirection::Out, FundingStage::Released { .. }) => match &self.choice {
				Some(choice) if choice.rail == FundingRail::Crypto => {
					FundingStatus::ConvertingTo { asset: choice.asset.clone() }
				},
				_ => FundingStatus::PreparingPayout,
			},
			(FundingDirection::Out, _) => FundingStatus::TransactionInitiated,
			(FundingDirection::In, _) => match self.last_step() {
				None | Some(FundingUpdate::AwaitingPayment) => match rail {
					Some(FundingRail::Bank) => FundingStatus::WaitingForTransfer,
					Some(FundingRail::Crypto) => FundingStatus::WaitingForDeposit,
					Some(FundingRail::Card) | None => FundingStatus::WaitingForPayment,
				},
				Some(_) => FundingStatus::ConvertingToCash,
			},
		}
	}

	/// Whether the provider's refusal to cancel is the last thing it reported.
	fn refusal_is_latest(&self) -> bool {
		matches!(self.last_update(), Some(FundingUpdate::CancelRefused { .. }))
	}

	/// When a session the provider still works on counts as taking longer:
	/// its chosen quote's time after the provider's latest report, or after
	/// it was assigned. `None` without a quoted time, while a top-up waits on
	/// the user's payment, or once it ended.
	fn longer_at_ms(&self) -> Option<u64> {
		if !self.serves_provider() || self.awaits_users_payment() {
			return None;
		}
		let eta_ms = self.choice.as_ref()?.quote.eta_secs?.saturating_mul(1_000);
		let heard_at_ms = self.updates.last().map(|record| record.at_ms).or(self.assigned_at_ms)?;
		Some(heard_at_ms.saturating_add(eta_ms))
	}

	/// Whether a top-up still waits on the user's payment, which can take
	/// days by bank: until money moved. A withdrawal never does, so its
	/// clock runs from the quote as soon as it is assigned.
	fn awaits_users_payment(&self) -> bool {
		self.direction == FundingDirection::In && !self.money_moved()
	}

	/// Whether the provider is retrying: it named another top-up after a
	/// first one claimed only part of the amount.
	fn progress_retrying(&self) -> bool {
		!self.is_terminal() && self.top_up_ids().len() > 1
	}

	/// Whether the provider's latest word on a released withdrawal's payout,
	/// with none reported yet, is that it is retrying it.
	fn payout_retrying(&self) -> bool {
		matches!(self.stage, FundingStage::Released { .. }) &&
			self.payout().is_none() &&
			self.updates.iter().rev().find_map(|record| match record.update {
				FundingUpdate::Converting => Some(false),
				FundingUpdate::PayoutRetrying => Some(true),
				_ => None,
			}) == Some(true)
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
		if self.provider_id.as_deref() != Some(provider_id) {
			return Err(HostFundingReportError::NotFound);
		}
		if let Some(answer) = cancel_answer(&update) {
			if !self.serves_provider() {
				return Err(HostFundingReportError::NotFound);
			}
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
			// A released withdrawal was handed to the host already.
			if self.is_terminal() {
				self.acknowledged = false;
			}
			return Ok(());
		}
		if matches!(
			update,
			FundingUpdate::Payout { .. } |
				FundingUpdate::Refunding { .. } |
				FundingUpdate::PayoutRetrying
		) || (matches!(update, FundingUpdate::Converting) &&
			self.direction == FundingDirection::Out)
		{
			if !self.is_terminal() {
				return Err(HostFundingReportError::OutOfOrder);
			}
			return self.report_after_release(update, host_id, now_ms);
		}
		if self.is_terminal() {
			return Err(HostFundingReportError::NotFound);
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

	/// Keep `state` from `provider_id` in place of what it saved before.
	pub fn save(&mut self, provider_id: &str, state: Vec<u8>) -> Result<(), SaveRefusal> {
		if !self.serves_provider() || self.provider_id.as_deref() != Some(provider_id) {
			return Err(SaveRefusal::NotFound);
		}
		if state.len() > MAX_SAVED_BYTES as usize {
			return Err(SaveRefusal::TooLarge);
		}
		self.saved = Some(state);
		Ok(())
	}

	/// Store what the provider reports about paying the user out of a
	/// released outbound session: that it is converting, once, that it is
	/// retrying the payout, any number of times, then the payout outcome,
	/// once, and after a failed payout or a cancel the user accepted the
	/// top-up refunding it, once. The host is handed the session again, as
	/// what it recorded no longer holds.
	fn report_after_release(
		&mut self,
		update: FundingUpdate,
		host_id: impl Fn(&[u8; 32]) -> [u8; 32],
		now_ms: u64,
	) -> Result<(), HostFundingReportError> {
		if !matches!(self.stage, FundingStage::Released { .. }) {
			return Err(HostFundingReportError::NotFound);
		}
		if let FundingUpdate::Refunding { top_up_id, amount } = &update {
			// A cancel the user accepted is refunded on its terms; a failed
			// payout up to what was debited.
			let fits = match (&self.cancel, &self.stage) {
				(Some(FundingCancel::Accepted { refund, .. }), _) => *amount == *refund,
				(_, FundingStage::Released { debited, .. }) => *amount <= *debited,
				_ => false,
			};
			if !self.awaits_refund() || !fits {
				return Err(HostFundingReportError::OutOfOrder);
			}
			let named = host_id(top_up_id);
			let record = self
				.started
				.iter_mut()
				.find(|record| record.host_id == named && record.named_at_ms.is_none())
				.filter(|record| record.amount == *amount)
				.ok_or(HostFundingReportError::NotStarted)?;
			record.named_at_ms = Some(now_ms);
			self.refund = Some(FundingRefund { top_up: record.clone(), refunded: None });
		} else {
			let converting = self
				.updates
				.iter()
				.any(|record| matches!(record.update, FundingUpdate::Converting));
			// A payout that lands although the user accepted a cancel won the
			// race: the session ends as paid out. Anything else on the way to
			// a payout is refused once the cancel was accepted.
			let lands = matches!(update, FundingUpdate::Payout { .. });
			if self.payout().is_some() ||
				(self.cancel_accepted() && !lands) ||
				(matches!(update, FundingUpdate::Converting) && converting)
			{
				return Err(HostFundingReportError::OutOfOrder);
			}
			// The payout won over an accepted cancel: keep what the user
			// accepted, so the ended session says so.
			if matches!(update, FundingUpdate::Payout { outcome: FundingPayout::PaidOut }) {
				self.cancel = match self.cancel.take() {
					Some(FundingCancel::Accepted { fee, refund }) => {
						Some(FundingCancel::Overtaken { fee, refund })
					},
					_ => None,
				};
			}
		}
		self.updates.push(FundingUpdateRecord { update, at_ms: now_ms });
		self.acknowledged = false;
		Ok(())
	}

	/// Whether the session's provider still has work on it: it is in flight,
	/// or it ended and the provider still owes the user something.
	pub fn serves_provider(&self) -> bool {
		!self.is_terminal() || self.awaits_provider()
	}

	/// Whether the session ended but its provider still owes the user: a
	/// released withdrawal whose payout outcome it has not reported, or
	/// whose refund, after a failed payout or a cancel the user accepted, is
	/// not settled yet. Whether the host recorded the session does not
	/// matter.
	pub fn awaits_provider(&self) -> bool {
		matches!(self.stage, FundingStage::Released { .. }) &&
			match (&self.refund, self.payout()) {
				(Some(refund), _) => refund.refunded.is_none(),
				(None, None | Some(FundingPayout::Failed { .. })) => true,
				(None, Some(FundingPayout::PaidOut)) => false,
			}
	}

	/// Whether a released withdrawal is owed a refund the provider has not
	/// reported: its payout failed, or the user accepted the provider's
	/// cancel offer.
	pub fn awaits_refund(&self) -> bool {
		matches!(self.stage, FundingStage::Released { .. }) &&
			self.refund.is_none() &&
			(matches!(self.payout(), Some(FundingPayout::Failed { .. })) ||
				self.cancel_accepted())
	}

	/// What a refund top-up for this released withdrawal moves: the refund
	/// of a cancel the user accepted, otherwise what was debited.
	pub fn refund_due(&self) -> Option<u128> {
		match (&self.cancel, &self.stage) {
			(Some(FundingCancel::Accepted { refund, .. }), FundingStage::Released { .. }) => {
				Some(*refund)
			},
			(_, FundingStage::Released { debited, .. }) => Some(*debited),
			_ => None,
		}
	}

	/// Whether the user accepted the provider's cancel offer.
	fn cancel_accepted(&self) -> bool {
		matches!(self.cancel, Some(FundingCancel::Accepted { .. }))
	}

	/// The provider's payout outcome, once it reported one.
	pub fn payout(&self) -> Option<&FundingPayout> {
		self.updates.iter().find_map(|record| match &record.update {
			FundingUpdate::Payout { outcome } => Some(outcome),
			_ => None,
		})
	}

	/// Whether the provider may offer to cancel on these terms. An inbound
	/// session whose credit started can no longer be cancelled; an outbound
	/// one can be offered only once released, refunded through `Refunding`,
	/// and not after its payout outcome or refund was reported. An offer
	/// cannot hand back more than the session moves.
	fn check_offer(&self, fee: u128, refund: u128) -> Result<(), HostFundingReportError> {
		let cap = match (self.direction, &self.stage) {
			(FundingDirection::In, _) if self.funds_started() => {
				return Err(HostFundingReportError::OutOfOrder);
			},
			(FundingDirection::In, _) => self.amount,
			(FundingDirection::Out, FundingStage::Released { debited, .. })
				if self.payout().is_none() && self.refund.is_none() =>
			{
				Some(*debited)
			},
			(FundingDirection::Out, _) => return Err(HostFundingReportError::OutOfOrder),
		};
		match cap {
			Some(cap) if fee.saturating_add(refund) > cap => Err(HostFundingReportError::Unknown {
				reason: "the offer exceeds the session amount".to_string(),
			}),
			_ => Ok(()),
		}
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
		if (self.is_terminal() && !self.awaits_refund()) ||
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
		if let Some(refund) = &self.refund &&
			refund.refunded.is_none()
		{
			return Some(Settlement::Refund(refund.top_up.clone()));
		}
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
		// A released withdrawal stays with its provider, and so does the
		// user's cancel of it.
		if !self.serves_provider() {
			self.cancel = None;
		}
		true
	}

	/// Record what a failed payout's refund returned once its claims are
	/// final, and hand the session to the host again. Returns whether it
	/// changed.
	pub fn settle_refund(&mut self, refunded: u128) -> bool {
		match &mut self.refund {
			Some(refund) if refund.refunded.is_none() => {
				refund.refunded = Some(refunded);
				self.acknowledged = false;
				true
			},
			_ => false,
		}
	}

	/// Whether the session is kept: it is in flight, the host has not
	/// recorded its outcome, or its provider still owes the user.
	pub fn needs_handoff(&self) -> bool {
		!self.is_terminal() || !self.acknowledged || self.awaits_provider()
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
		// A withdrawal whose payout failed or whose refund was reported is
		// owed its refund in full, so there is nothing left to cancel.
		if !self.serves_provider() ||
			matches!(self.payout(), Some(FundingPayout::Failed { .. })) ||
			self.refund.is_some()
		{
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
			Some(FundingCancel::Overtaken { .. }) => CancelOutcome::TooLate,
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
		if !self.serves_provider() {
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
			Some(FundingCancel::Offered { expires_at, .. }) if self.serves_provider() => expires_at,
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
			FundingCancel::Refused { .. } | FundingCancel::Overtaken { .. } => None,
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
		FundingUpdate::Refunding { top_up_id, amount } => Some((top_up_id, *amount)),
		_ => None,
	}
}

/// Order of an update within a session: a later update ranks higher, and a
/// top-up and a payment request rank where funds start moving. `Deposit`,
/// `Details`, what follows a release and the answers to a cancel have no
/// place in the order.
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
		FundingUpdate::Payout { .. } |
		FundingUpdate::Refunding { .. } |
		FundingUpdate::PayoutRetrying |
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
	/// The stored funding records are in a format newer than this app reads,
	/// as after a downgrade. They are left as they are, and the core writes
	/// nothing over them, so funding stays unavailable until the app is
	/// updated.
	#[display("funding records were stored by a newer app (format {format})")]
	NewerFormat {
		/// The stored format.
		format: u16,
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

/// Split `sessions` into those kept in full and the ended ones to compact.
/// Kept in full: every one its provider still has work on, the ended ones the
/// host has not recorded yet, newest first up to a fixed bound, and the newest
/// ended ones it has recorded, up to a fixed bound per owner. The ended ones
/// past either bound go to the pack, so no outcome is lost.
pub fn retained(
	sessions: impl IntoIterator<Item = FundingSession>,
) -> (Vec<FundingSession>, Vec<FundingSessionSummary>) {
	let (mut open, ended): (Vec<_>, Vec<_>) =
		sessions.into_iter().partition(FundingSession::serves_provider);
	let (mut unrecorded, mut recorded): (Vec<_>, Vec<_>) =
		ended.into_iter().partition(|session| !session.acknowledged);
	unrecorded.sort_by_key(|session| core::cmp::Reverse(session.settled_at_ms()));
	let unrecorded_overflow = unrecorded.split_off(UNACKNOWLEDGED_LIMIT.min(unrecorded.len()));
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
	let compacted = older.into_iter().chain(unrecorded_overflow);
	(open, compacted.map(FundingSessionSummary::from).collect())
}

/// Read every persisted session.
///
/// A blob that does not decode is set aside under
/// [`CoreStorageKey::FundingSessionsUnreadable`] and reads as no sessions; one
/// in a newer format fails with [`FundingSessionError::NewerFormat`] and stays
/// where it is.
pub async fn load_sessions(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Vec<FundingSession>, FundingSessionError> {
	load_versioned(storage, CoreStorageKey::FundingSessions, SESSIONS_FORMAT).await
}

/// Read the session pack, in no particular order.
///
/// A blob that does not decode is set aside under
/// [`CoreStorageKey::FundingSessionsUnreadable`] and reads as an empty pack;
/// one in a newer format fails with [`FundingSessionError::NewerFormat`] and
/// stays where it is.
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
/// where it is. A blob in a newer format is not this core's to move: loading
/// fails with [`FundingSessionError::NewerFormat`] and leaves it, so the
/// registry, which writes only after a load, never writes over it.
async fn load_versioned<T: Decode>(
	storage: &(impl CoreStorage + ?Sized),
	key: CoreStorageKey,
	format: u16,
) -> Result<Vec<T>, FundingSessionError> {
	let storage_error = |err: GenericError| FundingSessionError::Storage { reason: err.reason };
	let Some(blob) = storage.read_core_storage(key.clone()).await.map_err(storage_error)? else {
		return Ok(Vec::new());
	};
	match decode_versioned(&blob, format) {
		Decoded::Records(records) => return Ok(records),
		Decoded::Newer(stored) => return Err(FundingSessionError::NewerFormat { format: stored }),
		Decoded::Unreadable => {},
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

/// What a stored list turned out to be.
enum Decoded<T> {
	/// A list in the format this core writes.
	Records(Vec<T>),
	/// A list in a newer format, left for the app that wrote it.
	Newer(u16),
	/// Anything else: an older format or a corrupt blob.
	Unreadable,
}

/// Decode a stored list that leads with `format`.
fn decode_versioned<T: Decode>(blob: &[u8], format: u16) -> Decoded<T> {
	let input = &mut &blob[..];
	let Ok(stored) = u16::decode(input) else {
		return Decoded::Unreadable;
	};
	if stored > format {
		return Decoded::Newer(stored);
	}
	match Vec::<T>::decode(input) {
		Ok(records) if stored == format && input.is_empty() => Decoded::Records(records),
		_ => Decoded::Unreadable,
	}
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
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired }
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
				HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired },
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

	// The host owns the history: open sessions come first, then the ones it
	// has not recorded, newest first and not without bound, then the recorded
	// ones kept only so their owner can read how they ended. An unrecorded
	// session past the bound is compacted, not lost.
	#[test]
	fn the_core_keeps_open_then_unrecorded_then_recorded_sessions() {
		let ended = |index: usize, acknowledged| FundingSession {
			acknowledged,
			..expired(&format!("fs_s{index}"), NOW + index as u64)
		};
		let open = session(FundingDirection::Out);
		let recorded = (0..3).map(|index| ended(index, true));
		let unrecorded = (100..101 + UNACKNOWLEDGED_LIMIT).map(|index| ended(index, false));

		let (kept, compacted) = retained(recorded.chain(unrecorded).chain([open]));
		let kept: Vec<String> = kept.into_iter().map(|session| session.intent).collect();

		let newest_unrecorded =
			(101..101 + UNACKNOWLEDGED_LIMIT).rev().map(|index| format!("fs_s{index}"));
		let recorded_newest_first = (0..3).rev().map(|index| format!("fs_s{index}"));
		assert_eq!(
			(kept, compacted),
			(
				std::iter::once("fs_1".to_string())
					.chain(newest_unrecorded)
					.chain(recorded_newest_first)
					.collect::<Vec<_>>(),
				vec![FundingSessionSummary::from(ended(100, false))],
			)
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
				HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
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
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
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
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
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
				HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
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
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
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
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
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
		reported(&mut over, &[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }]);
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
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
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
		let received = FundingUpdate::PaymentReceived { finalized: true, mismatch: None };
		reported(&mut session, std::slice::from_ref(&received));
		cancelled(&mut session);
		reported(&mut session, &[FundingUpdate::CancelRefused { reason: "no".into() }]);

		assert_eq!(
			(session.assignment().last_update, report(&mut session, FundingUpdate::Converting)),
			(Some(received), Ok(()))
		);
	}

	// A payout already in flight can land after the user accepted a cancel.
	// The payout wins: the session ends paid out instead of waiting for a
	// refund the provider can no longer make.
	#[test]
	fn a_payout_landing_after_an_accepted_cancel_ends_the_withdrawal() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		released.cancel(NOW + 1);
		released
			.report(PROVIDER, offer(5, 95, None), |id| *id, NOW + 2)
			.expect("offered");
		released.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 2);
		let mut failed = released.clone();

		let landed = released.report(
			PROVIDER,
			FundingUpdate::Payout { outcome: FundingPayout::PaidOut },
			|id| *id,
			NOW + 3,
		);
		let failed_payout = failed.report(
			PROVIDER,
			FundingUpdate::Payout { outcome: FundingPayout::Failed { reason: "bank".into() } },
			|id| *id,
			NOW + 3,
		);

		let stored = FundingSession::decode(&mut &released.encode()[..]).expect("decodes");

		assert_eq!(
			(
				landed,
				released.cancel.clone(),
				released.status(NOW + 3),
				stored.status(NOW + 3),
				released.awaits_provider(),
				failed_payout,
				failed.awaits_refund(),
			),
			(
				Ok(()),
				Some(FundingCancel::Overtaken { fee: 5, refund: 95 }),
				FundingStatus::PaidOutAfterCancel,
				FundingStatus::PaidOutAfterCancel,
				false,
				Ok(()),
				true
			)
		);
	}

	#[test]
	fn a_cancel_after_the_payout_overtook_it_is_too_late() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		released.cancel = Some(FundingCancel::Overtaken { fee: 5, refund: 95 });

		assert_eq!(released.pending_cancel_item(), None);
	}

	// A refund after a failed payout is owed in full, so the user cannot be
	// steered into a fee by cancelling then.
	#[test]
	fn no_cancel_once_a_refund_is_owed() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		released
			.report(
				PROVIDER,
				FundingUpdate::Payout { outcome: FundingPayout::Failed { reason: "bank".into() } },
				|id| *id,
				NOW + 2,
			)
			.expect("failed payout");

		assert_eq!(released.cancel(NOW + 3), CancelOutcome::Refused);
	}

	// A refund top-up is sized by the refund the user accepted, not by the
	// debit, so it is recorded on the withdrawal it pays back.
	#[test]
	fn the_refund_due_follows_the_accepted_terms() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		let before = released.refund_due();
		released.cancel(NOW + 1);
		released
			.report(PROVIDER, offer(5, 95, None), |id| *id, NOW + 2)
			.expect("offered");
		released.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 2);

		assert_eq!((before, released.refund_due()), (Some(100), Some(95)));
	}

	// A released withdrawal stays with its provider until it pays out, so
	// the user can still cancel it: on terms the user accepts, the provider
	// refunds the debit less its fee instead of paying out, and the session
	// ends cancelled with what came back.
	#[test]
	fn an_accepted_cancel_of_a_released_withdrawal_is_refunded() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		let asked = released.cancel(NOW + 1);
		released
			.report(PROVIDER, offer(5, 95, None), |id| *id, NOW + 2)
			.expect("offered");
		let offered = released.status(NOW + 2);
		let answer = released.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 2);
		let cancelling = released.status(NOW + 2);
		let paid_out = released.clone().report(
			PROVIDER,
			FundingUpdate::Payout { outcome: FundingPayout::PaidOut },
			|id| *id,
			NOW + 3,
		);
		assert!(released.record_start([6; 32], 90));
		let off_terms = released.clone().report(
			PROVIDER,
			FundingUpdate::Refunding { top_up_id: [6; 32], amount: 90 },
			|id| *id,
			NOW + 3,
		);
		assert!(released.record_start([5; 32], 95));
		let refunding = FundingUpdate::Refunding { top_up_id: [5; 32], amount: 95 };
		let named = released.report(PROVIDER, refunding, |id| *id, NOW + 3);
		let settled = released.settle_refund(95);

		assert_eq!(
			(
				asked,
				offered,
				answer,
				cancelling,
				paid_out,
				off_terms,
				named,
				settled,
				released.status(NOW + 4),
				released.awaits_provider(),
			),
			(
				CancelOutcome::Requested,
				FundingStatus::CancelOffered { fee: 5, refund: 95 },
				CancelConfirmation::Accepted,
				FundingStatus::Cancelling,
				Ok(()),
				Err(HostFundingReportError::OutOfOrder),
				Ok(()),
				true,
				FundingStatus::Cancelled { refunded: Some(95) },
				false,
			)
		);
	}

	// The host words the user's cancel from the status while it is open: a
	// request, an offer to answer, and a refusal until the provider reports
	// its next step, after which the session reads as before.
	#[test]
	fn the_status_follows_the_users_cancel() {
		let mut session = served(FundingDirection::In);
		reported(
			&mut session,
			&[FundingUpdate::PaymentReceived { finalized: true, mismatch: None }],
		);
		cancelled(&mut session);
		let requested = session.status(NOW);
		reported(&mut session, &[FundingUpdate::CancelRefused { reason: "settled".into() }]);
		let refused = session.status(NOW);
		reported(&mut session, &[FundingUpdate::Converting]);
		let carried_on = session.status(NOW);
		cancelled(&mut session);
		reported(&mut session, &[offer(2, 98, None)]);
		let offered = session.status(NOW);
		session.confirm_cancel(None, NOW);

		assert_eq!(
			(requested, refused, carried_on, offered, session.status(NOW)),
			(
				FundingStatus::CancelRequested,
				FundingStatus::CancelRefused { reason: "settled".into() },
				FundingStatus::ConvertingToCash,
				FundingStatus::CancelOffered { fee: 2, refund: 98 },
				FundingStatus::ConvertingToCash,
			)
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
				HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
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

	// The CASH card opens a session with no amount and the user enters it in
	// the overlay, so the provider must be told the amount of the quote the
	// user chose; a product's own amount stands.
	#[test]
	fn the_chosen_quote_sets_an_amount_the_session_lacks() {
		let quote = FundingQuote {
			quote_id: "q1".to_string(),
			send_amount: 5_100,
			receive_amount: 5_000,
			provider_fee: 100,
			network_fee: 0,
			eta_secs: None,
			expires_at: None,
			asset_decimals: 2,
		};
		let choice = FundingChoice {
			quote,
			rail: FundingRail::Card,
			asset: "EUR".to_string(),
			network: None,
		};
		let mut open =
			FundingSession::new("fs_1".to_string(), None, FundingDirection::In, None, NOW);
		let mut asked = session(FundingDirection::In);

		assert!(open.assign(PROVIDER, Some(choice.clone()), NOW));
		assert!(asked.assign(PROVIDER, Some(choice), NOW));

		assert_eq!((open.assignment().amount, asked.assignment().amount), (Some(5_000), Some(100)));
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
			asset_decimals: 2,
		};
		let choice = FundingChoice { quote, rail, asset: "EUR".to_string(), network: None };
		assert!(session.assign(PROVIDER, Some(choice), NOW));
		session
	}

	// Each in-flight line follows the rail: a bank transfer waits for the
	// transfer, a card for the payment and crypto for the deposit, and all
	// convert once the provider has the money. A withdrawal is initiated
	// until released, then converts to the asset on crypto or prepares the
	// payout on card and bank.
	#[test]
	fn the_status_follows_the_direction_and_rail() {
		let status = |direction, rail, updates: &[FundingUpdate]| {
			let mut session = chosen(direction, rail);
			for (index, update) in updates.iter().enumerate() {
				report_at(&mut session, update.clone(), NOW + 1 + index as u64);
			}
			session.status(NOW + 10)
		};
		let received = FundingUpdate::PaymentReceived { finalized: false, mismatch: None };
		let collecting = FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 };
		let released = |rail, asset: &str| {
			let mut session = chosen(FundingDirection::Out, rail);
			if let Some(choice) = session.choice.as_mut() {
				choice.asset = asset.to_string();
			}
			report_at(&mut session, collecting.clone(), NOW + 1);
			assert!(session.settle(100, NOW + 2));
			session.status(NOW + 10)
		};

		assert_eq!(
			[
				status(FundingDirection::In, FundingRail::Bank, &[]),
				status(FundingDirection::In, FundingRail::Card, &[FundingUpdate::AwaitingPayment]),
				status(FundingDirection::In, FundingRail::Crypto, &[]),
				status(FundingDirection::In, FundingRail::Card, &[received]),
				status(FundingDirection::Out, FundingRail::Bank, std::slice::from_ref(&collecting)),
				released(FundingRail::Bank, "EUR"),
				released(FundingRail::Crypto, "USDT"),
			],
			[
				FundingStatus::WaitingForTransfer,
				FundingStatus::WaitingForPayment,
				FundingStatus::WaitingForDeposit,
				FundingStatus::ConvertingToCash,
				FundingStatus::TransactionInitiated,
				FundingStatus::PreparingPayout,
				FundingStatus::ConvertingTo { asset: "USDT".to_string() },
			]
		);
	}

	// A provider that holds the payment and stays silent past the time its
	// quote gave is taking a little longer, which the host learns to read
	// again for; a report starts the clock again.
	#[test]
	fn a_session_past_its_quoted_time_is_taking_longer() {
		let mut session = chosen(FundingDirection::In, FundingRail::Card);
		if let Some(choice) = session.choice.as_mut() {
			choice.quote.eta_secs = Some(60);
		}
		report_at(
			&mut session,
			FundingUpdate::PaymentReceived { finalized: false, mismatch: None },
			NOW,
		);
		let before = session.progress(NOW + 59_999);
		let after = session.status(NOW + 60_000);
		report_at(&mut session, FundingUpdate::Converting, NOW + 60_001);

		assert_eq!(
			(before.status, before.longer_at_ms, after, session.status(NOW + 60_002)),
			(
				FundingStatus::ConvertingToCash,
				Some(NOW + 60_000),
				FundingStatus::TakingLonger,
				FundingStatus::ConvertingToCash,
			)
		);
	}

	// Until the provider holds the payment a top-up waits on the user: a
	// transfer or deposit that takes days is not the provider taking longer,
	// so the waiting status stays and the host has no time to read again at.
	// A withdrawal keeps its clock from the quote, so by then it is taking
	// longer (and the moment, already past, is not given again).
	#[test]
	fn a_top_up_waiting_on_the_users_payment_is_never_taking_longer() {
		let quoted = |direction, rail| {
			let mut session = chosen(direction, rail);
			if let Some(choice) = session.choice.as_mut() {
				choice.quote.eta_secs = Some(60);
			}
			session
		};
		let mut withdrawal = quoted(FundingDirection::Out, FundingRail::Bank);
		report_at(
			&mut withdrawal,
			FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 },
			NOW,
		);
		let later = NOW + 600_000;
		let waiting = |session: &FundingSession| {
			let progress = session.progress(later);
			(progress.status, progress.longer_at_ms)
		};

		assert_eq!(
			[
				waiting(&quoted(FundingDirection::In, FundingRail::Bank)),
				waiting(&quoted(FundingDirection::In, FundingRail::Crypto)),
				waiting(&quoted(FundingDirection::In, FundingRail::Card)),
				waiting(&withdrawal),
			],
			[
				(FundingStatus::WaitingForTransfer, None),
				(FundingStatus::WaitingForDeposit, None),
				(FundingStatus::WaitingForPayment, None),
				(FundingStatus::TakingLonger, None),
			]
		);
	}

	// How a session ended is one line too: credited, paid out, the payment
	// given back, a failed payout with what its refund returned, or a failure
	// with its reason.
	#[test]
	fn the_status_says_how_the_session_ended() {
		let mut delivered = chosen(FundingDirection::In, FundingRail::Card);
		assert!(delivered.settle(100, NOW + 1));
		let mut refunded = chosen(FundingDirection::In, FundingRail::Card);
		assert!(refunded.fail(FundingFailure::Refunded, NOW + 1));
		let mut expired = chosen(FundingDirection::In, FundingRail::Card);
		assert!(expired.fail(FundingFailure::Expired, NOW + 1));
		let paid = |outcome| {
			let mut session = chosen(FundingDirection::Out, FundingRail::Card);
			report_at(
				&mut session,
				FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 },
				NOW + 1,
			);
			assert!(session.settle(100, NOW + 2));
			report_at(&mut session, FundingUpdate::Payout { outcome }, NOW + 3);
			session
		};
		let mut payout_failed = paid(FundingPayout::Failed { reason: "closed".to_string() });
		let failed_before_refund = payout_failed.status(NOW + 4);
		report_at(
			&mut payout_failed,
			FundingUpdate::Refunding { top_up_id: [6; 32], amount: 95 },
			NOW + 4,
		);
		assert!(payout_failed.settle_refund(95));

		assert_eq!(
			[
				delivered.status(NOW + 5),
				paid(FundingPayout::PaidOut).status(NOW + 5),
				refunded.status(NOW + 5),
				failed_before_refund,
				payout_failed.status(NOW + 5),
				expired.status(NOW + 5),
			],
			[
				FundingStatus::ToppedUp,
				FundingStatus::Sent,
				FundingStatus::Refunded,
				FundingStatus::PayoutFailed { refunded: None },
				FundingStatus::PayoutFailed { refunded: Some(95) },
				FundingStatus::Failed { reason: FundingFailure::Expired },
			]
		);
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

	// Hosts decide from this whether a second cancel can still end the
	// session, so it must say exactly what the core's own rule says.
	#[test]
	fn progress_reports_money_moved_as_the_core_rules_it() {
		let fresh = served(FundingDirection::In);
		let mut paid = served(FundingDirection::In);
		reported(&mut paid, &[FundingUpdate::PaymentReceived { finalized: false, mismatch: None }]);
		let mut started = served(FundingDirection::Out);
		assert!(started.record_start([7; 32], 100));

		let states = [&fresh, &paid, &started]
			.map(|session| (session.progress(NOW).money_moved, session.money_moved()));

		assert_eq!(states, [(false, false), (true, true), (true, true)]);
	}

	// The bar follows the design per rail: a card payment is approved once it
	// can no longer be reversed, a bank transfer goes straight from payment
	// to added, and a withdrawal runs on to the provider's payout. A step the
	// provider skipped takes the time of the first later one, so the bar
	// never shows a gap behind it.
	#[test]
	fn progress_follows_the_rail_and_fills_skipped_steps() {
		let mut card = chosen(FundingDirection::In, FundingRail::Card);
		report_at(
			&mut card,
			FundingUpdate::PaymentReceived { finalized: false, mismatch: None },
			NOW + 1,
		);
		report_at(&mut card, FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 }, NOW + 3);
		let mut bank = chosen(FundingDirection::In, FundingRail::Bank);
		report_at(
			&mut bank,
			FundingUpdate::PaymentReceived { finalized: false, mismatch: None },
			NOW + 2,
		);
		let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
		report_at(
			&mut out,
			FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 },
			NOW + 1,
		);
		assert!(out.settle(100, NOW + 4));
		report_at(&mut out, FundingUpdate::Payout { outcome: FundingPayout::PaidOut }, NOW + 6);

		assert_eq!(
			(steps(&card.progress(NOW)), steps(&bank.progress(NOW)), steps(&out.progress(NOW))),
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
					(FundingStep::Added, None),
				],
				vec![
					(FundingStep::Started, Some(NOW)),
					(FundingStep::Payment, Some(NOW + 1)),
					(FundingStep::Sent, Some(NOW + 4)),
					(FundingStep::Conversion, Some(NOW + 6)),
					(FundingStep::Payout, Some(NOW + 6)),
				],
			)
		);
	}

	// After a withdrawal is released the provider still converts what it
	// collected and then pays the user out; each is reported once and in
	// that order, and an inbound session takes neither once it has ended.
	#[test]
	fn a_released_withdrawal_takes_converting_then_payout_once_each() {
		let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
		report_at(
			&mut out,
			FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 },
			NOW + 1,
		);
		assert!(out.settle(100, NOW + 2));
		let mut delivered = chosen(FundingDirection::In, FundingRail::Card);
		report_at(
			&mut delivered,
			FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 },
			NOW + 1,
		);
		assert!(delivered.settle(100, NOW + 2));
		let paid_out = || FundingUpdate::Payout { outcome: FundingPayout::PaidOut };

		assert_eq!(
			(
				out.report(PROVIDER, FundingUpdate::Converting, |id| *id, NOW + 3),
				out.report(PROVIDER, FundingUpdate::Converting, |id| *id, NOW + 4),
				out.report(PROVIDER, paid_out(), |id| *id, NOW + 5),
				out.report(PROVIDER, FundingUpdate::Converting, |id| *id, NOW + 6),
				delivered.report(PROVIDER, FundingUpdate::Converting, |id| *id, NOW + 3),
			),
			(
				Ok(()),
				Err(HostFundingReportError::OutOfOrder),
				Ok(()),
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::NotFound),
			)
		);
	}

	// A provider retrying a payout says so as often as it tries again, only
	// between the release and the outcome; the session shows it is retrying
	// until the provider converts again or reports how the payout went.
	#[test]
	fn a_released_withdrawal_takes_payout_retries_until_the_outcome() {
		let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
		let retrying = || FundingUpdate::PayoutRetrying;
		let early = out.report(PROVIDER, retrying(), |id| *id, NOW + 1);
		report_at(
			&mut out,
			FundingUpdate::Collecting { payment_id: [2; 32], amount: 100 },
			NOW + 1,
		);
		assert!(out.settle(100, NOW + 2));
		let preparing = out.status(NOW + 2);
		let first = out.report(PROVIDER, retrying(), |id| *id, NOW + 3);
		let after_first = out.status(NOW + 3);
		let again = out.report(PROVIDER, retrying(), |id| *id, NOW + 4);
		let converting = out.report(PROVIDER, FundingUpdate::Converting, |id| *id, NOW + 5);
		let after_converting = out.status(NOW + 5);
		let after_conversion = out.report(PROVIDER, retrying(), |id| *id, NOW + 6);
		let retrying_again = out.status(NOW + 6);
		let paid_out = out.report(
			PROVIDER,
			FundingUpdate::Payout { outcome: FundingPayout::PaidOut },
			|id| *id,
			NOW + 7,
		);
		let late = out.report(PROVIDER, retrying(), |id| *id, NOW + 8);

		assert_eq!(
			(early, first, again, converting, after_conversion, paid_out, late),
			(
				Err(HostFundingReportError::OutOfOrder),
				Ok(()),
				Ok(()),
				Ok(()),
				Ok(()),
				Ok(()),
				Err(HostFundingReportError::OutOfOrder),
			)
		);
		assert_eq!(
			(preparing, after_first, after_converting, retrying_again, out.status(NOW + 8)),
			(
				FundingStatus::PreparingPayout,
				FundingStatus::RetryingPayout,
				FundingStatus::PreparingPayout,
				FundingStatus::RetryingPayout,
				FundingStatus::Sent,
			)
		);
		assert_eq!(out.expires_at(), None);
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
		let progress = session.progress(NOW);

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
		let progress = session.progress(NOW);
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

	// The user's CASH has already left when a payout fails, so the session
	// stays Released and the provider's payout outcome is kept beside it,
	// once. A payout reported before the release is out of order, and one
	// reported after the host recorded the session hands it to the host
	// again.
	#[test]
	fn a_released_session_takes_one_payout_outcome() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		let failed = FundingPayout::Failed { reason: "bank rejected the transfer".to_string() };
		let paid_out = || FundingUpdate::Payout { outcome: FundingPayout::PaidOut };
		let mut recorded = released.clone();
		recorded.acknowledged = true;
		let first = released.report(
			PROVIDER,
			FundingUpdate::Payout { outcome: failed.clone() },
			|id| *id,
			NOW + 2,
		);
		let second = released.report(PROVIDER, paid_out(), |id| *id, NOW + 3);
		let mut in_flight = served(FundingDirection::Out);
		let too_early = in_flight.report(PROVIDER, paid_out(), |id| *id, NOW);
		let after_recording = recorded.report(PROVIDER, paid_out(), |id| *id, NOW + 4);

		assert_eq!(
			(
				first,
				second,
				too_early,
				after_recording,
				recorded.acknowledged,
				released.progress(NOW).payout,
			),
			(
				Ok(()),
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::OutOfOrder),
				Ok(()),
				false,
				Some(failed),
			)
		);
	}

	// The host may record a withdrawal as soon as it is released, but the
	// provider has yet to pay the user out, so the session is served until it
	// reports how that went. After that it stays only as a record its owner
	// can read.
	#[test]
	fn a_recorded_withdrawal_is_kept_until_its_payout_is_reported() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		released.acknowledged = true;
		let served_before =
			retained([released.clone()]).0.iter().any(FundingSession::serves_provider);
		released
			.report(
				PROVIDER,
				FundingUpdate::Payout { outcome: FundingPayout::PaidOut },
				|id| *id,
				NOW + 2,
			)
			.expect("reported");
		released.acknowledged = true;

		let (kept_after, _) = retained([released]);
		assert_eq!(
			(
				served_before,
				kept_after.len(),
				kept_after.iter().any(FundingSession::serves_provider)
			),
			(true, 1, false)
		);
	}

	// A failed payout leaves the user's CASH with the provider, so the
	// provider gives it back with a top-up it names in `Refunding`, once and
	// only after the failure. The session is followed until that top-up's
	// claims are final, then handed to the host again with what came back.
	#[test]
	fn a_failed_payout_is_refunded_through_a_named_top_up() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		let refunding = |amount| FundingUpdate::Refunding { top_up_id: [5; 32], amount };
		let early = released.clone().report(PROVIDER, refunding(95), |id| *id, NOW + 2);
		let failed = FundingPayout::Failed { reason: "card closed".to_string() };
		released
			.report(PROVIDER, FundingUpdate::Payout { outcome: failed }, |id| *id, NOW + 2)
			.expect("reported");
		let unrecorded = released.clone().report(PROVIDER, refunding(95), |id| *id, NOW + 3);
		assert!(released.record_start([5; 32], 95));
		let other_amount = released.clone().report(PROVIDER, refunding(90), |id| *id, NOW + 3);
		let named = released.report(PROVIDER, refunding(95), |id| *id, NOW + 3);
		let again = released.clone().report(PROVIDER, refunding(95), |id| *id, NOW + 4);
		let due = released.settlement_due();
		released.acknowledged = true;
		let settled = released.settle_refund(95);

		assert_eq!(
			(early, unrecorded, other_amount, named, again, due),
			(
				Err(HostFundingReportError::OutOfOrder),
				Err(HostFundingReportError::NotStarted),
				Err(HostFundingReportError::NotStarted),
				Ok(()),
				Err(HostFundingReportError::OutOfOrder),
				Some(Settlement::Refund(StartedFunds {
					host_id: [5; 32],
					amount: 95,
					named_at_ms: Some(NOW + 3),
				})),
			)
		);
		assert_eq!(
			(
				settled,
				released.progress(NOW).refunded,
				released.acknowledged,
				released.awaits_provider(),
				released.settlement_due(),
			),
			(true, Some(95), false, false, None)
		);
	}

	// Until the provider refunds a failed payout it still owes the user, so
	// the session stays served and never expires.
	#[test]
	fn a_failed_payout_is_served_until_refunded() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		let failed = FundingPayout::Failed { reason: "card closed".to_string() };
		released
			.report(PROVIDER, FundingUpdate::Payout { outcome: failed }, |id| *id, NOW + 2)
			.expect("reported");
		released.acknowledged = true;

		assert_eq!(
			(
				released.serves_provider(),
				retained([released.clone()]).0.len(),
				released.expires_at()
			),
			(true, 1, None)
		);
	}

	// A provider keeps what it saved until it has reported the payout, as a
	// restarted worker still needs its order to pay the user out.
	#[test]
	fn a_released_withdrawal_keeps_its_saved_state_until_the_payout() {
		let mut released = served(FundingDirection::Out);
		reported(&mut released, &[FundingUpdate::Collecting { payment_id: [3; 32], amount: 100 }]);
		assert!(released.settle(100, NOW + 1));
		let saved_while_releasing = released.save(PROVIDER, b"order-7".to_vec());
		released
			.report(
				PROVIDER,
				FundingUpdate::Payout { outcome: FundingPayout::PaidOut },
				|id| *id,
				NOW + 2,
			)
			.expect("reported");

		assert_eq!(
			(saved_while_releasing, released.save(PROVIDER, b"order-8".to_vec())),
			(Ok(()), Err(SaveRefusal::NotFound))
		);
	}

	// A second top-up after a partial claim is the provider retrying, which
	// the host shows as "Retrying your payment"; one top-up is not.
	#[test]
	fn a_second_top_up_reads_as_retrying() {
		let mut session = served(FundingDirection::In);
		reported(&mut session, &[FundingUpdate::Crediting { top_up_id: [1; 32], amount: 60 }]);
		let after_one = session.progress(NOW).retrying;
		reported(&mut session, &[FundingUpdate::Crediting { top_up_id: [2; 32], amount: 40 }]);

		assert_eq!((after_one, session.progress(NOW).retrying), (false, true));
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
		let corrupt = |tail: u8| {
			let mut blob = SESSIONS_FORMAT.encode();
			blob.extend([0xff, tail]);
			blob
		};
		let first = corrupt(1);
		let second = corrupt(2);
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

	// An app downgraded below the one that stored its sessions cannot read
	// them, and must not move or overwrite them: the newer app reads them
	// again once it is back.
	#[test]
	fn a_blob_of_a_newer_format_is_left_where_it_is() {
		let storage = stub_platform();
		let blob = (SESSIONS_FORMAT + 1, vec![session(FundingDirection::In)]).encode();
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
			.expect("written");

		assert_eq!(
			(
				block_on(load_sessions(storage.as_ref())),
				block_on(storage.read_core_storage(CoreStorageKey::FundingSessions)),
				block_on(storage.read_core_storage(CoreStorageKey::FundingSessionsUnreadable)),
			),
			(
				Err(FundingSessionError::NewerFormat { format: SESSIONS_FORMAT + 1 }),
				Ok(Some(blob)),
				Ok(None),
			)
		);
	}

	// A blob of an older format is not this core's either, but it is not
	// coming back, so it is set aside like a corrupt one.
	#[test]
	fn a_blob_of_an_older_format_is_kept_aside() {
		let storage = stub_platform();
		let blob = (0u16, vec![session(FundingDirection::In)]).encode();
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
		let mut blob = PACK_FORMAT.encode();
		blob.extend([0xff, 0x01]);
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

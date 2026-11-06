//! Core-owned funding sessions and their status machine.
//!
//! A session is one funding intent in flight. The core owns it, not the host
//! overlay that opened it, so dismissing the overlay after starting is not
//! cancelling: the session keeps running, persists across app death in the
//! core database, and is re-attached by `Funding::status_subscribe`. It
//! expires on the core's own clock, whether or not anyone watches it: 24 hours
//! after it opened while no provider has taken it, and 7 days after its
//! provider's last update while no money moved. Once money moved, only the
//! provider ends the session, by settling it, or by cancelling it on terms the
//! user accepted.

use parity_scale_codec::{Decode, Encode};
use tracing::error;
use truapi::latest::{
	FUNDING_DEPOSIT_URI_MAX_BYTES, FUNDING_DETAIL_MAX_BYTES, FUNDING_DETAIL_MAX_CHARS,
	FundingAssignment, FundingCancelAnswer, FundingCancelDecision, FundingDeposit, FundingDetail,
	FundingDirection, FundingFact, FundingFailure, FundingFrameTarget, FundingInboundUpdate,
	FundingOutboundUpdate, FundingPayout, FundingQuote, FundingRail, FundingReceived,
	FundingUpdate, GenericError, HostFundingReportError, HostFundingStatusSubscribeItem,
};
use url::Url;

use crate::platform::ProductId;

/// How long a session may stay open before it expires.
const SESSION_WINDOW_MS: u64 = 24 * 60 * 60 * 1_000;
/// How many ended sessions the host has not acknowledged yet are handed to it
/// again when funding resumes, newest first, so a host that never
/// acknowledges them is not handed an ever longer list.
pub const UNACKNOWLEDGED_LIMIT: usize = 200;
/// Most top-ups a provider may name for one session, and most top-ups or
/// payments it may hold started for one at a time, as getcash claims or pays
/// in up to three attempts, each under a new id after the host refused the
/// last.
const TOP_UP_LIMIT: usize = 3;
/// How many `Details` updates a session keeps.
const DETAILS_LIMIT: usize = 8;
/// How many sets of deposit instructions a session keeps.
const DEPOSITS_LIMIT: usize = 8;
/// Most decimals an asset's amount can carry and still fit a `u128`.
const DECIMALS_LIMIT: u8 = 38;
/// URI schemes deposit instructions cannot use: a page, a script or a file
/// instead of a payment.
const NOT_PAYMENT_SCHEMES: &[&str] = &[
	"about",
	"blob",
	"data",
	"file",
	"ftp",
	"http",
	"https",
	"javascript",
	"vbscript",
	"ws",
	"wss",
];
/// How long an assigned session with no funds moving waits for its
/// provider's next update before it expires, so a provider that went silent
/// cannot hold the session and its worker for good. A bank transfer can take
/// days, so this is longer than the unassigned window.
const PROVIDER_SILENCE_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
/// How long a top-up the user cancelled before paying stays open to a payment
/// that arrives anyway, as a bank transfer already sent can take days.
const LATE_ARRIVAL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
/// An ended session as the session pack keeps it once nothing can change it.
/// It keeps what history needs, so its owner can still read how it ended and
/// the host can still list it; the pack is never trimmed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingSessionSummary {
	/// The session's identifier.
	pub intent: String,
	/// Product that opened the session, `None` when the host did.
	pub owner_product_id: Option<ProductId>,
	/// Which way value crossed the boundary.
	pub direction: FundingDirection,
	/// Amount sought, in the user's payment balance units.
	pub amount: Option<u128>,
	/// How the session ended, with what moved and when.
	pub stage: FundingStage,
	/// When the session was opened, in Unix milliseconds.
	pub opened_at_ms: u64,
	/// The provider that served it, `None` when none took it.
	pub provider_id: Option<ProductId>,
	/// The quote the provider was chosen on, with its rail, asset and
	/// network. `None` when no provider took it.
	pub choice: Option<FundingChoice>,
	/// How the host shows its ending: the row's status.
	pub status: FundingStatus,
	/// The provider's payout outcome on a withdrawal, once it reported one.
	pub payout: Option<FundingPayout>,
	/// What a refund returned to the balance once its claims were final.
	pub refunded: Option<u128>,
	/// The provider's latest transaction id.
	pub transaction_id: Option<String>,
	/// The provider's latest reference.
	pub reference: Option<String>,
}

impl From<FundingSession> for FundingSessionSummary {
	fn from(session: FundingSession) -> Self {
		session.summary()
	}
}

impl FundingSession {
	/// The session as history keeps it, leaving the session as it is.
	pub fn summary(&self) -> FundingSessionSummary {
		let ended_at_ms = self.stage.settled_at_ms().unwrap_or(self.opened_at_ms);
		FundingSessionSummary {
			intent: self.intent.clone(),
			owner_product_id: self.owner_product_id.clone(),
			direction: self.direction,
			amount: self.amount,
			stage: self.stage.clone(),
			opened_at_ms: self.opened_at_ms,
			provider_id: self.provider_id.clone(),
			choice: self.choice.clone(),
			status: self.status(ended_at_ms),
			payout: self.payout().cloned(),
			refunded: self.refund.as_ref().and_then(|refund| refund.refunded),
			transaction_id: self.transaction_id(),
			reference: self.reference(),
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
	pub owner_product_id: Option<ProductId>,
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
	/// Whether the host has told the user how the session ended. An ended
	/// session is handed to the host until it has.
	pub acknowledged: bool,
	/// Product id of the provider the user chose. `None` until they choose.
	pub provider_id: Option<ProductId>,
	/// When the provider was assigned, in Unix milliseconds.
	pub assigned_at_ms: Option<u64>,
	/// Where the user's cancel stands, while one is pending or answered.
	pub cancel: Option<FundingCancel>,
	/// Every update the provider reported, oldest first.
	pub updates: Vec<FundingUpdateRecord>,
	/// The top-ups or payment requests the provider started for the
	/// session, oldest first. A `Crediting` or `Collecting` report must name
	/// one the host took.
	pub started: Vec<StartedFunds>,
	/// What the user chose the provider on: its quote, and the rail and
	/// asset it priced. `None` until chosen, or when chosen unquoted.
	pub choice: Option<FundingChoice>,
	/// The refund of a failed payout, once the provider reported starting
	/// it.
	pub refund: Option<FundingRefund>,
	/// The provider's latest `Progress`. Not stored, since the provider
	/// reports it again as it checks.
	#[codec(skip)]
	pub detail: Option<FundingDetailRecord>,
}

/// A provider's latest `Progress`, with the step it was reported in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingDetailRecord {
	/// The fact and its time.
	pub detail: FundingDetail,
	/// The session's step when it was reported; the detail is shown only
	/// while the session is still on it.
	pub step: u32,
	/// When it was reported, in Unix milliseconds.
	pub at_ms: u64,
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

/// A session's progress as a host draws it: its milestones, each with when it
/// was reached, the line under them, and when the provider was last heard.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingProgress {
	/// The milestones in order: Started, Received and Added in; Started,
	/// Received and Sent out. One reached later fills in any skipped before
	/// it with its time.
	pub steps: Vec<FundingProgressStep>,
	/// The line under the milestones, saying what is happening now.
	pub band: FundingBand,
	/// When the provider last reported on the session, a `Progress`
	/// included, or took it, in Unix milliseconds, for "Checked with the
	/// provider 12s ago". `None` until a provider takes the session.
	pub last_heard_at_ms: Option<u64>,
	/// The line under the band: the provider's latest `Progress` fact for
	/// the current step. `None` when it reported none for this step, and the
	/// host shows "Checked with the provider" with `last_heard_at_ms`.
	pub detail: Option<FundingDetail>,
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
/// Each variant names the subtitle hosts show for it in their history, so
/// both apps word a session the same way. A row's title is its amount:
/// "$50 CASH" in flight, "+$50 CASH" in green once added, "-$50 CASH" out,
/// unsigned once nothing moved. The rail sits in a pill on the right ("Card",
/// "Bank" or "Crypto"). Sessions in flight are listed under "In progress",
/// ended ones under "Completed", where `{moment}` reads "Today at 12:45",
/// "Yesterday at 12:45", "Jun 12 at 16:40" or, in another year,
/// "Jun 12, 2025".
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingStatus {
	/// Inbound bank: waiting for the user's transfer.
	///
	/// Shown as "Waiting for your transfer".
	#[codec(index = 0)]
	WaitingForTransfer,
	/// Inbound card: waiting for the user to pay on the provider's screens.
	///
	/// Shown as "Waiting for your payment".
	#[codec(index = 1)]
	WaitingForPayment,
	/// Inbound crypto: waiting for the user's deposit.
	///
	/// Shown as "Waiting for your deposit".
	#[codec(index = 2)]
	WaitingForDeposit,
	/// Inbound: the provider has the payment and is turning it into the
	/// user's balance asset.
	///
	/// Shown as "On its way".
	#[codec(index = 3)]
	ConvertingToCash,
	/// Inbound: the provider started crediting the user's balance.
	///
	/// Shown as "Adding to your balance".
	#[codec(index = 4)]
	AddingToBalance,
	/// Outbound: the user started the withdrawal and pays the provider.
	///
	/// Shown as "Withdrawal initiated".
	#[codec(index = 5)]
	TransactionInitiated,
	/// Outbound crypto, after release: the provider converts what it
	/// collected to `asset`.
	///
	/// Shown as "Converting to {asset}…".
	#[codec(index = 6)]
	ConvertingTo {
		/// The asset the user receives.
		asset: String,
	},
	/// Outbound card or bank, after release: the provider prepares the
	/// payout.
	///
	/// Shown as "Preparing your payout".
	#[codec(index = 7)]
	PreparingPayout,
	/// Outbound, after release: a payout attempt failed and the provider
	/// is trying again.
	///
	/// Shown as "The provider is retrying the payout", in amber.
	#[codec(index = 8)]
	RetryingPayout,
	/// Inbound: the provider is retrying, with another top-up after one
	/// claimed only part of the amount.
	///
	/// Shown as "The provider is retrying. Don't pay again", in amber.
	#[codec(index = 9)]
	Retrying,
	/// Nothing came from the provider for longer than its quote said the
	/// session takes; on a top-up, only once the provider holds the user's
	/// payment.
	///
	/// Shown in amber as "Delayed, don't pay again" on a top-up, which only
	/// gets here once the user paid, and "Taking longer than usual" on a
	/// withdrawal.
	#[codec(index = 10)]
	TakingLonger,
	/// Inbound, ended: the balance was credited.
	///
	/// Shown as "{moment}".
	#[codec(index = 11)]
	ToppedUp,
	/// Outbound, ended: the provider paid the user out.
	///
	/// Shown as "{moment}".
	#[codec(index = 12)]
	Sent,
	/// Inbound, ended: the provider gave the user's payment back instead of
	/// crediting it.
	///
	/// Shown as "Refunded · {moment}".
	#[codec(index = 13)]
	Refunded,
	/// Outbound: the payout failed after the balance was debited. `refunded`
	/// is what the provider's refund returned, once final.
	///
	/// Shown as "Payout failed · {moment}"; the amount is unsigned.
	#[codec(index = 14)]
	PayoutFailed {
		/// What came back to the balance. `None` until the refund's claims
		/// are final.
		refunded: Option<u128>,
	},
	/// Ended without moving value.
	///
	/// Shown as "Expired · {moment}" when it expired, otherwise "Payment
	/// failed · {moment}"; the detail screen gives the reason.
	#[codec(index = 15)]
	Failed {
		/// Why.
		reason: FundingFailure,
	},
	/// The user asked to cancel and the provider has not answered.
	///
	/// Shown as "Cancel requested, waiting for the provider".
	#[codec(index = 16)]
	CancelRequested,
	/// The provider can cancel on these terms and waits for the user, who
	/// answers with `confirm_funding_cancel`. The offer's expiry is on the
	/// session's `cancel`.
	///
	/// Shown as "Cancel offer waiting for you".
	#[codec(index = 17)]
	CancelOffered {
		/// What the provider keeps, in payment balance units.
		fee: u128,
		/// What the user gets back, in payment balance units.
		refund: u128,
	},
	/// The provider will not cancel, and has reported nothing since. The
	/// session carries on.
	///
	/// Shown as "The provider can't cancel".
	#[codec(index = 18)]
	CancelRefused {
		/// Why, as the provider put it.
		reason: String,
	},
	/// The user accepted the provider's offer and the provider is refunding
	/// the user.
	///
	/// Shown as "Cancelling…".
	#[codec(index = 19)]
	Cancelling,
	/// Outbound, ended: cancelled on the provider's terms after the balance
	/// was debited. `refunded` is what came back, once final.
	///
	/// Shown as "Cancelled · {moment}".
	#[codec(index = 20)]
	Cancelled {
		/// What came back to the balance. `None` until the refund's claims
		/// are final.
		refunded: Option<u128>,
	},
	/// Outbound, ended: the user accepted a cancel, but the payout landed
	/// first, so the money reached its destination.
	///
	/// Shown as "{moment}".
	#[codec(index = 21)]
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

/// A milestone a session reaches, labelled on its timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingStep {
	/// The session was opened.
	///
	/// Shown as "Started".
	Started,
	/// The provider has the user's payment: in, it saw the payment; out,
	/// the user's balance paid it.
	///
	/// Shown as "Received" in and "Received by provider" out.
	Received,
	/// In: the funds reached the user's balance.
	///
	/// Shown as "Added".
	Added,
	/// Out: the provider paid the user out.
	///
	/// Shown as "Sent".
	Sent,
}

/// What is happening now, for the line under a session's milestones.
///
/// Hosts word it with the session's rail, asset and amount: "{rail}" is
/// "card" or "bank", "{asset}" the crypto the user pays or receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingBand {
	/// In: nothing from the user yet.
	///
	/// Shown as "Waiting for your payment" (card), "If you've sent the money
	/// from your bank, it's on its way to us. We'll let you know when it's
	/// arrived." (bank) or "Waiting for your deposit" (crypto).
	AwaitingPayment,
	/// In: the provider saw the payment and is confirming it.
	///
	/// Shown as "Checking your payment" (card or bank) or "Confirming your
	/// {asset} deposit" (crypto).
	CheckingPayment,
	/// In: the payment is confirmed and being converted.
	///
	/// Shown as "Your money is on its way".
	OnItsWay,
	/// In: the provider is crediting the user's balance.
	///
	/// Shown as "Adding {amount} CASH to your balance".
	Adding,
	/// In, ended: the balance was credited.
	///
	/// Shown as "{amount} CASH added to your balance".
	Added,
	/// Out: the user's Cash is being paid to the provider.
	///
	/// Shown as "Converting your Cash".
	Converting,
	/// Out, released: the provider is preparing the payout.
	///
	/// Shown as "The provider is preparing your payout" (card or bank) or
	/// "Exchanging your money for {asset}" (crypto).
	PreparingPayout,
	/// Out, released: the payout is under way.
	///
	/// Shown as "Sending the money to your {rail}" or "Sending your {asset}".
	Sending,
	/// Out, ended: the payout arrived.
	///
	/// Shown as "Sent to your {rail}" or "Your {asset} is on its way to your
	/// wallet".
	Sent,
	/// The provider is retrying a payment or payout.
	///
	/// Shown in amber as "The provider is retrying. Don't pay again." (card
	/// or bank top-up, or "Don't send the money again." by bank) or "The
	/// provider is retrying the payout" (withdrawal).
	Retrying,
	/// Nothing came from the provider for longer than its quote said.
	///
	/// Shown in amber as "Delayed. Your top-up will continue automatically,
	/// don't pay again." (top-up) or "Taking longer than usual. It will
	/// continue automatically." (withdrawal).
	Delayed,
	/// A cancel is being asked, offered or settled; the status says which.
	Cancel,
	/// Ended without moving the money it was for: failed, expired, refunded
	/// or cancelled; the status says which.
	Ended,
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

/// A top-up or payment request the provider started for a session with
/// `payment.topUp` or `payment.request`, recorded before the host is asked.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct StartedFunds {
	/// Which way they move: a top-up a withdrawal is refunded with moves in.
	pub direction: FundingDirection,
	/// The id the host received, with the provider's product hashed in.
	pub host_id: [u8; 32],
	/// Amount it asks to move, which the session settles from.
	pub amount: u128,
	/// Digest of the rest of the request, so a repeat under the same id must
	/// ask the same.
	pub terms: [u8; 32],
	/// When the core recorded it, in Unix milliseconds.
	pub recorded_at_ms: u64,
	/// Whether the host took it. Until then no report can name it, and it
	/// goes once the host is known not to hold it.
	pub accepted: bool,
	/// When a report named it, in Unix milliseconds. A record is named at
	/// most once.
	pub named_at_ms: Option<u64>,
}

/// What recording funds a provider starts for a session found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartRecord {
	/// Recorded now.
	New,
	/// Recorded before with the same amount and terms: a retried call.
	Repeat,
	/// Recorded before under this id with another amount or terms.
	Conflict,
	/// The session holds as many funds as it may.
	Full,
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
		/// Why, as the provider put it; empty when its answer did not fit.
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum CancelConfirmation {
	/// The offer was accepted and passed to the provider to settle.
	Accepted,
	/// The offer was declined. The session carries on.
	Declined,
	/// The offer had lapsed, so it was declined.
	Lapsed,
	/// The session has no offer to answer, or none on the terms the host
	/// showed.
	NoOffer,
	/// Too late: the provider started crediting the user, so the offer is
	/// declined and the session carries on.
	TooLate,
}

/// The terms of a cancel offer the user accepts, as the host showed them.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct CancelTerms {
	/// What the provider keeps, in payment balance units.
	pub fee: u128,
	/// What the user gets back, in payment balance units.
	pub refund: u128,
}

/// A call about the user's cancel the provider still has to hear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelCall {
	/// Ask the provider to stop.
	Cancel,
	/// Tell the provider the user's answer to its offer.
	Settle(FundingCancelDecision),
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
		owner_product_id: Option<ProductId>,
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
			refund: None,
			detail: None,
		}
	}

	/// Assign the session to the provider the user chose, on the quote they
	/// chose it by. A session without an amount takes the amount the quote
	/// prices. Returns whether it was open and not yet assigned.
	pub fn assign(&mut self, provider_id: &ProductId, choice: FundingChoice, now_ms: u64) -> bool {
		if self.is_terminal() || self.provider_id.is_some() {
			return false;
		}
		self.provider_id = Some(provider_id.clone());
		self.assigned_at_ms = Some(now_ms);
		if self.amount.is_none() {
			self.amount = Some(choice.balance_amount(self.direction));
		}
		self.choice = Some(choice);
		true
	}

	/// The session as the provider serving it receives it.
	pub fn assignment(&self) -> FundingAssignment {
		FundingAssignment {
			intent: self.intent.clone(),
			direction: self.direction,
			amount: self.amount,
			expires_at: self.expires_at(),
			last_update: self.last_ranked_update().cloned(),
			quote: self.choice.as_ref().map(|choice| choice.quote.clone()),
		}
	}

	/// The provider's most recent update.
	pub fn last_update(&self) -> Option<&FundingUpdate> {
		self.updates.last().map(|record| &record.update)
	}

	/// The provider's most recent update that moves the session on, which
	/// `Deposit` and `Details` do not.
	pub fn last_ranked_update(&self) -> Option<&FundingUpdate> {
		self.updates
			.iter()
			.rev()
			.map(|record| &record.update)
			.find(|update| update.rank().is_some())
	}

	/// The session's progress for its direction and rail at `now_ms`.
	pub fn progress(&self, now_ms: u64) -> FundingProgress {
		let steps: &[FundingStep] = match self.direction {
			FundingDirection::In => {
				&[FundingStep::Started, FundingStep::Received, FundingStep::Added]
			},
			FundingDirection::Out => {
				&[FundingStep::Started, FundingStep::Received, FundingStep::Sent]
			},
		};
		let first = |matches: fn(&FundingUpdate) -> bool| {
			self.updates
				.iter()
				.find(|record| matches(&record.update))
				.map(|record| record.at_ms)
		};
		let reached = |step: FundingStep| match (step, &self.stage) {
			(FundingStep::Started, _) => Some(self.opened_at_ms),
			(FundingStep::Received, FundingStage::Released { settled_at_ms, .. }) => {
				Some(*settled_at_ms)
			},
			(FundingStep::Received, _) if self.direction == FundingDirection::In => {
				first(|update| {
					matches!(
						update,
						FundingUpdate::In(FundingInboundUpdate::PaymentReceived { .. }) |
							FundingUpdate::In(FundingInboundUpdate::Converting) |
							FundingUpdate::In(FundingInboundUpdate::Crediting { .. })
					)
				})
			},
			(FundingStep::Added, FundingStage::Delivered { settled_at_ms, .. }) => {
				Some(*settled_at_ms)
			},
			(FundingStep::Sent, FundingStage::Released { .. }) => match self.payout() {
				Some(FundingPayout::PaidOut) => first(|update| {
					matches!(update, FundingUpdate::Out(FundingOutboundUpdate::Payout { .. }))
				}),
				_ => None,
			},
			_ => None,
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
			band: self.band(now_ms),
			last_heard_at_ms: self
				.updates
				.last()
				.map(|record| record.at_ms)
				.max(self.detail.as_ref().map(|record| record.at_ms))
				.max(self.assigned_at_ms),
			detail: self
				.detail
				.as_ref()
				.filter(|record| record.step == self.step())
				.map(|record| record.detail.clone()),
			failed_at_ms: match self.stage {
				FundingStage::Failed { settled_at_ms, .. } => Some(settled_at_ms),
				_ => None,
			},
			transaction_id: self.transaction_id(),
			reference: self.reference(),
			deposit: self.latest(|update| match update {
				FundingUpdate::In(FundingInboundUpdate::Deposit { deposit }) => Some(deposit),
				_ => None,
			}),
			mismatch: self
				.latest(|update| match update {
					FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
						mismatch, ..
					}) => Some(mismatch),
					_ => None,
				})
				.flatten(),
			retrying: self.is_progress_retrying(),
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
			FundingStage::Released { .. } if self.is_cancel_accepted() => {
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
			Some(FundingCancel::Refused { reason }) => {
				return FundingStatus::CancelRefused { reason: reason.clone() };
			},
			Some(FundingCancel::Accepted { .. }) => return FundingStatus::Cancelling,
			Some(FundingCancel::Declined | FundingCancel::Overtaken { .. }) | None => {},
		}
		if self.is_progress_retrying() {
			return FundingStatus::Retrying;
		}
		if self.is_payout_retrying() {
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
			(FundingDirection::In, _) => match self.last_ranked_update() {
				None | Some(FundingUpdate::In(FundingInboundUpdate::AwaitingPayment)) => match rail
				{
					Some(FundingRail::Bank) => FundingStatus::WaitingForTransfer,
					Some(FundingRail::Crypto) => FundingStatus::WaitingForDeposit,
					Some(FundingRail::Card) | None => FundingStatus::WaitingForPayment,
				},
				Some(
					FundingUpdate::In(FundingInboundUpdate::Crediting { .. }) |
					FundingUpdate::In(FundingInboundUpdate::Delivered),
				) => FundingStatus::AddingToBalance,
				Some(_) => FundingStatus::ConvertingToCash,
			},
		}
	}

	/// The line under the session's milestones at `now_ms`.
	fn band(&self, now_ms: u64) -> FundingBand {
		match self.status(now_ms) {
			FundingStatus::ToppedUp => return FundingBand::Added,
			FundingStatus::Sent | FundingStatus::PaidOutAfterCancel => return FundingBand::Sent,
			FundingStatus::Retrying | FundingStatus::RetryingPayout => {
				return FundingBand::Retrying;
			},
			FundingStatus::TakingLonger => return FundingBand::Delayed,
			FundingStatus::CancelRequested |
			FundingStatus::CancelOffered { .. } |
			FundingStatus::CancelRefused { .. } |
			FundingStatus::Cancelling => return FundingBand::Cancel,
			FundingStatus::Refunded |
			FundingStatus::PayoutFailed { .. } |
			FundingStatus::Failed { .. } |
			FundingStatus::Cancelled { .. } => return FundingBand::Ended,
			_ => {},
		}
		match (self.direction, &self.stage) {
			(FundingDirection::Out, FundingStage::Released { .. }) => {
				let converting = self.updates.iter().any(|record| {
					matches!(record.update, FundingUpdate::Out(FundingOutboundUpdate::Converting))
				});
				if converting { FundingBand::Sending } else { FundingBand::PreparingPayout }
			},
			(FundingDirection::Out, _) => FundingBand::Converting,
			(FundingDirection::In, _) => match self.last_ranked_update() {
				None | Some(FundingUpdate::In(FundingInboundUpdate::AwaitingPayment)) => {
					FundingBand::AwaitingPayment
				},
				Some(FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
					finalized: false,
					..
				})) => FundingBand::CheckingPayment,
				Some(
					FundingUpdate::In(FundingInboundUpdate::Crediting { .. }) |
					FundingUpdate::In(FundingInboundUpdate::Delivered),
				) => FundingBand::Adding,
				Some(_) => FundingBand::OnItsWay,
			},
		}
	}

	/// When a session the provider still works on counts as taking longer:
	/// its chosen quote's time after the provider's latest report, or after
	/// it was assigned. `None` without a quoted time, while a top-up waits on
	/// the user's payment, or once it ended.
	pub fn longer_at_ms(&self) -> Option<u64> {
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
	fn is_progress_retrying(&self) -> bool {
		!self.is_terminal() && self.top_up_ids().len() > 1
	}

	/// Whether the provider's latest word on a released withdrawal's payout,
	/// with none reported yet, is that it is retrying it.
	fn is_payout_retrying(&self) -> bool {
		matches!(self.stage, FundingStage::Released { .. }) &&
			self.payout().is_none() &&
			self.updates.iter().rev().find_map(|record| match record.update {
				FundingUpdate::Out(FundingOutboundUpdate::Converting) => Some(false),
				FundingUpdate::Out(FundingOutboundUpdate::PayoutRetrying) => Some(true),
				_ => None,
			}) == Some(true)
	}

	/// What `field` reads from the provider's most recent update it reads
	/// anything from.
	fn latest<T: Clone>(&self, field: impl Fn(&FundingUpdate) -> Option<&T>) -> Option<T> {
		self.updates.iter().rev().find_map(|record| field(&record.update)).cloned()
	}

	/// The provider's latest transaction id.
	fn transaction_id(&self) -> Option<String> {
		self.latest(|update| match update {
			FundingUpdate::Details { transaction_id, .. } => transaction_id.as_ref(),
			_ => None,
		})
	}

	/// The provider's latest reference.
	fn reference(&self) -> Option<String> {
		self.latest(|update| match update {
			FundingUpdate::Details { reference, .. } => reference.as_ref(),
			_ => None,
		})
	}

	/// Store `update` from `provider_id`. A `Failed` update ends the session.
	///
	/// `host_id` turns a top-up or payment id the provider chose into the id
	/// the host received; a `Crediting` or `Collecting` update must name, by
	/// that id and amount, funds the provider started for this session and no
	/// report named yet.
	pub fn report(
		&mut self,
		provider_id: &ProductId,
		update: FundingUpdate,
		host_id: impl Fn(&[u8; 32]) -> [u8; 32],
		now_ms: u64,
	) -> Result<(), HostFundingReportError> {
		if self.provider_id.as_ref() != Some(provider_id) {
			return Err(HostFundingReportError::NotFound);
		}
		if let FundingUpdate::Progress { detail } = update {
			if !self.serves_provider() {
				return Err(HostFundingReportError::NotFound);
			}
			if !self.fits(&detail) {
				return Err(HostFundingReportError::InvalidDetail);
			}
			self.detail = Some(FundingDetailRecord { detail, step: self.step(), at_ms: now_ms });
			return Ok(());
		}
		if matches!(
			update,
			FundingUpdate::In(FundingInboundUpdate::PaymentReceived { .. }) |
				FundingUpdate::In(FundingInboundUpdate::Converting) |
				FundingUpdate::In(FundingInboundUpdate::Crediting { .. })
		) && self.takes_late_arrival(now_ms)
		{
			self.reopen_for_late_arrival();
		}
		if matches!(
			update,
			FundingUpdate::Out(
				FundingOutboundUpdate::Converting |
					FundingOutboundUpdate::Payout { .. } |
					FundingOutboundUpdate::Refunding { .. } |
					FundingOutboundUpdate::PayoutRetrying
			)
		) {
			if !self.is_terminal() {
				return Err(HostFundingReportError::OutOfOrder);
			}
			return self.report_after_release(update, host_id, now_ms);
		}
		// A released withdrawal's provider still works on its payout, so it
		// can name the payout's transaction or reference until it reports
		// how the payout went.
		if matches!(update, FundingUpdate::Details { .. }) && self.awaits_provider() {
			if !self.detail_fits(&update, now_ms) {
				return Err(HostFundingReportError::InvalidDetail);
			}
			self.updates.push(FundingUpdateRecord { update, at_ms: now_ms });
			return Ok(());
		}
		if self.is_terminal() {
			return Err(HostFundingReportError::NotFound);
		}
		if !self.follows(&update) {
			return Err(HostFundingReportError::OutOfOrder);
		}
		if !self.detail_fits(&update, now_ms) {
			return Err(HostFundingReportError::InvalidDetail);
		}
		if let Some((id, amount)) = update.named_funds() {
			let named = host_id(id);
			let record = self
				.started
				.iter_mut()
				.find(|record| record.host_id == named && record.named_at_ms.is_none())
				.filter(|record| record.accepted && record.amount == amount)
				.ok_or(HostFundingReportError::NotStarted)?;
			record.named_at_ms = Some(now_ms);
		}
		if let FundingUpdate::Failed { reason } = &update {
			self.stage = FundingStage::Failed { reason: reason.clone(), settled_at_ms: now_ms };
		}
		self.updates.push(FundingUpdateRecord { update, at_ms: now_ms });
		// A provider that reports its next step has carried on: a decline it
		// was owed is not repeated, and its refusal is no longer news.
		if self.is_terminal() ||
			matches!(self.cancel, Some(FundingCancel::Declined | FundingCancel::Refused { .. }))
		{
			self.cancel = None;
		}
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
		if let FundingUpdate::Out(FundingOutboundUpdate::Refunding { top_up_id, amount }) = &update
		{
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
			let converting = self.updates.iter().any(|record| {
				matches!(record.update, FundingUpdate::Out(FundingOutboundUpdate::Converting))
			});
			// A payout that lands although the user accepted a cancel won the
			// race: the session ends as paid out. Anything else on the way to
			// a payout is refused once the cancel was accepted.
			let lands = matches!(update, FundingUpdate::Out(FundingOutboundUpdate::Payout { .. }));
			if self.payout().is_some() ||
				(self.is_cancel_accepted() && !lands) ||
				(matches!(update, FundingUpdate::Out(FundingOutboundUpdate::Converting)) &&
					converting)
			{
				return Err(HostFundingReportError::OutOfOrder);
			}
			// The payout won over an accepted cancel: keep what the user
			// accepted, so the ended session says so.
			if matches!(
				update,
				FundingUpdate::Out(FundingOutboundUpdate::Payout {
					outcome: FundingPayout::PaidOut
				})
			) {
				self.cancel = match self.cancel.take() {
					Some(FundingCancel::Accepted { fee, refund }) => {
						Some(FundingCancel::Overtaken { fee, refund })
					},
					_ => None,
				};
			}
		}
		self.updates.push(FundingUpdateRecord { update, at_ms: now_ms });
		if matches!(self.cancel, Some(FundingCancel::Refused { .. })) {
			self.cancel = None;
		}
		self.acknowledged = false;
		Ok(())
	}

	/// Which step the session is on, for telling whether a `Progress` is
	/// still current: its stage and how many of the provider's steps it
	/// reported. References and deposit instructions do not move it.
	fn step(&self) -> u32 {
		let stage = match self.stage {
			FundingStage::Open => 0,
			FundingStage::Failed { .. } => 1,
			FundingStage::Delivered { .. } => 2,
			FundingStage::Released { .. } => 3,
		};
		let steps = self
			.updates
			.iter()
			.filter(|record| {
				!matches!(
					record.update,
					FundingUpdate::In(FundingInboundUpdate::Deposit { .. }) |
						FundingUpdate::Details { .. }
				)
			})
			.count();
		u32::try_from(steps).unwrap_or(u32::MAX >> 2) << 2 | stage
	}

	/// Whether `detail` fits the session: counts within their totals, a
	/// confirmation count on a crypto session, a bank's send on an inbound
	/// bank one, and an `Other` text within its limits.
	fn fits(&self, detail: &FundingDetail) -> bool {
		let rail = self.choice.as_ref().map(|choice| choice.rail);
		match &detail.fact {
			FundingFact::Confirmations { done, required } => {
				*required > 0 &&
					done <= required &&
					rail.is_none_or(|rail| rail == FundingRail::Crypto)
			},
			FundingFact::Parts { done, total } => *total > 0 && done <= total,
			FundingFact::SentByBank => {
				self.direction == FundingDirection::In &&
					rail.is_none_or(|rail| rail == FundingRail::Bank)
			},
			FundingFact::Attempt { n, of } => *n > 0 && of.is_none_or(|of| of >= *n),
			FundingFact::Other { text } => is_plain_line(text),
			FundingFact::Checked |
			FundingFact::PriceChecked |
			FundingFact::Scheduled |
			FundingFact::ExpectedBy |
			FundingFact::InReview => true,
		}
	}

	/// Whether the session's provider still has work on it: it is in flight,
	/// or it ended and the provider still owes the user something.
	pub fn serves_provider(&self) -> bool {
		!self.is_terminal() || self.awaits_provider()
	}

	/// Whether a provider is assigned and still has work on the session.
	pub fn has_working_provider(&self) -> bool {
		self.provider_id.is_some() && self.serves_provider()
	}

	/// Whether the session ended but its provider still owes the user: a
	/// released withdrawal whose payout outcome it has not reported, or
	/// whose refund, after a failed payout or a cancel the user accepted, is
	/// not settled yet. Whether the host acknowledged the session does not
	/// matter.
	fn awaits_provider(&self) -> bool {
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
				self.is_cancel_accepted())
	}

	/// Whether the user accepted the provider's cancel offer.
	fn is_cancel_accepted(&self) -> bool {
		matches!(self.cancel, Some(FundingCancel::Accepted { .. }))
	}

	/// The provider's payout outcome, once it reported one.
	fn payout(&self) -> Option<&FundingPayout> {
		self.updates.iter().find_map(|record| match &record.update {
			FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome }) => Some(outcome),
			_ => None,
		})
	}

	/// Whether `update`'s own data is within its limits. `Details` and
	/// `Deposit` sit outside the order of the others, so the session keeps
	/// a bounded number of each; their texts are short plain lines; deposit
	/// instructions name something to pay, have not expired, and pay by the
	/// rail and asset of the quote the user chose, when there was one.
	fn detail_fits(&self, update: &FundingUpdate, now_ms: u64) -> bool {
		let kept = |same: fn(&FundingUpdate) -> bool| {
			self.updates.iter().filter(|record| same(&record.update)).count()
		};
		match update {
			FundingUpdate::Details { transaction_id, reference } => {
				kept(|update| matches!(update, FundingUpdate::Details { .. })) < DETAILS_LIMIT &&
					(transaction_id.is_some() || reference.is_some()) &&
					[transaction_id, reference]
						.into_iter()
						.flatten()
						.all(|text| is_plain_line(text))
			},
			FundingUpdate::In(FundingInboundUpdate::Deposit { deposit }) => {
				kept(|update| {
					matches!(update, FundingUpdate::In(FundingInboundUpdate::Deposit { .. }))
				}) < DEPOSITS_LIMIT &&
					deposit.fits(self.choice.as_ref(), now_ms)
			},
			FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
				mismatch: Some(received),
				..
			}) => is_plain_line(&received.asset),
			_ => true,
		}
	}

	/// Whether the provider may offer to cancel on these terms. An inbound
	/// session whose credit started can no longer be cancelled; an outbound
	/// one can be offered only once released, refunded through `Refunding`,
	/// and not after its payout outcome or refund was reported. An offer
	/// cannot hand back more than the session moves.
	fn offer_fits(&self, fee: u128, refund: u128) -> bool {
		let cap = match (self.direction, &self.stage) {
			(FundingDirection::In, _) if self.funds_started() => return false,
			(FundingDirection::In, _) => self.amount,
			(FundingDirection::Out, FundingStage::Released { debited, .. })
				if self.payout().is_none() && self.refund.is_none() =>
			{
				Some(*debited)
			},
			(FundingDirection::Out, _) => return false,
		};
		cap.is_none_or(|cap| fee.saturating_add(refund) <= cap)
	}

	/// Whether `update` may come next. Updates only move forward, a session
	/// may credit through a few top-ups, and once funds move the provider
	/// can no longer fail it: the top-ups or payment decide.
	fn follows(&self, update: &FundingUpdate) -> bool {
		let fits_direction = match update {
			FundingUpdate::In(_) => self.direction == FundingDirection::In,
			FundingUpdate::Out(_) => self.direction == FundingDirection::Out,
			FundingUpdate::Failed { .. } |
			FundingUpdate::Details { .. } |
			FundingUpdate::Progress { .. } => true,
		};
		let last = self.last_ranked_update().and_then(FundingUpdate::rank);
		let rank = update.rank();
		let top_ups = self.top_up_ids();
		fits_direction &&
			match update {
				FundingUpdate::Details { .. } => true,
				FundingUpdate::Failed { .. } => !self.funds_started(),
				FundingUpdate::In(FundingInboundUpdate::Deposit { .. }) => !self.funds_moving(),
				FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id, .. }) => {
					last <= rank && top_ups.len() < TOP_UP_LIMIT && !top_ups.contains(&top_up_id)
				},
				FundingUpdate::In(FundingInboundUpdate::Delivered) => {
					last < rank && !top_ups.is_empty()
				},
				_ => last < rank,
			}
	}

	/// The ids of the top-ups the provider named, in order.
	fn top_up_ids(&self) -> Vec<&[u8; 32]> {
		self.updates
			.iter()
			.filter_map(|record| match &record.update {
				FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id, .. }) => {
					Some(top_up_id)
				},
				_ => None,
			})
			.collect()
	}

	/// Record funds the serving provider starts for the session under
	/// `host_id`, not yet taken by the host. The caller checked the session
	/// takes funds moving in `direction`.
	pub fn record_start(
		&mut self,
		direction: FundingDirection,
		host_id: [u8; 32],
		amount: u128,
		terms: [u8; 32],
		now_ms: u64,
	) -> StartRecord {
		if let Some(record) = self.started.iter().find(|record| record.host_id == host_id) {
			return if record.amount == amount && record.terms == terms {
				StartRecord::Repeat
			} else {
				StartRecord::Conflict
			};
		}
		if self.started.len() >= TOP_UP_LIMIT {
			return StartRecord::Full;
		}
		self.started.push(StartedFunds {
			direction,
			host_id,
			amount,
			terms,
			recorded_at_ms: now_ms,
			accepted: false,
			named_at_ms: None,
		});
		StartRecord::New
	}

	/// Whether a payment for this top-up may still arrive although the user
	/// cancelled it: it was cancelled before money moved, a provider served
	/// it, and less than a week passed. A bank transfer the user had already
	/// sent still reaches the provider then.
	pub fn takes_late_arrival(&self, now_ms: u64) -> bool {
		self.direction == FundingDirection::In &&
			self.provider_id.is_some() &&
			self.started.is_empty() &&
			matches!(
				self.stage,
				FundingStage::Failed { reason: FundingFailure::Cancelled, settled_at_ms }
					if now_ms < settled_at_ms.saturating_add(LATE_ARRIVAL_MS)
			)
	}

	/// Open the cancelled top-up again for the payment that arrived, so it
	/// settles as any other and the host is handed it again.
	pub fn reopen_for_late_arrival(&mut self) {
		self.stage = FundingStage::Open;
		self.cancel = None;
		self.acknowledged = false;
	}

	/// Mark the funds under `host_id` as taken by the host. Returns whether
	/// that changed them.
	pub fn accept_start(&mut self, host_id: &[u8; 32]) -> bool {
		self.started
			.iter_mut()
			.find(|record| record.host_id == *host_id && !record.accepted)
			.map(|record| record.accepted = true)
			.is_some()
	}

	/// Drop the record of funds under `host_id` the host did not take.
	/// Returns whether one went.
	pub fn forget_start(&mut self, host_id: &[u8; 32]) -> bool {
		let before = self.started.len();
		self.started.retain(|record| record.host_id != *host_id || record.accepted);
		self.started.len() != before
	}

	/// The funds recorded for the session that the host has not taken yet.
	pub fn pending_starts(&self) -> impl Iterator<Item = &StartedFunds> {
		self.started.iter().filter(|record| !record.accepted)
	}

	/// Whether funds are moving: the provider named a top-up or a payment
	/// request it started. The named funds then decide the outcome.
	pub fn funds_moving(&self) -> bool {
		self.updates.iter().any(|record| record.update.named_funds().is_some())
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
			self.updates.iter().any(|record| {
				matches!(
					record.update,
					FundingUpdate::In(FundingInboundUpdate::PaymentReceived { .. })
				)
			})
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
		// The last step, not the last update: a detail or deposit must not
		// hide the outcome the provider handed over.
		match self.last_ranked_update()? {
			FundingUpdate::In(FundingInboundUpdate::Delivered) => {
				Some(Settlement::TopUps(named.cloned().collect()))
			},
			FundingUpdate::Out(FundingOutboundUpdate::Collecting { .. }) => {
				named.next().cloned().map(Settlement::Payment)
			},
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
	/// whatever the answer, terms other than the offer's accept nothing, and
	/// an inbound session whose credit started is too late.
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
			return CancelConfirmation::NoOffer;
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

	/// Apply the provider's answer to the user's cancel. An answer is taken
	/// only while the cancel waits for one: `Stopped` before money moved, an
	/// offer only on terms it may make. Any other answer counts as a refusal
	/// with no reason, so the user can ask again. Returns whether it changed
	/// the session.
	pub fn answer_cancel(&mut self, answer: FundingCancelAnswer, now_ms: u64) -> bool {
		if !self.serves_provider() || self.cancel != Some(FundingCancel::Requested) {
			return false;
		}
		let answered = match answer {
			FundingCancelAnswer::Stopped if !self.money_moved() => {
				self.fail(FundingFailure::Cancelled, now_ms)
			},
			FundingCancelAnswer::Offer { fee, refund, expires_at }
				if self.offer_fits(fee, refund) =>
			{
				self.cancel = Some(FundingCancel::Offered { fee, refund, expires_at });
				true
			},
			FundingCancelAnswer::Refused { reason } => {
				self.cancel = Some(FundingCancel::Refused { reason });
				true
			},
			answer => {
				error!(intent = %self.intent, ?answer, "a provider's cancel answer does not fit");
				self.cancel = Some(FundingCancel::Refused { reason: String::new() });
				true
			},
		};
		// A released withdrawal was handed to the host already.
		if answered && self.is_terminal() {
			self.acknowledged = false;
		}
		answered
	}

	/// Record that the provider heard the user's answer to its offer. A
	/// decline is then done with; an accepted offer stays until the provider
	/// ends the session.
	pub fn cancel_settled(&mut self, decision: &FundingCancelDecision) -> bool {
		let heard = matches!(decision, FundingCancelDecision::Declined) &&
			self.cancel == Some(FundingCancel::Declined);
		if heard {
			self.cancel = None;
		}
		heard
	}

	/// What the provider still has to hear about the user's cancel when its
	/// worker connects: a request it has not answered, or the user's answer to
	/// its offer. An accepted offer is repeated until the provider ends the
	/// session, as a worker that stopped before handling it would otherwise
	/// never refund.
	pub fn pending_cancel_call(&self) -> Option<CancelCall> {
		match self.cancel.as_ref()? {
			FundingCancel::Requested => Some(CancelCall::Cancel),
			FundingCancel::Accepted { fee, refund } => {
				Some(CancelCall::Settle(FundingCancelDecision::Accepted {
					fee: *fee,
					refund: *refund,
				}))
			},
			FundingCancel::Declined => Some(CancelCall::Settle(FundingCancelDecision::Declined)),
			// The provider answered already; asking again would invite a new
			// offer while the user looks at this one.
			FundingCancel::Offered { .. } |
			FundingCancel::Refused { .. } |
			FundingCancel::Overtaken { .. } => None,
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

impl FundingUpdate {
	/// The provider's id and amount of the funds this update names, if any.
	pub fn named_funds(&self) -> Option<(&[u8; 32], u128)> {
		match self {
			FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id, amount }) => {
				Some((top_up_id, *amount))
			},
			FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id, amount }) => {
				Some((payment_id, *amount))
			},
			FundingUpdate::Out(FundingOutboundUpdate::Refunding { top_up_id, amount }) => {
				Some((top_up_id, *amount))
			},
			_ => None,
		}
	}

	/// This update's order within a session: a later update ranks higher,
	/// and a top-up and a payment request rank where funds start moving.
	/// `Deposit`, `Details` and what follows a release have no place in the
	/// order.
	fn rank(&self) -> Option<u8> {
		Some(match self {
			FundingUpdate::In(FundingInboundUpdate::AwaitingPayment) => 0,
			FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
				finalized: false, ..
			}) => 1,
			FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
				finalized: true, ..
			}) => 2,
			FundingUpdate::In(FundingInboundUpdate::Converting) => 3,
			FundingUpdate::In(FundingInboundUpdate::Crediting { .. }) |
			FundingUpdate::Out(FundingOutboundUpdate::Collecting { .. }) => 4,
			FundingUpdate::In(FundingInboundUpdate::Delivered) => 5,
			FundingUpdate::Failed { .. } => 6,
			FundingUpdate::In(FundingInboundUpdate::Deposit { .. }) |
			FundingUpdate::Details { .. } |
			FundingUpdate::Out(FundingOutboundUpdate::Payout { .. }) |
			FundingUpdate::Out(FundingOutboundUpdate::Refunding { .. }) |
			FundingUpdate::Out(FundingOutboundUpdate::Converting) |
			FundingUpdate::Out(FundingOutboundUpdate::PayoutRetrying) |
			FundingUpdate::Progress { .. } => return None,
		})
	}
}

impl FundingDeposit {
	/// Whether the deposit names something to pay that has not expired by
	/// `now_ms`, with plain texts, and pays by the rail and asset of `choice`,
	/// the quote the user chose, when there was one.
	fn fits(&self, choice: Option<&FundingChoice>, now_ms: u64) -> bool {
		let live = |expires_at: &Option<u64>| expires_at.is_none_or(|at| at > now_ms);
		let chosen = |rail: FundingRail, asset: &str| {
			choice.is_none_or(|choice| {
				choice.rail == rail && choice.asset.eq_ignore_ascii_case(asset)
			})
		};
		match self {
			FundingDeposit::Crypto {
				address,
				network,
				asset,
				amount,
				decimals,
				uri,
				expires_at,
				..
			} => {
				*amount > 0 &&
					*decimals <= DECIMALS_LIMIT &&
					[address, network, asset].into_iter().all(|text| is_plain_line(text)) &&
					uri.as_deref().is_none_or(is_payment_uri) &&
					live(expires_at) &&
					chosen(FundingRail::Crypto, asset)
			},
			FundingDeposit::Bank {
				amount,
				currency,
				decimals,
				beneficiary,
				account,
				bank_code,
				reference,
				expires_at,
			} => {
				*amount > 0 &&
					*decimals <= DECIMALS_LIMIT &&
					is_plain_line(currency) &&
					is_plain_line(reference) &&
					[beneficiary, account, bank_code]
						.into_iter()
						.flatten()
						.all(|text| is_plain_line(text)) &&
					live(expires_at) &&
					chosen(FundingRail::Bank, currency)
			},
		}
	}
}

/// Why a session operation failed.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum FundingSessionError {
	/// The core database failed, or none is configured.
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

/// `target` as the host may show it: an App route must be a relative route
/// inside the provider's App, and a page and its return URL must be https URLs
/// with a host and no credentials. Both are passed on as parsed, so the host
/// opens exactly what was checked.
pub fn checked_frame_target(target: FundingFrameTarget) -> Option<FundingFrameTarget> {
	match target {
		FundingFrameTarget::App { route } => {
			checked_app_route(&route).map(|route| FundingFrameTarget::App { route })
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

/// `route` as the provider's App is handed it: the path, query and fragment
/// it resolves to inside the App. Only relative syntax is taken: no scheme
/// and no authority, so no host, credentials or port, whether written with
/// `//` or with the backslashes a browser reads as slashes.
fn checked_app_route(route: &str) -> Option<String> {
	const APP: &str = "https://app.invalid/";
	let authority = ["//", "\\", "/\\", "\\/"].iter().any(|lead| route.starts_with(lead));
	if authority || Url::parse(route).is_ok() {
		return None;
	}
	let joined = Url::parse(APP).ok()?.join(route).ok()?;
	let inside = joined.host_str() == Some("app.invalid") &&
		joined.port().is_none() &&
		joined.username().is_empty() &&
		joined.password().is_none();
	inside.then(|| joined[url::Position::BeforePath..].to_string())
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

/// Split `sessions` into those kept in full and the ended ones to compact
/// into the pack. Kept in full: every one its provider still has work on, a
/// released withdrawal whose payout outcome is not in, as a payout can still
/// land after a cancel was refunded, and a cancelled top-up a late payment
/// may still open again. Every other ended session is compacted at once,
/// whether or not the host acknowledged it.
pub fn retained(
	sessions: impl IntoIterator<Item = FundingSession>,
	now_ms: u64,
) -> (Vec<FundingSession>, Vec<FundingSession>) {
	sessions.into_iter().partition(|session| {
		session.serves_provider() ||
			(matches!(session.stage, FundingStage::Released { .. }) &&
				session.payout().is_none()) ||
			session.takes_late_arrival(now_ms)
	})
}

/// Whether `uri` is a payment URI a host can put in a QR code: short, with
/// no spaces or hidden characters, and of a scheme that is not a page, a
/// script or a file.
fn is_payment_uri(uri: &str) -> bool {
	uri.len() <= FUNDING_DEPOSIT_URI_MAX_BYTES &&
		!uri.chars().any(|c| c.is_whitespace() || is_hidden(c)) &&
		Url::parse(uri).is_ok_and(|url| !NOT_PAYMENT_SCHEMES.contains(&url.scheme()))
}

/// Whether `text` is a non-empty line of plain text within the limits a
/// provider's update text keeps to: no line breaks, control characters,
/// invisible characters or characters that change the text's direction.
fn is_plain_line(text: &str) -> bool {
	!text.trim().is_empty() &&
		text.len() <= FUNDING_DETAIL_MAX_BYTES &&
		text.chars().count() <= FUNDING_DETAIL_MAX_CHARS &&
		!text.chars().any(is_hidden)
}

/// Whether `c` breaks a line, is invisible, or changes the text's direction.
fn is_hidden(c: char) -> bool {
	c.is_control() ||
		matches!(c, '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' |
			'\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}')
}

/// A quote chosen for a USDT top-up of 100, for tests.
#[cfg(test)]
pub fn test_choice() -> FundingChoice {
	FundingChoice {
		quote: FundingQuote {
			quote_id: "q-test".to_string(),
			send_amount: 102,
			receive_amount: 100,
			provider_fee: 2,
			network_fee: 0,
			eta_secs: None,
			expires_at: None,
			asset_decimals: 6,
		},
		rail: FundingRail::Crypto,
		asset: "USDT".to_string(),
		network: Some("polkadot".to_string()),
	}
}

#[cfg(test)]
mod tests;

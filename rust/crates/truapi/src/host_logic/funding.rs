//! Core-owned funding sessions and their status machine.
//!
//! A session is one funding intent in flight. The core owns it, not the host
//! overlay that opened it, so dismissing the overlay after starting is not
//! cancelling: the session keeps running, persists across app death through
//! [`CoreStorageKey::FundingSessions`], and is re-attached by
//! `Funding::status_subscribe`. It expires on the core's own clock, whether or
//! not anyone watches it: 24 hours after it opened while no provider has taken
//! it, and 7 days after its provider's last update while no funds move. Once
//! a top-up or payment request moves funds, only its outcome ends the
//! session.

use parity_scale_codec::{Decode, Encode};
use tracing::error;
use truapi::latest::{
	FundingAssignment, FundingDirection, FundingFailure, FundingFrameTarget, FundingUpdate,
	GenericError, HostFundingReportError, HostFundingStatusSubscribeItem,
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
/// Version of the stored sessions blob, which leads it as a `u16`. A change
/// to how [`FundingSession`] encodes bumps it, and [`decode_sessions`] keeps
/// reading every earlier version, so an upgrade keeps the sessions in flight.
const SESSIONS_FORMAT: u16 = 2;

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
	/// Whether the user asked to cancel and the provider has not answered,
	/// while a cancel can still stop the session.
	pub cancel_requested: bool,
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

/// What a cancel did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
	/// The session ended as cancelled: no provider serves it, or its
	/// provider left an earlier cancel unanswered.
	Ended,
	/// The provider serving it was asked to stop.
	Requested,
	/// Too late: the session ended, or the user's payment already reached
	/// the provider.
	Refused,
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
			cancel_requested: false,
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
			last_update: self.last_update().cloned(),
		}
	}

	/// The provider's most recent update.
	pub fn last_update(&self) -> Option<&FundingUpdate> {
		self.updates.last().map(|record| &record.update)
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
		if self.is_terminal() || self.holds_payment() {
			self.cancel_requested = false;
		}
		Ok(())
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
		let last = self.last_update().map(update_rank);
		let top_ups = self.top_up_ids();
		fits_direction &&
			match update {
				FundingUpdate::Failed { .. } => !self.funds_moving(),
				FundingUpdate::Crediting { top_up_id, .. } => {
					last <= Some(update_rank(update)) &&
						top_ups.len() < TOP_UP_LIMIT &&
						!top_ups.contains(&top_up_id)
				},
				FundingUpdate::Delivered => last < Some(update_rank(update)) && !top_ups.is_empty(),
				_ => last < Some(update_rank(update)),
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
	/// request it started. From then on the session neither expires nor
	/// cancels.
	pub fn funds_moving(&self) -> bool {
		self.updates.iter().any(|record| named_funds(&record.update).is_some())
	}

	/// Whether the user's payment reached the provider or funds are moving,
	/// after which a cancel no longer applies.
	fn holds_payment(&self) -> bool {
		self.funds_moving() ||
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
		match self.last_update()? {
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
		self.cancel_requested = false;
		true
	}

	/// Whether the host still has to hear of the session when funding
	/// resumes: it is in flight, or it ended and the host has not recorded
	/// the outcome.
	pub fn needs_handoff(&self) -> bool {
		!self.is_terminal() || !self.acknowledged
	}

	/// Cancel at the user's request: end the session if no provider serves
	/// it, otherwise ask the provider to stop, which it answers by reporting.
	/// A second cancel the provider has not answered ends the session, so a
	/// provider that went silent cannot hold it. Refused once the user's
	/// payment reached the provider, as getcash refuses once anything
	/// arrived.
	pub fn cancel(&mut self, now_ms: u64) -> CancelOutcome {
		if self.is_terminal() || (self.provider_id.is_some() && self.holds_payment()) {
			return CancelOutcome::Refused;
		}
		if self.provider_id.is_none() || self.cancel_requested {
			self.fail(FundingFailure::Cancelled, now_ms);
			return CancelOutcome::Ended;
		}
		self.cancel_requested = true;
		CancelOutcome::Requested
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
		self.cancel_requested = false;
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
	/// provider's last update until the user's payment reaches the provider.
	/// `None` once it ended, the provider holds the user's payment or funds
	/// move: expiring it then would write off money the provider holds.
	pub fn expires_at(&self) -> Option<u64> {
		if self.is_terminal() || self.holds_payment() {
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
/// top-up and a payment request rank where funds start moving.
fn update_rank(update: &FundingUpdate) -> u8 {
	match update {
		FundingUpdate::AwaitingPayment => 0,
		FundingUpdate::PaymentReceived { finalized: false } => 1,
		FundingUpdate::PaymentReceived { finalized: true } => 2,
		FundingUpdate::Converting => 3,
		FundingUpdate::Crediting { .. } | FundingUpdate::Collecting { .. } => 4,
		FundingUpdate::Delivered => 5,
		FundingUpdate::Failed { .. } => 6,
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

/// Decode a stored sessions blob of any version this core reads, or `None`
/// when it is not one.
fn decode_sessions(blob: &[u8]) -> Option<Vec<FundingSession>> {
	let input = &mut &blob[..];
	let sessions = match u16::decode(input).ok()? {
		1 => Vec::<SessionV1>::decode(input)
			.ok()?
			.into_iter()
			.map(FundingSession::from)
			.collect(),
		SESSIONS_FORMAT => Vec::<FundingSession>::decode(input).ok()?,
		_ => return None,
	};
	input.is_empty().then_some(sessions)
}

/// A session as stored in format 1, before providers served sessions.
#[derive(Encode, Decode)]
struct SessionV1 {
	intent: String,
	owner_product_id: Option<String>,
	direction: FundingDirection,
	amount: Option<u128>,
	stage: FundingStage,
	opened_at_ms: u64,
	deadline_ms: u64,
	acknowledged: bool,
}

impl From<SessionV1> for FundingSession {
	fn from(session: SessionV1) -> Self {
		Self {
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
	}
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

	// The provider holds the user's money once it has seen the payment, so
	// only it can stop the session before that, and nobody can after. The
	// pending request is then dropped, so it is not handed to a restarted
	// worker again.
	#[test]
	fn a_served_session_asks_its_provider_until_the_payment_arrives() {
		let mut before = served(FundingDirection::In);
		let asked = before.cancel(NOW);
		let pending = before.cancel_requested;
		reported(&mut before, &[FundingUpdate::PaymentReceived { finalized: false }]);

		assert_eq!(
			(asked, pending, before.is_terminal(), before.cancel_requested, before.cancel(NOW)),
			(CancelOutcome::Requested, true, false, false, CancelOutcome::Refused)
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

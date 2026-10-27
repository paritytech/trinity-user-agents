//! Host-global registry of funding sessions.
//!
//! A session outlives the surface that opened it and is not scoped to one
//! product connection: the Balance card opens one, and a product that reloads
//! attaches to it with `status_subscribe`. So the registry hangs off
//! [`RuntimeServices`] rather than off a product runtime.
//!
//! Every change goes through [`FundingRegistry::commit`], which serialises
//! writes so persisted snapshots land in order, persists before anyone hears of
//! the change, and then notifies subscribers and, once the write lock is
//! released, the host. Background work is never started from a commit
//! directly: a commit wakes the reconciler, one task that derives from the
//! sessions which worker holds, settlement followers, engine lookups, worker
//! hand-overs and timers must exist, and brings them in line one pass at a
//! time.

use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::{
		Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
};

use core::time::Duration;

use futures::{
	channel::{mpsc, oneshot},
	future::{BoxFuture, Either},
	lock::Mutex as AsyncMutex,
	stream::{self, BoxStream, StreamExt},
};
use truapi::latest::{
	FundingAssignment, FundingCancelAnswer, FundingCancelDecision, FundingDirection,
	FundingFailure, FundingQuoteAnswer, FundingQuoteAsk, FundingUpdate, GenericError,
	HostFundingReportError, HostFundingStatusSubscribeItem, HostPaymentStatusSubscribeError,
	HostPaymentStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError,
	HostPaymentTopUpStatusSubscribeItem,
};

use super::services::RuntimeServices;
use crate::{
	host_logic::funding::{
		CancelCall, CancelConfirmation, CancelOutcome, CancelTerms, FundingChoice, FundingSession,
		FundingSessionError, FundingSessionSummary, Settlement, StartRecord, StartedFunds,
		load_pack, load_sessions, pack_blob, retained, store_pack, store_sessions,
	},
	platform::{
		CoreStorage, FundingPlatform, FundingPresentOutcome, FundingPresentation, ProductContext,
		ProductExecutionKind, ProductId,
	},
	runtime::payment_id::host_payment_id,
	unix_time::current_unix_millis,
};

/// Wait before retrying an expiry sweep whose write failed.
const SWEEP_RETRY: Duration = Duration::from_secs(30);
/// First wait before reading a top-up's or payment's status again, when the
/// last read ended without an outcome.
const FOLLOW_RETRY_FIRST: Duration = Duration::from_secs(1);
/// Longest wait between those reads.
const FOLLOW_RETRY_LONGEST: Duration = Duration::from_secs(5 * 60);
/// How long a top-up or payment the host does not know is waited for after a
/// report named it, or after the core recorded it with no host call in
/// flight, before it counts as having moved nothing. A payment the provider
/// requested can wait this long on the user's approval.
const UNKNOWN_FUNDS_GRACE_MS: u64 = 60 * 60 * 1_000;
/// How long the host's engine may take to say whether it holds funds.
const ENGINE_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Wait before asking the engine again about recorded funds it could not
/// place.
const RESOLVE_RETRY: Duration = Duration::from_secs(30);
/// How long a provider's worker may take to answer one call before it counts
/// as unanswered.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Wait before handing a worker its sessions again after a call went
/// unanswered.
const CATCH_UP_RETRY: Duration = Duration::from_secs(5);

type Subscribers = HashMap<String, Vec<mpsc::UnboundedSender<HostFundingStatusSubscribeItem>>>;

/// A provider worker's connection, through which the core calls it. Each
/// call takes the worker's one answer; a worker that errs, goes away or never
/// answers gives none.
pub trait FundingProviderLink: Send + Sync {
	/// Hand the worker a session. Returns whether it took it.
	fn assign(&self, session: FundingAssignment) -> BoxFuture<'static, bool>;
	/// Ask the worker to price `ask`, and take its answer.
	fn quote(&self, ask: FundingQuoteAsk) -> BoxFuture<'static, Option<FundingQuoteAnswer>>;
	/// Ask the worker to stop a session, and take its answer.
	fn cancel(&self, intent: String) -> BoxFuture<'static, Option<FundingCancelAnswer>>;
	/// Tell the worker the user's answer to its cancel offer. Returns whether
	/// it heard it.
	fn settle_cancel(
		&self,
		intent: String,
		decision: FundingCancelDecision,
	) -> BoxFuture<'static, bool>;
}

/// A provider's worker connection, and how far handing it what it serves
/// has got.
struct Link {
	link: Arc<dyn FundingProviderLink>,
	/// Tells this connection from a later one of the same provider.
	generation: u64,
	catch_up: CatchUp,
}

/// Whether a worker connection has been handed every session it serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatchUp {
	/// The worker has not said it is ready.
	Connecting,
	/// Due once the clock passes `not_before_ms`, in Unix milliseconds.
	Due { not_before_ms: u64 },
	/// Being handed over.
	Running,
	/// Every call was answered on this connection.
	Done,
}

/// A call the core puts to a provider's worker about one session.
enum ProviderCall {
	Assign(Box<FundingAssignment>),
	Cancel(String, CancelCall),
}

/// Session changes the host has not been told of yet, in commit order.
#[derive(Default)]
struct Announcements {
	pending: VecDeque<(String, HostFundingStatusSubscribeItem)>,
	/// Whether a caller is delivering them.
	delivering: bool,
}

/// Host-global funding sessions, their subscribers and the host surface.
#[derive(Default)]
pub struct FundingRegistry {
	sessions: Mutex<HashMap<String, FundingSession>>,
	/// Recorded sessions compacted out of [`Self::sessions`], by intent. A
	/// session can sit in both after a write that stored its summary but not
	/// the trimmed sessions; the full record then wins.
	pack: Mutex<HashMap<String, FundingSessionSummary>>,
	/// Sessions whose overlay is still up: live in memory, so the overlay can
	/// quote and pick a provider for them, but not stored until the user
	/// starts them.
	presenting: Mutex<HashSet<String>>,
	subscribers: Mutex<Subscribers>,
	announcements: Mutex<Announcements>,
	/// Held across every load and write; the flag records whether persisted
	/// sessions have been loaded.
	writes: AsyncMutex<bool>,
	/// Whether persisted sessions have been loaded, readable without the
	/// write lock.
	loaded: AtomicBool,
	platform: OnceLock<Arc<dyn FundingPlatform>>,
	/// Each provider's worker connection, by product id.
	links: Mutex<HashMap<String, Link>>,
	/// Callers waiting for a provider's worker to connect, by product id.
	link_waiters: Mutex<HashMap<String, Vec<oneshot::Sender<()>>>>,
	/// The generation the next link takes.
	next_generation: AtomicU64,
	/// Sessions holding a reference on their provider's worker, with that
	/// provider. Only the reconciler changes it.
	holding: Mutex<HashMap<String, String>>,
	/// Sessions whose top-ups or payment are being followed to an outcome.
	following: Mutex<HashSet<String>>,
	/// Ids of funds whose host call is in flight, so no second call under
	/// one id runs meanwhile and the reconciler leaves their record alone.
	starting: Mutex<HashSet<[u8; 32]>>,
	/// Recorded funds the host has not taken, by id, with when the reconciler
	/// may next ask the engine about them: `u64::MAX` while it is asking.
	resolving: Mutex<HashMap<[u8; 32], u64>>,
	/// Wakes the reconciler task, once one runs.
	reconciler: Mutex<Option<mpsc::UnboundedSender<()>>>,
	/// The earliest time a background step asked to be tried again, in Unix
	/// milliseconds.
	retry_at: Mutex<Option<u64>>,
	/// The services the registry belongs to, for worker references and
	/// following top-ups and payments. Empty in a registry built on its own.
	services: Weak<RuntimeServices>,
}

impl FundingRegistry {
	/// Install the host's funding surface. Set-once; returns whether this call
	/// installed it.
	pub fn install_platform(&self, platform: Arc<dyn FundingPlatform>) -> bool {
		self.platform.set(platform).is_ok()
	}

	/// The host's funding surface, when one is installed.
	pub fn platform(&self) -> Option<Arc<dyn FundingPlatform>> {
		self.platform.get().cloned()
	}

	/// A registry belonging to `services`.
	pub fn new(services: Weak<RuntimeServices>) -> Self {
		Self { services, ..Self::default() }
	}

	/// Assign open session `intent` to the provider the user chose, and hand
	/// it to that provider. Returns whether it was open and unassigned.
	pub async fn select_provider(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		intent: &str,
		provider_id: &str,
		choice: Option<FundingChoice>,
	) -> Result<bool, FundingSessionError> {
		let intent = intent.to_string();
		let provider = provider_id.to_string();
		let assigned = self
			.commit(storage, now_ms, move |sessions| {
				let assigned = sessions.get_mut(&intent).and_then(|session| {
					session.assign(&provider, choice, now_ms).then(|| session.assignment())
				});
				let changed = assigned.iter().map(|session| session.intent.clone()).collect();
				(assigned, changed)
			})
			.await?;
		let Some(session) = assigned else {
			return Ok(false);
		};
		self.call(provider_id, vec![ProviderCall::Assign(Box::new(session))]);
		Ok(true)
	}

	/// Take `link` as the way to call `provider_id`'s worker, in place of any
	/// earlier one. Nothing is sent until [`Self::connected`], apart from
	/// calls made in the meantime.
	pub fn link(&self, provider_id: &str, link: Arc<dyn FundingProviderLink>) {
		let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
		self.lock_links().insert(
			provider_id.to_string(),
			Link { link, generation, catch_up: CatchUp::Connecting },
		);
	}

	/// Drop `link`, unless a newer one replaced it.
	pub fn unlink(&self, provider_id: &str, link: &Arc<dyn FundingProviderLink>) {
		let mut links = self.lock_links();
		if links.get(provider_id).is_some_and(|held| Arc::ptr_eq(&held.link, link)) {
			links.remove(provider_id);
		}
	}

	/// The worker on `provider_id`'s link is ready: the reconciler assigns it
	/// every session it still serves, oldest first, each followed by the
	/// cancel call it has not answered, one call at a time, and repeats the
	/// lot while a call goes unanswered. Once per link, so a worker that
	/// checks its connection again is not handed everything again. A worker
	/// treats every call as idempotent, as one sent before it was ready is
	/// repeated then.
	pub fn connected(&self, provider_id: &str) {
		let readied = {
			let mut links = self.lock_links();
			let readied = links
				.get_mut(provider_id)
				.filter(|held| held.catch_up == CatchUp::Connecting)
				.map(|held| held.catch_up = CatchUp::Due { not_before_ms: 0 })
				.is_some();
			for waiter in lock(&self.link_waiters).remove(provider_id).unwrap_or_default() {
				let _ = waiter.send(());
			}
			readied
		};
		if readied {
			self.wake();
		}
	}

	/// The link to `provider_id`'s worker once that worker has connected,
	/// waiting for it when it has not. A caller holds the worker meanwhile,
	/// so the host runs it, and bounds the wait itself.
	pub async fn connected_link(&self, provider_id: &str) -> Arc<dyn FundingProviderLink> {
		loop {
			let connected = {
				let links = self.lock_links();
				if let Some(held) =
					links.get(provider_id).filter(|held| held.catch_up != CatchUp::Connecting)
				{
					return held.link.clone();
				}
				let (sender, connected) = oneshot::channel();
				lock(&self.link_waiters)
					.entry(provider_id.to_string())
					.or_default()
					.push(sender);
				connected
			};
			let _ = connected.await;
		}
	}

	/// What `provider_id`'s worker has to hear when it connects, one list per
	/// session it serves, oldest first.
	fn pending_calls(&self, provider_id: &str) -> Vec<Vec<ProviderCall>> {
		let sessions = self.lock_sessions();
		let mut served: Vec<&FundingSession> = sessions
			.values()
			.filter(|session| {
				!session.is_terminal() && session.provider_id.as_deref() == Some(provider_id)
			})
			.collect();
		served.sort_by_key(|session| session.opened_at_ms);
		served
			.into_iter()
			.map(|session| {
				core::iter::once(ProviderCall::Assign(Box::new(session.assignment())))
					.chain(
						session
							.pending_cancel_call()
							.map(|call| ProviderCall::Cancel(session.intent.clone(), call)),
					)
					.collect()
			})
			.collect()
	}

	/// Store `update` from `provider` on session `intent`. A `Crediting` or
	/// `Collecting` update names funds recorded for that session alone, and
	/// uses them up.
	pub async fn report(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		provider: &ProductContext,
		intent: &str,
		update: FundingUpdate,
	) -> Result<Result<(), HostFundingReportError>, FundingSessionError> {
		let intent = intent.to_string();
		let provider_id = provider.product_id.as_str();
		let host_id = |id: &[u8; 32]| host_payment_id(provider, *id);
		self.commit(storage, now_ms, move |sessions| {
			let reported = match sessions.get_mut(&intent) {
				Some(session) => session.report(provider_id, update, host_id, now_ms),
				None => Err(HostFundingReportError::NotFound),
			};
			let changed = Vec::from_iter(reported.is_ok().then_some(intent));
			(reported, changed)
		})
		.await
	}

	/// Take `host_id` for one host call at a time: `None` while another call
	/// under it is in flight. The id is free again once the guard drops.
	pub fn begin_start(self: &Arc<Self>, host_id: [u8; 32]) -> Option<StartGuard> {
		self.lock_starting()
			.insert(host_id)
			.then(|| StartGuard { registry: self.clone(), host_id })
	}

	/// Whether session `intent` recorded funds under `host_id`.
	pub fn recorded_start(&self, intent: &str, host_id: &[u8; 32]) -> bool {
		self.lock_sessions()
			.get(intent)
			.is_some_and(|session| session.started.iter().any(|record| record.host_id == *host_id))
	}

	/// Whether a session of `provider_id` recorded funds under `host_id`.
	pub fn holds_start(&self, provider_id: &str, host_id: &[u8; 32]) -> bool {
		self.lock_sessions().values().any(|session| {
			session.provider_id.as_deref() == Some(provider_id) &&
				session.started.iter().any(|record| record.host_id == *host_id)
		})
	}

	/// Record funds `provider_id` starts for session `intent`, before the
	/// host is asked. `None` when that session is not open and served by
	/// `provider_id` in their direction; a `Conflict` when another of its
	/// sessions holds their id.
	pub async fn record_start(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		provider_id: &str,
		intent: &str,
		asked: FundsAsked,
	) -> Result<Option<StartRecord>, FundingSessionError> {
		let FundsAsked { direction, host_id, amount, terms } = asked;
		self.commit(storage, now_ms, |sessions| {
			let elsewhere = sessions.values().any(|session| {
				session.intent != intent &&
					session.provider_id.as_deref() == Some(provider_id) &&
					session.started.iter().any(|record| record.host_id == host_id)
			});
			let Some(session) = sessions.get_mut(intent).filter(|session| {
				!session.is_terminal() &&
					session.direction == direction &&
					session.provider_id.as_deref() == Some(provider_id)
			}) else {
				return (None, Vec::new());
			};
			if elsewhere {
				return (Some(StartRecord::Conflict), Vec::new());
			}
			let recorded = session.record_start(host_id, amount, terms, now_ms);
			// Recorded funds stop the session expiring, which its watchers see.
			let changed =
				Vec::from_iter((recorded == StartRecord::New).then(|| intent.to_string()));
			(Some(recorded), changed)
		})
		.await
	}

	/// Settle the record of funds under `host_id` on session `intent` once
	/// the host answered: kept as taken when `taken`, dropped otherwise
	/// unless the host took them before.
	pub async fn finish_start(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		intent: &str,
		host_id: [u8; 32],
		taken: bool,
	) -> Result<(), FundingSessionError> {
		self.commit(storage, now_ms, |sessions| {
			let changed = sessions.get_mut(intent).is_some_and(|session| {
				if taken { session.accept_start(&host_id) } else { session.forget_start(&host_id) }
			});
			((), Vec::from_iter(changed.then(|| intent.to_string())))
		})
		.await
	}

	/// Whether open session `intent` is assigned to `provider_id`.
	pub fn is_serving(&self, provider_id: &str, intent: &str) -> bool {
		self.get(intent).is_some_and(|session| {
			!session.is_terminal() && session.provider_id.as_deref() == Some(provider_id)
		})
	}

	/// Snapshot one session.
	pub fn get(&self, intent: &str) -> Option<FundingSession> {
		self.lock_sessions().get(intent).cloned()
	}

	/// Every session the user started that the host still has to hear of, for
	/// its progress and history views: those in flight first, then the ended
	/// ones it has not recorded, each newest first.
	pub fn sessions(&self) -> Vec<FundingSession> {
		let mut sessions: Vec<_> = {
			let presenting = self.lock_presenting();
			self.lock_sessions()
				.values()
				.filter(|session| session.needs_handoff() && !presenting.contains(&session.intent))
				.cloned()
				.collect()
		};
		sessions.sort_by_key(|session| {
			(
				session.is_terminal(),
				core::cmp::Reverse(session.settled_at_ms().unwrap_or(session.opened_at_ms)),
			)
		});
		sessions
	}

	/// Every ended session the host recorded or the core compacted into the
	/// pack, as summaries newest first, each once: what a host rebuilds its
	/// own history from when that history was lost.
	pub fn history(&self) -> Vec<FundingSessionSummary> {
		let mut history: Vec<_> = {
			let sessions = self.lock_sessions();
			let recorded = sessions
				.values()
				.filter(|session| session.is_terminal() && session.acknowledged)
				.map(FundingSession::summary);
			let pack = self.lock_pack();
			let compacted =
				pack.values().filter(|summary| !sessions.contains_key(&summary.intent)).cloned();
			recorded.chain(compacted).collect()
		};
		history.sort_by_key(|summary| core::cmp::Reverse(summary.settled_at_ms()));
		history
	}

	/// Who opened session `intent`, held in full or in the pack: `None` when
	/// no such session is held, `Some(None)` when the host opened it.
	pub fn owner(&self, intent: &str) -> Option<Option<ProductId>> {
		if let Some(session) = self.lock_sessions().get(intent) {
			return Some(session.owner_product_id.clone());
		}
		self.lock_pack().get(intent).map(|summary| summary.owner_product_id.clone())
	}

	/// Record that the host has written ended session `intent` into its own
	/// history, so it is no longer handed over. Once newer recorded sessions
	/// push it out of the full record it is compacted into the pack, where its
	/// owner can still read how it ended. Returns whether the session was ended
	/// and not yet acknowledged.
	pub async fn acknowledge(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		intent: &str,
	) -> Result<bool, FundingSessionError> {
		let intent = intent.to_string();
		self.commit(storage, now_ms, move |sessions| {
			let acknowledged = sessions
				.get_mut(&intent)
				.filter(|session| session.is_terminal() && !session.acknowledged)
				.map(|session| session.acknowledged = true)
				.is_some();
			(acknowledged, Vec::new())
		})
		.await
	}

	/// Cancel session `intent` at the user's request: end it if no provider
	/// serves it, otherwise ask the provider to stop. A session whose overlay
	/// is still up is not the host's to cancel yet: the overlay ends it by
	/// closing.
	pub async fn cancel(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		intent: &str,
	) -> Result<CancelOutcome, FundingSessionError> {
		let target = intent.to_string();
		let (outcome, provider) = self
			.commit(storage, now_ms, move |sessions| {
				let Some(session) =
					sessions.get_mut(&target).filter(|_| !self.lock_presenting().contains(&target))
				else {
					return ((CancelOutcome::Refused, None), Vec::new());
				};
				let outcome = session.cancel(now_ms);
				let provider = session.provider_id.clone();
				let changed = match outcome {
					CancelOutcome::Ended | CancelOutcome::Requested => vec![target],
					CancelOutcome::Pending | CancelOutcome::TooLate | CancelOutcome::Refused => {
						Vec::new()
					},
				};
				((outcome, provider), changed)
			})
			.await?;
		if let (CancelOutcome::Requested, Some(provider)) = (outcome, provider) {
			self.call(
				&provider,
				vec![ProviderCall::Cancel(intent.to_string(), CancelCall::Cancel)],
			);
		}
		Ok(outcome)
	}

	/// Answer the provider's cancel offer on session `intent`, and pass the
	/// answer to the provider.
	pub async fn confirm_cancel(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		intent: &str,
		terms: Option<CancelTerms>,
	) -> Result<CancelConfirmation, FundingSessionError> {
		let target = intent.to_string();
		let (confirmation, call) = self
			.commit(storage, now_ms, move |sessions| {
				let Some(session) = sessions.get_mut(&target) else {
					return ((CancelConfirmation::NoOffer, None), Vec::new());
				};
				let confirmation = session.confirm_cancel(terms, now_ms);
				if confirmation == CancelConfirmation::NoOffer {
					return ((confirmation, None), Vec::new());
				}
				let call = session.provider_id.clone().zip(session.pending_cancel_call());
				((confirmation, call), vec![target])
			})
			.await?;
		if let Some((provider, call)) = call {
			self.call(&provider, vec![ProviderCall::Cancel(intent.to_string(), call)]);
		}
		Ok(confirmation)
	}

	/// Watch one session: its current stage at once, then one item per change,
	/// in commit order and never repeated. The stream ends after a terminal
	/// stage, so a session that has already ended, including one the host
	/// recorded or one compacted into the pack, yields that one item. It never
	/// errors; dropping it stops watching. `None` when no such session is held.
	pub fn subscribe(
		&self,
		intent: &str,
	) -> Option<BoxStream<'static, HostFundingStatusSubscribeItem>> {
		// A commit fans out under this lock after writing, so holding it from
		// the read to the registration means no change is missed.
		let mut subscribers = self.lock_subscribers();
		let Some(session) = self.get(intent) else {
			let item = self.lock_pack().get(intent)?.wire_item();
			return Some(stream::once(async move { item }).boxed());
		};
		let terminal = session.is_terminal();
		let current = stream::once(async move { session.wire_item() });
		if terminal {
			return Some(current.boxed());
		}
		let (sender, receiver) = mpsc::unbounded();
		let senders = subscribers.entry(intent.to_string()).or_default();
		// A product that reloads subscribes again without the session
		// changing, so watchers it left are dropped here too.
		senders.retain(|sender| !sender.is_closed());
		senders.push(sender);
		Some(current.chain(receiver).boxed())
	}

	/// Hold `session` in memory while its overlay is up, so the overlay can
	/// work with it, but keep it out of storage until [`Self::start`].
	/// Returns `false`, holding nothing new, when a session with its intent
	/// is already held, in full or in the pack.
	async fn present(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		session: FundingSession,
	) -> Result<bool, FundingSessionError> {
		let intent = session.intent.clone();
		let presented = self
			.commit(storage, now_ms, |sessions| {
				if sessions.contains_key(&session.intent) ||
					self.lock_pack().contains_key(&session.intent)
				{
					return (false, Vec::new());
				}
				self.lock_presenting().insert(session.intent.clone());
				sessions.insert(session.intent.clone(), session);
				(true, Vec::new())
			})
			.await;
		if presented.is_err() {
			let _ = self.discard(&intent).await;
		}
		presented
	}

	/// The user started presented session `intent`: store it and announce
	/// it. A session that cannot be stored is discarded, as the caller is
	/// told it failed; until then it stays presented, so no other commit
	/// stores it.
	async fn start(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		intent: &str,
	) -> Result<(), FundingSessionError> {
		// Leaving the overlay inside the commit, so no other commit stores
		// the session before this one announces it.
		let started = self
			.commit(storage, now_ms, |_| {
				self.lock_presenting().remove(intent);
				((), vec![intent.to_string()])
			})
			.await;
		if started.is_err() &&
			!self.discard(intent).await &&
			let Some(session) = self.get(intent)
		{
			// A provider took it under the overlay, so it is stored and runs
			// as started: announce it like any started session.
			self.fan_out(&session);
		}
		started
	}

	/// Drop presented session `intent`, unless a provider took it while the
	/// overlay was up: that provider may already be working on it, and the
	/// session is stored from then on. Returns whether it was dropped.
	///
	/// An unassigned presented session was never stored, so this cannot
	/// fail; holding the write lock keeps any commit from storing it on the
	/// way out. A product that reloaded under the overlay may be watching it
	/// by its id, so its stream ends as cancelled: the user never started it.
	async fn discard(&self, intent: &str) -> bool {
		let _writes = self.writes.lock().await;
		if self
			.lock_sessions()
			.get(intent)
			.is_some_and(|session| session.provider_id.is_some())
		{
			// Its provider may be working on it and it is stored, so it runs
			// as started instead of staying hidden under an overlay that is
			// gone.
			self.lock_presenting().remove(intent);
			return false;
		}
		self.lock_presenting().remove(intent);
		self.lock_sessions().remove(intent);
		let cancelled =
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled };
		for sender in self.lock_subscribers().remove(intent).unwrap_or_default() {
			// A watcher that already went away has nothing to hear.
			let _ = sender.unbounded_send(cancelled.clone());
		}
		true
	}

	/// Apply `edit` to a copy of the sessions, persist the result, then make it
	/// current and notify every session `edit` names. An edit may move
	/// sessions in or out of the overlay; when the write fails, that move is
	/// undone along with the rest.
	///
	/// Persisted sessions are loaded first on the first call, and every
	/// commit expires the sessions past their deadline, so a long suspend
	/// cannot leave an overdue session open.
	pub async fn commit<R>(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		edit: impl FnOnce(&mut HashMap<String, FundingSession>) -> (R, Vec<String>),
	) -> Result<R, FundingSessionError> {
		let mut loaded = self.writes.lock().await;
		let presenting_before = self.lock_presenting().clone();
		let mut working = self.lock_sessions().clone();
		let mut changed = Vec::new();
		if !*loaded {
			for session in load_sessions(storage).await? {
				working.entry(session.intent.clone()).or_insert(session);
			}
			let stored = load_pack(storage).await?;
			self.lock_pack()
				.extend(stored.into_iter().map(|summary| (summary.intent.clone(), summary)));
		}
		let (result, edited) = edit(&mut working);
		// A session the host is told about must be stored too, which covers
		// one leaving the overlay: started, though nothing in it changed.
		let announces = !edited.is_empty();
		changed.extend(edited);
		let mut lapsed = Vec::new();
		for session in working.values_mut() {
			if session.expire_if_due(now_ms) {
				changed.push(session.intent.clone());
			}
			if session.lapse_cancel_offer(now_ms) {
				changed.push(session.intent.clone());
				lapsed.extend(session.provider_id.clone().map(|provider| {
					(
						provider,
						ProviderCall::Cancel(
							session.intent.clone(),
							CancelCall::Settle(FundingCancelDecision::Declined),
						),
					)
				}));
			}
		}
		// The write lock keeps the in-memory map unchanged until this commit
		// replaces it, so it still holds the sessions as they were before `edit`.
		if !*loaded || announces || working != *self.lock_sessions() {
			let (kept, compacted) = retained(working.into_values());
			// An assigned session is stored even while its overlay is up, as
			// its provider's reports are stored before they are answered.
			let (started, stored_set_changed) = {
				let presenting = self.lock_presenting();
				let before = self.lock_sessions();
				let stored = |presenting: &HashSet<String>, session: &FundingSession| {
					!presenting.contains(&session.intent) || session.provider_id.is_some()
				};
				let started: Vec<&FundingSession> =
					kept.iter().filter(|session| stored(&presenting, session)).collect();
				let stored_before =
					before.values().filter(|session| stored(&presenting_before, session)).count();
				// A session only shown in the overlay is not stored, so holding
				// one changes nothing worth writing.
				let unchanged = started.len() == stored_before &&
					started.iter().all(|session| {
						stored(&presenting_before, session) &&
							before.get(&session.intent) == Some(*session)
					});
				(started, !unchanged)
			};
			if !*loaded || announces || stored_set_changed {
				// The pack first, so no session leaves the sessions blob before
				// its summary is stored.
				let written = match self.store_compacted(storage, &compacted).await {
					Ok(()) => store_sessions(storage, &started).await,
					Err(error) => Err(error),
				};
				if let Err(error) = written {
					*self.lock_presenting() = presenting_before;
					return Err(error);
				}
			}
			self.lock_pack()
				.extend(compacted.into_iter().map(|summary| (summary.intent.clone(), summary)));
			*self.lock_sessions() =
				kept.into_iter().map(|session| (session.intent.clone(), session)).collect();
		}
		*loaded = true;
		self.loaded.store(true, Ordering::Release);
		// Each session once, even when an edit and its expiry both name it.
		let mut announced = HashSet::new();
		changed.retain(|intent| announced.insert(intent.clone()));
		for intent in changed {
			if let Some(session) = self.get(&intent) {
				self.fan_out(&session);
			}
		}
		drop(loaded);
		for (provider, call) in lapsed {
			self.call(&provider, vec![call]);
		}
		self.announce();
		self.wake();
		Ok(result)
	}

	/// Wake the reconciler, which brings background work in line with the
	/// sessions, and start it on first use. A registry built on its own has
	/// none.
	pub fn wake(&self) {
		let Some(services) = self.services.upgrade() else {
			return;
		};
		let mut reconciler = self.lock_reconciler();
		if reconciler.as_ref().is_some_and(|wake| wake.unbounded_send(()).is_ok()) {
			return;
		}
		let (wake, woken) = mpsc::unbounded();
		*reconciler = Some(wake);
		drop(reconciler);
		(services.spawner)(Box::pin(reconcile(Arc::downgrade(&services), woken)));
	}

	/// Ask the reconciler to run again by `at_ms`, in Unix milliseconds.
	fn retry_by(&self, at_ms: u64) {
		{
			let mut retry_at = self.lock_retry_at();
			*retry_at = Some(retry_at.map_or(at_ms, |held| held.min(at_ms)));
		}
		self.wake();
	}

	/// One reconciling pass: load the sessions if they are not loaded, expire
	/// the overdue ones, then hold workers, follow settlements, place
	/// recorded funds and hand workers their sessions, in that order. Returns
	/// how long until the next pass is due on its own, `None` when only a
	/// wake brings one.
	async fn reconcile_once(&self, services: &Arc<RuntimeServices>) -> Option<u64> {
		let now_ms = current_unix_millis();
		let overdue = self.next_deadline().is_some_and(|deadline_ms| deadline_ms <= now_ms);
		if !self.loaded.load(Ordering::Acquire) || overdue {
			match self.sweep(services.platform.as_ref(), now_ms).await {
				Ok(()) => {},
				// Left for the newer app that wrote them; nothing runs on them.
				Err(FundingSessionError::NewerFormat { .. }) => return None,
				Err(error) => {
					tracing::warn!(%error, "funding sweep failed");
					if self.loaded.load(Ordering::Acquire) {
						self.reconcile_work(services, now_ms);
					}
					return Some(SWEEP_RETRY.as_millis() as u64);
				},
			}
		}
		self.reconcile_work(services, now_ms);
		self.next_wake(now_ms)
	}

	fn reconcile_work(&self, services: &Arc<RuntimeServices>, now_ms: u64) {
		self.retain_quotes(services);
		self.hold_workers(services);
		self.follow_settlements(services);
		self.resolve_starts(services, now_ms);
		self.catch_up_links(services, now_ms);
	}

	/// Keep quotes only for open sessions still waiting on a provider.
	fn retain_quotes(&self, services: &RuntimeServices) {
		let quoting: HashSet<String> = self
			.lock_sessions()
			.values()
			.filter(|session| !session.is_terminal() && session.provider_id.is_none())
			.map(|session| session.intent.clone())
			.collect();
		services.funding_quotes().retain_sessions(|intent| quoting.contains(intent));
	}

	/// Every assigned open session holds its provider's worker. Only the
	/// reconciler changes the holds, so they are taken and let go in the
	/// order it derives them; a worker that starts and calls back changes the
	/// sessions, which the next pass picks up.
	fn hold_workers(&self, services: &RuntimeServices) {
		let assigned: HashMap<String, String> = self
			.lock_sessions()
			.values()
			.filter(|session| !session.is_terminal())
			.filter_map(|session| Some((session.intent.clone(), session.provider_id.clone()?)))
			.collect();
		let (acquire, release) = {
			let mut holding = self.lock_holding();
			let mut acquire = Vec::new();
			for (intent, provider) in &assigned {
				if !holding.contains_key(intent) {
					holding.insert(intent.clone(), provider.clone());
					acquire.push(provider.clone());
				}
			}
			let mut release = Vec::new();
			holding.retain(|intent, provider| {
				let kept = assigned.contains_key(intent);
				if !kept {
					release.push(core::mem::take(provider));
				}
				kept
			});
			(acquire, release)
		};
		// Taken before let go, so a provider moving from an ended session to
		// a new one keeps its worker running.
		for provider in acquire.iter().filter_map(|provider| provider.parse::<ProductId>().ok()) {
			services.worker_ledger.acquire(&provider);
		}
		for provider in release.iter().filter_map(|provider| provider.parse::<ProductId>().ok()) {
			services.worker_ledger.release(&provider);
		}
	}

	/// Follow every session whose outcome is due, once the engine its funds
	/// went through is installed.
	fn follow_settlements(&self, services: &Arc<RuntimeServices>) {
		let due: Vec<(String, String, Settlement)> = self
			.lock_sessions()
			.values()
			.filter_map(|session| {
				let settlement = session.settlement_due()?;
				Some((session.intent.clone(), session.provider_id.clone()?, settlement))
			})
			.collect();
		for (intent, provider, settlement) in due {
			let engine = match settlement {
				Settlement::TopUps(_) => services.top_up_platform().is_some(),
				Settlement::Payment(_) => services.payment_platform().is_some(),
			};
			if engine && self.lock_following().insert(intent.clone()) {
				services.clone().follow_settlement(intent, provider, settlement);
			}
		}
	}

	/// Ask the engine about recorded funds the host has not taken and no
	/// call is starting: taken once it holds them, dropped once it still does
	/// not [`UNKNOWN_FUNDS_GRACE_MS`] after they were recorded.
	fn resolve_starts(&self, services: &Arc<RuntimeServices>, now_ms: u64) {
		let pending: Vec<(String, String, FundingDirection, StartedFunds)> = self
			.lock_sessions()
			.values()
			.filter(|session| !session.is_terminal())
			.flat_map(|session| {
				session.pending_starts().filter_map(|funds| {
					Some((
						session.intent.clone(),
						session.provider_id.clone()?,
						session.direction,
						funds.clone(),
					))
				})
			})
			.collect();
		let mut resolving = self.lock_resolving();
		resolving.retain(|host_id, _| pending.iter().any(|(.., funds)| funds.host_id == *host_id));
		let starting = self.lock_starting();
		for (intent, provider, direction, funds) in pending {
			let engine = match direction {
				FundingDirection::In => services.top_up_platform().is_some(),
				FundingDirection::Out => services.payment_platform().is_some(),
			};
			if !engine ||
				starting.contains(&funds.host_id) ||
				resolving.get(&funds.host_id).is_some_and(|after_ms| *after_ms > now_ms)
			{
				continue;
			}
			resolving.insert(funds.host_id, u64::MAX);
			(services.spawner)(Box::pin(
				services.clone().resolve_start(intent, provider, direction, funds),
			));
		}
	}

	/// Hand each ready worker every session it serves, one call at a time.
	fn catch_up_links(&self, services: &Arc<RuntimeServices>, now_ms: u64) {
		let due: Vec<(String, u64, Arc<dyn FundingProviderLink>)> = self
			.lock_links()
			.iter_mut()
			.filter_map(|(provider, held)| match held.catch_up {
				CatchUp::Due { not_before_ms } if not_before_ms <= now_ms => {
					held.catch_up = CatchUp::Running;
					Some((provider.clone(), held.generation, held.link.clone()))
				},
				_ => None,
			})
			.collect();
		for (provider, generation, link) in due {
			let services = services.clone();
			(services.spawner.clone())(Box::pin(async move {
				let registry = services.funding().clone();
				let mut answered = true;
				for calls in registry.pending_calls(&provider) {
					for call in calls {
						answered &= registry.put(&services, link.as_ref(), call).await;
					}
				}
				registry.caught_up(&provider, generation, answered);
			}));
		}
	}

	/// Record how handing link `generation` of `provider_id` its sessions
	/// went: done when every call was answered, due again shortly otherwise.
	/// A link replaced meanwhile is left as it is.
	fn caught_up(&self, provider_id: &str, generation: u64, answered: bool) {
		let retry_ms = current_unix_millis().saturating_add(CATCH_UP_RETRY.as_millis() as u64);
		if let Some(held) = self
			.lock_links()
			.get_mut(provider_id)
			.filter(|held| held.generation == generation)
		{
			held.catch_up =
				if answered { CatchUp::Done } else { CatchUp::Due { not_before_ms: retry_ms } };
		}
		if !answered {
			self.retry_by(retry_ms);
		}
	}

	/// Milliseconds until the next pass is due on its own: a session expires
	/// or its offer lapses, a step asked to be retried, or a link is due to
	/// be handed its sessions again.
	fn next_wake(&self, now_ms: u64) -> Option<u64> {
		let retry = {
			let mut retry_at = self.lock_retry_at();
			let held = *retry_at;
			if held.is_some_and(|at_ms| at_ms <= now_ms) {
				*retry_at = None;
			}
			held
		};
		let link = self
			.lock_links()
			.values()
			.filter_map(|held| match held.catch_up {
				CatchUp::Due { not_before_ms } => Some(not_before_ms),
				_ => None,
			})
			.min();
		[self.next_deadline(), retry, link]
			.into_iter()
			.flatten()
			.min()
			.map(|at_ms| at_ms.saturating_sub(now_ms))
	}

	/// Put `calls` to `provider_id`'s worker one after another, on a task of
	/// their own, and store its answers. Without a link they wait for the
	/// worker to connect, which repeats whatever is still pending.
	fn call(&self, provider_id: &str, calls: Vec<ProviderCall>) {
		let Some(services) = self.services.upgrade() else {
			return;
		};
		let Some(link) = self.lock_links().get(provider_id).map(|held| held.link.clone()) else {
			return;
		};
		let spawner = services.spawner.clone();
		spawner(Box::pin(async move {
			for call in calls {
				services.funding().put(&services, link.as_ref(), call).await;
			}
		}));
	}

	/// Put one call to a provider's worker and store its answer. Returns
	/// whether the worker answered within [`CALL_TIMEOUT`] and the answer was
	/// stored.
	async fn put(
		&self,
		services: &RuntimeServices,
		link: &dyn FundingProviderLink,
		call: ProviderCall,
	) -> bool {
		let (intent, answered) = match call {
			ProviderCall::Assign(session) => {
				let intent = session.intent.clone();
				let taken = within(CALL_TIMEOUT, link.assign(*session)).await.unwrap_or(false);
				if !taken {
					tracing::debug!(%intent, "a provider's worker did not take its session");
				}
				return taken;
			},
			ProviderCall::Cancel(intent, CancelCall::Cancel) => {
				let Some(answer) =
					within(CALL_TIMEOUT, link.cancel(intent.clone())).await.flatten()
				else {
					tracing::debug!(%intent, "a provider's worker did not answer a cancel");
					return false;
				};
				let target = intent.clone();
				let answered = self
					.commit(services.platform.as_ref(), current_unix_millis(), move |sessions| {
						let now_ms = current_unix_millis();
						let changed = sessions
							.get_mut(&target)
							.is_some_and(|session| session.answer_cancel(answer, now_ms));
						((), if changed { vec![target] } else { Vec::new() })
					})
					.await;
				(intent, answered)
			},
			ProviderCall::Cancel(intent, CancelCall::Settle(decision)) => {
				let heard =
					within(CALL_TIMEOUT, link.settle_cancel(intent.clone(), decision.clone()))
						.await
						.unwrap_or(false);
				if !heard {
					tracing::debug!(%intent, "a provider's worker did not hear a cancel decision");
					return false;
				}
				let target = intent.clone();
				let answered = self
					.commit(services.platform.as_ref(), current_unix_millis(), move |sessions| {
						if let Some(session) = sessions.get_mut(&target) {
							session.cancel_settled(&decision);
						}
						((), Vec::new())
					})
					.await;
				(intent, answered)
			},
		};
		match answered {
			Ok(()) => true,
			Err(error) => {
				tracing::warn!(%intent, %error, "storing a provider's cancel answer failed");
				false
			},
		}
	}

	/// Store the pack with `compacted` added; nothing to write when it is
	/// empty. The blob is encoded under the pack lock, so the pack is not
	/// copied to write it.
	async fn store_compacted(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		compacted: &[FundingSessionSummary],
	) -> Result<(), FundingSessionError> {
		if compacted.is_empty() {
			return Ok(());
		}
		let blob = {
			let pack = self.lock_pack();
			// A summary stored by a commit whose sessions write then failed is
			// compacted again; it is written once.
			let held = pack
				.values()
				.filter(|summary| !compacted.iter().any(|fresh| fresh.intent == summary.intent));
			pack_blob(held.chain(compacted))
		};
		store_pack(storage, blob).await
	}

	/// Load persisted sessions if they are not loaded yet and expire the
	/// overdue ones; the reconciler then keeps expiring the rest on time.
	pub async fn load(
		&self,
		storage: &(impl CoreStorage + ?Sized),
	) -> Result<(), FundingSessionError> {
		self.sweep(storage, current_unix_millis()).await
	}

	/// Commit no edit, which loads persisted sessions on first use and
	/// expires the overdue ones.
	async fn sweep(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
	) -> Result<(), FundingSessionError> {
		self.commit(storage, now_ms, |_| ((), Vec::new())).await
	}

	fn next_deadline(&self) -> Option<u64> {
		self.lock_sessions().values().filter_map(FundingSession::next_alarm_ms).min()
	}

	/// Tell subscribers about a session's current stage and queue it for the
	/// host. A terminal stage ends the subscriber streams.
	fn fan_out(&self, session: &FundingSession) {
		let item = session.wire_item();
		{
			let mut subscribers = self.lock_subscribers();
			if let Some(senders) = subscribers.get_mut(&session.intent) {
				senders.retain(|sender| sender.unbounded_send(item.clone()).is_ok());
			}
			if session.is_terminal() {
				subscribers.remove(&session.intent);
			}
		}
		// The host hears of a session once the user starts it.
		if self.platform.get().is_some() && !self.lock_presenting().contains(&session.intent) {
			self.lock_announcements().pending.push_back((session.intent.clone(), item));
		}
	}

	/// Hand queued changes to the host in commit order, outside the write
	/// lock so the host may call back into the registry. One caller delivers
	/// at a time, and a change committed from inside the callback is
	/// delivered by that caller once the callback returns.
	fn announce(&self) {
		let Some(platform) = self.platform() else {
			return;
		};
		if core::mem::replace(&mut self.lock_announcements().delivering, true) {
			return;
		}
		loop {
			let next = {
				let mut announcements = self.lock_announcements();
				let next = announcements.pending.pop_front();
				announcements.delivering = next.is_some();
				next
			};
			let Some((intent, item)) = next else {
				return;
			};
			platform.funding_session_changed(intent, item);
		}
	}

	fn lock_announcements(&self) -> MutexGuard<'_, Announcements> {
		lock(&self.announcements)
	}

	fn lock_reconciler(&self) -> MutexGuard<'_, Option<mpsc::UnboundedSender<()>>> {
		lock(&self.reconciler)
	}

	fn lock_retry_at(&self) -> MutexGuard<'_, Option<u64>> {
		lock(&self.retry_at)
	}

	fn lock_starting(&self) -> MutexGuard<'_, HashSet<[u8; 32]>> {
		lock(&self.starting)
	}

	fn lock_resolving(&self) -> MutexGuard<'_, HashMap<[u8; 32], u64>> {
		lock(&self.resolving)
	}

	fn lock_pack(&self) -> MutexGuard<'_, HashMap<String, FundingSessionSummary>> {
		lock(&self.pack)
	}

	fn lock_presenting(&self) -> MutexGuard<'_, HashSet<String>> {
		lock(&self.presenting)
	}

	fn lock_sessions(&self) -> MutexGuard<'_, HashMap<String, FundingSession>> {
		lock(&self.sessions)
	}

	fn lock_subscribers(&self) -> MutexGuard<'_, Subscribers> {
		lock(&self.subscribers)
	}

	fn lock_links(&self) -> MutexGuard<'_, HashMap<String, Link>> {
		self.links.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn lock_holding(&self) -> MutexGuard<'_, HashMap<String, String>> {
		lock(&self.holding)
	}

	fn lock_following(&self) -> MutexGuard<'_, HashSet<String>> {
		lock(&self.following)
	}
}

/// Lock `mutex`, recovering its value if a holder panicked: every value the
/// funding modules guard stays valid between statements.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Funds a provider's worker asks the host to move for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundsAsked {
	/// Which way they move.
	pub direction: FundingDirection,
	/// The id the host receives them under.
	pub host_id: [u8; 32],
	/// How much they move.
	pub amount: u128,
	/// Digest of the rest of the request.
	pub terms: [u8; 32],
}

/// An id whose host call is in flight, from [`FundingRegistry::begin_start`].
/// Dropping it lets the next call under the id start.
pub struct StartGuard {
	registry: Arc<FundingRegistry>,
	host_id: [u8; 32],
}

impl Drop for StartGuard {
	fn drop(&mut self) {
		self.registry.lock_starting().remove(&self.host_id);
		// The reconciler leaves a record alone while its call is in flight,
		// so it looks again once the call is done.
		self.registry.wake();
	}
}

/// Run reconciling passes, each after a wake or once the last one's wait is
/// up. Wakes that come during a pass bring one more. The task ends once a pass
/// leaves nothing timed and no wake is queued, so an idle host keeps no task
/// alive; the next wake starts another.
async fn reconcile(services: Weak<RuntimeServices>, mut woken: mpsc::UnboundedReceiver<()>) {
	loop {
		// Wakes queued so far are covered by the pass about to run.
		while woken.try_recv().is_ok() {}
		let wait_ms = {
			let Some(services) = services.upgrade() else {
				return;
			};
			let registry = services.funding().clone();
			registry.reconcile_once(&services).await
		};
		let woke = match wait_ms {
			Some(wait_ms) => {
				let delay = futures_timer::Delay::new(Duration::from_millis(wait_ms));
				match futures::future::select(delay, woken.next()).await {
					Either::Left(_) => true,
					Either::Right((woke, _)) => woke.is_some(),
				}
			},
			None => {
				let Some(services) = services.upgrade() else {
					return;
				};
				// Under the lock a wake sends through, so none is lost between
				// this check and the handle going.
				let mut reconciler = services.funding().lock_reconciler();
				let queued = woken.try_recv().is_ok();
				if !queued {
					*reconciler = None;
				}
				queued
			},
		};
		if !woke {
			return;
		}
	}
}

/// `future`'s output, or `None` once `limit` passed first.
async fn within<T>(limit: Duration, future: impl Future<Output = T>) -> Option<T> {
	match futures::future::select(core::pin::pin!(future), futures_timer::Delay::new(limit)).await {
		Either::Left((output, _)) => Some(output),
		Either::Right(_) => None,
	}
}

/// Whether the host's engine holds funds under `host_id` that `provider`
/// started in `direction`, from the first status it gives. `Err` when it
/// could not say: no engine, a failed read or no answer in time.
pub(crate) async fn engine_holds(
	services: &RuntimeServices,
	provider: &ProductContext,
	direction: FundingDirection,
	host_id: [u8; 32],
) -> Result<bool, String> {
	match direction {
		FundingDirection::In => {
			let platform = services.top_up_platform().ok_or("no top-up engine")?;
			let mut statuses = platform.subscribe_top_up_status(provider, host_id);
			match within(ENGINE_READ_TIMEOUT, statuses.next()).await {
				None => Err("the top-up engine did not answer".to_string()),
				Some(Some(Ok(_))) => Ok(true),
				Some(None | Some(Err(HostPaymentTopUpStatusSubscribeError::TopUpNotFound))) => {
					Ok(false)
				},
				Some(Some(Err(HostPaymentTopUpStatusSubscribeError::Unknown { reason }))) => {
					Err(reason)
				},
			}
		},
		FundingDirection::Out => {
			let platform = services.payment_platform().ok_or("no payment engine")?;
			let mut statuses = platform.subscribe_payment_status(provider, host_id);
			match within(ENGINE_READ_TIMEOUT, statuses.next()).await {
				None => Err("the payment engine did not answer".to_string()),
				Some(Some(Ok(_))) => Ok(true),
				Some(None | Some(Err(HostPaymentStatusSubscribeError::PaymentNotFound))) => {
					Ok(false)
				},
				Some(Some(Err(HostPaymentStatusSubscribeError::Unknown { reason }))) => Err(reason),
			}
		},
	}
}

/// Why a session could not be opened.
#[derive(Debug, derive_more::Display)]
pub enum OpenFundingError {
	/// The host has no funding surface.
	#[display("host has no funding surface")]
	Unsupported,
	/// The user closed the overlay before starting.
	#[display("funding dismissed")]
	Dismissed,
	/// The amount sought is zero.
	#[display("funding amount must be positive")]
	InvalidAmount,
	/// A session with this intent is already held.
	#[display("a funding session with this id already exists")]
	AlreadyExists,
	/// The task showing the overlay was dropped before it finished, as when
	/// the runtime shuts down.
	#[display("funding overlay stopped")]
	Stopped,
	/// The session could not be stored.
	#[display("{_0}")]
	Session(FundingSessionError),
	/// The host failed to show the overlay.
	#[display("{}", _0.reason)]
	Present(GenericError),
}

/// First wait before retrying a failed load of the sessions on resume.
const RESUME_RETRY_FIRST: Duration = Duration::from_secs(1);
/// Longest wait between retries of that load.
const RESUME_RETRY_LONGEST: Duration = Duration::from_secs(60);

impl RuntimeServices {
	/// Install the host's funding surface and resume the sessions stored
	/// before a restart. Set-once; returns whether this call installed it.
	pub fn install_funding_platform(self: &Arc<Self>, platform: Arc<dyn FundingPlatform>) -> bool {
		let installed = self.funding().install_platform(platform);
		if installed {
			self.resume_funding();
		}
		installed
	}

	/// Open a session on the host's own behalf and show the overlay. Returns
	/// the session id, or `None` when the user closed the overlay without
	/// starting.
	pub async fn open_host_funding(
		self: &Arc<Self>,
		direction: FundingDirection,
		amount: Option<u128>,
	) -> Result<Option<String>, GenericError> {
		let intent = format!("fs_{}", nanoid::nanoid!(10));
		match self.open_funding(None, intent, direction, amount).await {
			Ok(session) => Ok(Some(session.intent)),
			Err(OpenFundingError::Dismissed) => Ok(None),
			Err(error) => Err(GenericError { reason: error.to_string() }),
		}
	}

	/// Load persisted sessions, which starts the reconciler on them, so
	/// sessions from before a restart end on time and the host hears of them.
	pub fn resume_funding(self: &Arc<Self>) {
		let services = self.clone();
		(self.spawner)(Box::pin(async move {
			let registry = services.funding();
			// The host's session views read what the core has loaded, so a
			// failed load is retried until the sessions are in.
			let mut retry_in = RESUME_RETRY_FIRST;
			loop {
				let loaded = registry
					.commit(services.platform.as_ref(), current_unix_millis(), |sessions| {
						// Ended sessions the host has not recorded are
						// handed over again, so its history gets every
						// outcome.
						let pending = sessions
							.values()
							.filter(|session| session.needs_handoff())
							.map(|session| session.intent.clone())
							.collect();
						((), pending)
					})
					.await;
				match loaded {
					Ok(()) => break,
					// Left for the newer app that wrote them; retrying cannot
					// read them.
					Err(error @ FundingSessionError::NewerFormat { .. }) => {
						tracing::warn!(%error, "funding stays unavailable");
						break;
					},
					Err(error) => {
						tracing::warn!(%error, ?retry_in, "loading funding sessions failed");
						futures_timer::Delay::new(retry_in).await;
						retry_in = (retry_in * 2).min(RESUME_RETRY_LONGEST);
					},
				}
			}
		}));
	}

	/// End session `intent` with `stage` as though its funds had moved.
	/// Returns whether it was still in flight.
	#[cfg(test)]
	pub async fn settle_funding_for_test(
		&self,
		intent: &str,
		stage: crate::host_logic::funding::FundingStage,
	) -> Result<bool, FundingSessionError> {
		let intent = intent.to_string();
		self.funding()
			.commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
				let settled = sessions
					.get_mut(&intent)
					.filter(|session| !session.is_terminal())
					.map(|session| session.stage = stage)
					.is_some();
				(settled, Vec::from_iter(settled.then_some(intent)))
			})
			.await
	}

	/// Record that the host wrote ended session `intent` into its own
	/// history. Returns whether it was ended and not yet acknowledged.
	pub async fn acknowledge_funding_session(
		&self,
		intent: &str,
	) -> Result<bool, FundingSessionError> {
		self.funding()
			.acknowledge(self.platform.as_ref(), current_unix_millis(), intent)
			.await
	}

	/// Cancel session `intent` at the user's request, and say what that did.
	pub async fn cancel_funding(&self, intent: &str) -> Result<CancelOutcome, FundingSessionError> {
		self.funding()
			.cancel(self.platform.as_ref(), current_unix_millis(), intent)
			.await
	}

	/// Answer the provider's cancel offer on session `intent`: accept it on
	/// `terms`, the terms the host showed, or decline it with `None`.
	pub async fn confirm_funding_cancel(
		&self,
		intent: &str,
		terms: Option<CancelTerms>,
	) -> Result<CancelConfirmation, FundingSessionError> {
		self.funding()
			.confirm_cancel(self.platform.as_ref(), current_unix_millis(), intent, terms)
			.await
	}

	/// Hand open session `intent` to the provider the user chose, which must
	/// be one of the host's providers serving its direction, on the quote the
	/// user chose it by, which must be one the core offered for this session,
	/// not yet expired and, when the session has an amount, pricing that
	/// amount. Returns whether it was open and not yet assigned.
	pub async fn select_funding_provider(
		self: &Arc<Self>,
		intent: &str,
		provider_id: &str,
		quote_id: Option<&str>,
	) -> Result<bool, FundingSessionError> {
		let provider = provider_id
			.parse::<ProductId>()
			.map_err(|error| FundingSessionError::InvalidProvider { reason: error.to_string() })?;
		let registry = self.funding();
		if registry.get(intent).is_some() &&
			!self.funding_providers().is_quote_target(provider.as_str())
		{
			return Err(FundingSessionError::InvalidProvider {
				reason: format!("{provider} is not a funding provider"),
			});
		}
		let choice = match quote_id {
			None => None,
			Some(quote_id) => Some(
				self.funding_quotes()
					.offered(intent, provider.as_str(), quote_id, current_unix_millis())
					.ok_or_else(|| FundingSessionError::UnknownQuote {
						quote_id: quote_id.to_string(),
					})?,
			),
		};
		if let (Some(choice), Some(session)) = (&choice, self.funding().get(intent)) &&
			let Some(amount) = session.amount
		{
			let priced = choice.balance_amount(session.direction);
			if priced != amount {
				return Err(FundingSessionError::QuoteAmountMismatch {
					quote_id: choice.quote.quote_id.clone(),
					priced,
					amount,
				});
			}
		}
		registry
			.select_provider(
				self.platform.as_ref(),
				current_unix_millis(),
				intent,
				provider.as_str(),
				choice,
			)
			.await
	}

	/// Follow a session's top-ups or payment to the end, then settle it with
	/// what moved, from the amounts the provider started them with. A failed
	/// write of the outcome is retried until it is stored or the session
	/// ended some other way; the reconciler follows the session again if this
	/// stops with it still due.
	fn follow_settlement(
		self: Arc<Self>,
		intent: String,
		provider_id: String,
		settlement: Settlement,
	) {
		let spawner = self.spawner.clone();
		spawner(Box::pin(async move {
			let registry = self.funding().clone();
			let provider = match provider_id.parse::<ProductId>() {
				Ok(product_id) => {
					ProductContext::new_with_execution(product_id, ProductExecutionKind::Worker)
				},
				Err(error) => {
					// Kept as followed: nothing could settle it, and following it
					// again would fail the same way.
					tracing::warn!(%error, "a funding session names an invalid provider");
					return;
				},
			};
			let moved = match settlement {
				Settlement::TopUps(top_ups) => {
					let mut credited: Option<u128> = Some(0);
					for funds in &top_ups {
						let Some(claimed) = self
							.follow_funds(&provider, &intent, FundingDirection::In, funds)
							.await
						else {
							credited = None;
							break;
						};
						credited = credited.map(|credited| credited.saturating_add(claimed));
					}
					credited
				},
				Settlement::Payment(funds) => {
					self.follow_funds(&provider, &intent, FundingDirection::Out, &funds).await
				},
			};
			if let Some(moved) = moved {
				let mut retry_in = FOLLOW_RETRY_FIRST;
				loop {
					let settling = intent.clone();
					let settled = registry
						.commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
							let now_ms = current_unix_millis();
							let settled = sessions
								.get_mut(&settling)
								.is_some_and(|session| session.settle(moved, now_ms));
							((), if settled { vec![settling] } else { Vec::new() })
						})
						.await;
					let Err(error) = settled else {
						break;
					};
					tracing::warn!(%error, ?retry_in, "settling a funding session failed");
					futures_timer::Delay::new(retry_in).await;
					retry_in = (retry_in * 2).min(FOLLOW_RETRY_LONGEST);
				}
			}
			registry.lock_following().remove(&intent);
			registry.wake();
		}));
	}

	/// Place funds recorded for session `intent` that the host has not
	/// taken: kept as taken once the engine holds them, dropped once it still
	/// does not [`UNKNOWN_FUNDS_GRACE_MS`] after they were recorded, and asked
	/// about again later otherwise. A call under their id that started
	/// meanwhile settles the record itself.
	async fn resolve_start(
		self: Arc<Self>,
		intent: String,
		provider_id: String,
		direction: FundingDirection,
		funds: StartedFunds,
	) {
		let registry = self.funding().clone();
		let held = match provider_id.parse::<ProductId>() {
			Ok(product_id) => {
				let provider =
					ProductContext::new_with_execution(product_id, ProductExecutionKind::Worker);
				engine_holds(&self, &provider, direction, funds.host_id).await
			},
			Err(error) => Err(error.to_string()),
		};
		let now_ms = current_unix_millis();
		let lapse_ms = funds.recorded_at_ms.saturating_add(UNKNOWN_FUNDS_GRACE_MS);
		let retry_ms = now_ms.saturating_add(RESOLVE_RETRY.as_millis() as u64);
		let (taken, next_ms) = match held {
			Ok(true) => (Some(true), None),
			Ok(false) if now_ms >= lapse_ms => (Some(false), None),
			Ok(false) => (None, Some(lapse_ms)),
			Err(reason) => {
				tracing::warn!(%reason, "reading recorded funds from the host failed");
				(None, Some(retry_ms))
			},
		};
		let next_ms = match taken {
			None => next_ms,
			Some(taken) => {
				let host_id = funds.host_id;
				let target = intent.clone();
				let written = registry
					.commit(self.platform.as_ref(), now_ms, |sessions| {
						let starting = registry.lock_starting().contains(&host_id);
						let changed = !starting &&
							sessions.get_mut(&target).is_some_and(|session| {
								if taken {
									session.accept_start(&host_id)
								} else {
									session.forget_start(&host_id)
								}
							});
						((), Vec::from_iter(changed.then_some(target)))
					})
					.await;
				written
					.inspect_err(|error| {
						tracing::warn!(%error, "storing where recorded funds stand failed");
					})
					.err()
					.map(|_| retry_ms)
			},
		};
		match next_ms {
			Some(next_ms) => {
				registry.lock_resolving().insert(funds.host_id, next_ms);
				registry.retry_by(next_ms);
			},
			None => {
				registry.lock_resolving().remove(&funds.host_id);
			},
		}
	}

	/// What `funds` moved, read again after a growing wait until the host
	/// reports an outcome. `None` once session `intent` is no longer open or
	/// the host has no engine to ask.
	async fn follow_funds(
		&self,
		provider: &ProductContext,
		intent: &str,
		direction: FundingDirection,
		funds: &StartedFunds,
	) -> Option<u128> {
		let mut retry_in = FOLLOW_RETRY_FIRST;
		loop {
			let read = match direction {
				FundingDirection::In => {
					let Some(platform) = self.top_up_platform() else {
						tracing::warn!("no top-up platform to follow a funding session's claims");
						return None;
					};
					let statuses = platform.subscribe_top_up_status(provider, funds.host_id);
					top_up_moved(statuses, funds, current_unix_millis()).await
				},
				FundingDirection::Out => {
					let Some(platform) = self.payment_platform() else {
						tracing::warn!("no payment platform to follow a funding session's payment");
						return None;
					};
					let statuses = platform.subscribe_payment_status(provider, funds.host_id);
					payment_moved(statuses, funds, current_unix_millis()).await
				},
			};
			if read.is_some() {
				return read;
			}
			futures_timer::Delay::new(retry_in).await;
			retry_in = (retry_in * 2).min(FOLLOW_RETRY_LONGEST);
			if self.funding().get(intent).is_none_or(|session| session.is_terminal()) {
				return None;
			}
		}
	}

	/// Open a session and show the host's funding overlay for it: the one
	/// path a product's `request` and the host's own Balance card both take.
	///
	/// A session the user never starts, or that the host fails to show, is
	/// discarded rather than left open. `intent` must be new:
	/// [`OpenFundingError::AlreadyExists`] otherwise.
	pub async fn open_funding(
		self: &Arc<Self>,
		product: Option<&ProductContext>,
		intent: String,
		direction: FundingDirection,
		amount: Option<u128>,
	) -> Result<FundingSession, OpenFundingError> {
		let registry = self.funding();
		let platform = registry.platform().ok_or(OpenFundingError::Unsupported)?;
		if amount == Some(0) {
			return Err(OpenFundingError::InvalidAmount);
		}
		let now_ms = current_unix_millis();
		let session = FundingSession::new(
			intent,
			product.map(|product| product.product_id.clone()),
			direction,
			amount,
			now_ms,
		);
		// Live while the overlay is up, so the overlay can quote and pick a
		// provider, but stored only once the user starts it: an overlay the
		// app is killed under leaves no session behind.
		let presented = registry
			.present(self.platform.as_ref(), now_ms, session.clone())
			.await
			.map_err(OpenFundingError::Session)?;
		if !presented {
			return Err(OpenFundingError::AlreadyExists);
		}
		// The overlay runs on its own task, so a caller that goes away while it
		// is up, such as a product reloading, cannot leave the session
		// presenting: the task still starts or discards it, and the product
		// watches the outcome by its id.
		let (finished, outcome) = oneshot::channel();
		let services = self.clone();
		let product = product.cloned();
		(self.spawner)(Box::pin(async move {
			let result = services.finish_presenting(platform, product, session).await;
			// A caller that went away no longer waits for the result, and
			// the session is already settled either way.
			let _ = finished.send(result);
		}));
		outcome.await.unwrap_or(Err(OpenFundingError::Stopped))
	}

	/// Show the overlay for presented `session`, then start or discard it by
	/// what the user did.
	async fn finish_presenting(
		self: Arc<Self>,
		platform: Arc<dyn FundingPlatform>,
		product: Option<ProductContext>,
		session: FundingSession,
	) -> Result<FundingSession, OpenFundingError> {
		let registry = self.funding();
		let storage = self.platform.as_ref();
		let presentation = FundingPresentation {
			intent: session.intent.clone(),
			direction: session.direction,
			amount: session.amount,
		};
		let presented = platform.present_funding(product.as_ref(), presentation).await;
		match presented {
			Ok(FundingPresentOutcome::Started) => {
				registry
					.start(storage, current_unix_millis(), &session.intent)
					.await
					.map_err(OpenFundingError::Session)?;
				Ok(registry.get(&session.intent).unwrap_or(session))
			},
			outcome => {
				if registry.discard(&session.intent).await {
					return Err(match outcome {
						Ok(_) => OpenFundingError::Dismissed,
						Err(error) => OpenFundingError::Present(error),
					});
				}
				// A provider took the session while the overlay was up, so it
				// runs as started; unless money already moved, the provider is
				// asked to stop, as the user left without starting.
				let now_ms = current_unix_millis();
				registry
					.start(storage, now_ms, &session.intent)
					.await
					.map_err(OpenFundingError::Session)?;
				if registry.get(&session.intent).is_some_and(|kept| !kept.money_moved()) {
					registry
						.cancel(storage, now_ms, &session.intent)
						.await
						.map_err(OpenFundingError::Session)?;
				}
				Ok(registry.get(&session.intent).unwrap_or(session))
			},
		}
	}
}

/// What a top-up moved, from its statuses: its recorded amount once a claim
/// is final, what a partial claim reports, nothing once it was not claimed or
/// the host still does not know it [`UNKNOWN_FUNDS_GRACE_MS`] after a report
/// named it. `None` while undecided: an unfinalized claim, a read that failed,
/// or a stream that ended first.
async fn top_up_moved(
	mut statuses: BoxStream<
		'static,
		Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>,
	>,
	funds: &StartedFunds,
	now_ms: u64,
) -> Option<u128> {
	while let Some(status) = statuses.next().await {
		match status {
			Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true }) => {
				return Some(funds.amount);
			},
			Ok(HostPaymentTopUpStatusSubscribeItem::ClaimedPartially { actual_claimed }) => {
				return Some(actual_claimed);
			},
			Ok(HostPaymentTopUpStatusSubscribeItem::NotClaimed) => return Some(0),
			Err(HostPaymentTopUpStatusSubscribeError::TopUpNotFound) => {
				return unknown_for_good(funds, now_ms).then_some(0);
			},
			Err(HostPaymentTopUpStatusSubscribeError::Unknown { reason }) => {
				tracing::warn!(%reason, "reading a top-up's status failed");
				return None;
			},
			Ok(_) => {},
		}
	}
	None
}

/// What a payment moved, from its statuses: its recorded amount once
/// completed, what a partial claim reports, nothing once it failed or the
/// host still does not know it [`UNKNOWN_FUNDS_GRACE_MS`] after a report named
/// it. `None` while undecided.
async fn payment_moved(
	mut statuses: BoxStream<
		'static,
		Result<HostPaymentStatusSubscribeItem, HostPaymentStatusSubscribeError>,
	>,
	funds: &StartedFunds,
	now_ms: u64,
) -> Option<u128> {
	while let Some(status) = statuses.next().await {
		match status {
			Ok(HostPaymentStatusSubscribeItem::Completed) => return Some(funds.amount),
			Ok(HostPaymentStatusSubscribeItem::ClaimedPartially { actual_claimed }) => {
				return Some(actual_claimed);
			},
			Ok(HostPaymentStatusSubscribeItem::Failed { .. }) => return Some(0),
			Err(HostPaymentStatusSubscribeError::PaymentNotFound) => {
				return unknown_for_good(funds, now_ms).then_some(0);
			},
			Err(HostPaymentStatusSubscribeError::Unknown { reason }) => {
				tracing::warn!(%reason, "reading a payment's status failed");
				return None;
			},
			Ok(HostPaymentStatusSubscribeItem::Processing) => {},
		}
	}
	None
}

/// Whether funds the host does not know have been unknown long enough after
/// a report named them to count as never started.
fn unknown_for_good(funds: &StartedFunds, now_ms: u64) -> bool {
	now_ms.saturating_sub(funds.named_at_ms.unwrap_or_default()) >= UNKNOWN_FUNDS_GRACE_MS
}

#[cfg(test)]
mod tests {
	use super::*;

	use futures::executor::block_on;
	use parity_scale_codec::Encode;

	use crate::{
		host_logic::funding::{FundingStage, RECORDED_PER_OWNER},
		platform::CoreStorageKey,
		test_support::stub_platform,
	};

	const NOW: u64 = 1_700_000_000_000;
	const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

	fn session(intent: &str, opened_at_ms: u64) -> FundingSession {
		FundingSession::new(
			intent.to_string(),
			Some(ProductId::for_test("wallet.dot")),
			FundingDirection::In,
			Some(100),
			opened_at_ms,
		)
	}

	fn insert(registry: &FundingRegistry, storage: &dyn CoreStorage, session: FundingSession) {
		block_on(registry.commit(storage, NOW, move |sessions| {
			sessions.insert(session.intent.clone(), session);
			((), Vec::new())
		}))
		.expect("inserted");
	}

	#[test]
	fn a_settled_session_survives_later_saves() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		insert(&registry, storage.as_ref(), session("fs_old", NOW - DAY_MS));
		block_on(registry.sweep(storage.as_ref(), NOW)).expect("swept");

		insert(&registry, storage.as_ref(), session("fs_new", NOW));

		let persisted: Vec<String> = block_on(load_sessions(storage.as_ref()))
			.expect("loaded")
			.into_iter()
			.map(|session| session.intent)
			.collect();
		assert_eq!(persisted, ["fs_new", "fs_old"]);
	}

	#[test]
	fn a_subscriber_sees_the_current_stage_then_expiry_and_the_end() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		insert(&registry, storage.as_ref(), session("fs_1", NOW));
		let stream = registry.subscribe("fs_1").expect("session exists");

		block_on(registry.sweep(storage.as_ref(), NOW + DAY_MS)).expect("swept");

		assert_eq!(
			block_on(stream.collect::<Vec<_>>()),
			vec![
				HostFundingStatusSubscribeItem::InProgress { expires_at: Some(NOW + DAY_MS) },
				HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired },
			]
		);
	}

	#[test]
	fn subscribing_drops_watchers_that_went_away() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		insert(&registry, storage.as_ref(), session("fs_1", NOW));
		for _ in 0..3 {
			drop(registry.subscribe("fs_1").expect("session exists"));
		}
		let _live = registry.subscribe("fs_1").expect("session exists");

		assert_eq!(registry.lock_subscribers()["fs_1"].len(), 1);
	}

	#[test]
	fn a_restart_restores_open_sessions_and_expires_overdue_ones() {
		let storage = stub_platform();
		let before = FundingRegistry::default();
		insert(&before, storage.as_ref(), session("fs_live", NOW));
		insert(&before, storage.as_ref(), session("fs_overdue", NOW - DAY_MS));

		let after = FundingRegistry::default();
		block_on(after.sweep(storage.as_ref(), NOW)).expect("loaded");

		assert_eq!(after.get("fs_live"), Some(session("fs_live", NOW)));
		assert_eq!(
			after.get("fs_overdue").map(|session| session.stage),
			Some(FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms: NOW })
		);
	}

	// The host's list shows what is in flight first, then what ended, each
	// newest first.
	#[test]
	fn sessions_list_in_flight_first_then_ended_newest_first() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		let ended = |intent, settled_at_ms| {
			let mut session = session(intent, settled_at_ms - 1);
			session.fail(FundingFailure::Expired, settled_at_ms);
			session
		};
		for session in [
			ended("fs_old", NOW - 2),
			session("fs_live_old", NOW - 5),
			ended("fs_new", NOW - 1),
			session("fs_live_new", NOW),
		] {
			insert(&registry, storage.as_ref(), session);
		}

		assert_eq!(
			registry
				.sessions()
				.into_iter()
				.map(|session| session.intent)
				.collect::<Vec<_>>(),
			["fs_live_new", "fs_live_old", "fs_new", "fs_old"]
		);
	}

	fn worker(provider_id: &str) -> ProductContext {
		ProductContext::new_with_execution(
			ProductId::for_test(provider_id),
			ProductExecutionKind::Worker,
		)
	}

	/// Funds a report named, as the core follows them.
	fn named_funds_record(host_id: [u8; 32], amount: u128) -> StartedFunds {
		StartedFunds {
			host_id,
			amount,
			terms: [0; 32],
			recorded_at_ms: NOW,
			accepted: true,
			named_at_ms: Some(NOW),
		}
	}

	/// Record funds `provider` starts under `host_id` for session `intent`.
	fn record(
		registry: &FundingRegistry,
		storage: &dyn CoreStorage,
		provider: &str,
		intent: &str,
		host_id: [u8; 32],
		amount: u128,
	) -> Option<StartRecord> {
		block_on(registry.record_start(
			storage,
			NOW,
			provider,
			intent,
			FundsAsked { direction: FundingDirection::In, host_id, amount, terms: [0; 32] },
		))
		.expect("stored")
	}

	// Funds belong to the session the provider names when it starts them,
	// whatever their amount: a first top-up of 100 for session B, which asks
	// 200, is B's even while session A asks 100, so A can still be
	// cancelled and B can name it.
	#[test]
	fn started_funds_belong_to_the_session_they_were_started_for() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		for (intent, amount) in [("fs_a", 100), ("fs_b", 200)] {
			let mut session = FundingSession::new(
				intent.to_string(),
				Some(ProductId::for_test("wallet.dot")),
				FundingDirection::In,
				Some(amount),
				NOW,
			);
			assert!(session.assign("ramp.dot", None, NOW));
			insert(&registry, storage.as_ref(), session);
		}
		let host_id = host_payment_id(&worker("ramp.dot"), [1; 32]);
		let recorded = record(&registry, storage.as_ref(), "ramp.dot", "fs_b", host_id, 100);
		block_on(registry.finish_start(storage.as_ref(), NOW, "fs_b", host_id, true))
			.expect("stored");
		let report = |intent: &str| {
			block_on(registry.report(
				storage.as_ref(),
				NOW,
				&worker("ramp.dot"),
				intent,
				FundingUpdate::Crediting { top_up_id: [1; 32], amount: 100 },
			))
			.expect("stored")
		};

		assert_eq!(
			(
				recorded,
				report("fs_a"),
				report("fs_b"),
				block_on(registry.cancel(storage.as_ref(), NOW, "fs_a")).expect("stored"),
				registry.get("fs_a").expect("kept").started,
			),
			(
				Some(StartRecord::New),
				Err(HostFundingReportError::NotStarted),
				Ok(()),
				CancelOutcome::Requested,
				Vec::new(),
			)
		);
	}

	// A top-up settles one session once: its id cannot be recorded for a
	// second session of the provider, a session it was not started for
	// cannot name it, and nor can another provider, or a session the
	// provider does not serve.
	#[test]
	fn started_funds_are_named_once_by_the_session_they_were_started_for() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		let served = |intent: &str, provider: &str| {
			let mut session = session(intent, NOW);
			assert!(session.assign(provider, None, NOW));
			session
		};
		for session in
			[served("fs_a", "ramp.dot"), served("fs_b", "ramp.dot"), served("fs_c", "other.dot")]
		{
			insert(&registry, storage.as_ref(), session);
		}
		let host_id = host_payment_id(&worker("ramp.dot"), [9; 32]);
		let recorded = record(&registry, storage.as_ref(), "ramp.dot", "fs_a", host_id, 100);
		let again = record(&registry, storage.as_ref(), "ramp.dot", "fs_b", host_id, 100);
		let unserved = record(&registry, storage.as_ref(), "ramp.dot", "fs_c", host_id, 100);
		block_on(registry.finish_start(storage.as_ref(), NOW, "fs_a", host_id, true))
			.expect("stored");
		let crediting = FundingUpdate::Crediting { top_up_id: [9; 32], amount: 100 };
		let report = |provider: &str, intent: &str| {
			block_on(registry.report(
				storage.as_ref(),
				NOW,
				&worker(provider),
				intent,
				crediting.clone(),
			))
			.expect("stored")
		};

		assert_eq!(
			(
				(recorded, again, unserved),
				report("ramp.dot", "fs_a"),
				report("ramp.dot", "fs_b"),
				report("other.dot", "fs_c"),
				registry.get("fs_a").expect("kept").started,
			),
			(
				(Some(StartRecord::New), Some(StartRecord::Conflict), None),
				Ok(()),
				Err(HostFundingReportError::NotStarted),
				Err(HostFundingReportError::NotStarted),
				vec![named_funds_record(host_id, 100)],
			)
		);
	}

	// A claim that is not final yet can still be reversed, and a top-up the
	// host does not know may only be slow to appear: neither settles the
	// session until the host decides or the grace period runs out.
	#[test]
	fn only_a_final_claim_or_a_long_unknown_top_up_settles() {
		let funds = named_funds_record([1; 32], 100);
		let read = |statuses: Vec<_>, now_ms| {
			block_on(top_up_moved(stream::iter(statuses).boxed(), &funds, now_ms))
		};
		let not_found = || vec![Err(HostPaymentTopUpStatusSubscribeError::TopUpNotFound)];

		assert_eq!(
			(
				read(
					vec![Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false })],
					NOW
				),
				read(
					vec![
						Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false }),
						Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true })
					],
					NOW
				),
				read(
					vec![Ok(HostPaymentTopUpStatusSubscribeItem::ClaimedPartially {
						actual_claimed: 40
					})],
					NOW
				),
				read(not_found(), NOW + UNKNOWN_FUNDS_GRACE_MS - 1),
				read(not_found(), NOW + UNKNOWN_FUNDS_GRACE_MS),
				read(
					vec![Err(HostPaymentTopUpStatusSubscribeError::Unknown {
						reason: "offline".into()
					})],
					NOW
				),
			),
			(None, Some(100), Some(40), None, Some(0), None)
		);
	}

	#[test]
	fn a_payment_settles_from_its_completion_or_a_long_absence() {
		let funds = named_funds_record([1; 32], 100);
		let read = |statuses: Vec<_>, now_ms| {
			block_on(payment_moved(stream::iter(statuses).boxed(), &funds, now_ms))
		};
		let not_found = || vec![Err(HostPaymentStatusSubscribeError::PaymentNotFound)];

		assert_eq!(
			(
				read(vec![Ok(HostPaymentStatusSubscribeItem::Processing)], NOW),
				read(vec![Ok(HostPaymentStatusSubscribeItem::Completed)], NOW),
				read(not_found(), NOW + UNKNOWN_FUNDS_GRACE_MS - 1),
				read(not_found(), NOW + UNKNOWN_FUNDS_GRACE_MS),
			),
			(None, Some(100), None, Some(0))
		);
	}

	/// A host that, told of a change, cancels the next session in
	/// `cancels` from inside the callback, as a host recording its history
	/// there would acknowledge from there.
	struct ReentrantHost {
		registry: OnceLock<Weak<FundingRegistry>>,
		storage: Arc<crate::test_support::StubPlatform>,
		cancels: Mutex<VecDeque<&'static str>>,
		/// Each session the host heard of, and whether the write lock was
		/// free at the time.
		heard: Mutex<Vec<(String, bool)>>,
	}

	#[async_trait::async_trait]
	impl FundingPlatform for ReentrantHost {
		async fn present_funding(
			&self,
			_product: Option<&ProductContext>,
			_session: FundingPresentation,
		) -> Result<FundingPresentOutcome, GenericError> {
			Ok(FundingPresentOutcome::Dismissed)
		}

		async fn present_provider_frame(
			&self,
			_provider: &ProductContext,
			_intent: String,
			_target: truapi::latest::FundingFrameTarget,
		) -> Result<truapi::latest::FundingFrameOutcome, GenericError> {
			Ok(truapi::latest::FundingFrameOutcome::Closed)
		}

		fn funding_session_changed(&self, intent: String, _status: HostFundingStatusSubscribeItem) {
			let registry = self.registry.get().and_then(Weak::upgrade).expect("registry alive");
			// Calling back while the lock is held would hang, so that is
			// recorded instead of attempted.
			let writes_free = registry.writes.try_lock().is_some();
			self.heard.lock().expect("heard mutex poisoned").push((intent, writes_free));
			let next = self.cancels.lock().expect("cancels mutex poisoned").pop_front();
			if let (true, Some(next)) = (writes_free, next) {
				let storage = self.storage.clone();
				std::thread::spawn(move || block_on(registry.cancel(storage.as_ref(), NOW, next)))
					.join()
					.expect("cancel thread")
					.expect("cancelled");
			}
		}
	}

	// A host told of a change may act on it straight away, and must still
	// hear of every change in the order it was made.
	#[test]
	fn the_host_hears_of_changes_in_order_and_may_call_back() {
		let storage = stub_platform();
		let registry = Arc::new(FundingRegistry::default());
		insert(&registry, storage.as_ref(), session("fs_a", NOW));
		insert(&registry, storage.as_ref(), session("fs_b", NOW));
		let host = Arc::new(ReentrantHost {
			registry: OnceLock::new(),
			storage: storage.clone(),
			cancels: Mutex::new(VecDeque::from(["fs_b"])),
			heard: Mutex::new(Vec::new()),
		});
		assert!(host.registry.set(Arc::downgrade(&registry)).is_ok());
		assert!(registry.install_platform(host.clone()));

		assert_eq!(
			block_on(registry.cancel(storage.as_ref(), NOW, "fs_a")).expect("cancelled"),
			CancelOutcome::Ended
		);

		assert_eq!(
			*host.heard.lock().expect("heard mutex poisoned"),
			[("fs_a".to_string(), true), ("fs_b".to_string(), true)]
		);
	}

	#[test]
	fn a_failed_save_changes_nothing_in_memory() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		insert(&registry, storage.as_ref(), session("fs_1", NOW));
		let failing = crate::test_support::StubPlatform {
			local_storage_error: Some("disk full"),
			..Default::default()
		};

		let failed = block_on(registry.commit(&failing, NOW, |sessions| {
			sessions.clear();
			((), Vec::new())
		}));

		assert!(failed.is_err());
		assert_eq!(registry.get("fs_1"), Some(session("fs_1", NOW)));
	}

	#[test]
	fn a_session_whose_start_failed_is_not_stored_by_a_later_commit() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		block_on(registry.present(storage.as_ref(), NOW, session("fs_1", NOW))).expect("presented");
		let failing = crate::test_support::StubPlatform {
			local_storage_error: Some("disk full"),
			..Default::default()
		};

		let started = block_on(registry.start(&failing, NOW, "fs_1"));
		insert(&registry, storage.as_ref(), session("fs_2", NOW));

		assert!(started.is_err());
		assert_eq!(registry.get("fs_1"), None);
		assert_eq!(block_on(load_sessions(storage.as_ref())), Ok(vec![session("fs_2", NOW)]));
	}

	// A product that reloads after the host recorded its session must still
	// learn how it ended, not read it as a session that never existed.
	#[test]
	fn a_recorded_session_still_answers_its_owner_after_a_restart() {
		let storage = stub_platform();
		let before = FundingRegistry::default();
		insert(&before, storage.as_ref(), session("fs_1", NOW - DAY_MS));
		block_on(before.sweep(storage.as_ref(), NOW)).expect("expired");
		assert!(block_on(before.acknowledge(storage.as_ref(), NOW, "fs_1")).expect("recorded"));
		let expired = HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired };

		let after = FundingRegistry::default();
		block_on(after.sweep(storage.as_ref(), NOW)).expect("loaded");
		let replay = after.subscribe("fs_1").expect("recorded session kept");

		assert_eq!(
			(
				block_on(replay.collect::<Vec<_>>()),
				after.sessions(),
				after.subscribe("fs_9").is_none()
			),
			(vec![expired], Vec::new(), true)
		);
	}

	// A recorded session pushed out by newer ones of its owner is compacted,
	// not dropped: after a restart its owner still reads how it ended, and
	// the host can rebuild its history from every recorded session.
	#[test]
	fn a_compacted_session_still_answers_its_owner_after_a_restart() {
		let storage = stub_platform();
		let before = FundingRegistry::default();
		let recorded = |n: u64| FundingSession {
			acknowledged: true,
			stage: FundingStage::Delivered { credited: u128::from(n) + 1, settled_at_ms: NOW + n },
			..session(&format!("fs_{n}"), NOW)
		};
		block_on(before.commit(storage.as_ref(), NOW, |sessions| {
			for n in 0..=RECORDED_PER_OWNER as u64 {
				sessions.insert(format!("fs_{n}"), recorded(n));
			}
			((), Vec::new())
		}))
		.expect("stored");

		let after = FundingRegistry::default();
		block_on(after.sweep(storage.as_ref(), NOW)).expect("loaded");
		let replay = after.subscribe("fs_0").expect("compacted session kept");

		assert_eq!(
			(
				after.get("fs_0"),
				block_on(replay.collect::<Vec<_>>()),
				after.owner("fs_0"),
				after.get("fs_1").is_some(),
				after.history(),
			),
			(
				None,
				vec![HostFundingStatusSubscribeItem::Delivered { credited: 1 }],
				Some(Some(ProductId::for_test("wallet.dot"))),
				true,
				(0..=RECORDED_PER_OWNER as u64)
					.rev()
					.map(|n| recorded(n).summary())
					.collect::<Vec<_>>(),
			)
		);
	}

	// Showing a session in the overlay stores nothing, so it cannot fail on a
	// store that is down and leaves the stored set as it was.
	#[test]
	fn holding_a_session_in_the_overlay_writes_nothing() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		insert(&registry, storage.as_ref(), session("fs_1", NOW));
		let failing = crate::test_support::StubPlatform {
			local_storage_error: Some("disk full"),
			..Default::default()
		};

		let presented = block_on(registry.present(&failing, NOW, session("fs_2", NOW)));

		assert_eq!(
			(presented, block_on(load_sessions(storage.as_ref()))),
			(Ok(true), Ok(vec![session("fs_1", NOW)]))
		);
	}

	// An intent names one session for good, so one held in full, presented or
	// compacted cannot be presented again.
	#[test]
	fn a_held_intent_cannot_be_presented_again() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		insert(&registry, storage.as_ref(), session("fs_1", NOW));
		registry.lock_pack().insert(
			"fs_packed".to_string(),
			FundingSessionSummary::from(FundingSession {
				stage: FundingStage::Delivered { credited: 1, settled_at_ms: NOW },
				..session("fs_packed", NOW)
			}),
		);

		assert_eq!(
			[
				block_on(registry.present(storage.as_ref(), NOW, session("fs_1", NOW))),
				block_on(registry.present(storage.as_ref(), NOW, session("fs_packed", NOW))),
				block_on(registry.present(storage.as_ref(), NOW, session("fs_new", NOW))),
				block_on(registry.present(storage.as_ref(), NOW, session("fs_new", NOW))),
			],
			[Ok(false), Ok(false), Ok(true), Ok(false)]
		);
	}

	// A product watching a session the user then dismissed hears that it
	// ended, rather than waiting on a session that is gone.
	#[test]
	fn a_discarded_session_ends_its_watchers_as_cancelled() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		assert_eq!(
			block_on(registry.present(storage.as_ref(), NOW, session("fs_1", NOW))),
			Ok(true)
		);
		let watch = registry.subscribe("fs_1").expect("presented session watchable");

		block_on(registry.discard("fs_1"));

		assert_eq!(
			(block_on(watch.collect::<Vec<_>>()), registry.get("fs_1")),
			(
				vec![
					HostFundingStatusSubscribeItem::InProgress { expires_at: Some(NOW + DAY_MS) },
					HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
				],
				None,
			)
		);
	}

	// Sessions a newer app stored are left alone: every commit fails, and none
	// writes over them.
	#[test]
	fn sessions_of_a_newer_format_make_the_registry_read_only() {
		let storage = stub_platform();
		let blob = (u16::MAX, vec![session("fs_1", NOW)]).encode();
		block_on(storage.write_core_storage(CoreStorageKey::FundingSessions, blob.clone()))
			.expect("written");
		let registry = FundingRegistry::default();

		let presented = block_on(registry.present(storage.as_ref(), NOW, session("fs_2", NOW)));

		assert_eq!(
			(
				presented,
				registry.get("fs_2"),
				block_on(storage.read_core_storage(CoreStorageKey::FundingSessions)),
			),
			(Err(FundingSessionError::NewerFormat { format: u16::MAX }), None, Ok(Some(blob)),)
		);
	}

	#[test]
	fn a_session_whose_overlay_is_up_cannot_be_cancelled() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		block_on(registry.present(storage.as_ref(), NOW, session("fs_1", NOW))).expect("presented");

		assert_eq!(
			block_on(registry.cancel(storage.as_ref(), NOW, "fs_1")),
			Ok(CancelOutcome::Refused)
		);
		assert_eq!(registry.get("fs_1").map(|session| session.is_terminal()), Some(false));
	}
}

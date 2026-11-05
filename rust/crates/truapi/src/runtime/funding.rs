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

#[cfg(not(target_arch = "wasm32"))]
mod store;
#[cfg(test)]
pub(crate) use store::put_session_row;
#[cfg(test)]
pub(crate) use store::{fail_writes, stored_sessions};

/// A wasm core has no core database, so it holds no funding session: every
/// commit fails before it changes anything.
#[cfg(target_arch = "wasm32")]
mod store {
	use crate::host_logic::funding::{FundingSession, FundingSessionError, FundingSessionSummary};

	pub(super) enum Db {}

	pub(super) async fn load(
		db: &Db,
	) -> Result<(Vec<FundingSession>, Vec<FundingSessionSummary>), FundingSessionError> {
		match *db {}
	}

	pub(super) async fn write(
		db: &Db,
		_sessions: &[&FundingSession],
		_removed: Vec<String>,
		_summaries: &[FundingSessionSummary],
	) -> Result<(), FundingSessionError> {
		match *db {}
	}
}

use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::{
		Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
};

use core::{ops::ControlFlow, time::Duration};

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
		retained,
	},
	platform::{
		ExecutableKind, FundingPlatform, FundingPresentOutcome, FundingPresentation,
		ProductContext, ProductId,
	},
	runtime::payment_id::host_payment_id,
	time::{Backoff, retry},
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
	/// Being handed over, and due again once that ends, as a call outside
	/// it went unanswered meanwhile.
	RunningThenDue,
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

/// What a native host draws its funding screens from, pushed by the core so
/// the host never reads it back. A host without one gets only
/// [`FundingPlatform::funding_session_changed`].
pub trait FundingFeed: Send + Sync {
	/// A session changed: its status and the progress the host draws. Every
	/// session the host has to hear of comes through here once more when
	/// funding resumes. `progress` is `None` once the core keeps only the
	/// session's summary.
	fn funding_session_changed(
		&self,
		intent: String,
		status: HostFundingStatusSubscribeItem,
		progress: Option<crate::host_logic::funding::FundingProgress>,
	);
}

/// Host-global funding sessions, their subscribers and the host surface.
#[derive(Default)]
pub struct FundingRegistry {
	sessions: Mutex<HashMap<String, FundingSession>>,
	/// Recorded sessions compacted out of [`Self::sessions`], by intent.
	/// Should a session sit in both, the full record wins.
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
	/// The feed a native host draws its funding screens from.
	feed: OnceLock<Arc<dyn FundingFeed>>,
	/// The core database the sessions are stored in.
	db: OnceLock<store::Db>,
	/// Each provider's worker connection, by product id.
	links: Mutex<HashMap<ProductId, Link>>,
	/// Callers waiting for a provider's worker to connect, by product id.
	link_waiters: Mutex<HashMap<ProductId, Vec<oneshot::Sender<()>>>>,
	/// The generation the next link takes.
	next_generation: AtomicU64,
	/// Sessions holding a reference on their provider's worker, with that
	/// provider. Only the reconciler changes it.
	holding: Mutex<HashMap<String, ProductId>>,
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

	/// Install the feed the host draws its funding screens from, before the
	/// funding surface, whose install resumes funding. Set-once; returns
	/// whether this call installed it.
	pub fn install_feed(&self, feed: Arc<dyn FundingFeed>) -> bool {
		self.feed.set(feed).is_ok()
	}

	/// The host's funding surface, when one is installed.
	pub fn platform(&self) -> Option<Arc<dyn FundingPlatform>> {
		self.platform.get().cloned()
	}

	/// Install the core database the sessions are stored in. Set-once;
	/// returns whether this call installed it.
	#[cfg(not(target_arch = "wasm32"))]
	pub fn install_db(&self, db: crate::store::Db) -> bool {
		self.db.set(db).is_ok()
	}

	/// The core database, or a storage error while none is installed.
	fn db(&self) -> Result<&store::Db, FundingSessionError> {
		self.db.get().ok_or_else(|| FundingSessionError::Storage {
			reason: "no core database configured".to_string(),
		})
	}

	/// A registry belonging to `services`.
	pub fn new(services: Weak<RuntimeServices>) -> Self {
		Self { services, ..Self::default() }
	}

	/// Assign open session `intent` to the provider the user chose, and hand
	/// it to that provider. Returns whether it was open and unassigned.
	pub async fn select_provider(
		&self,
		now_ms: u64,
		intent: &str,
		provider_id: &ProductId,
		choice: FundingChoice,
	) -> Result<bool, FundingSessionError> {
		let intent = intent.to_string();
		let provider = provider_id.clone();
		let assigned = self
			.commit(now_ms, move |sessions| {
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
	pub fn link(&self, provider_id: &ProductId, link: Arc<dyn FundingProviderLink>) {
		let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
		self.lock_links()
			.insert(provider_id.clone(), Link { link, generation, catch_up: CatchUp::Connecting });
	}

	/// Drop `link`, unless a newer one replaced it.
	pub fn unlink(&self, provider_id: &ProductId, link: &Arc<dyn FundingProviderLink>) {
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
	pub fn connected(&self, provider_id: &ProductId) {
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
	pub async fn connected_link(&self, provider_id: &ProductId) -> Arc<dyn FundingProviderLink> {
		self.connected_link_besides(provider_id, None).await
	}

	/// As [`Self::connected_link`], but waiting past `besides`, a link a call
	/// just failed on, for the connection that follows it.
	pub async fn connected_link_besides(
		&self,
		provider_id: &ProductId,
		besides: Option<&Arc<dyn FundingProviderLink>>,
	) -> Arc<dyn FundingProviderLink> {
		loop {
			let connected = {
				let links = self.lock_links();
				if let Some(held) = links.get(provider_id).filter(|held| {
					held.catch_up != CatchUp::Connecting &&
						!besides.is_some_and(|failed| Arc::ptr_eq(&held.link, failed))
				}) {
					return held.link.clone();
				}
				let (sender, connected) = oneshot::channel();
				lock(&self.link_waiters).entry(provider_id.clone()).or_default().push(sender);
				connected
			};
			let _ = connected.await;
		}
	}

	/// What `provider_id`'s worker has to hear when it connects, one list per
	/// session it serves, oldest first.
	fn pending_calls(&self, provider_id: &ProductId) -> Vec<Vec<ProviderCall>> {
		let sessions = self.lock_sessions();
		let mut served: Vec<&FundingSession> = sessions
			.values()
			.filter(|session| {
				!session.is_terminal() && session.provider_id.as_ref() == Some(provider_id)
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
		now_ms: u64,
		provider: &ProductContext,
		intent: &str,
		update: FundingUpdate,
	) -> Result<Result<(), HostFundingReportError>, FundingSessionError> {
		let intent = intent.to_string();
		let provider_id = &provider.product_id;
		let host_id = |id: &[u8; 32]| host_payment_id(provider, *id);
		self.commit(now_ms, move |sessions| {
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
	pub fn holds_start(&self, provider_id: &ProductId, host_id: &[u8; 32]) -> bool {
		self.lock_sessions().values().any(|session| {
			session.provider_id.as_ref() == Some(provider_id) &&
				session.started.iter().any(|record| record.host_id == *host_id)
		})
	}

	/// Record funds `provider_id` starts for session `intent`, before the
	/// host is asked. `None` when that session is not open and served by
	/// `provider_id` in their direction; a `Conflict` when another of its
	/// sessions holds their id.
	pub async fn record_start(
		&self,
		now_ms: u64,
		provider_id: &ProductId,
		intent: &str,
		asked: FundsAsked,
	) -> Result<Option<StartRecord>, FundingSessionError> {
		let FundsAsked { direction, host_id, amount, terms } = asked;
		self.commit(now_ms, |sessions| {
			let elsewhere = sessions.values().any(|session| {
				session.intent != intent &&
					session.provider_id.as_ref() == Some(provider_id) &&
					session.started.iter().any(|record| record.host_id == host_id)
			});
			let Some(session) = sessions.get_mut(intent).filter(|session| {
				!session.is_terminal() &&
					session.direction == direction &&
					session.provider_id.as_ref() == Some(provider_id)
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
		now_ms: u64,
		intent: &str,
		host_id: [u8; 32],
		taken: bool,
	) -> Result<(), FundingSessionError> {
		self.commit(now_ms, |sessions| {
			let changed = sessions.get_mut(intent).is_some_and(|session| {
				if taken { session.accept_start(&host_id) } else { session.forget_start(&host_id) }
			});
			((), Vec::from_iter(changed.then(|| intent.to_string())))
		})
		.await
	}

	/// Whether open session `intent` is assigned to `provider_id`.
	pub fn is_serving(&self, provider_id: &ProductId, intent: &str) -> bool {
		self.get(intent).is_some_and(|session| {
			!session.is_terminal() && session.provider_id.as_ref() == Some(provider_id)
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
		now_ms: u64,
		intent: &str,
	) -> Result<bool, FundingSessionError> {
		let intent = intent.to_string();
		self.commit(now_ms, move |sessions| {
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
		now_ms: u64,
		intent: &str,
	) -> Result<CancelOutcome, FundingSessionError> {
		let target = intent.to_string();
		let (outcome, provider) = self
			.commit(now_ms, move |sessions| {
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
		now_ms: u64,
		intent: &str,
		terms: Option<CancelTerms>,
	) -> Result<CancelConfirmation, FundingSessionError> {
		let target = intent.to_string();
		let (confirmation, call) = self
			.commit(now_ms, move |sessions| {
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
		now_ms: u64,
		session: FundingSession,
	) -> Result<bool, FundingSessionError> {
		let intent = session.intent.clone();
		let presented = self
			.commit(now_ms, |sessions| {
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
	async fn start(&self, now_ms: u64, intent: &str) -> Result<(), FundingSessionError> {
		// Leaving the overlay inside the commit, so no other commit stores
		// the session before this one announces it.
		let started = self
			.commit(now_ms, |_| {
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
	/// way out. Its product may be watching it, so the stream ends as
	/// cancelled: the user never started it.
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

	/// Apply `edit` to a copy of the sessions, persist what changed, then make
	/// it current and notify every session `edit` names. An edit may move
	/// sessions in or out of the overlay; when the write fails, that move is
	/// undone along with the rest.
	///
	/// Persisted sessions are loaded first on the first call, and every
	/// commit expires the sessions past their deadline, so a long suspend
	/// cannot leave an overdue session open.
	pub async fn commit<R>(
		&self,
		now_ms: u64,
		edit: impl FnOnce(&mut HashMap<String, FundingSession>) -> (R, Vec<String>),
	) -> Result<R, FundingSessionError> {
		let mut loaded = self.writes.lock().await;
		let db = self.db()?;
		if !*loaded {
			// Held as stored, so a commit writes only the rows it changes.
			let (sessions, summaries) = store::load(db).await?;
			self.lock_sessions()
				.extend(sessions.into_iter().map(|session| (session.intent.clone(), session)));
			self.lock_pack()
				.extend(summaries.into_iter().map(|summary| (summary.intent.clone(), summary)));
			*loaded = true;
			self.loaded.store(true, Ordering::Release);
		}
		let presenting_before = self.lock_presenting().clone();
		let mut working = self.lock_sessions().clone();
		let mut changed = Vec::new();
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
		if announces || working != *self.lock_sessions() {
			let (kept, compacted) = retained(working.into_values());
			let kept: HashMap<String, FundingSession> =
				kept.into_iter().map(|session| (session.intent.clone(), session)).collect();
			// An assigned session is stored even while its overlay is up, as
			// its provider's reports are stored before they are answered. A
			// session only shown in the overlay is not stored, so holding one
			// changes nothing worth writing.
			let stored = |presenting: &HashSet<String>, session: &FundingSession| {
				!presenting.contains(&session.intent) || session.provider_id.is_some()
			};
			let (put, removed) = {
				let presenting = self.lock_presenting();
				let before = self.lock_sessions();
				let put: Vec<&FundingSession> = kept
					.values()
					.filter(|session| {
						stored(&presenting, session) &&
							before.get(&session.intent).is_none_or(|held| {
								held != *session || !stored(&presenting_before, held)
							})
					})
					.collect();
				let removed: Vec<String> = before
					.values()
					.filter(|held| {
						stored(&presenting_before, held) &&
							!kept
								.get(&held.intent)
								.is_some_and(|session| stored(&presenting, session))
					})
					.map(|held| held.intent.clone())
					.collect();
				(put, removed)
			};
			let writes = !put.is_empty() || !removed.is_empty() || !compacted.is_empty();
			if writes && let Err(error) = store::write(db, &put, removed, &compacted).await {
				*self.lock_presenting() = presenting_before;
				return Err(error);
			}
			self.lock_pack()
				.extend(compacted.into_iter().map(|summary| (summary.intent.clone(), summary)));
			*self.lock_sessions() = kept;
		}
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
	/// none, and one without a database holds no session to work on.
	pub fn wake(&self) {
		let Some(services) = self.services.upgrade().filter(|_| self.db.get().is_some()) else {
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
			match self.sweep(now_ms).await {
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
		let assigned: HashMap<String, ProductId> = self
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
					release.push(provider.clone());
				}
				kept
			});
			(acquire, release)
		};
		// Taken before let go, so a provider moving from an ended session to
		// a new one keeps its worker running.
		for provider in &acquire {
			services.worker_ledger.acquire(provider);
		}
		for provider in &release {
			services.worker_ledger.release(provider);
		}
	}

	/// Follow every session whose outcome is due, once the engine its funds
	/// went through is installed.
	fn follow_settlements(&self, services: &Arc<RuntimeServices>) {
		let due: Vec<(String, ProductId, Settlement)> = self
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
		let pending: Vec<(String, ProductId, FundingDirection, StartedFunds)> = self
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
		let due: Vec<(ProductId, u64, Arc<dyn FundingProviderLink>)> = self
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
						answered &= registry.put(link.as_ref(), call).await;
					}
				}
				registry.caught_up(&provider, generation, answered);
			}));
		}
	}

	/// Record how handing link `generation` of `provider_id` its sessions
	/// went: done when every call was answered, due again shortly otherwise.
	/// A link replaced meanwhile is left as it is.
	fn caught_up(&self, provider_id: &ProductId, generation: u64, answered: bool) {
		let retry_ms = current_unix_millis().saturating_add(CATCH_UP_RETRY.as_millis() as u64);
		let due = {
			let mut links = self.lock_links();
			let held = links.get_mut(provider_id).filter(|held| held.generation == generation);
			let due = !answered ||
				held.as_ref().is_some_and(|held| held.catch_up == CatchUp::RunningThenDue);
			if let Some(held) = held {
				held.catch_up =
					if due { CatchUp::Due { not_before_ms: retry_ms } } else { CatchUp::Done };
			}
			due
		};
		if due {
			self.retry_by(retry_ms);
		}
	}

	/// A call outside a catch-up went unanswered on link `generation` of
	/// `provider_id`: hand the link its sessions again, which repeats the
	/// call. A link still connecting or already due is caught up anyway, and
	/// one being caught up is due again once that ends.
	fn call_unanswered(&self, provider_id: &ProductId, generation: u64) {
		let retry_ms = current_unix_millis().saturating_add(CATCH_UP_RETRY.as_millis() as u64);
		let mut links = self.lock_links();
		let Some(held) = links.get_mut(provider_id).filter(|held| held.generation == generation)
		else {
			return;
		};
		match held.catch_up {
			CatchUp::Done => held.catch_up = CatchUp::Due { not_before_ms: retry_ms },
			CatchUp::Running => held.catch_up = CatchUp::RunningThenDue,
			CatchUp::Connecting | CatchUp::Due { .. } | CatchUp::RunningThenDue => return,
		}
		drop(links);
		self.retry_by(retry_ms);
	}

	/// Milliseconds until the next pass is due on its own: a session expires
	/// or its offer lapses, a step asked to be retried, a link is due to be
	/// handed its sessions again, or recorded funds are due to be read again
	/// or dropped.
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
		// Only deadlines still ahead: one already past waits on a call in
		// flight under its id, which wakes the reconciler when it ends.
		let resolving = self
			.lock_resolving()
			.values()
			.copied()
			.filter(|at_ms| *at_ms > now_ms && *at_ms != u64::MAX)
			.min();
		[self.next_deadline(), retry, link, resolving]
			.into_iter()
			.flatten()
			.min()
			.map(|at_ms| at_ms.saturating_sub(now_ms))
	}

	/// Put `calls` to `provider_id`'s worker one after another, on a task of
	/// their own, and store its answers. Without a link they wait for the
	/// worker to connect, which repeats whatever is still pending. A call
	/// that goes unanswered leaves the link due to be handed its sessions
	/// again, which repeats it.
	fn call(&self, provider_id: &ProductId, calls: Vec<ProviderCall>) {
		let Some(services) = self.services.upgrade() else {
			return;
		};
		let Some((link, generation)) = self
			.lock_links()
			.get(provider_id)
			.map(|held| (held.link.clone(), held.generation))
		else {
			return;
		};
		let provider_id = provider_id.clone();
		let spawner = services.spawner.clone();
		spawner(Box::pin(async move {
			let registry = services.funding().clone();
			let mut answered = true;
			for call in calls {
				answered &= registry.put(link.as_ref(), call).await;
			}
			if !answered {
				registry.call_unanswered(&provider_id, generation);
			}
		}));
	}

	/// Put one call to a provider's worker and store its answer. Returns
	/// whether the worker answered within [`CALL_TIMEOUT`] and the answer was
	/// stored.
	async fn put(&self, link: &dyn FundingProviderLink, call: ProviderCall) -> bool {
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
					.commit(current_unix_millis(), move |sessions| {
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
					.commit(current_unix_millis(), move |sessions| {
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

	/// Load persisted sessions if they are not loaded yet and expire the
	/// overdue ones; the reconciler then keeps expiring the rest on time.
	pub async fn load(&self) -> Result<(), FundingSessionError> {
		self.sweep(current_unix_millis()).await
	}

	/// Commit no edit, which loads persisted sessions on first use and
	/// expires the overdue ones.
	async fn sweep(&self, now_ms: u64) -> Result<(), FundingSessionError> {
		self.commit(now_ms, |_| ((), Vec::new())).await
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
			platform.funding_session_changed(intent.clone(), item.clone());
			if let Some(feed) = self.feed.get() {
				// Read as the session stands now, which is at least as new as
				// `item`.
				let progress = self.get(&intent).map(|session| session.progress());
				feed.funding_session_changed(intent, item, progress);
			}
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

	fn lock_links(&self) -> MutexGuard<'_, HashMap<ProductId, Link>> {
		self.links.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn lock_holding(&self) -> MutexGuard<'_, HashMap<String, ProductId>> {
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

/// A fresh session id.
pub fn new_funding_intent() -> String {
	format!("fs_{}", nanoid::nanoid!(10))
}

// Pauses between retries when loading the sessions fails on resume.
#[cfg(not(test))]
const RESUME_RETRY_PAUSES: Backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(60));
#[cfg(test)]
const RESUME_RETRY_PAUSES: Backoff =
	Backoff::new(Duration::from_millis(1), Duration::from_millis(4));

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
		match self.open_funding(None, new_funding_intent(), direction, amount).await {
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
			let mut pauses = RESUME_RETRY_PAUSES;
			retry(
				|| async {
					let loaded = registry
						.commit(current_unix_millis(), |sessions| {
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
						Ok(()) => ControlFlow::Break(()),
						// Left for the newer app that wrote them; retrying cannot
						// read them.
						Err(error @ FundingSessionError::NewerFormat { .. }) => {
							tracing::warn!(%error, "funding stays unavailable");
							ControlFlow::Break(())
						},
						Err(error) => ControlFlow::Continue(error),
					}
				},
				|error| {
					let retry_in = pauses.next_pause();
					tracing::warn!(%error, ?retry_in, "loading funding sessions failed");
					retry_in
				},
			)
			.await;
		}));
	}

	/// End session `intent` with `stage` as though its funds had moved.
	/// Returns whether it was still in flight.
	#[cfg(any(test, feature = "test-host"))]
	pub async fn settle_funding_for_test(
		&self,
		intent: &str,
		stage: crate::host_logic::funding::FundingStage,
	) -> Result<bool, FundingSessionError> {
		let intent = intent.to_string();
		self.funding()
			.commit(current_unix_millis(), move |sessions| {
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
		self.funding().acknowledge(current_unix_millis(), intent).await
	}

	/// Cancel session `intent` at the user's request, and say what that did.
	pub async fn cancel_funding(&self, intent: &str) -> Result<CancelOutcome, FundingSessionError> {
		self.funding().cancel(current_unix_millis(), intent).await
	}

	/// Answer the provider's cancel offer on session `intent`: accept it on
	/// `terms`, the terms the host showed, or decline it with `None`.
	pub async fn confirm_funding_cancel(
		&self,
		intent: &str,
		terms: Option<CancelTerms>,
	) -> Result<CancelConfirmation, FundingSessionError> {
		self.funding().confirm_cancel(current_unix_millis(), intent, terms).await
	}

	/// Hand open session `intent` to the provider the user chose, on the
	/// quote they chose it by. The quote must be one the core offered for
	/// this session, not yet expired and, when the session has an amount,
	/// pricing that amount; the core asked it in the session's direction, so
	/// the provider serves it. Returns whether it was open and not yet
	/// assigned.
	pub async fn select_funding_provider(
		self: &Arc<Self>,
		intent: &str,
		provider_id: &ProductId,
		quote_id: &str,
	) -> Result<bool, FundingSessionError> {
		let registry = self.funding();
		let Some(session) = registry.get(intent) else {
			return Ok(false);
		};
		if !self.funding_providers().is_quote_target(provider_id) {
			return Err(FundingSessionError::InvalidProvider {
				reason: format!("{provider_id} is not a funding provider"),
			});
		}
		let choice = self
			.funding_quotes()
			.offered(intent, provider_id, quote_id, current_unix_millis())
			.ok_or_else(|| FundingSessionError::UnknownQuote { quote_id: quote_id.to_string() })?;
		if let Some(amount) = session.amount {
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
			.select_provider(current_unix_millis(), intent, provider_id, choice)
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
		provider_id: ProductId,
		settlement: Settlement,
	) {
		let spawner = self.spawner.clone();
		spawner(Box::pin(async move {
			let registry = self.funding().clone();
			let provider = ProductContext::new_with_execution(provider_id, ExecutableKind::Worker);
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
						.commit(current_unix_millis(), move |sessions| {
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
		provider_id: ProductId,
		direction: FundingDirection,
		funds: StartedFunds,
	) {
		let registry = self.funding().clone();
		let provider = ProductContext::new_with_execution(provider_id, ExecutableKind::Worker);
		let held = engine_holds(&self, &provider, direction, funds.host_id).await;
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
					.commit(now_ms, |sessions| {
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

	/// Open a session and show the host's funding overlay for it, then wait
	/// for the user to start it or close the overlay. The host's own Balance
	/// card takes this path; a product's `request` stops at
	/// [`Self::show_funding`].
	pub async fn open_funding(
		self: &Arc<Self>,
		product: Option<&ProductContext>,
		intent: String,
		direction: FundingDirection,
		amount: Option<u128>,
	) -> Result<FundingSession, OpenFundingError> {
		let finished = self.show_funding(product, intent, direction, amount).await?;
		finished.await.unwrap_or(Err(OpenFundingError::Stopped))
	}

	/// Open a session and show the host's funding overlay for it. Returns once
	/// the session is held, so it can be watched at once, with the overlay's
	/// outcome to wait for.
	///
	/// A session the user never starts, or that the host fails to show, is
	/// discarded rather than left open. `intent` must be new:
	/// [`OpenFundingError::AlreadyExists`] otherwise.
	pub async fn show_funding(
		self: &Arc<Self>,
		product: Option<&ProductContext>,
		intent: String,
		direction: FundingDirection,
		amount: Option<u128>,
	) -> Result<oneshot::Receiver<Result<FundingSession, OpenFundingError>>, OpenFundingError> {
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
			.present(now_ms, session.clone())
			.await
			.map_err(OpenFundingError::Session)?;
		if !presented {
			return Err(OpenFundingError::AlreadyExists);
		}
		// The overlay runs on its own task, so nobody has to wait on it: the
		// task starts or discards the session, and watchers hear the outcome.
		let (finished, outcome) = oneshot::channel();
		let services = self.clone();
		let product = product.cloned();
		(self.spawner)(Box::pin(async move {
			let result = services.finish_presenting(platform, product, session).await;
			if let Err(error @ (OpenFundingError::Present(_) | OpenFundingError::Session(_))) =
				&result
			{
				tracing::warn!(%error, "the funding overlay failed");
			}
			// A caller that does not wait for the result has nothing to hear.
			let _ = finished.send(result);
		}));
		Ok(outcome)
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
		let presentation = FundingPresentation {
			intent: session.intent.clone(),
			direction: session.direction,
			amount: session.amount,
		};
		let presented = platform.present_funding(product.as_ref(), presentation).await;
		match presented {
			Ok(FundingPresentOutcome::Started) => {
				registry
					.start(current_unix_millis(), &session.intent)
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
					.start(now_ms, &session.intent)
					.await
					.map_err(OpenFundingError::Session)?;
				if registry.get(&session.intent).is_some_and(|kept| !kept.money_moved()) {
					registry
						.cancel(now_ms, &session.intent)
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
mod tests;

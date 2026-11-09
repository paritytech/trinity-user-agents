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

mod book;
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
	use super::PackedSession;
	use crate::host_logic::funding::{FundingSession, FundingSessionError};

	pub(super) enum Db {}

	pub(super) async fn load(
		db: &Db,
	) -> Result<(Vec<FundingSession>, Vec<PackedSession>), FundingSessionError> {
		match *db {}
	}

	pub(super) async fn write(
		db: &Db,
		_sessions: &[&FundingSession],
		_removed: Vec<String>,
		_summaries: &[PackedSession],
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

use core::{hash::Hash, ops::ControlFlow, time::Duration};

use futures::{
	channel::{mpsc, oneshot},
	future::{BoxFuture, Either},
	lock::Mutex as AsyncMutex,
	stream::{self, BoxStream, StreamExt},
};
use parity_scale_codec::{Decode, Encode};
use truapi::latest::{
	FundingAssignment, FundingCancelAnswer, FundingCancelDecision, FundingDirection,
	FundingFailure, FundingQuoteAnswer, FundingQuoteAsk, FundingUpdate, GenericError,
	HostFundingReportError, HostFundingStatusSubscribeItem, HostPaymentStatusSubscribeError,
	HostPaymentStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError,
	HostPaymentTopUpStatusSubscribeItem,
};

pub use crate::host_logic::funding::FundingBookEntry;
pub use book::FundingSessionBook;

use super::{funding_quotes::WorkerHold, services::RuntimeServices};
use crate::{
	host_logic::{
		funding::{
			CancelCall, CancelConfirmation, CancelOutcome, CancelTerms, FundingChoice,
			FundingSession, FundingSessionError, FundingSessionSummary, Settlement, StartRecord,
			StartedFunds, UNACKNOWLEDGED_LIMIT, retained,
		},
		funding_providers::FundingSelection,
	},
	platform::{
		ExecutableKind, FundingPlatform, FundingPresentOutcome, FundingPresentation,
		ProductContext, ProductId,
	},
	runtime::payment_id::host_payment_id,
	time::{Backoff, retry},
	unix_time::current_unix_millis,
	wake::WakeSignal,
};

/// Wait before retrying an expiry sweep whose write failed.
const SWEEP_RETRY_MS: u64 = 30_000;
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
const RESOLVE_RETRY_MS: u64 = 30_000;
/// How long a provider's worker may take to answer one call before it counts
/// as unanswered.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Wait before handing a worker its sessions again after a call went
/// unanswered.
const CATCH_UP_RETRY_MS: u64 = 5_000;

/// An ended session in the session pack, with whether the host has told the
/// user how it ended. One it has not is handed to it again when funding
/// resumes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct PackedSession {
	/// The session as history keeps it.
	summary: FundingSessionSummary,
	/// Whether the host told the user how the session ended.
	acknowledged: bool,
}

impl From<&FundingSession> for PackedSession {
	fn from(session: &FundingSession) -> Self {
		Self { summary: session.summary(), acknowledged: session.acknowledged }
	}
}

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

/// Every provider's worker connection, and who waits for one to connect.
#[derive(Default)]
struct Links {
	/// Each provider's worker connection, by product id.
	by_provider: HashMap<ProductId, Link>,
	/// Callers waiting for a provider's worker to connect, by product id.
	waiters: HashMap<ProductId, WakeSignal>,
}

impl Links {
	/// Link `generation` of `provider_id`, unless a later link replaced it.
	fn current(&mut self, provider_id: &ProductId, generation: u64) -> Option<&mut Link> {
		self.by_provider
			.get_mut(provider_id)
			.filter(|held| held.generation == generation)
	}
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

/// How to reach the reconciler task, and when it asked to run again.
#[derive(Default)]
struct Reconciler {
	/// Wakes the task, once one runs.
	wake: Option<mpsc::UnboundedSender<()>>,
	/// The earliest time a background step asked to be tried again, in Unix
	/// milliseconds.
	retry_at: Option<u64>,
}

/// Session changes the host has not been told of yet, in commit order.
#[derive(Default)]
struct Announcements {
	/// Each change with the record the session had when it was queued, or
	/// `None` for one compacted before then.
	pending: VecDeque<(String, HostFundingStatusSubscribeItem, Option<Box<FundingSession>>)>,
	/// Whether a caller is delivering them.
	delivering: bool,
	/// The [`FundingRegistry::is_in_progress`] the host was last told.
	in_progress: Option<bool>,
}

/// One change the host is told of.
enum Announcement {
	Session(String, HostFundingStatusSubscribeItem, Option<Box<FundingSession>>),
	InProgress(bool),
}

/// What a host draws its funding screens from, pushed by the core so the
/// host never reads it back. Native hosts get it through their funding
/// callbacks; a host without one gets only
/// [`FundingPlatform::funding_session_changed`].
pub trait FundingFeed: Send + Sync {
	/// A session changed: its status, the session as the core now holds it
	/// and the progress the host draws. Every session the host has to hear
	/// of comes through here once more when funding resumes. `session` and
	/// `progress` are `None` once the core keeps only the session's summary.
	fn funding_session_changed(
		&self,
		intent: String,
		status: HostFundingStatusSubscribeItem,
		session: Option<FundingSession>,
		progress: Option<crate::host_logic::funding::FundingProgress>,
	);

	/// Whether a provider still works on any session: once funding resumes,
	/// and again whenever it changes. A host that remembers it starts
	/// funding at its next launch, so the providers carry on from their last
	/// checkpoint without waiting for a product.
	fn funding_in_progress_changed(&self, in_progress: bool);

	/// What session `intent`'s overlay offers: before the overlay shows,
	/// after each selection the host reports while it is up, and after each
	/// quote list, which can teach the core new limits.
	fn funding_options_changed(
		&self,
		intent: String,
		options: crate::host_logic::funding_providers::FundingOptions,
	);
}

/// What commits change, behind one lock: a commit replaces it and tells the
/// watchers under that lock, so a reader never sees one part of a commit
/// without the rest.
#[derive(Default)]
struct Held {
	/// Sessions held in full, by intent.
	sessions: HashMap<String, FundingSession>,
	/// Ended sessions compacted out of `sessions`, by intent. Should a
	/// session sit in both, the full record wins.
	pack: HashMap<String, PackedSession>,
	/// Sessions whose overlay is still up, with what the user picked in it so
	/// far: live in memory, so the overlay can quote and pick a provider for
	/// them, but not stored until the user starts them. Closing the overlay
	/// drops its selection with it.
	presenting: HashMap<String, Option<FundingSelection>>,
	subscribers: Subscribers,
	/// Sessions past the moment they count as taking longer whose host has
	/// heard so, as nothing else changes in them then.
	told_longer: HashSet<String>,
}

/// What a commit's edit works on: copies of the sessions and their
/// overlays, which become current once what changed is stored, and the pack
/// as it stands, with the rows the edit rewrites kept apart. A commit whose
/// write fails drops it, so none of it reaches memory.
pub struct Draft<'a> {
	/// Sessions held in full, by intent.
	pub sessions: HashMap<String, FundingSession>,
	/// Sessions whose overlay is up, with what the user picked in each.
	presenting: HashMap<String, Option<FundingSelection>>,
	pack: &'a HashMap<String, PackedSession>,
	/// Rows of the pack this edit rewrites, by intent.
	repacked: HashMap<String, PackedSession>,
}

impl Draft<'_> {
	/// Compacted session `intent`, as this edit leaves it.
	fn packed(&self, intent: &str) -> Option<&PackedSession> {
		self.repacked.get(intent).or_else(|| self.pack.get(intent))
	}
}

/// What a commit makes current once it is stored.
struct Update {
	/// The sessions still held in full.
	kept: HashMap<String, FundingSession>,
	/// Sessions compacted as they ended.
	ended: Vec<FundingSession>,
	presenting: HashMap<String, Option<FundingSelection>>,
	/// Kept sessions whose row is written.
	put: Vec<String>,
	/// Sessions whose row is deleted.
	removed: Vec<String>,
	/// Pack rows written: the sessions compacted, then those rewritten.
	packed: Vec<PackedSession>,
}

impl Update {
	/// Whether anything is written.
	fn writes(&self) -> bool {
		!self.put.is_empty() || !self.removed.is_empty() || !self.packed.is_empty()
	}
}

impl Held {
	/// What making `sessions`, `presenting` and `repacked` current changes,
	/// compacting what ended by `now_ms`.
	fn update(
		&self,
		sessions: HashMap<String, FundingSession>,
		presenting: HashMap<String, Option<FundingSelection>>,
		repacked: HashMap<String, PackedSession>,
		now_ms: u64,
	) -> Update {
		let (kept, ended) = retained(sessions.into_values(), now_ms);
		let kept: HashMap<String, FundingSession> =
			kept.into_iter().map(|session| (session.intent.clone(), session)).collect();
		// An assigned session is stored even while its overlay is up, as its
		// provider's reports are stored before they are answered. A session
		// only shown in the overlay is not stored, so holding one changes
		// nothing worth writing.
		let stored = |presenting: &HashMap<String, _>, session: &FundingSession| {
			!presenting.contains_key(&session.intent) || session.provider_id.is_some()
		};
		let put = kept
			.values()
			.filter(|session| {
				stored(&presenting, session) &&
					self.sessions
						.get(&session.intent)
						.is_none_or(|held| held != *session || !stored(&self.presenting, held))
			})
			.map(|session| session.intent.clone())
			.collect();
		let removed = self
			.sessions
			.values()
			.filter(|held| {
				stored(&self.presenting, held) &&
					!kept.get(&held.intent).is_some_and(|session| stored(&presenting, session))
			})
			.map(|held| held.intent.clone())
			.collect();
		let packed = ended.iter().map(PackedSession::from).chain(repacked.into_values()).collect();
		Update { kept, ended, presenting, put, removed, packed }
	}

	/// Make stored `update` current. Returns the sessions it compacted as
	/// they ended, by intent.
	fn apply(&mut self, update: Update) -> HashMap<String, FundingSession> {
		let Update { kept, ended, presenting, packed, .. } = update;
		self.pack
			.extend(packed.into_iter().map(|packed| (packed.summary.intent.clone(), packed)));
		self.sessions = kept;
		// A selection the host reported while the write was in flight stands.
		self.presenting = presenting
			.into_iter()
			.map(|(intent, selection)| {
				let current = self.presenting.remove(&intent).unwrap_or(selection);
				(intent, current)
			})
			.collect();
		ended.into_iter().map(|session| (session.intent.clone(), session)).collect()
	}
}

/// Host-global funding sessions, their subscribers and the host surface.
#[derive(Default)]
pub struct FundingRegistry {
	/// The sessions, their overlays and their watchers.
	held: Mutex<Held>,
	announcements: Mutex<Announcements>,
	/// Held across every load and write, so persisted snapshots land in
	/// order.
	writes: AsyncMutex<()>,
	/// Whether persisted sessions have been loaded. Set under
	/// [`Self::writes`], and readable without it.
	loaded: AtomicBool,
	platform: OnceLock<Arc<dyn FundingPlatform>>,
	feed: OnceLock<Arc<dyn FundingFeed>>,
	/// The core database the sessions are stored in.
	db: OnceLock<store::Db>,
	/// Each provider's worker connection, and who waits for one.
	links: Mutex<Links>,
	/// The generation the next link takes.
	next_generation: AtomicU64,
	/// Each session's hold on its provider's worker, kept in line with the
	/// sessions by [`Self::hold_workers`].
	holding: Mutex<HashMap<String, WorkerHold>>,
	/// Sessions whose top-ups or payment are being followed to an outcome.
	following: Mutex<HashSet<String>>,
	/// Ids of funds whose host call is in flight, so no second call under
	/// one id runs meanwhile and the reconciler leaves their record alone.
	starting: Mutex<HashSet<[u8; 32]>>,
	/// Recorded funds the host has not taken, by id, with when the reconciler
	/// may next ask the engine about them: `u64::MAX` while it is asking.
	resolving: Mutex<HashMap<[u8; 32], u64>>,
	/// The reconciler task's handle and when it is next due.
	reconciler: Mutex<Reconciler>,
	/// The services the registry belongs to, for worker references and
	/// following top-ups and payments. Empty in a registry built on its own.
	services: Weak<RuntimeServices>,
	/// The provider frames open, by session, with the provider showing each.
	frames: Mutex<HashMap<String, ProductContext>>,
	/// The funding state the host reads back, filled as the host is told
	/// what changed.
	book: FundingSessionBook,
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

	/// Install the feed the host draws its funding screens from, before the
	/// funding surface, whose install resumes funding. Set-once; returns
	/// whether this call installed it.
	pub fn install_feed(&self, feed: Arc<dyn FundingFeed>) -> bool {
		self.feed.set(feed).is_ok()
	}

	/// The host's funding feed, when one is installed.
	pub fn feed(&self) -> Option<Arc<dyn FundingFeed>> {
		self.feed.get().cloned()
	}

	/// The funding state the host reads back.
	pub fn book(&self) -> &FundingSessionBook {
		&self.book
	}

	/// Every session the host has heard of that the registry holds in full,
	/// with its progress at `now_ms`, newest opened first.
	pub fn entries(&self, now_ms: u64) -> Vec<FundingBookEntry> {
		let mut entries: Vec<FundingBookEntry> = self
			.announced()
			.into_iter()
			.map(|session| FundingBookEntry { progress: session.progress(now_ms), session })
			.collect();
		entries.sort_by_key(|entry| core::cmp::Reverse(entry.session.opened_at_ms));
		entries
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

	/// The services the registry belongs to, while they are alive.
	pub fn services(&self) -> Option<Arc<RuntimeServices>> {
		self.services.upgrade()
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
			.commit(now_ms, move |draft| {
				let assigned = draft.sessions.get_mut(&intent).and_then(|session| {
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
			.by_provider
			.insert(provider_id.clone(), Link { link, generation, catch_up: CatchUp::Connecting });
	}

	/// Drop `link`, unless a newer one replaced it.
	pub fn unlink(&self, provider_id: &ProductId, link: &Arc<dyn FundingProviderLink>) {
		let mut links = self.lock_links();
		if links
			.by_provider
			.get(provider_id)
			.is_some_and(|held| Arc::ptr_eq(&held.link, link))
		{
			links.by_provider.remove(provider_id);
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
				.by_provider
				.get_mut(provider_id)
				.filter(|held| held.catch_up == CatchUp::Connecting)
				.map(|held| held.catch_up = CatchUp::Due { not_before_ms: 0 })
				.is_some();
			if let Some(waiters) = links.waiters.remove(provider_id) {
				waiters.wake();
			}
			readied
		};
		if readied {
			self.wake();
		}
	}

	/// The link to `provider_id`'s worker once that worker has connected,
	/// waiting for it when it has not, and past `besides`, a link a call just
	/// failed on, for the connection that follows it. A caller holds the
	/// worker meanwhile, so the host runs it, and bounds the wait itself.
	pub async fn connected_link_besides(
		&self,
		provider_id: &ProductId,
		besides: Option<&Arc<dyn FundingProviderLink>>,
	) -> Arc<dyn FundingProviderLink> {
		loop {
			let mut connected = {
				let mut links = self.lock_links();
				if let Some(held) = links.by_provider.get(provider_id).filter(|held| {
					held.catch_up != CatchUp::Connecting &&
						!besides.is_some_and(|failed| Arc::ptr_eq(&held.link, failed))
				}) {
					return held.link.clone();
				}
				links.waiters.entry(provider_id.clone()).or_default().subscribe()
			};
			connected.next().await;
		}
	}

	/// What `provider_id`'s worker has to hear when it connects: each session
	/// it serves, oldest first, followed by its pending cancel call.
	fn pending_calls(&self, provider_id: &ProductId) -> Vec<ProviderCall> {
		let held = self.lock_held();
		let mut served: Vec<&FundingSession> = held
			.sessions
			.values()
			.filter(|session| {
				session.serves_provider() && session.provider_id.as_ref() == Some(provider_id)
			})
			.collect();
		served.sort_by_key(|session| session.opened_at_ms);
		served
			.into_iter()
			.flat_map(|session| {
				core::iter::once(ProviderCall::Assign(Box::new(session.assignment()))).chain(
					session
						.pending_cancel_call()
						.map(|call| ProviderCall::Cancel(session.intent.clone(), call)),
				)
			})
			.collect()
	}

	/// Store `update` from `provider` on session `intent`. A `Crediting`,
	/// `Collecting` or `Refunding` update names funds recorded for that
	/// session alone, and uses them up.
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
		self.commit(now_ms, move |draft| {
			let reported = match draft.sessions.get_mut(&intent) {
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
	pub fn begin_start(self: &Arc<Self>, host_id: [u8; 32]) -> Option<Claim<[u8; 32]>> {
		self.claim(|registry| &registry.starting, host_id)
	}

	/// Take `key` in `set`, or `None` while it is taken.
	fn claim<K: Eq + Hash + Clone>(
		self: &Arc<Self>,
		set: fn(&FundingRegistry) -> &Mutex<HashSet<K>>,
		key: K,
	) -> Option<Claim<K>> {
		lock(set(self))
			.insert(key.clone())
			.then(|| Claim { registry: self.clone(), set, key })
	}

	/// Whether session `intent` recorded funds under `host_id`.
	pub fn recorded_start(&self, intent: &str, host_id: &[u8; 32]) -> bool {
		self.lock_held()
			.sessions
			.get(intent)
			.is_some_and(|session| session.started.iter().any(|record| record.host_id == *host_id))
	}

	/// Whether a session of `provider_id` recorded funds under `host_id`.
	pub fn holds_start(&self, provider_id: &ProductId, host_id: &[u8; 32]) -> bool {
		self.lock_held().sessions.values().any(|session| {
			session.provider_id.as_ref() == Some(provider_id) &&
				session.started.iter().any(|record| record.host_id == *host_id)
		})
	}

	/// Record funds `provider_id` starts for session `intent`, before the
	/// host is asked. The session must be served by `provider_id` and open in
	/// their direction; a withdrawal whose payout failed takes top-ups too,
	/// as its refund, and a top-up the user cancelled before paying takes one
	/// as a payment that arrived anyway, and opens again. `None` when the
	/// session takes no such funds; a `Conflict` when another of its sessions
	/// holds their id.
	pub async fn record_start(
		&self,
		now_ms: u64,
		provider_id: &ProductId,
		intent: &str,
		asked: FundsAsked,
	) -> Result<Option<StartRecord>, FundingSessionError> {
		let FundsAsked { direction, host_id, amount, terms } = asked;
		self.commit(now_ms, |draft| {
			let sessions = &mut draft.sessions;
			let elsewhere = sessions.values().any(|session| {
				session.intent != intent &&
					session.provider_id.as_ref() == Some(provider_id) &&
					session.started.iter().any(|record| record.host_id == host_id)
			});
			let Some(session) = sessions.get_mut(intent).filter(|session| {
				session.provider_id.as_ref() == Some(provider_id) &&
					((!session.is_terminal() && session.direction == direction) ||
						(direction == FundingDirection::In &&
							(session.awaits_refund() ||
								session.takes_late_arrival(now_ms))))
			}) else {
				return (None, Vec::new());
			};
			if elsewhere {
				return (Some(StartRecord::Conflict), Vec::new());
			}
			// A payment for a top-up the user cancelled arrived anyway.
			let reopened = session.takes_late_arrival(now_ms);
			if reopened {
				session.reopen_for_late_arrival();
			}
			let recorded = session.record_start(direction, host_id, amount, terms, now_ms);
			// Recorded funds stop the session expiring, which its watchers see.
			let changed = Vec::from_iter(
				(reopened || recorded == StartRecord::New).then(|| intent.to_string()),
			);
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
		self.commit(now_ms, |draft| {
			let changed = draft.sessions.get_mut(intent).is_some_and(|session| {
				if taken { session.accept_start(&host_id) } else { session.forget_start(&host_id) }
			});
			((), Vec::from_iter(changed.then(|| intent.to_string())))
		})
		.await
	}

	/// Whether session `intent` is assigned to `provider_id` and still has
	/// work for it: open, or released and awaiting its payout outcome.
	pub fn is_serving(&self, provider_id: &ProductId, intent: &str) -> bool {
		self.get(intent).is_some_and(|session| {
			session.serves_provider() && session.provider_id.as_ref() == Some(provider_id)
		})
	}

	/// Snapshot one session.
	pub fn get(&self, intent: &str) -> Option<FundingSession> {
		self.lock_held().sessions.get(intent).cloned()
	}

	/// How session `intent` ended, whether held in full or compacted. `None`
	/// while it is open or when no such session is held.
	#[cfg(test)]
	pub fn ended(&self, intent: &str) -> Option<HostFundingStatusSubscribeItem> {
		let held = self.lock_held();
		match held.sessions.get(intent) {
			Some(session) => session.is_terminal().then(|| session.wire_item()),
			None => held.pack.get(intent).map(|packed| packed.summary.wire_item()),
		}
	}

	/// Every session held in full whose overlay is no longer up: those the
	/// host has heard of.
	pub fn announced(&self) -> Vec<FundingSession> {
		let held = self.lock_held();
		held.sessions
			.values()
			.filter(|session| !held.presenting.contains_key(&session.intent))
			.cloned()
			.collect()
	}

	/// Whether session `intent`'s payment moved, so its provider's frame is
	/// done: it ended, held in full or compacted, or reached a milestone past
	/// Started. `false` for a session the core does not hold.
	pub fn payment_moved(&self, intent: &str) -> bool {
		let held = self.lock_held();
		match held.sessions.get(intent) {
			Some(session) => session.payment_moved(),
			None => held.pack.contains_key(intent),
		}
	}

	/// Track the frame `provider` shows for session `intent` until the
	/// returned guard drops, so the core can close it once the payment moved.
	/// One that already moved is closed as soon as the session next changes.
	pub fn track_frame(&self, intent: &str, provider: ProductContext) -> FrameTrack<'_> {
		lock(&self.frames).insert(intent.to_string(), provider);
		FrameTrack { registry: self, intent: intent.to_string() }
	}

	/// Close session `intent`'s provider frame through `platform` if one is
	/// open and its payment moved, once.
	fn close_moved_frame(&self, platform: &dyn FundingPlatform, intent: &str) {
		if !self.payment_moved(intent) {
			return;
		}
		let Some(provider) = lock(&self.frames).remove(intent) else {
			return;
		};
		platform.close_provider_frame(&provider, intent.to_string());
	}

	/// The sessions whose overlay is up.
	pub fn presenting(&self) -> Vec<String> {
		self.lock_held().presenting.keys().cloned().collect()
	}

	/// Record what the user picked in session `intent`'s overlay. Returns
	/// whether its overlay is up; a closed one keeps nothing.
	pub fn select(&self, intent: &str, selection: FundingSelection) -> bool {
		self.lock_held()
			.presenting
			.get_mut(intent)
			.map(|held| *held = Some(selection))
			.is_some()
	}

	/// What the user picked so far in session `intent`'s overlay.
	pub fn selection(&self, intent: &str) -> Option<FundingSelection> {
		self.lock_held().presenting.get(intent).cloned().flatten()
	}

	/// Whether a provider still works on any session: one in flight, or a
	/// withdrawal whose payout or refund it has not reported.
	pub fn is_in_progress(&self) -> bool {
		self.lock_held().sessions.values().any(FundingSession::has_working_provider)
	}

	/// Every ended session, held in full or compacted into the pack, as
	/// summaries newest first, each once: what the host shows as history. A
	/// cancel that moved nothing is left out.
	pub fn history(&self) -> Vec<FundingSessionSummary> {
		let mut history: Vec<_> = {
			let held = self.lock_held();
			let recorded = held
				.sessions
				.values()
				.filter(|session| session.is_terminal())
				.map(FundingSession::summary);
			let compacted = held
				.pack
				.values()
				.filter(|packed| !held.sessions.contains_key(&packed.summary.intent))
				.map(|packed| packed.summary.clone());
			recorded
				.chain(compacted)
				.filter(|summary| !summary.cancelled_unmoved())
				.collect()
		};
		history.sort_by_key(|summary| core::cmp::Reverse(summary.settled_at_ms()));
		history
	}

	/// Ended session `intent`'s summary, held in full or compacted.
	pub fn summary(&self, intent: &str) -> Option<FundingSessionSummary> {
		let held = self.lock_held();
		match held.sessions.get(intent) {
			Some(session) => session.is_terminal().then(|| session.summary()),
			None => held.pack.get(intent).map(|packed| packed.summary.clone()),
		}
	}

	/// Who opened session `intent`, held in full or in the pack: `None` when
	/// no such session is held, `Some(None)` when the host opened it.
	pub fn owner(&self, intent: &str) -> Option<Option<ProductId>> {
		let held = self.lock_held();
		if let Some(session) = held.sessions.get(intent) {
			return Some(session.owner_product_id.clone());
		}
		held.pack.get(intent).map(|packed| packed.summary.owner_product_id.clone())
	}

	/// Record that the host told the user how ended session `intent` went,
	/// so it is no longer handed over when funding resumes. The session may
	/// be held in full, as one its provider still works on, or compacted
	/// already. Returns whether the session was ended and not yet
	/// acknowledged.
	pub async fn acknowledge(
		&self,
		now_ms: u64,
		intent: &str,
	) -> Result<bool, FundingSessionError> {
		self.commit(now_ms, |draft| {
			if let Some(session) = draft
				.sessions
				.get_mut(intent)
				.filter(|session| session.is_terminal() && !session.acknowledged)
			{
				session.acknowledged = true;
				return (true, Vec::new());
			}
			let Some(packed) = draft.packed(intent).filter(|packed| !packed.acknowledged) else {
				return (false, Vec::new());
			};
			let packed = PackedSession { acknowledged: true, ..packed.clone() };
			draft.repacked.insert(intent.to_string(), packed);
			(true, Vec::new())
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
			.commit(now_ms, move |draft| {
				if draft.presenting.contains_key(&target) {
					return ((CancelOutcome::Refused, None), Vec::new());
				}
				let Some(session) = draft.sessions.get_mut(&target) else {
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
			.commit(now_ms, move |draft| {
				let Some(session) = draft.sessions.get_mut(&target) else {
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
		// A commit replaces the sessions and fans out to their subscribers
		// under this lock, so holding it from the read to the registration
		// means every change is heard once.
		let mut held = self.lock_held();
		let Some(session) = held.sessions.get(intent).cloned() else {
			let item = held.pack.get(intent)?.summary.wire_item();
			return Some(stream::once(async move { item }).boxed());
		};
		let terminal = session.is_terminal();
		let current = stream::once(async move { session.wire_item() });
		if terminal {
			return Some(current.boxed());
		}
		let (sender, receiver) = mpsc::unbounded();
		let senders = held.subscribers.entry(intent.to_string()).or_default();
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
			.commit(now_ms, |draft| {
				if draft.sessions.contains_key(&session.intent) ||
					draft.packed(&session.intent).is_some()
				{
					return (false, Vec::new());
				}
				draft.presenting.insert(session.intent.clone(), None);
				draft.sessions.insert(session.intent.clone(), session);
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
			.commit(now_ms, |draft| {
				draft.presenting.remove(intent);
				((), vec![intent.to_string()])
			})
			.await;
		if started.is_err() && !self.discard(intent).await {
			// A provider took it under the overlay, so it is stored and runs
			// as started: announce it like any started session.
			let mut held = self.lock_held();
			if let Some(session) = held.sessions.get(intent).cloned() {
				self.fan_out(&mut held, &session);
			}
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
		let watchers = {
			let mut held = self.lock_held();
			held.presenting.remove(intent);
			if held.sessions.get(intent).is_some_and(|session| session.provider_id.is_some()) {
				// Its provider may be working on it and it is stored, so it
				// runs as started instead of staying hidden under an overlay
				// that is gone.
				return false;
			}
			held.sessions.remove(intent);
			held.subscribers.remove(intent).unwrap_or_default()
		};
		let cancelled =
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled };
		for sender in watchers {
			// A watcher that already went away has nothing to hear.
			let _ = sender.unbounded_send(cancelled.clone());
		}
		true
	}

	/// Apply `edit` to a [`Draft`], persist what changed, then make it current
	/// and notify every session `edit` names. A write that fails drops the
	/// draft, so memory keeps what was stored.
	///
	/// Persisted sessions are loaded first on the first call, and every
	/// commit expires the sessions past their deadline, so a long suspend
	/// cannot leave an overdue session open.
	pub async fn commit<R>(
		&self,
		now_ms: u64,
		edit: impl FnOnce(&mut Draft<'_>) -> (R, Vec<String>),
	) -> Result<R, FundingSessionError> {
		let writes = self.writes.lock().await;
		let db = self.db()?;
		if !self.loaded.load(Ordering::Acquire) {
			// Held as stored, so a commit writes only the rows it changes.
			let (sessions, summaries) = store::load(db).await?;
			let mut held = self.lock_held();
			held.sessions
				.extend(sessions.into_iter().map(|session| (session.intent.clone(), session)));
			held.pack.extend(
				summaries.into_iter().map(|packed| (packed.summary.intent.clone(), packed)),
			);
			self.loaded.store(true, Ordering::Release);
		}
		let mut lapsed = Vec::new();
		// The write lock keeps every commit out until this one is done, so
		// what the draft was copied from is still current when it lands.
		let (result, mut changed, mut update) = {
			let held = self.lock_held();
			let mut draft = Draft {
				sessions: held.sessions.clone(),
				presenting: held.presenting.clone(),
				pack: &held.pack,
				repacked: HashMap::new(),
			};
			let (result, edited) = edit(&mut draft);
			// A session the host is told about must be stored too, which
			// covers one leaving the overlay: started, though nothing in it
			// changed.
			let announces = !edited.is_empty();
			let mut changed = edited;
			for session in draft.sessions.values_mut() {
				if session.expire_if_due(now_ms) {
					changed.push(session.intent.clone());
				}
				// Nothing to tell the user about a cancel that moved nothing.
				if session.is_terminal() &&
					!session.acknowledged &&
					session.summary().cancelled_unmoved()
				{
					session.acknowledged = true;
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
			let Draft { sessions, presenting, repacked, .. } = draft;
			let edits = announces ||
				!repacked.is_empty() ||
				sessions != held.sessions ||
				presenting.len() != held.presenting.len() ||
				presenting.keys().any(|intent| !held.presenting.contains_key(intent));
			let update = edits.then(|| held.update(sessions, presenting, repacked, now_ms));
			(result, changed, update)
		};
		if let Some(update) = update.as_mut().filter(|update| update.writes()) {
			let put: Vec<&FundingSession> =
				update.put.iter().filter_map(|intent| update.kept.get(intent)).collect();
			store::write(db, &put, core::mem::take(&mut update.removed), &update.packed).await?;
		}
		{
			// Replaced and fanned out under one lock, so a subscriber either
			// reads the sessions before this commit and hears it, or reads
			// them after and does not.
			let mut held = self.lock_held();
			let mut ending = update.map(|update| held.apply(update)).unwrap_or_default();
			// Each session once, even when an edit and its expiry both name it.
			let mut announced = HashSet::new();
			changed.retain(|intent| announced.insert(intent.clone()));
			for intent in changed {
				// A session this commit compacted as it ended goes out in full
				// once more.
				let session =
					held.sessions.get(&intent).cloned().or_else(|| ending.remove(&intent));
				if let Some(session) = session {
					self.fan_out(&mut held, &session);
				} else if let Some(packed) = held.pack.get(&intent) {
					self.tell_host_packed(packed);
				}
			}
			for session in now_taking_longer(&mut held, now_ms) {
				if !announced.contains(&session.intent) {
					self.tell_host(&held, &session);
				}
			}
		}
		drop(writes);
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
		if reconciler.wake.as_ref().is_some_and(|wake| wake.unbounded_send(()).is_ok()) {
			return;
		}
		let (wake, woken) = mpsc::unbounded();
		reconciler.wake = Some(wake);
		drop(reconciler);
		(services.spawner)(Box::pin(reconcile(Arc::downgrade(&services), woken)));
	}

	/// Ask the reconciler to run again by `at_ms`, in Unix milliseconds.
	fn retry_by(&self, at_ms: u64) {
		{
			let mut reconciler = self.lock_reconciler();
			reconciler.retry_at = Some(reconciler.retry_at.map_or(at_ms, |held| held.min(at_ms)));
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
					return Some(SWEEP_RETRY_MS);
				},
			}
		}
		self.reconcile_work(services, now_ms);
		self.next_wake(now_ms)
	}

	fn reconcile_work(&self, services: &Arc<RuntimeServices>, now_ms: u64) {
		self.hold_workers(services);
		self.follow_settlements(services);
		self.resolve_starts(services, now_ms);
		self.catch_up_links(services, now_ms);
	}

	/// Every assigned open session holds its provider's worker. The
	/// reconciler keeps the holds current, and an overlay closing brings them
	/// in line before it lets its own go. Holds are taken outside every lock
	/// and kept only if the session still wants them once the lock is back,
	/// so a pass that read the sessions before a newer one released a hold
	/// cannot take it again. A worker that starts and calls back changes the
	/// sessions, which the next pass picks up.
	fn hold_workers(&self, services: &Arc<RuntimeServices>) {
		let (released, missing) = {
			let held = self.lock_held();
			let sessions = &held.sessions;
			let mut holding = self.lock_holding();
			let stale: Vec<String> = holding
				.iter()
				.filter(|(intent, hold)| held_for(sessions, intent) != Some(hold.provider()))
				.map(|(intent, _)| intent.clone())
				.collect();
			let released: Vec<WorkerHold> =
				stale.iter().filter_map(|intent| holding.remove(intent)).collect();
			let missing: Vec<(String, ProductId)> = sessions
				.keys()
				.filter(|intent| !holding.contains_key(*intent))
				.filter_map(|intent| Some((intent.clone(), held_for(sessions, intent)?.clone())))
				.collect();
			(released, missing)
		};
		let taken: Vec<(String, WorkerHold)> = missing
			.into_iter()
			.map(|(intent, provider)| (intent, WorkerHold::acquire(services, &provider)))
			.collect();
		let unwanted: Vec<WorkerHold> = {
			let held = self.lock_held();
			let sessions = &held.sessions;
			let mut holding = self.lock_holding();
			taken
				.into_iter()
				.filter_map(|(intent, hold)| {
					if holding.contains_key(&intent) ||
						held_for(sessions, &intent) != Some(hold.provider())
					{
						return Some(hold);
					}
					holding.insert(intent, hold);
					None
				})
				.collect()
		};
		// Taken before let go, so a provider moving from an ended session to
		// a new one keeps its worker running.
		drop(unwanted);
		drop(released);
	}

	/// Follow every session whose outcome is due, once the engine its funds
	/// went through is installed.
	fn follow_settlements(&self, services: &Arc<RuntimeServices>) {
		let due: Vec<(String, ProductId, Settlement)> = self
			.lock_held()
			.sessions
			.values()
			.filter_map(|session| {
				let settlement = session.settlement_due()?;
				Some((session.intent.clone(), session.provider_id.clone()?, settlement))
			})
			.collect();
		for (intent, provider, settlement) in due {
			let engine = match settlement {
				Settlement::TopUps(_) | Settlement::Refund(_) => {
					services.top_up_platform().is_some()
				},
				Settlement::Payment(_) => services.payment_platform().is_some(),
			};
			if engine &&
				let Some(following) =
					services.funding().claim(|registry| &registry.following, intent)
			{
				services.clone().follow_settlement(following, provider, settlement);
			}
		}
	}

	/// Ask the engine about recorded funds the host has not taken and no
	/// call is starting: taken once it holds them, dropped once it still does
	/// not [`UNKNOWN_FUNDS_GRACE_MS`] after they were recorded.
	fn resolve_starts(&self, services: &Arc<RuntimeServices>, now_ms: u64) {
		let pending: Vec<(String, ProductId, StartedFunds)> = self
			.lock_held()
			.sessions
			.values()
			.filter(|session| session.serves_provider())
			.flat_map(|session| {
				session.pending_starts().filter_map(|funds| {
					Some((session.intent.clone(), session.provider_id.clone()?, funds.clone()))
				})
			})
			.collect();
		let mut resolving = self.lock_resolving();
		resolving.retain(|host_id, _| pending.iter().any(|(.., funds)| funds.host_id == *host_id));
		let starting = self.lock_starting();
		for (intent, provider, funds) in pending {
			let engine = match funds.direction {
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
			let asking = Resolving {
				registry: services.funding().clone(),
				host_id: funds.host_id,
				next_ms: None,
			};
			(services.spawner)(Box::pin(
				services.clone().resolve_start(asking, intent, provider, funds),
			));
		}
	}

	/// Hand each ready worker every session it serves, one call at a time.
	fn catch_up_links(&self, services: &Arc<RuntimeServices>, now_ms: u64) {
		let due: Vec<(ProductId, u64, Arc<dyn FundingProviderLink>)> = self
			.lock_links()
			.by_provider
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
			let catching = CatchingUp {
				registry: services.funding().clone(),
				provider,
				generation,
				answered: false,
			};
			(services.spawner)(Box::pin(async move {
				let mut catching = catching;
				let registry = catching.registry.clone();
				let mut answered = true;
				for call in registry.pending_calls(&catching.provider) {
					answered &= registry.put(link.as_ref(), call).await;
				}
				catching.answered = answered;
			}));
		}
	}

	/// Record how handing link `generation` of `provider_id` its sessions
	/// went: done when every call was answered, due again shortly otherwise.
	/// A link replaced meanwhile is left as it is.
	fn caught_up(&self, provider_id: &ProductId, generation: u64, answered: bool) {
		let retry_ms = current_unix_millis().saturating_add(CATCH_UP_RETRY_MS);
		let due = {
			let mut links = self.lock_links();
			let held = links.current(provider_id, generation);
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
		let retry_ms = current_unix_millis().saturating_add(CATCH_UP_RETRY_MS);
		let mut links = self.lock_links();
		let Some(held) = links.current(provider_id, generation) else {
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
			let mut reconciler = self.lock_reconciler();
			let held = reconciler.retry_at;
			if held.is_some_and(|at_ms| at_ms <= now_ms) {
				reconciler.retry_at = None;
			}
			held
		};
		let link = self
			.lock_links()
			.by_provider
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
			.by_provider
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
					.commit(current_unix_millis(), move |draft| {
						let now_ms = current_unix_millis();
						let changed = draft
							.sessions
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
					.commit(current_unix_millis(), move |draft| {
						if let Some(session) = draft.sessions.get_mut(&target) {
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
		let held = self.lock_held();
		held.sessions
			.values()
			.flat_map(|session| {
				let longer =
					session.longer_at_ms().filter(|_| !held.told_longer.contains(&session.intent));
				[session.next_alarm_ms(), longer]
			})
			.flatten()
			.min()
	}

	/// Tell subscribers about a session's current stage and queue it for the
	/// host. A terminal stage ends the subscriber streams.
	fn fan_out(&self, held: &mut Held, session: &FundingSession) {
		let item = session.wire_item();
		if let Some(senders) = held.subscribers.get_mut(&session.intent) {
			senders.retain(|sender| sender.unbounded_send(item.clone()).is_ok());
		}
		if session.is_terminal() {
			held.subscribers.remove(&session.intent);
		}
		self.tell_host(held, session);
	}

	/// Queue the ending of compacted session `packed` for the host again, as
	/// it has not told the user how it went.
	fn tell_host_packed(&self, packed: &PackedSession) {
		if self.platform.get().is_some() {
			self.lock_announcements().pending.push_back((
				packed.summary.intent.clone(),
				packed.summary.wire_item(),
				None,
			));
		}
	}

	/// Queue `session`'s current stage for the host, which hears of a session
	/// once the user starts it.
	fn tell_host(&self, held: &Held, session: &FundingSession) {
		if self.platform.get().is_some() && !held.presenting.contains_key(&session.intent) {
			self.lock_announcements().pending.push_back((
				session.intent.clone(),
				session.wire_item(),
				Some(Box::new(session.clone())),
			));
		}
	}

	/// Hand queued changes to the host in commit order, outside the write
	/// lock so the host may call back into the registry, then whether a
	/// provider still works on any session, when that changed. Each session
	/// goes with the record and progress it has when delivered, which are at
	/// least as new as its status, or once it is compacted with the record it
	/// was queued with. One caller delivers at a time, and a
	/// change committed from inside the callback is delivered by that caller
	/// once the callback returns.
	fn announce(&self) {
		let Some(platform) = self.platform() else {
			return;
		};
		if core::mem::replace(&mut self.lock_announcements().delivering, true) {
			return;
		}
		loop {
			let in_progress = self.is_in_progress();
			let next = {
				let mut announcements = self.lock_announcements();
				let next = match announcements.pending.pop_front() {
					Some((intent, item, queued)) => {
						Some(Announcement::Session(intent, item, queued))
					},
					None if announcements.in_progress != Some(in_progress) => {
						announcements.in_progress = Some(in_progress);
						Some(Announcement::InProgress(in_progress))
					},
					None => None,
				};
				announcements.delivering = next.is_some();
				next
			};
			let feed = self.feed();
			match next {
				Some(Announcement::Session(intent, item, queued)) => {
					self.close_moved_frame(platform.as_ref(), &intent);
					platform.funding_session_changed(intent.clone(), item.clone());
					// The record the session has now, or the one it was queued
					// with once it is compacted.
					let session = self.get(&intent).or(queued.map(|queued| *queued));
					if session.is_none() {
						self.book.overlay_closed(&intent);
					}
					if let Some(feed) = feed {
						let progress =
							session.as_ref().map(|session| session.progress(current_unix_millis()));
						feed.funding_session_changed(intent, item, session, progress);
					}
				},
				Some(Announcement::InProgress(in_progress)) => {
					if let Some(feed) = feed {
						feed.funding_in_progress_changed(in_progress);
					}
				},
				None => return,
			}
		}
	}

	fn lock_announcements(&self) -> MutexGuard<'_, Announcements> {
		lock(&self.announcements)
	}

	fn lock_reconciler(&self) -> MutexGuard<'_, Reconciler> {
		lock(&self.reconciler)
	}

	fn lock_starting(&self) -> MutexGuard<'_, HashSet<[u8; 32]>> {
		lock(&self.starting)
	}

	fn lock_resolving(&self) -> MutexGuard<'_, HashMap<[u8; 32], u64>> {
		lock(&self.resolving)
	}

	fn lock_held(&self) -> MutexGuard<'_, Held> {
		lock(&self.held)
	}

	fn lock_links(&self) -> MutexGuard<'_, Links> {
		lock(&self.links)
	}

	fn lock_holding(&self) -> MutexGuard<'_, HashMap<String, WorkerHold>> {
		lock(&self.holding)
	}
}

/// A provider frame the registry tracks, until it drops.
pub struct FrameTrack<'a> {
	registry: &'a FundingRegistry,
	intent: String,
}

impl Drop for FrameTrack<'_> {
	fn drop(&mut self) {
		lock(&self.registry.frames).remove(&self.intent);
	}
}

/// Lock `mutex`, recovering its value if a holder panicked: every value the
/// funding modules guard stays valid between statements.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Funds a provider's worker asks the host to move for a session.
#[derive(Clone)]
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

/// A key taken in one of the registry's sets of work in flight: an id whose
/// host call is in flight, from [`FundingRegistry::begin_start`], or a
/// session whose settlement is being followed, from
/// [`FundingRegistry::follow_settlements`]. Dropping it, as the work ends or
/// its task is dropped, frees the key; the reconciler leaves taken work alone,
/// so it looks again then.
pub struct Claim<K: Eq + Hash> {
	registry: Arc<FundingRegistry>,
	set: fn(&FundingRegistry) -> &Mutex<HashSet<K>>,
	key: K,
}

impl<K: Eq + Hash> Drop for Claim<K> {
	fn drop(&mut self) {
		lock((self.set)(&self.registry)).remove(&self.key);
		self.registry.wake();
	}
}

/// Recorded funds the engine is being asked about, from
/// [`FundingRegistry::resolve_starts`]. Dropping it leaves them due again at
/// `next_ms`, or at once when that is `None`, so a task dropped while it asks
/// does not keep them from being placed.
struct Resolving {
	registry: Arc<FundingRegistry>,
	host_id: [u8; 32],
	/// When the engine is asked again, in Unix milliseconds.
	next_ms: Option<u64>,
}

impl Drop for Resolving {
	fn drop(&mut self) {
		match self.next_ms {
			Some(next_ms) => {
				self.registry.lock_resolving().insert(self.host_id, next_ms);
				self.registry.retry_by(next_ms);
			},
			None => {
				self.registry.lock_resolving().remove(&self.host_id);
			},
		}
	}
}

/// A worker being handed its sessions, from
/// [`FundingRegistry::catch_up_links`]. Dropping it records how the hand-over
/// went, so one whose task is dropped is due again.
struct CatchingUp {
	registry: Arc<FundingRegistry>,
	provider: ProductId,
	generation: u64,
	/// Whether every call was answered; `false` until the hand-over ends.
	answered: bool,
}

impl Drop for CatchingUp {
	fn drop(&mut self) {
		self.registry.caught_up(&self.provider, self.generation, self.answered);
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
					reconciler.wake = None;
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
pub(crate) async fn within<T>(limit: Duration, future: impl Future<Output = T>) -> Option<T> {
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
			// The host's session views are fed from what the core has
			// loaded, so a failed load is retried until the sessions are in.
			let mut pauses = RESUME_RETRY_PAUSES;
			retry(
				|| async {
					let loaded = registry
						.commit(current_unix_millis(), |draft| {
							// Ended draft.sessions the host has not acknowledged are
							// handed over again, so the user hears every
							// outcome: those held in full, then the newest
							// compacted ones.
							let mut pending: Vec<String> = draft
								.sessions
								.values()
								.filter(|session| session.needs_handoff())
								.map(|session| session.intent.clone())
								.collect();
							let mut unseen: Vec<PackedSession> = draft
								.pack
								.values()
								.filter(|packed| {
									!packed.acknowledged &&
										!draft.sessions.contains_key(&packed.summary.intent)
								})
								.cloned()
								.collect();
							unseen.sort_by_key(|packed| {
								core::cmp::Reverse(packed.summary.settled_at_ms())
							});
							pending.extend(
								unseen
									.into_iter()
									.take(UNACKNOWLEDGED_LIMIT)
									.map(|packed| packed.summary.intent),
							);
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
			.commit(current_unix_millis(), move |draft| {
				let settled = draft
					.sessions
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
	/// the provider serves it. Once assigned, the quotes still asked of other
	/// providers end. Returns whether it was open and not yet
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
		let assigned = registry
			.select_provider(current_unix_millis(), intent, provider_id, choice)
			.await?;
		if assigned {
			// The others' answers can no longer be used, so the quotes still
			// asked of them stop and let their workers go.
			self.funding_quotes().end(intent);
		}
		Ok(assigned)
	}

	/// Follow a session's top-ups, payment or refund to the end, then settle
	/// it with what moved, from the amounts the provider started them with. A
	/// failed write of the outcome is retried until it is stored or the
	/// session ended some other way; the reconciler follows the session again
	/// if this stops with it still due.
	fn follow_settlement(
		self: Arc<Self>,
		following: Claim<String>,
		provider_id: ProductId,
		settlement: Settlement,
	) {
		let spawner = self.spawner.clone();
		spawner(Box::pin(async move {
			let registry = following.registry.clone();
			let intent = following.key.clone();
			let provider = ProductContext::new_with_execution(provider_id, ExecutableKind::Worker);
			let refund = matches!(settlement, Settlement::Refund(_));
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
				Settlement::Refund(funds) => {
					self.follow_funds(&provider, &intent, FundingDirection::In, &funds).await
				},
			};
			if let Some(moved) = moved {
				let mut pauses = Backoff::new(FOLLOW_RETRY_FIRST, FOLLOW_RETRY_LONGEST);
				loop {
					let settling = intent.clone();
					let settled = registry
						.commit(current_unix_millis(), move |draft| {
							let now_ms = current_unix_millis();
							let settled =
								draft.sessions.get_mut(&settling).is_some_and(|session| {
									if refund {
										session.settle_refund(moved)
									} else {
										session.settle(moved, now_ms)
									}
								});
							((), if settled { vec![settling] } else { Vec::new() })
						})
						.await;
					let Err(error) = settled else {
						break;
					};
					let retry_in = pauses.next_pause();
					tracing::warn!(%error, ?retry_in, "settling a funding session failed");
					futures_timer::Delay::new(retry_in).await;
				}
			}
			drop(following);
		}));
	}

	/// Place funds recorded for session `intent` that the host has not
	/// taken: kept as taken once the engine holds them, dropped once it still
	/// does not [`UNKNOWN_FUNDS_GRACE_MS`] after they were recorded, and asked
	/// about again later otherwise. A call under their id that started
	/// meanwhile settles the record itself.
	async fn resolve_start(
		self: Arc<Self>,
		mut asking: Resolving,
		intent: String,
		provider_id: ProductId,
		funds: StartedFunds,
	) {
		let registry = self.funding().clone();
		let provider = ProductContext::new_with_execution(provider_id, ExecutableKind::Worker);
		let held = engine_holds(&self, &provider, funds.direction, funds.host_id).await;
		let now_ms = current_unix_millis();
		let lapse_ms = funds.recorded_at_ms.saturating_add(UNKNOWN_FUNDS_GRACE_MS);
		let retry_ms = now_ms.saturating_add(RESOLVE_RETRY_MS);
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
					.commit(now_ms, |draft| {
						let starting = registry.lock_starting().contains(&host_id);
						let changed = !starting &&
							draft.sessions.get_mut(&target).is_some_and(|session| {
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
		asking.next_ms = next_ms;
	}

	/// What `funds` moved, read again after a growing wait until the host
	/// reports an outcome. `None` once session `intent` has nothing left to
	/// settle or the host has no engine to ask.
	async fn follow_funds(
		&self,
		provider: &ProductContext,
		intent: &str,
		direction: FundingDirection,
		funds: &StartedFunds,
	) -> Option<u128> {
		let mut pauses = Backoff::new(FOLLOW_RETRY_FIRST, FOLLOW_RETRY_LONGEST);
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
			futures_timer::Delay::new(pauses.next_pause()).await;
			if self
				.funding()
				.get(intent)
				.is_none_or(|session| session.settlement_due().is_none())
			{
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
		self.refresh_funding_providers();
		self.send_funding_options(&session.intent);
		// Every provider's worker starts as the overlay opens, so it is up by
		// the time the first quote is asked of it.
		let warm: Vec<_> = self
			.funding_providers()
			.quote_targets()
			.iter()
			.map(|provider_id| super::funding_quotes::WorkerHold::acquire(&self, provider_id))
			.collect();
		let presented = platform.present_funding(product.as_ref(), presentation).await;
		registry.book().overlay_closed(&session.intent);
		self.funding_quotes().end(&session.intent);
		// A provider handed the session while the overlay was up keeps its
		// worker: its hold for the session is taken before the overlay's go.
		registry.hold_workers(&self);
		drop(warm);
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

/// The sessions in `held` that reached the moment they count as taking
/// longer by `now_ms` since the host last heard of them. A later report
/// moves that moment, so the host hears of it again.
fn now_taking_longer(held: &mut Held, now_ms: u64) -> Vec<FundingSession> {
	let Held { sessions, told_longer, .. } = held;
	told_longer.retain(|intent| {
		sessions
			.get(intent)
			.and_then(FundingSession::longer_at_ms)
			.is_some_and(|at_ms| at_ms <= now_ms)
	});
	sessions
		.values()
		.filter(|session| {
			session.longer_at_ms().is_some_and(|at_ms| at_ms <= now_ms) &&
				told_longer.insert(session.intent.clone())
		})
		.cloned()
		.collect()
}

/// The provider whose worker session `intent` holds: its assigned provider
/// while it still has work for it.
fn held_for<'a>(
	sessions: &'a HashMap<String, FundingSession>,
	intent: &str,
) -> Option<&'a ProductId> {
	sessions
		.get(intent)
		.filter(|session| session.serves_provider())
		.and_then(|session| session.provider_id.as_ref())
}

#[cfg(test)]
mod tests;

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
//! released, the host.

use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::{
		Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak,
		atomic::{AtomicBool, Ordering},
	},
};

use core::time::Duration;

use futures::{
	channel::mpsc,
	lock::Mutex as AsyncMutex,
	stream::{self, BoxStream, StreamExt},
};
use truapi::latest::{
	FundingDirection, FundingQuoteAnswer, FundingQuoteAsk, FundingUpdate, GenericError,
	HostFundingReportError, HostFundingServeSubscribeItem, HostFundingStatusSubscribeItem,
	HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem,
	HostPaymentTopUpStatusSubscribeError, HostPaymentTopUpStatusSubscribeItem,
};

use super::services::RuntimeServices;
use crate::{
	host_logic::funding::{
		CancelOutcome, FundingChoice, FundingSession, FundingSessionError, SaveRefusal, Settlement,
		StartedFunds, load_sessions, named_funds, retained, store_sessions,
	},
	platform::{
		CoreStorage, FundingPlatform, FundingPresentOutcome, FundingPresentation, Platform,
		ProductContext, ProductExecutionKind,
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
/// report named it, before it counts as having moved nothing. A payment the
/// provider requested can wait this long on the user's approval.
const UNKNOWN_FUNDS_GRACE_MS: u64 = 60 * 60 * 1_000;

type Subscribers = HashMap<String, Vec<mpsc::UnboundedSender<HostFundingStatusSubscribeItem>>>;
type Servers = HashMap<String, Vec<mpsc::UnboundedSender<HostFundingServeSubscribeItem>>>;

/// A quote ask sent to a provider and not yet answered.
struct PendingAsk {
	provider_id: String,
	ask: FundingQuoteAsk,
	answer: futures::channel::oneshot::Sender<FundingQuoteAnswer>,
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
	/// Sessions whose overlay is still up: live in memory, so the overlay can
	/// quote and pick a provider for them, but not stored until the user
	/// starts them.
	presenting: Mutex<HashSet<String>>,
	subscribers: Mutex<Subscribers>,
	announcements: Mutex<Announcements>,
	/// Held across every load and write; the flag records whether persisted
	/// sessions have been loaded.
	writes: AsyncMutex<bool>,
	/// Whether a task is waiting on the next deadline.
	sweeping: AtomicBool,
	platform: OnceLock<Arc<dyn FundingPlatform>>,
	/// Each provider's open `serve_subscribe` streams, by product id.
	servers: Mutex<Servers>,
	/// Quote asks waiting for their provider's answer, by ask id.
	asks: Mutex<HashMap<String, PendingAsk>>,
	/// Sessions holding a reference on their provider's worker, with that
	/// provider.
	holding: Mutex<HashMap<String, String>>,
	/// Sessions whose top-ups or payment are being followed to an outcome.
	following: Mutex<HashSet<String>>,
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
		self.serve_item(
			provider_id,
			HostFundingServeSubscribeItem::Assigned { session: Box::new(session) },
		);
		Ok(true)
	}

	/// The sessions assigned to `provider_id` and requests to cancel them:
	/// every one still in flight first, then each later one. An assignment
	/// may arrive more than once.
	pub fn serve(&self, provider_id: &str) -> BoxStream<'static, HostFundingServeSubscribeItem> {
		let mut servers = self.lock_servers();
		let mut assigned: Vec<_> = self
			.lock_sessions()
			.values()
			.filter(|session| {
				session.serves_provider() && session.provider_id.as_deref() == Some(provider_id)
			})
			.cloned()
			.collect();
		assigned.sort_by_key(|session| session.opened_at_ms);
		let asks: Vec<HostFundingServeSubscribeItem> = self
			.lock_asks()
			.iter()
			.filter(|(_, pending)| pending.provider_id == provider_id)
			.map(|(ask_id, pending)| HostFundingServeSubscribeItem::Quote {
				ask_id: ask_id.clone(),
				ask: pending.ask.clone(),
			})
			.collect();
		let replay: Vec<_> = assigned
			.into_iter()
			.flat_map(|session| {
				let cancel = session.cancel_requested.then(|| {
					HostFundingServeSubscribeItem::Cancel { intent: session.intent.clone() }
				});
				core::iter::once(HostFundingServeSubscribeItem::Assigned {
					session: Box::new(session.assignment()),
				})
				.chain(cancel)
			})
			.chain(asks)
			.collect();
		let (sender, receiver) = mpsc::unbounded();
		servers.entry(provider_id.to_string()).or_default().push(sender);
		stream::iter(replay).chain(receiver).boxed()
	}

	/// Ask `provider_id` to price `ask`. The ask reaches the provider's open
	/// streams now and any it opens while it is pending, so a worker that is
	/// still starting receives it; the receiver yields the provider's answer.
	pub fn ask_quote(
		&self,
		provider_id: &str,
		ask: FundingQuoteAsk,
	) -> (String, futures::channel::oneshot::Receiver<FundingQuoteAnswer>) {
		let ask_id = format!("qa_{}", nanoid::nanoid!(10));
		let (answer, answered) = futures::channel::oneshot::channel();
		// Held across both steps so a stream `serve` opens in between neither
		// misses the ask nor receives it twice, from its replay and here.
		let mut servers = self.lock_servers();
		self.lock_asks().insert(
			ask_id.clone(),
			PendingAsk { provider_id: provider_id.to_string(), ask: ask.clone(), answer },
		);
		if let Some(senders) = servers.get_mut(provider_id) {
			let item = HostFundingServeSubscribeItem::Quote { ask_id: ask_id.clone(), ask };
			senders.retain(|sender| sender.unbounded_send(item.clone()).is_ok());
		}
		(ask_id, answered)
	}

	/// Deliver `provider_id`'s answer to ask `ask_id`. Returns whether the
	/// ask was pending and sent to that provider.
	pub fn answer_quote(
		&self,
		provider_id: &str,
		ask_id: &str,
		answer: FundingQuoteAnswer,
	) -> bool {
		let mut asks = self.lock_asks();
		if asks.get(ask_id).is_none_or(|pending| pending.provider_id != provider_id) {
			return false;
		}
		asks.remove(ask_id).is_some_and(|pending| pending.answer.send(answer).is_ok())
	}

	/// Drop ask `ask_id` without an answer, as when it timed out.
	pub fn withdraw_ask(&self, ask_id: &str) {
		self.lock_asks().remove(ask_id);
	}

	/// Store `update` from `provider` on session `intent`. Funds a
	/// `Crediting`, `Collecting` or `Refunding` update names are used up: no
	/// other session of the provider can name them after.
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
		let named = named_funds(&update).map(|(id, _)| host_id(id));
		self.commit(storage, now_ms, move |sessions| {
			let reported = match sessions.get_mut(&intent) {
				Some(session) => session.report(provider_id, update, host_id, now_ms),
				None => Err(HostFundingReportError::NotFound),
			};
			if reported.is_err() {
				return (reported, Vec::new());
			}
			if let Some(named) = named {
				for other in sessions.values_mut().filter(|other| {
					other.intent != intent && other.provider_id.as_deref() == Some(provider_id)
				}) {
					other.forget_start(&named);
				}
			}
			(reported, vec![intent])
		})
		.await
	}

	/// Record funds `provider_id` started under `host_id`, so a report can
	/// name them: on the one session it serves in `direction` whose amount
	/// is `amount`, when exactly one is, and otherwise on every such
	/// session, since a top-up does not say which session it is for. A
	/// withdrawal whose payout failed takes top-ups too, as its refund, and
	/// asks for what it debited. Returns
	/// whether any session recorded them.
	pub async fn record_start(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		provider_id: &str,
		direction: FundingDirection,
		host_id: [u8; 32],
		amount: u128,
	) -> Result<bool, FundingSessionError> {
		self.commit(storage, now_ms, |sessions| {
			let served = |session: &FundingSession| {
				session.provider_id.as_deref() == Some(provider_id) &&
					((!session.is_terminal() && session.direction == direction) ||
						(direction == FundingDirection::In && session.awaits_refund()))
			};
			let asks = |session: &FundingSession| match session.stage {
				crate::host_logic::funding::FundingStage::Released { debited, .. } => debited,
				_ => session.amount.unwrap_or_default(),
			};
			let matching: Vec<String> = sessions
				.values()
				.filter(|session| served(session) && asks(session) == amount)
				.map(|session| session.intent.clone())
				.collect();
			let mut recorded = false;
			for session in sessions.values_mut().filter(|session| {
				served(session) && (matching.len() != 1 || matching.contains(&session.intent))
			}) {
				recorded |= session.record_start(host_id, amount);
			}
			(recorded, Vec::new())
		})
		.await
	}

	/// Drop what [`Self::record_start`] recorded for funds the host refused.
	pub async fn forget_start(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		provider_id: &str,
		host_id: [u8; 32],
	) -> Result<(), FundingSessionError> {
		self.commit(storage, now_ms, |sessions| {
			for session in sessions
				.values_mut()
				.filter(|session| session.provider_id.as_deref() == Some(provider_id))
			{
				session.forget_start(&host_id);
			}
			((), Vec::new())
		})
		.await
	}

	/// Keep `state` as `provider_id`'s own state for session `intent`.
	pub async fn save(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		provider_id: &str,
		intent: &str,
		state: Vec<u8>,
	) -> Result<Result<(), SaveRefusal>, FundingSessionError> {
		let intent = intent.to_string();
		let provider = provider_id.to_string();
		self.commit(storage, now_ms, move |sessions| {
			let saved = match sessions.get_mut(&intent) {
				Some(session) => session.save(&provider, state),
				None => Err(SaveRefusal::NotFound),
			};
			(saved, Vec::new())
		})
		.await
	}

	/// Whether session `intent` is assigned to `provider_id` and still has
	/// work for it: open, or released and awaiting its payout outcome.
	pub fn is_serving(&self, provider_id: &str, intent: &str) -> bool {
		self.get(intent).is_some_and(|session| {
			session.serves_provider() && session.provider_id.as_deref() == Some(provider_id)
		})
	}

	/// Snapshot one session.
	pub fn get(&self, intent: &str) -> Option<FundingSession> {
		self.lock_sessions().get(intent).cloned()
	}

	/// Whether a provider still works on any session: one in flight, or a
	/// withdrawal whose payout or refund it has not reported.
	pub fn in_progress(&self) -> bool {
		self.lock_sessions()
			.values()
			.any(|session| session.provider_id.is_some() && session.serves_provider())
	}

	/// Every session the user started that the core keeps, for the host's
	/// progress and history views: those in flight first, then the ended
	/// ones, each newest first.
	pub fn sessions(&self) -> Vec<FundingSession> {
		let mut sessions: Vec<_> = {
			let presenting = self.lock_presenting();
			self.lock_sessions()
				.values()
				.filter(|session| !presenting.contains(&session.intent))
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

	/// Record that the host has written ended session `intent` into its own
	/// history, so it is no longer handed over and can age out. Returns
	/// whether the session was ended and not yet acknowledged.
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
	/// serves it, otherwise ask the provider to stop.
	pub async fn cancel(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		intent: &str,
	) -> Result<CancelOutcome, FundingSessionError> {
		let target = intent.to_string();
		let (outcome, provider) = self
			.commit(storage, now_ms, move |sessions| {
				let Some(session) = sessions.get_mut(&target) else {
					return ((CancelOutcome::Refused, None), Vec::new());
				};
				let outcome = session.cancel(now_ms);
				let provider = session.provider_id.clone();
				let changed =
					if outcome == CancelOutcome::Refused { Vec::new() } else { vec![target] };
				((outcome, provider), changed)
			})
			.await?;
		if let (CancelOutcome::Requested, Some(provider)) = (outcome, provider) {
			self.serve_item(
				&provider,
				HostFundingServeSubscribeItem::Cancel { intent: intent.to_string() },
			);
		}
		Ok(outcome)
	}

	/// Watch one session, receiving its current stage immediately. A terminal
	/// session yields that one item and then ends.
	pub fn subscribe(
		&self,
		intent: &str,
	) -> Option<BoxStream<'static, HostFundingStatusSubscribeItem>> {
		// A commit fans out under this lock after writing, so holding it from
		// the read to the registration means no change is missed.
		let mut subscribers = self.lock_subscribers();
		let session = self.get(intent)?;
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
	async fn present(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		session: FundingSession,
	) -> Result<(), FundingSessionError> {
		let intent = session.intent.clone();
		let presented = self
			.commit(storage, now_ms, |sessions| {
				self.lock_presenting().insert(session.intent.clone());
				sessions.insert(session.intent.clone(), session);
				((), Vec::new())
			})
			.await;
		if presented.is_err() {
			let _ = self.discard(&intent).await;
		}
		presented
	}

	/// The user started presented session `intent`: store it and announce
	/// it. A session that cannot be stored is discarded, as the caller is
	/// told it failed.
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
		if started.is_err() {
			let _ = self.discard(intent).await;
		}
		started
	}

	/// Drop presented session `intent`, unless a provider took it while the
	/// overlay was up: that provider may already be working on it, and the
	/// session is stored from then on. Returns whether it was dropped.
	///
	/// An unassigned presented session was never stored, so this cannot
	/// fail; holding the write lock keeps any commit from storing it on the
	/// way out.
	async fn discard(&self, intent: &str) -> bool {
		let _writes = self.writes.lock().await;
		if self
			.lock_sessions()
			.get(intent)
			.is_some_and(|session| session.provider_id.is_some())
		{
			return false;
		}
		self.lock_presenting().remove(intent);
		self.lock_sessions().remove(intent);
		true
	}

	/// Apply `edit` to a copy of the sessions, persist the result, then make it
	/// current and notify every session `edit` names.
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
		let before = self.lock_sessions().clone();
		let mut working = before.clone();
		let mut changed = Vec::new();
		if !*loaded {
			for session in load_sessions(storage).await? {
				working.entry(session.intent.clone()).or_insert(session);
			}
		}
		let (result, edited) = edit(&mut working);
		// A session the host is told about must be stored too, which covers
		// one leaving the overlay: started, though nothing in it changed.
		let announces = !edited.is_empty();
		changed.extend(edited);
		for session in working.values_mut() {
			if session.expire_if_due(now_ms) {
				changed.push(session.intent.clone());
			}
			if !session.serves_provider() {
				session.saved = None;
			}
		}
		if !*loaded || announces || working != before {
			let kept = retained(working.into_values());
			// An assigned session is stored even while its overlay is up, as
			// its provider's reports are stored before they are answered.
			let started: Vec<_> = {
				let presenting = self.lock_presenting();
				kept.iter()
					.filter(|session| {
						!presenting.contains(&session.intent) || session.provider_id.is_some()
					})
					.cloned()
					.collect()
			};
			store_sessions(storage, &started).await?;
			*self.lock_sessions() =
				kept.into_iter().map(|session| (session.intent.clone(), session)).collect();
		}
		*loaded = true;
		// Each session once, even when an edit and its expiry both name it.
		let mut announced = HashSet::new();
		changed.retain(|intent| announced.insert(intent.clone()));
		for intent in changed {
			if let Some(session) = self.get(&intent) {
				self.fan_out(&session);
			}
		}
		drop(loaded);
		self.announce();
		self.follow_sessions();
		Ok(result)
	}

	/// Bring worker references and followers in line with the sessions:
	/// every assigned open session holds its provider's worker, and every
	/// session whose outcome is due has its top-ups or payment followed.
	fn follow_sessions(&self) {
		let Some(services) = self.services.upgrade() else {
			return;
		};
		let sessions = self.lock_sessions().clone();
		let (acquire, release) = {
			let mut holding = self.lock_holding();
			let mut acquire = Vec::new();
			for session in sessions.values().filter(|session| session.serves_provider()) {
				if let Some(provider) = &session.provider_id &&
					!holding.contains_key(&session.intent)
				{
					holding.insert(session.intent.clone(), provider.clone());
					acquire.push(provider.clone());
				}
			}
			let ended: Vec<String> = holding
				.keys()
				.filter(|intent| {
					sessions.get(*intent).is_none_or(|session| !session.serves_provider())
				})
				.cloned()
				.collect();
			let release: Vec<String> =
				ended.iter().filter_map(|intent| holding.remove(intent)).collect();
			(acquire, release)
		};
		for provider in &acquire {
			services.worker_ledger.acquire(provider);
		}
		for provider in &release {
			services.worker_ledger.release(provider);
		}
		services.funding_quotes().retain_sessions(|intent| {
			sessions
				.get(intent)
				.is_some_and(|session| !session.is_terminal() && session.provider_id.is_none())
		});
		for session in sessions.values() {
			let (Some(settlement), Some(provider)) =
				(session.settlement_due(), session.provider_id.clone())
			else {
				continue;
			};
			if self.lock_following().insert(session.intent.clone()) {
				services.clone().follow_settlement(session.intent.clone(), provider, settlement);
			}
		}
	}

	/// Send `item` to every open stream of `provider_id`.
	fn serve_item(&self, provider_id: &str, item: HostFundingServeSubscribeItem) {
		if let Some(senders) = self.lock_servers().get_mut(provider_id) {
			senders.retain(|sender| sender.unbounded_send(item.clone()).is_ok());
		}
	}

	/// Load persisted sessions if they are not loaded yet and expire the
	/// overdue ones, then keep expiring the rest on time.
	pub async fn load(
		self: &Arc<Self>,
		services: &RuntimeServices,
	) -> Result<(), FundingSessionError> {
		self.sweep(services.platform.as_ref(), current_unix_millis()).await?;
		self.keep_expiring(services);
		Ok(())
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

	/// Keep one task waiting on the earliest deadline while any session can
	/// expire, so a session expires on time whether or not anyone asks. The
	/// task ends once none can or the registry is dropped.
	pub fn keep_expiring(self: &Arc<Self>, services: &RuntimeServices) {
		if self.sweeping.swap(true, Ordering::AcqRel) {
			return;
		}
		let registry = Arc::downgrade(self);
		let storage: Arc<dyn Platform> = services.platform.clone();
		(services.spawner)(Box::pin(async move {
			while let Some(wait_ms) = Self::next_wait(&registry) {
				futures_timer::Delay::new(Duration::from_millis(wait_ms)).await;
				let Some(live) = registry.upgrade() else {
					return;
				};
				if let Err(error) = live.sweep(storage.as_ref(), current_unix_millis()).await {
					tracing::warn!(%error, "funding expiry sweep failed");
					futures_timer::Delay::new(SWEEP_RETRY).await;
				}
			}
		}));
	}

	/// Milliseconds until the earliest open deadline, or `None` once nothing
	/// is open, which also clears the sweeping flag.
	fn next_wait(registry: &Weak<Self>) -> Option<u64> {
		let live = registry.upgrade()?;
		loop {
			if let Some(deadline_ms) = live.next_deadline() {
				return Some(deadline_ms.saturating_sub(current_unix_millis()));
			}
			live.sweeping.store(false, Ordering::Release);
			// A session opened after the check would otherwise wait for the
			// next caller to arm a sweep.
			if live.next_deadline().is_none() || live.sweeping.swap(true, Ordering::AcqRel) {
				return None;
			}
		}
	}

	fn next_deadline(&self) -> Option<u64> {
		self.lock_sessions().values().filter_map(FundingSession::expires_at).min()
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
		if self.platform.get().is_some() {
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

	fn lock_presenting(&self) -> MutexGuard<'_, HashSet<String>> {
		lock(&self.presenting)
	}

	fn lock_sessions(&self) -> MutexGuard<'_, HashMap<String, FundingSession>> {
		lock(&self.sessions)
	}

	fn lock_asks(&self) -> MutexGuard<'_, HashMap<String, PendingAsk>> {
		lock(&self.asks)
	}

	fn lock_subscribers(&self) -> MutexGuard<'_, Subscribers> {
		lock(&self.subscribers)
	}

	fn lock_servers(&self) -> MutexGuard<'_, Servers> {
		lock(&self.servers)
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

/// Why a session could not be opened.
#[derive(Debug, derive_more::Display)]
pub enum OpenFundingError {
	/// The host has no funding surface.
	#[display("host has no funding surface")]
	Unsupported,
	/// The user closed the overlay before starting.
	#[display("funding dismissed")]
	Dismissed,
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
	/// Load persisted sessions and arm expiry for them, so sessions from
	/// before a restart end on time and the host hears of them.
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
					Ok(()) => break registry.keep_expiring(&services),
					Err(error) => {
						tracing::warn!(%error, ?retry_in, "loading funding sessions failed");
						futures_timer::Delay::new(retry_in).await;
						retry_in = (retry_in * 2).min(RESUME_RETRY_LONGEST);
					},
				}
			}
		}));
	}

	/// End session `intent` with `stage` as though its funds had moved, for
	/// test hosts that drive the flow with no chain behind it. Returns
	/// whether it was still in flight.
	#[cfg(feature = "test-host")]
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

	/// Cancel session `intent` at the user's request. Returns whether the
	/// cancel was taken: the session ended, or its provider was asked to
	/// stop.
	pub async fn cancel_funding(&self, intent: &str) -> Result<bool, FundingSessionError> {
		let outcome = self
			.funding()
			.cancel(self.platform.as_ref(), current_unix_millis(), intent)
			.await?;
		Ok(outcome != CancelOutcome::Refused)
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
		let provider = ProductContext::new_with_execution(
			provider_id.to_string(),
			ProductExecutionKind::Worker,
		)
		.map_err(|error| FundingSessionError::InvalidProvider { reason: error.to_string() })?;
		let registry = self.funding();
		if let Some(session) = registry.get(intent) &&
			!self.funding_providers().is_candidate(&provider.product_id, session.direction)
		{
			return Err(FundingSessionError::InvalidProvider {
				reason: format!(
					"{} is not a funding provider for this session",
					provider.product_id
				),
			});
		}
		let choice = match quote_id {
			None => None,
			Some(quote_id) => Some(
				self.funding_quotes()
					.offered(intent, &provider.product_id, quote_id, current_unix_millis())
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
				&provider.product_id,
				choice,
			)
			.await
	}

	/// Follow a session's top-ups, payment or refund to the end, then settle
	/// it with what moved, from the amounts the provider started them with.
	fn follow_settlement(
		self: Arc<Self>,
		intent: String,
		provider_id: String,
		settlement: Settlement,
	) {
		let spawner = self.spawner.clone();
		spawner(Box::pin(async move {
			let registry = self.funding();
			let provider =
				match ProductContext::new_with_execution(provider_id, ProductExecutionKind::Worker)
				{
					Ok(provider) => provider,
					Err(error) => {
						tracing::warn!(%error, "a funding session names an invalid provider");
						registry.lock_following().remove(&intent);
						return;
					},
				};
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
				let settling = intent.clone();
				let settled = registry
					.commit(self.platform.as_ref(), current_unix_millis(), move |sessions| {
						let now_ms = current_unix_millis();
						let settled = sessions.get_mut(&settling).is_some_and(|session| {
							if refund {
								session.settle_refund(moved)
							} else {
								session.settle(moved, now_ms)
							}
						});
						((), if settled { vec![settling] } else { Vec::new() })
					})
					.await;
				if let Err(error) = settled {
					tracing::warn!(%error, "settling a funding session failed");
				}
			}
			registry.lock_following().remove(&intent);
		}));
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
			if self
				.funding()
				.get(intent)
				.is_none_or(|session| session.settlement_due().is_none())
			{
				return None;
			}
		}
	}

	/// Open a session and show the host's funding overlay for it: the one
	/// path a product's `request` and the host's own Balance card both take.
	///
	/// A session the user never starts, or that the host fails to show, is
	/// discarded rather than left open.
	pub async fn open_funding(
		self: &Arc<Self>,
		product: Option<&ProductContext>,
		direction: FundingDirection,
		amount: Option<u128>,
	) -> Result<FundingSession, OpenFundingError> {
		let registry = self.funding();
		let platform = registry.platform().ok_or(OpenFundingError::Unsupported)?;
		let now_ms = current_unix_millis();
		let session = FundingSession::new(
			format!("fs_{}", nanoid::nanoid!(10)),
			product.map(|product| product.product_id.clone()),
			direction,
			amount,
			now_ms,
		);
		// Live while the overlay is up, so the overlay can quote and pick a
		// provider, but stored only once the user starts it: an overlay the
		// app is killed under leaves no session behind.
		let storage = self.platform.as_ref();
		registry
			.present(storage, now_ms, session.clone())
			.await
			.map_err(OpenFundingError::Session)?;
		let presented = platform
			.present_funding(
				product,
				FundingPresentation { intent: session.intent.clone(), direction, amount },
			)
			.await;
		match presented {
			Ok(FundingPresentOutcome::Started) => {
				registry
					.start(storage, current_unix_millis(), &session.intent)
					.await
					.map_err(OpenFundingError::Session)?;
				registry.keep_expiring(self);
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
				// runs as started; unless funds already move, the provider is
				// asked to stop, as the user left without starting.
				let now_ms = current_unix_millis();
				registry
					.start(storage, now_ms, &session.intent)
					.await
					.map_err(OpenFundingError::Session)?;
				if registry.get(&session.intent).is_some_and(|kept| !kept.funds_moving()) {
					registry
						.cancel(storage, now_ms, &session.intent)
						.await
						.map_err(OpenFundingError::Session)?;
				}
				registry.keep_expiring(self);
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

	use futures::{FutureExt, executor::block_on};
	use truapi::latest::FundingFailure;

	use crate::{host_logic::funding::FundingStage, test_support::stub_platform};

	const NOW: u64 = 1_700_000_000_000;
	const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

	fn session(intent: &str, opened_at_ms: u64) -> FundingSession {
		FundingSession::new(
			intent.to_string(),
			Some("wallet.dot".to_string()),
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
				HostFundingStatusSubscribeItem::Failed {
					reason: FundingFailure::Expired,
					moved: 0,
				},
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

	// A provider that restarts after a withdrawal was released must still be
	// handed it, or it could never report how the payout went; once it has,
	// there is nothing left for it to do.
	#[test]
	fn a_released_withdrawal_is_served_until_its_payout_is_reported() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		let mut released =
			FundingSession { direction: FundingDirection::Out, ..session("fs_out", NOW) };
		assert!(released.assign("ramp.dot", None, NOW));
		released.updates.push(crate::host_logic::funding::FundingUpdateRecord {
			update: FundingUpdate::Collecting { payment_id: [4; 32], amount: 100 },
			at_ms: NOW,
		});
		assert!(released.settle(100, NOW));
		insert(&registry, storage.as_ref(), released);
		let first_served = || match registry.serve("ramp.dot").next().now_or_never().flatten() {
			Some(HostFundingServeSubscribeItem::Assigned { session }) => Some(session.intent),
			_ => None,
		};

		let before = first_served();
		let reported = block_on(registry.report(
			storage.as_ref(),
			NOW + 1,
			&worker("ramp.dot"),
			"fs_out",
			FundingUpdate::Payout { outcome: truapi::latest::FundingPayout::PaidOut },
		))
		.expect("stored");
		let after = first_served();

		assert_eq!((before, reported, after), (Some("fs_out".to_string()), Ok(()), None));
	}

	fn worker(provider_id: &str) -> ProductContext {
		ProductContext::new_with_execution(provider_id.to_string(), ProductExecutionKind::Worker)
			.expect("test provider id is valid")
	}

	// A top-up does not say which session it is for, so its amount picks the
	// session when exactly one asks for it; otherwise every session the
	// provider serves may name it.
	#[test]
	fn started_funds_go_to_the_one_session_asking_for_that_amount() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		for (intent, amount) in [("fs_fifty", 50), ("fs_thirty", 30)] {
			let mut session = FundingSession::new(
				intent.to_string(),
				Some("wallet.dot".to_string()),
				FundingDirection::In,
				Some(amount),
				NOW,
			);
			assert!(session.assign("ramp.dot", None, NOW));
			insert(&registry, storage.as_ref(), session);
		}
		let record = |id: u8, amount: u128| {
			block_on(registry.record_start(
				storage.as_ref(),
				NOW,
				"ramp.dot",
				FundingDirection::In,
				[id; 32],
				amount,
			))
			.expect("recorded")
		};
		record(1, 50);
		record(2, 20);
		let started = |intent: &str| {
			registry
				.get(intent)
				.expect("kept")
				.started
				.iter()
				.map(|record| record.host_id[0])
				.collect::<Vec<_>>()
		};

		assert_eq!((started("fs_fifty"), started("fs_thirty")), (vec![1, 2], vec![2]));
	}

	// A withdrawal whose payout failed is refunded with a top-up, so it takes
	// top-ups like an inbound session does, and its amount picks it.
	#[test]
	fn a_refund_top_up_goes_to_the_withdrawal_whose_payout_failed() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		let mut inbound = session("fs_in", NOW);
		assert!(inbound.assign("ramp.dot", None, NOW));
		insert(&registry, storage.as_ref(), inbound);
		let mut withdrawal =
			FundingSession { direction: FundingDirection::Out, ..session("fs_out", NOW) };
		assert!(withdrawal.assign("ramp.dot", None, NOW));
		withdrawal.record_start([4; 32], 80);
		withdrawal
			.report(
				"ramp.dot",
				FundingUpdate::Collecting { payment_id: [4; 32], amount: 80 },
				|id| *id,
				NOW,
			)
			.expect("collecting");
		assert!(withdrawal.settle(80, NOW));
		let failed = truapi::latest::FundingPayout::Failed { reason: "card closed".to_string() };
		withdrawal
			.report("ramp.dot", FundingUpdate::Payout { outcome: failed }, |id| *id, NOW)
			.expect("payout");
		insert(&registry, storage.as_ref(), withdrawal);

		let recorded = block_on(registry.record_start(
			storage.as_ref(),
			NOW,
			"ramp.dot",
			FundingDirection::In,
			[7; 32],
			80,
		))
		.expect("recorded");
		let started = |intent: &str| {
			registry
				.get(intent)
				.expect("kept")
				.started
				.iter()
				.any(|record| record.host_id == [7; 32])
		};

		assert_eq!((recorded, started("fs_out"), started("fs_in")), (true, true, false));
	}

	// A top-up settles one session once, from the amount it was started with:
	// a provider cannot name it in a second session, replay it in a session it
	// was not started for, or name another provider's.
	#[test]
	fn started_funds_are_named_once_by_a_session_they_were_started_for() {
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
		let recorded = block_on(registry.record_start(
			storage.as_ref(),
			NOW,
			"ramp.dot",
			FundingDirection::In,
			host_id,
			100,
		))
		.expect("recorded");
		insert(&registry, storage.as_ref(), served("fs_late", "ramp.dot"));
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
				recorded,
				report("ramp.dot", "fs_a"),
				report("ramp.dot", "fs_b"),
				report("ramp.dot", "fs_late"),
				report("other.dot", "fs_c"),
				registry.get("fs_a").expect("kept").started,
			),
			(
				true,
				Ok(()),
				Err(HostFundingReportError::NotStarted),
				Err(HostFundingReportError::NotStarted),
				Err(HostFundingReportError::NotStarted),
				vec![StartedFunds { host_id, amount: 100, named_at_ms: Some(NOW) }],
			)
		);
	}

	// A claim that is not final yet can still be reversed, and a top-up the
	// host does not know may only be slow to appear: neither settles the
	// session until the host decides or the grace period runs out.
	#[test]
	fn only_a_final_claim_or_a_long_unknown_top_up_settles() {
		let funds = StartedFunds { host_id: [1; 32], amount: 100, named_at_ms: Some(NOW) };
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
		let funds = StartedFunds { host_id: [1; 32], amount: 100, named_at_ms: Some(NOW) };
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
}

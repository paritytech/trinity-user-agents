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
use truapi::latest::{FundingDirection, GenericError, HostFundingStatusSubscribeItem};

use super::services::RuntimeServices;
use crate::{
	host_logic::funding::{
		FundingSession, FundingSessionError, load_sessions, retained, store_sessions,
	},
	platform::{
		CoreStorage, FundingPlatform, FundingPresentOutcome, FundingPresentation, Platform,
		ProductContext,
	},
	unix_time::current_unix_millis,
};

/// Wait before retrying an expiry sweep whose write failed.
const SWEEP_RETRY: Duration = Duration::from_secs(30);

type Subscribers = HashMap<String, Vec<mpsc::UnboundedSender<HostFundingStatusSubscribeItem>>>;

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

	/// Record that the host has written ended session `intent` into its own
	/// history, so it is no longer handed over. Its owner can still read how it
	/// ended until newer recorded sessions push it out. Returns whether the
	/// session was ended and not yet acknowledged.
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

	/// Cancel open session `intent`. Returns whether it was still open. A
	/// session whose overlay is still up is not the host's to cancel yet: the
	/// overlay ends it by closing.
	pub async fn cancel(
		&self,
		storage: &(impl CoreStorage + ?Sized),
		now_ms: u64,
		intent: &str,
	) -> Result<bool, FundingSessionError> {
		let intent = intent.to_string();
		self.commit(storage, now_ms, move |sessions| {
			let cancelled = !self.lock_presenting().contains(&intent) &&
				sessions.get_mut(&intent).is_some_and(|session| session.cancel(now_ms));
			(cancelled, Vec::from_iter(cancelled.then_some(intent)))
		})
		.await
	}

	/// Watch one session: its current stage at once, then one item per change,
	/// in commit order and never repeated. The stream ends after a terminal
	/// stage, so a session that has already ended, including one the host
	/// recorded, yields that one item. It never errors; dropping it stops
	/// watching. `None` when no such session is held.
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
			self.discard(&intent).await;
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
		if started.is_err() {
			self.discard(intent).await;
		}
		started
	}

	/// Drop presented session `intent`. It was never stored, so this cannot
	/// fail; holding the write lock keeps any commit from storing it on the
	/// way out.
	async fn discard(&self, intent: &str) {
		let _writes = self.writes.lock().await;
		self.lock_presenting().remove(intent);
		self.lock_sessions().remove(intent);
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
		}
		// The write lock keeps the in-memory map unchanged until this commit
		// replaces it, so it still holds the sessions as they were before `edit`.
		if !*loaded || announces || working != *self.lock_sessions() {
			let kept = retained(working.into_values());
			let (started, stored_set_changed) = {
				let presenting = self.lock_presenting();
				let before = self.lock_sessions();
				let started: Vec<&FundingSession> =
					kept.iter().filter(|session| !presenting.contains(&session.intent)).collect();
				let stored_before = before
					.values()
					.filter(|session| !presenting_before.contains(&session.intent))
					.count();
				// A session only shown in the overlay is not stored, so holding
				// one changes nothing worth writing.
				let unchanged = started.len() == stored_before &&
					started.iter().all(|session| {
						!presenting_before.contains(&session.intent) &&
							before.get(&session.intent) == Some(*session)
					});
				(started, !unchanged)
			};
			if (!*loaded || announces || stored_set_changed) &&
				let Err(error) = store_sessions(storage, &started).await
			{
				*self.lock_presenting() = presenting_before;
				return Err(error);
			}
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
		Ok(result)
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

	/// Keep one task waiting on the earliest open deadline while any session
	/// is open, so a session expires on time whether or not anyone asks. The
	/// task ends once no session is open or the registry is dropped.
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
		self.lock_sessions()
			.values()
			.filter(|session| !session.is_terminal())
			.map(|session| session.deadline_ms)
			.min()
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
		self.announcements.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn lock_presenting(&self) -> MutexGuard<'_, HashSet<String>> {
		self.presenting.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn lock_sessions(&self) -> MutexGuard<'_, HashMap<String, FundingSession>> {
		self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn lock_subscribers(&self) -> MutexGuard<'_, Subscribers> {
		self.subscribers.lock().unwrap_or_else(PoisonError::into_inner)
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
		match self.open_funding(None, direction, amount).await {
			Ok(session) => Ok(Some(session.intent)),
			Err(OpenFundingError::Dismissed) => Ok(None),
			Err(error) => Err(GenericError { reason: error.to_string() }),
		}
	}

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

	/// Cancel open session `intent`. Returns whether it was still open.
	pub async fn cancel_funding(&self, intent: &str) -> Result<bool, FundingSessionError> {
		self.funding()
			.cancel(self.platform.as_ref(), current_unix_millis(), intent)
			.await
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
		if amount == Some(0) {
			return Err(OpenFundingError::InvalidAmount);
		}
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
				registry.discard(&session.intent).await;
				Err(match outcome {
					Ok(_) => OpenFundingError::Dismissed,
					Err(error) => OpenFundingError::Present(error),
				})
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use futures::executor::block_on;
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

		assert!(block_on(registry.cancel(storage.as_ref(), NOW, "fs_a")).expect("cancelled"));

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
		let expired =
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired, moved: 0 };

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
			(Ok(()), Ok(vec![session("fs_1", NOW)]))
		);
	}

	#[test]
	fn a_session_whose_overlay_is_up_cannot_be_cancelled() {
		let storage = stub_platform();
		let registry = FundingRegistry::default();
		block_on(registry.present(storage.as_ref(), NOW, session("fs_1", NOW))).expect("presented");

		assert_eq!(block_on(registry.cancel(storage.as_ref(), NOW, "fs_1")), Ok(false));
		assert_eq!(registry.get("fs_1").map(|session| session.is_terminal()), Some(false));
	}
}

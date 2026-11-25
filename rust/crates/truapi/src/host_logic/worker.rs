//! Reference-counted demand on product workers.
//!
//! A product has one worker. The host keeps one reference count per worker:
//! the first reference starts it, and when the count returns to zero the host
//! may stop it. The ledger owns the counts; starting and stopping the
//! executable stays with the host.

use std::{
	collections::{HashMap, VecDeque},
	sync::{Arc, Mutex, MutexGuard, OnceLock},
};

use core::time::Duration;

use crate::{platform::ProductId, runtime::RuntimeServices, time::Backoff};

/// What the host does with a product's worker after demand on it changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerTransition {
	/// Demand went from none to some: the host starts the worker.
	Start,
	/// Demand went from some to none: the host may stop the worker.
	Stop,
}

/// Host control of product workers, driven by reference-counted demand.
/// Requests arrive in ledger order, whether the host or core took or dropped
/// the reference that crossed zero.
pub trait WorkerController: Send + Sync {
	/// Start the identified attempt for a product with demand.
	fn start_worker(&self, product_id: &ProductId, attempt_id: u64);

	/// Stop the product's current attempt after the last release or before replacement.
	fn stop_worker(&self, product_id: &ProductId);

	/// The product's worker crashed `crashes` times in a row while it was
	/// needed, so the host tells the user it is not working. The core keeps
	/// starting it again, waiting longer each time.
	fn worker_crashing(&self, _product_id: &ProductId, _crashes: u32) {}
}

/// An exit more than this long after the previous one starts a new run of
/// crashes, in milliseconds.
const CRASH_RUN_WINDOW_MS: u64 = 60_000;
/// The wait before restarting a worker after its second crash in a run; it
/// doubles with each further crash.
const RESTART_BACKOFF_FIRST: Duration = Duration::from_secs(1);
/// The longest wait before restarting a crashed worker.
const RESTART_BACKOFF_LONGEST: Duration = Duration::from_secs(5 * 60);
/// The crash in a run on which the host is told the worker is not working.
const CRASH_NOTICE_AFTER: u32 = 3;

/// What the ledger reports to the host, in the order it happened.
enum LedgerEvent {
	Start { product_id: ProductId, attempt_id: u64 },
	Stop { product_id: ProductId },
	Crashing { product_id: ProductId, consecutive_crashes: u32 },
}

/// A product's run of crashes.
struct CrashRun {
	/// Crashes in the run so far.
	crashes: u32,
	/// The waits before the run's next restarts.
	backoff: Backoff,
	/// When the last one was reported, in Unix milliseconds.
	last_exit_ms: u64,
}

/// The holds on one product's worker and the attempt started for them.
struct Demand {
	/// Live [`crate::WorkerHold`]s. Core operations and host modalities each
	/// take their own, so the first one starts the worker and dropping the
	/// last one stops it.
	count: usize,
	/// ID of the latest attempt started for these holds. Reports from older
	/// attempts are ignored.
	attempt_id: u64,
}

/// Counts and the transitions they have produced but not yet reported.
#[derive(Default)]
struct LedgerState {
	/// Reference counts keyed by product id. A product with no references has
	/// no entry.
	references: HashMap<ProductId, Demand>,
	/// Shared across products so each worker boot has a distinct ID within the session.
	last_attempt_id: u64,
	/// Set once by [`WorkerLedger::shutdown`]. A hold taken afterwards counts
	/// for nothing, so no worker starts after the session ended.
	closed: bool,
	/// Each product's current run of crashes.
	crashes: HashMap<ProductId, CrashRun>,
	/// Events in ledger order, waiting for the draining thread.
	pending: VecDeque<LedgerEvent>,
	/// Whether a thread is already draining `pending`.
	draining: bool,
}

impl LedgerState {
	/// Queue asking the host to move `product_id`'s worker by `transition`.
	fn request(&mut self, product_id: &ProductId, transition: WorkerTransition) {
		match transition {
			WorkerTransition::Start => {
				self.last_attempt_id += 1;
				let attempt_id = self.last_attempt_id;
				self.references.get_mut(product_id).expect("start requires demand").attempt_id =
					attempt_id;
				self.pending
					.push_back(LedgerEvent::Start { product_id: product_id.clone(), attempt_id });
			},
			WorkerTransition::Stop => {
				self.pending.push_back(LedgerEvent::Stop { product_id: product_id.clone() })
			},
		}
	}
}

/// Reference counts for every product worker the host tracks, keyed by
/// product id. A product with no references has no entry.
#[derive(Default)]
pub struct WorkerLedger {
	state: Mutex<LedgerState>,
	controller: OnceLock<Arc<dyn WorkerController>>,
}

impl WorkerLedger {
	fn state(&self) -> MutexGuard<'_, LedgerState> {
		self.state.lock().expect("worker ledger mutex poisoned")
	}

	/// Take one reference on the product's worker. The first one requests
	/// [`WorkerTransition::Start`] through the installed controller.
	pub fn acquire(&self, product_id: &ProductId) {
		{
			let mut state = self.state();
			if state.closed {
				return;
			}
			let demand = state
				.references
				.entry(product_id.clone())
				.or_insert(Demand { count: 0, attempt_id: 0 });
			demand.count += 1;
			if demand.count != 1 {
				return;
			}
			state.request(product_id, WorkerTransition::Start);
		}
		self.drain();
	}

	/// Release one reference. The last one requests [`WorkerTransition::Stop`]
	/// through the installed controller. Releasing a product with no reference
	/// is a no-op.
	pub fn release(&self, product_id: &ProductId) {
		{
			let mut state = self.state();
			let Some(demand) = state.references.get_mut(product_id) else {
				return;
			};
			if demand.count > 1 {
				demand.count -= 1;
				return;
			}
			// The last one: an entry is dropped rather than left at zero, so
			// every stored count is at least one and none of this can wrap.
			state.references.remove(product_id);
			state.request(product_id, WorkerTransition::Stop);
		}
		self.drain();
	}

	/// The host reports at `now_ms` that the product's worker stopped
	/// running without being asked to: it crashed, failed to boot or the
	/// system killed it. With no references held nothing needs it, and this
	/// is a no-op that answers `None`. So is a report from an attempt that
	/// has been replaced.
	///
	/// Otherwise the exit joins the product's run of crashes, which starts
	/// anew after [`CRASH_RUN_WINDOW_MS`] without one. The first crash of a
	/// run requests [`WorkerTransition::Start`] at once and answers `None`.
	/// A later one answers the wait before the caller calls
	/// [`Self::restart`]: [`RESTART_BACKOFF_FIRST`], doubling with each
	/// crash up to [`RESTART_BACKOFF_LONGEST`]. The run's
	/// [`CRASH_NOTICE_AFTER`]th crash tells the host through
	/// [`WorkerController::worker_crashing`].
	fn exited(&self, product_id: &ProductId, attempt_id: u64, now_ms: u64) -> Option<u64> {
		let wait_ms = {
			let mut state = self.state();
			let demand = state.references.get(product_id)?;
			if attempt_id != demand.attempt_id {
				return None;
			}
			let run = state.crashes.entry(product_id.clone()).or_insert(CrashRun {
				crashes: 0,
				backoff: Backoff::new(RESTART_BACKOFF_FIRST, RESTART_BACKOFF_LONGEST),
				last_exit_ms: now_ms,
			});
			if now_ms.saturating_sub(run.last_exit_ms) > CRASH_RUN_WINDOW_MS {
				run.crashes = 0;
				run.backoff.reset();
			}
			run.crashes = run.crashes.saturating_add(1);
			run.last_exit_ms = now_ms;
			let consecutive_crashes = run.crashes;
			// The first crash of a run restarts at once.
			let wait_ms =
				(consecutive_crashes > 1).then(|| run.backoff.next_pause().as_millis() as u64);
			if consecutive_crashes == CRASH_NOTICE_AFTER {
				state.pending.push_back(LedgerEvent::Crashing {
					product_id: product_id.clone(),
					consecutive_crashes,
				});
			}
			if wait_ms.is_none() {
				state.request(product_id, WorkerTransition::Start);
			}
			wait_ms
		};
		self.drain();
		wait_ms
	}

	/// Restart a crashed worker once its backoff has passed: requests
	/// [`WorkerTransition::Start`] unless nothing holds the worker any more
	/// or it was started meanwhile.
	fn restart(&self, product_id: &ProductId, attempt_id: u64) {
		{
			let mut state = self.state();
			let Some(demand) = state.references.get(product_id) else { return };
			if attempt_id != demand.attempt_id {
				return;
			}
			state.request(product_id, WorkerTransition::Start);
		}
		self.drain();
	}

	/// Whether this specific attempt still serves current demand.
	pub fn is_current(&self, product_id: &ProductId, attempt_id: u64) -> bool {
		self.state()
			.references
			.get(product_id)
			.is_some_and(|demand| demand.attempt_id == attempt_id)
	}

	/// Restart a referenced product after an update, preserving its consumers.
	pub fn reload(&self, product_id: &ProductId) {
		{
			let mut state = self.state();
			if !state.references.contains_key(product_id) {
				return;
			}
			state.crashes.remove(product_id);
			state.request(product_id, WorkerTransition::Stop);
			state.request(product_id, WorkerTransition::Start);
		}
		self.drain();
	}

	/// Release all session demand and refuse subsequent acquisitions and restarts.
	pub fn shutdown(&self) {
		{
			let mut state = self.state();
			if state.closed {
				return;
			}
			state.closed = true;
			state.crashes.clear();
			for product_id in core::mem::take(&mut state.references).into_keys() {
				state.request(&product_id, WorkerTransition::Stop);
			}
		}
		self.drain();
	}

	/// Install the host's controller for every transition the ledger requests.
	/// Set-once, so a host cannot be swapped out from under a live reference.
	/// Returns whether this call installed it.
	///
	/// Anything that crossed zero before this is still queued and is delivered
	/// now: the counts that produced it stand, so dropping it would leave the
	/// ledger holding a reference the host was never told to start.
	#[must_use]
	pub fn install_worker_controller(&self, controller: Arc<dyn WorkerController>) -> bool {
		if self.controller.set(controller).is_err() {
			return false;
		}
		self.drain();
		true
	}

	/// References currently held on a product's worker.
	#[cfg(test)]
	pub fn count(&self, product_id: &ProductId) -> usize {
		self.state().references.get(product_id).map_or(0, |demand| demand.count)
	}

	/// Request queued transitions through the controller, one thread at a time and in
	/// the order the counts produced them.
	///
	/// The controller is a host callback across a UniFFI or wasm boundary, so it
	/// may take a reference of its own before it returns. Calling it under the
	/// ledger mutex would deadlock on that, and dropping the mutex before
	/// calling it lets two threads deliver in the opposite order to the one
	/// their counts crossed zero in. Queueing under the mutex fixes the order,
	/// and a single draining thread delivers the queue with no lock held: a
	/// re-entrant acquire or release from inside the controller appends and
	/// returns, leaving the delivery to the drain already in progress.
	///
	/// A [`DrainGuard`] clears `draining` when this scope exits, including by
	/// an unwinding panic from the controller, so a delivery that panics still
	/// leaves the ledger able to drain the transitions still queued behind
	/// it. The panic itself is not caught here: it keeps unwinding into
	/// whichever `acquire` or `release` call triggered this drain.
	fn drain(&self) {
		// Nothing to deliver to yet. The queue keeps its order and its
		// contents until a controller is installed, which drains it.
		let Some(controller) = self.controller.get() else {
			return;
		};
		{
			let mut state = self.state();
			if state.draining {
				return;
			}
			state.draining = true;
		}
		let mut guard = DrainGuard { ledger: self, armed: true };
		loop {
			let next = {
				let mut state = self.state();
				let Some(next) = state.pending.pop_front() else {
					// Clearing the flag under the same lock that found the
					// queue empty is what makes stopping safe. Clearing it
					// after the unlock leaves a gap in which another thread
					// queues a transition, sees a drain still in progress and
					// returns, and then finds no drain left to deliver it.
					state.draining = false;
					guard.armed = false;
					return;
				};
				next
			};
			match next {
				LedgerEvent::Start { product_id, attempt_id } => {
					controller.start_worker(&product_id, attempt_id)
				},
				LedgerEvent::Stop { product_id } => controller.stop_worker(&product_id),
				LedgerEvent::Crashing { product_id, consecutive_crashes } => {
					controller.worker_crashing(&product_id, consecutive_crashes)
				},
			}
		}
	}
}

/// Clears [`LedgerState::draining`] for a drain that ends without clearing it
/// itself, which is a delivery unwinding out of a controller panic, so the
/// transitions queued behind it stay drainable by the next `acquire` or
/// `release`. A drain that runs the queue dry clears the flag under the lock
/// and disarms this guard.
///
/// This protects only an unwinding panic. `truapi` also builds for
/// `wasm32-unknown-unknown`, where a trap has no unwind to run this guard's
/// `Drop`: a controller that traps on that target still leaves `draining` set.
struct DrainGuard<'a> {
	ledger: &'a WorkerLedger,
	armed: bool,
}

impl Drop for DrainGuard<'_> {
	fn drop(&mut self) {
		if self.armed {
			self.ledger.state().draining = false;
		}
	}
}

impl RuntimeServices {
	/// Retry while demand remains and `attempt` is still the product's current one.
	pub fn worker_attempt_exited(self: &Arc<Self>, product_id: &ProductId, attempt: u64) {
		let Some(wait_ms) =
			self.worker_ledger.exited(product_id, attempt, crate::current_unix_millis())
		else {
			return;
		};
		let services = Arc::downgrade(self);
		let product_id = product_id.clone();
		self.spawner.spawn(async move {
			futures_timer::Delay::new(Duration::from_millis(wait_ms)).await;
			if let Some(services) = services.upgrade() {
				services.worker_ledger.restart(&product_id, attempt);
			}
		});
	}
}

#[cfg(test)]
mod tests;

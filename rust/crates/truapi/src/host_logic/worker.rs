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

use crate::platform::ProductId;

/// What the host does with a product's worker after demand on it changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
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
	/// Request that the host start the worker or allow it to stop.
	fn request_worker_transition(&self, product_id: &ProductId, transition: WorkerTransition);
}

/// Counts and the transitions they have produced but not yet reported.
#[derive(Default)]
struct LedgerState {
	/// Reference counts keyed by product id. A product with no references has
	/// no entry.
	references: HashMap<ProductId, usize>,
	/// Transitions in ledger order, waiting for the draining thread.
	pending: VecDeque<(ProductId, WorkerTransition)>,
	/// Whether a thread is already draining `pending`.
	draining: bool,
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
			let count = state.references.entry(product_id.clone()).or_insert(0);
			*count += 1;
			if *count != 1 {
				return;
			}
			state.pending.push_back((product_id.clone(), WorkerTransition::Start));
		}
		self.drain();
	}

	/// Release one reference. The last one requests [`WorkerTransition::Stop`]
	/// through the installed controller. Releasing a product with no reference
	/// is a no-op.
	pub fn release(&self, product_id: &ProductId) {
		{
			let mut state = self.state();
			let Some(count) = state.references.get_mut(product_id) else {
				return;
			};
			if *count > 1 {
				*count -= 1;
				return;
			}
			// The last one: an entry is dropped rather than left at zero, so
			// every stored count is at least one and none of this can wrap.
			state.references.remove(product_id);
			state.pending.push_back((product_id.clone(), WorkerTransition::Stop));
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
		self.state().references.get(product_id).copied().unwrap_or(0)
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
			controller.request_worker_transition(&next.0, next.1);
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

#[cfg(test)]
mod tests;

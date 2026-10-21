//! Reference-counted demand on product workers.
//!
//! A product has one worker. The host keeps one reference count per worker:
//! the first reference starts it, and when the count returns to zero the host
//! may stop it. The ledger owns the counts; starting and stopping the
//! executable stays with the host.
//!
//! The ledger also holds one holder of its own: the foreground hold. A product
//! that declares [`WorkerKeepAlive::Foreground`] gets one reference while the
//! host reports the app in the foreground, and loses it when the host reports
//! the app in the background.

use std::{
	collections::{BTreeSet, HashMap, VecDeque},
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

/// When the worker of a product runs, beside the holders of its modalities.
///
/// The product declares it in the `keepAlive` field of its Worker executable
/// manifest, and the host gives it to the core with
/// [`WorkerLedger::set_keep_alive`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum WorkerKeepAlive {
	/// The worker runs only while a modality holds it. This is the default.
	#[default]
	OnDemand,
	/// The worker also runs while the app is in the foreground.
	Foreground,
}

/// Host notified whenever demand on a product's worker crosses zero, whoever
/// took or dropped the reference that crossed it.
///
/// Every transition the ledger produces arrives here, in the order the ledger
/// produced it: the ones the host asks for through [`WorkerLedger::acquire`]
/// and [`WorkerLedger::release`], and the ones the core causes on a product's
/// behalf, such as an open render stream.
pub trait WorkerDemandObserver: Send + Sync {
	/// Demand on `product_id` crossed zero in the direction `transition`.
	fn worker_demand_changed(&self, product_id: &ProductId, transition: WorkerTransition);
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
	/// The host reports the app in the foreground.
	foreground: bool,
	/// The products that declare [`WorkerKeepAlive::Foreground`]. While
	/// `foreground` is true, each of them has one reference in `references`
	/// for the foreground hold.
	keep_alive: BTreeSet<ProductId>,
}

impl LedgerState {
	/// Add one reference, and queue [`WorkerTransition::Start`] for the first.
	fn acquire(&mut self, product_id: &ProductId) {
		let count = self.references.entry(product_id.clone()).or_insert(0);
		*count += 1;
		if *count == 1 {
			self.pending.push_back((product_id.clone(), WorkerTransition::Start));
		}
	}

	/// Remove one reference, and queue [`WorkerTransition::Stop`] for the
	/// last. A product with no reference stays as it is.
	fn release(&mut self, product_id: &ProductId) {
		let Some(count) = self.references.get_mut(product_id) else {
			return;
		};
		if *count > 1 {
			*count -= 1;
			return;
		}
		// The last one: an entry is dropped rather than left at zero, so
		// every stored count is at least one and none of this can wrap.
		self.references.remove(product_id);
		self.pending.push_back((product_id.clone(), WorkerTransition::Stop));
	}
}

/// Reference counts for every product worker the host tracks, keyed by
/// product id. A product with no references has no entry.
#[derive(Default)]
pub struct WorkerLedger {
	state: Mutex<LedgerState>,
	observer: OnceLock<Arc<dyn WorkerDemandObserver>>,
}

impl WorkerLedger {
	fn state(&self) -> MutexGuard<'_, LedgerState> {
		self.state.lock().expect("worker ledger mutex poisoned")
	}

	/// Take one reference on the product's worker. The first one reports
	/// [`WorkerTransition::Start`] to the installed observer.
	pub fn acquire(&self, product_id: &ProductId) {
		self.state().acquire(product_id);
		self.drain();
	}

	/// Release one reference. The last one reports [`WorkerTransition::Stop`]
	/// to the installed observer. Releasing a product with no reference is a
	/// no-op.
	pub fn release(&self, product_id: &ProductId) {
		self.state().release(product_id);
		self.drain();
	}

	/// Record whether the app is in the foreground. The change takes the
	/// foreground hold of each product that declares
	/// [`WorkerKeepAlive::Foreground`], or releases it. The ledger starts in
	/// the background, so a host that never calls this holds nothing.
	///
	/// The counts and the flag change under one lock, so a foreground hold
	/// never releases a reference that another holder took.
	pub fn set_app_foreground(&self, foreground: bool) {
		{
			let mut state = self.state();
			if state.foreground == foreground {
				return;
			}
			state.foreground = foreground;
			let products: Vec<ProductId> = state.keep_alive.iter().cloned().collect();
			for product_id in &products {
				if foreground {
					state.acquire(product_id);
				} else {
					state.release(product_id);
				}
			}
		}
		self.drain();
	}

	/// Record when the worker of `product_id` runs. While the app is in the
	/// foreground, a change to [`WorkerKeepAlive::Foreground`] takes the
	/// foreground hold, and a change back releases it. The host sets
	/// [`WorkerKeepAlive::OnDemand`] again when the product goes, for example
	/// at the end of its chat session.
	pub fn set_keep_alive(&self, product_id: &ProductId, keep_alive: WorkerKeepAlive) {
		{
			let mut state = self.state();
			let changed = match keep_alive {
				WorkerKeepAlive::Foreground => state.keep_alive.insert(product_id.clone()),
				WorkerKeepAlive::OnDemand => state.keep_alive.remove(product_id),
			};
			if !changed || !state.foreground {
				return;
			}
			match keep_alive {
				WorkerKeepAlive::Foreground => state.acquire(product_id),
				WorkerKeepAlive::OnDemand => state.release(product_id),
			}
		}
		self.drain();
	}

	/// Whether the host reports the app in the foreground.
	pub fn app_foreground(&self) -> bool {
		self.state().foreground
	}

	/// What `product_id` declares with [`Self::set_keep_alive`].
	pub fn keep_alive(&self, product_id: &ProductId) -> WorkerKeepAlive {
		if self.state().keep_alive.contains(product_id) {
			WorkerKeepAlive::Foreground
		} else {
			WorkerKeepAlive::OnDemand
		}
	}

	/// Install the host's observer for every transition the ledger produces.
	/// Set-once, so a host cannot be swapped out from under a live reference.
	/// Returns whether this call installed it.
	///
	/// Anything that crossed zero before this is still queued and is delivered
	/// now: the counts that produced it stand, so dropping it would leave the
	/// ledger holding a reference the host was never told to start.
	#[must_use]
	pub fn install_demand_observer(&self, observer: Arc<dyn WorkerDemandObserver>) -> bool {
		if self.observer.set(observer).is_err() {
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

	/// Report queued transitions to the observer, one thread at a time and in
	/// the order the counts produced them.
	///
	/// The observer is a host callback across a UniFFI or wasm boundary, so it
	/// may take a reference of its own before it returns. Calling it under the
	/// ledger mutex would deadlock on that, and dropping the mutex before
	/// calling it lets two threads deliver in the opposite order to the one
	/// their counts crossed zero in. Queueing under the mutex fixes the order,
	/// and a single draining thread delivers the queue with no lock held: a
	/// re-entrant acquire or release from inside the observer appends and
	/// returns, leaving the delivery to the drain already in progress.
	///
	/// A [`DrainGuard`] clears `draining` when this scope exits, including by
	/// an unwinding panic from the observer, so a delivery that panics still
	/// leaves the ledger able to drain the transitions still queued behind
	/// it. The panic itself is not caught here: it keeps unwinding into
	/// whichever `acquire` or `release` call triggered this drain.
	fn drain(&self) {
		// Nothing to deliver to yet. The queue keeps its order and its
		// contents until an observer is installed, which drains it.
		let Some(observer) = self.observer.get() else {
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
			observer.worker_demand_changed(&next.0, next.1);
		}
	}
}

/// Clears [`LedgerState::draining`] for a drain that ends without clearing it
/// itself, which is a delivery unwinding out of an observer panic, so the
/// transitions queued behind it stay drainable by the next `acquire` or
/// `release`. A drain that runs the queue dry clears the flag under the lock
/// and disarms this guard.
///
/// This protects only an unwinding panic. `truapi` also builds for
/// `wasm32-unknown-unknown`, where a trap has no unwind to run this guard's
/// `Drop`: an observer that traps on that target still leaves `draining` set.
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
mod tests {
	use std::sync::Weak;

	use super::*;

	/// Observer recording every transition in delivery order.
	#[derive(Default)]
	struct Recorder {
		seen: Mutex<Vec<(String, WorkerTransition)>>,
	}

	impl Recorder {
		fn seen(&self) -> Vec<(String, WorkerTransition)> {
			self.seen.lock().expect("recorder mutex poisoned").clone()
		}
	}

	impl WorkerDemandObserver for Recorder {
		fn worker_demand_changed(&self, product_id: &ProductId, transition: WorkerTransition) {
			self.seen
				.lock()
				.expect("recorder mutex poisoned")
				.push((product_id.to_string(), transition));
		}
	}

	fn observed() -> (WorkerLedger, Arc<Recorder>) {
		let ledger = WorkerLedger::default();
		let recorder = Arc::new(Recorder::default());
		assert!(ledger.install_demand_observer(recorder.clone()));
		(ledger, recorder)
	}

	#[test]
	fn first_reference_starts_and_last_release_stops() {
		let (ledger, recorder) = observed();
		ledger.acquire(&ProductId::for_test("a.dot"));
		ledger.acquire(&ProductId::for_test("a.dot"));
		ledger.release(&ProductId::for_test("a.dot"));
		ledger.release(&ProductId::for_test("a.dot"));
		ledger.acquire(&ProductId::for_test("a.dot"));
		assert_eq!(
			recorder.seen(),
			vec![
				("a.dot".to_string(), WorkerTransition::Start),
				("a.dot".to_string(), WorkerTransition::Stop),
				("a.dot".to_string(), WorkerTransition::Start),
			]
		);
	}

	#[test]
	fn releasing_without_a_reference_is_a_no_op() {
		let (ledger, recorder) = observed();
		ledger.release(&ProductId::for_test("a.dot"));
		assert!(recorder.seen().is_empty());
		ledger.acquire(&ProductId::for_test("a.dot"));
		ledger.release(&ProductId::for_test("a.dot"));
		ledger.release(&ProductId::for_test("a.dot"));
		assert_eq!(
			recorder.seen(),
			vec![
				("a.dot".to_string(), WorkerTransition::Start),
				("a.dot".to_string(), WorkerTransition::Stop),
			]
		);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 0);
	}

	#[test]
	fn products_are_counted_separately() {
		let (ledger, recorder) = observed();
		ledger.acquire(&ProductId::for_test("a.dot"));
		ledger.acquire(&ProductId::for_test("b.dot"));
		ledger.release(&ProductId::for_test("a.dot"));
		ledger.release(&ProductId::for_test("b.dot"));
		assert_eq!(
			recorder.seen(),
			vec![
				("a.dot".to_string(), WorkerTransition::Start),
				("b.dot".to_string(), WorkerTransition::Start),
				("a.dot".to_string(), WorkerTransition::Stop),
				("b.dot".to_string(), WorkerTransition::Stop),
			]
		);
	}

	fn start(product_id: &str) -> (String, WorkerTransition) {
		(product_id.to_string(), WorkerTransition::Start)
	}

	fn stop(product_id: &str) -> (String, WorkerTransition) {
		(product_id.to_string(), WorkerTransition::Stop)
	}

	/// A product that declares a foreground worker runs while the app is in
	/// the foreground, stops in the background, and starts again on return.
	#[test]
	fn a_foreground_product_runs_while_the_app_is_in_the_foreground() {
		let (ledger, recorder) = observed();
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::Foreground);
		assert!(recorder.seen().is_empty(), "the ledger starts in the background");

		ledger.set_app_foreground(true);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 1);
		ledger.set_app_foreground(false);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 0);
		ledger.set_app_foreground(true);

		assert_eq!(recorder.seen(), vec![start("a.dot"), stop("a.dot"), start("a.dot")]);
		assert!(ledger.app_foreground());
	}

	/// A declaration while the app is in the foreground takes the hold at once,
	/// and the change back to on demand releases it.
	#[test]
	fn a_declaration_in_the_foreground_takes_and_releases_the_hold_at_once() {
		let (ledger, recorder) = observed();
		ledger.set_app_foreground(true);
		assert!(recorder.seen().is_empty(), "no product declares a foreground worker");

		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::Foreground);
		assert_eq!(ledger.keep_alive(&ProductId::for_test("a.dot")), WorkerKeepAlive::Foreground);
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::OnDemand);

		assert_eq!(recorder.seen(), vec![start("a.dot"), stop("a.dot")]);
		assert_eq!(ledger.keep_alive(&ProductId::for_test("a.dot")), WorkerKeepAlive::OnDemand);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 0);
	}

	/// A product has at most one foreground hold, however often the host
	/// repeats a report or a declaration.
	#[test]
	fn a_repeated_report_or_declaration_holds_once() {
		let (ledger, recorder) = observed();
		ledger.set_app_foreground(true);
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::Foreground);
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::Foreground);
		ledger.set_app_foreground(true);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 1);

		ledger.set_app_foreground(false);
		ledger.set_app_foreground(false);
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::OnDemand);
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::OnDemand);

		assert_eq!(recorder.seen(), vec![start("a.dot"), stop("a.dot")]);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 0);
	}

	/// The foreground hold is one holder among the others: the background
	/// releases only its own reference, so a room screen that holds the worker
	/// keeps it running, and the worker stops when the screen closes.
	#[test]
	fn the_background_releases_only_the_foreground_hold() {
		let (ledger, recorder) = observed();
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::Foreground);
		ledger.set_app_foreground(true);
		ledger.acquire(&ProductId::for_test("a.dot"));

		ledger.set_app_foreground(false);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 1);
		assert_eq!(recorder.seen(), vec![start("a.dot")]);

		ledger.release(&ProductId::for_test("a.dot"));
		assert_eq!(recorder.seen(), vec![start("a.dot"), stop("a.dot")]);
	}

	/// A product that declares nothing gets no hold from the foreground, and
	/// each product that declares one gets its own.
	#[test]
	fn only_the_products_that_declare_it_are_held() {
		let (ledger, recorder) = observed();
		ledger.set_keep_alive(&ProductId::for_test("b.dot"), WorkerKeepAlive::Foreground);
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::Foreground);
		ledger.set_keep_alive(&ProductId::for_test("c.dot"), WorkerKeepAlive::OnDemand);

		ledger.set_app_foreground(true);
		ledger.set_app_foreground(false);

		assert_eq!(
			recorder.seen(),
			vec![start("a.dot"), start("b.dot"), stop("a.dot"), stop("b.dot")]
		);
		assert_eq!(ledger.count(&ProductId::for_test("c.dot")), 0);
	}

	/// A declaration that comes before the observer is kept with its
	/// transition, as a reference is.
	#[test]
	fn a_foreground_hold_before_the_observer_is_reported_once_it_arrives() {
		let ledger = WorkerLedger::default();
		ledger.set_app_foreground(true);
		ledger.set_keep_alive(&ProductId::for_test("a.dot"), WorkerKeepAlive::Foreground);

		let recorder = Arc::new(Recorder::default());
		assert!(ledger.install_demand_observer(recorder.clone()));

		assert_eq!(recorder.seen(), vec![start("a.dot")]);
	}

	/// Observer that takes and drops a reference of its own from inside the
	/// callback, which is what a host does when a `Stop` makes it tear down a
	/// holder that still held a reference.
	struct Reentrant {
		ledger: Weak<WorkerLedger>,
		seen: Mutex<Vec<WorkerTransition>>,
		/// Cleared after the one re-entrant pair, so the pair it produces does
		/// not produce another.
		reenter_on: Mutex<Option<WorkerTransition>>,
	}

	impl WorkerDemandObserver for Reentrant {
		fn worker_demand_changed(&self, product_id: &ProductId, transition: WorkerTransition) {
			self.seen.lock().expect("recorder mutex poisoned").push(transition);
			{
				let mut reenter_on = self.reenter_on.lock().expect("gate mutex poisoned");
				if *reenter_on != Some(transition) {
					return;
				}
				*reenter_on = None;
			}
			let ledger = self.ledger.upgrade().expect("ledger outlives its observer");
			ledger.acquire(product_id);
			ledger.release(product_id);
		}
	}

	#[test]
	fn a_reentrant_call_from_the_observer_does_not_deadlock() {
		let ledger = Arc::new(WorkerLedger::default());
		let observer = Arc::new(Reentrant {
			ledger: Arc::downgrade(&ledger),
			seen: Mutex::new(Vec::new()),
			reenter_on: Mutex::new(Some(WorkerTransition::Stop)),
		});
		assert!(ledger.install_demand_observer(observer.clone()));

		ledger.acquire(&ProductId::for_test("a.dot"));
		ledger.release(&ProductId::for_test("a.dot"));

		// The nested pair runs while the outer `Stop` is still on the stack, so
		// its own `Start`/`Stop` are delivered after it, by the outer drain.
		assert_eq!(
			*observer.seen.lock().expect("recorder mutex poisoned"),
			vec![
				WorkerTransition::Start,
				WorkerTransition::Stop,
				WorkerTransition::Start,
				WorkerTransition::Stop,
			]
		);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 0);
	}

	/// Observer that reports what it was given and then blocks in its first
	/// callback until released, so another thread can cross zero meanwhile.
	struct Gated {
		seen: Mutex<Vec<WorkerTransition>>,
		entered: std::sync::mpsc::SyncSender<()>,
		gate: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
	}

	impl WorkerDemandObserver for Gated {
		fn worker_demand_changed(&self, _product_id: &ProductId, transition: WorkerTransition) {
			self.seen.lock().expect("recorder mutex poisoned").push(transition);
			let gate = self.gate.lock().expect("gate mutex poisoned").take();
			let Some(gate) = gate else {
				return;
			};
			self.entered.send(()).expect("test thread gone");
			gate.recv().expect("test thread gone");
		}
	}

	#[test]
	fn a_transition_still_being_reported_holds_back_the_next_one() {
		let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
		let (gate_tx, gate_rx) = std::sync::mpsc::sync_channel(1);
		let ledger = Arc::new(WorkerLedger::default());
		let observer = Arc::new(Gated {
			seen: Mutex::new(Vec::new()),
			entered: entered_tx,
			gate: Mutex::new(Some(gate_rx)),
		});
		assert!(ledger.install_demand_observer(observer.clone()));
		let seen = || observer.seen.lock().expect("recorder mutex poisoned").clone();

		let starting = std::thread::spawn({
			let ledger = ledger.clone();
			move || ledger.acquire(&ProductId::for_test("a.dot"))
		});
		entered_rx.recv().expect("observer never ran");

		// The reference is gone and the ledger knows it, but the observer is
		// still inside the `Start` it was handed, so the `Stop` waits its turn
		// instead of overtaking on this thread.
		std::thread::spawn({
			let ledger = ledger.clone();
			move || ledger.release(&ProductId::for_test("a.dot"))
		})
		.join()
		.expect("stop thread panicked");
		assert_eq!(seen(), vec![WorkerTransition::Start]);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 0);

		ledger.acquire(&ProductId::for_test("a.dot"));
		assert_eq!(seen(), vec![WorkerTransition::Start]);

		gate_tx.send(()).expect("observer gone");
		starting.join().expect("start thread panicked");
		assert_eq!(
			seen(),
			vec![WorkerTransition::Start, WorkerTransition::Stop, WorkerTransition::Start,]
		);
	}

	/// Counts every transition it is handed, from any thread.
	#[derive(Default)]
	struct Counter {
		delivered: Mutex<usize>,
	}

	impl Counter {
		fn delivered(&self) -> usize {
			*self.delivered.lock().expect("counter mutex poisoned")
		}
	}

	impl WorkerDemandObserver for Counter {
		fn worker_demand_changed(&self, _product_id: &ProductId, _transition: WorkerTransition) {
			*self.delivered.lock().expect("counter mutex poisoned") += 1;
		}
	}

	#[test]
	fn a_reference_taken_before_the_observer_is_reported_once_it_arrives() {
		let ledger = WorkerLedger::default();
		// A host that counts before it listens still owns the reference, so
		// the transition has to survive the wait rather than be dropped: the
		// count never crosses zero again to produce a second one.
		ledger.acquire(&ProductId::for_test("a.dot"));

		let recorder = Arc::new(Recorder::default());
		assert!(ledger.install_demand_observer(recorder.clone()));
		assert_eq!(recorder.seen(), vec![("a.dot".to_string(), WorkerTransition::Start)]);

		ledger.release(&ProductId::for_test("a.dot"));
		assert_eq!(
			recorder.seen(),
			vec![
				("a.dot".to_string(), WorkerTransition::Start),
				("a.dot".to_string(), WorkerTransition::Stop),
			]
		);
	}

	/// Nothing stays queued once the ledger falls quiet, however two threads
	/// crossing zero at the same time interleaved.
	///
	/// This covers the window between a drain finding the queue empty and
	/// giving up its claim on delivery. A transition queued inside that window
	/// finds a drain apparently still in progress, so unless the claim is
	/// dropped under the same lock that found the queue empty, nothing is left
	/// to deliver it. Later traffic would hide that by draining it late, so
	/// each round is checked while both threads are idle. The interleaving is
	/// the scheduler's, so a regression fails this often rather than always.
	#[test]
	fn nothing_stays_queued_once_the_ledger_falls_quiet() {
		const ROUNDS: usize = 5_000;
		const THREADS: usize = 2;

		let ledger = Arc::new(WorkerLedger::default());
		let counter = Arc::new(Counter::default());
		assert!(ledger.install_demand_observer(counter.clone()));

		// Both threads run each round together and the round is checked only
		// after both are done, so a transition left queued is not drained by
		// the next round's traffic before the check sees it missing.
		let start = Arc::new(std::sync::Barrier::new(THREADS + 1));
		let finished = Arc::new(std::sync::Barrier::new(THREADS + 1));
		let threads: Vec<_> = (0..THREADS)
			.map(|index| {
				let ledger = ledger.clone();
				let start = start.clone();
				let finished = finished.clone();
				std::thread::spawn(move || {
					// One product per thread, so each round owes exactly one
					// `Start` and one `Stop` per thread whatever the order.
					let product_id = ProductId::for_test(&format!("product-{index}.dot"));
					for _ in 0..ROUNDS {
						start.wait();
						ledger.acquire(&product_id);
						ledger.release(&product_id);
						finished.wait();
					}
				})
			})
			.collect();

		for round in 1..=ROUNDS {
			start.wait();
			finished.wait();
			assert_eq!(
				counter.delivered(),
				round * THREADS * 2,
				"a transition was still queued at the end of round {round}"
			);
		}
		for thread in threads {
			thread.join().expect("worker thread panicked");
		}
	}

	/// Observer that panics on its first call and records every call after.
	struct PanicsOnFirstCall {
		seen: Mutex<Vec<WorkerTransition>>,
		called_before: Mutex<bool>,
	}

	impl WorkerDemandObserver for PanicsOnFirstCall {
		fn worker_demand_changed(&self, _product_id: &ProductId, transition: WorkerTransition) {
			let mut called_before = self.called_before.lock().expect("gate mutex poisoned");
			if !*called_before {
				*called_before = true;
				drop(called_before);
				panic!("observer panics on its first call");
			}
			self.seen.lock().expect("recorder mutex poisoned").push(transition);
		}
	}

	#[test]
	fn a_panicking_delivery_still_lets_a_later_transition_through() {
		let ledger = Arc::new(WorkerLedger::default());
		let observer = Arc::new(PanicsOnFirstCall {
			seen: Mutex::new(Vec::new()),
			called_before: Mutex::new(false),
		});
		assert!(ledger.install_demand_observer(observer.clone()));

		let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			ledger.acquire(&ProductId::for_test("a.dot"));
		}));
		assert!(unwound.is_err(), "the panic should propagate to acquire");

		// A later transition still reaches the observer: the drain flag was
		// reset despite the earlier delivery unwinding.
		ledger.release(&ProductId::for_test("a.dot"));
		assert_eq!(
			*observer.seen.lock().expect("recorder mutex poisoned"),
			vec![WorkerTransition::Stop]
		);
		assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 0);
	}
}

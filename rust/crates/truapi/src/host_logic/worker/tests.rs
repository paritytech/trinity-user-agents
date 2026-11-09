use std::sync::Weak;

use super::*;

/// Observer recording every transition in delivery order.
#[derive(Default)]
struct Recorder {
	seen: Mutex<Vec<(String, WorkerTransition)>>,
	crashing: Mutex<Vec<(String, u32)>>,
}

impl Recorder {
	fn seen(&self) -> Vec<(String, WorkerTransition)> {
		self.seen.lock().expect("recorder mutex poisoned").clone()
	}
}

impl WorkerController for Recorder {
	fn request_worker_transition(&self, product_id: &ProductId, transition: WorkerTransition) {
		self.seen
			.lock()
			.expect("recorder mutex poisoned")
			.push((product_id.to_string(), transition));
	}

	fn worker_crashing(&self, product_id: &ProductId, crashes: u32) {
		self.crashing
			.lock()
			.expect("recorder mutex poisoned")
			.push((product_id.to_string(), crashes));
	}
}

fn observed() -> (WorkerLedger, Arc<Recorder>) {
	let ledger = WorkerLedger::default();
	let recorder = Arc::new(Recorder::default());
	assert!(ledger.install_worker_controller(recorder.clone()));
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
fn a_worker_that_exits_while_needed_is_started_again() {
	let (ledger, recorder) = observed();
	assert_eq!(ledger.exited(&ProductId::for_test("a.dot"), 0), None);
	assert!(recorder.seen().is_empty(), "a worker nothing needs stays stopped");
	ledger.acquire(&ProductId::for_test("a.dot"));
	ledger.acquire(&ProductId::for_test("a.dot"));
	assert_eq!(ledger.exited(&ProductId::for_test("a.dot"), 0), None, "restarted at once");
	assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 2, "the references stand");
	ledger.release(&ProductId::for_test("a.dot"));
	ledger.release(&ProductId::for_test("a.dot"));
	assert_eq!(
		recorder.seen(),
		vec![
			("a.dot".to_string(), WorkerTransition::Start),
			("a.dot".to_string(), WorkerTransition::Start),
			("a.dot".to_string(), WorkerTransition::Stop),
		]
	);
}

// A worker that keeps crashing waits longer before each restart, and the
// host hears it is not working on the run's third crash; a quiet minute
// starts a new run.
#[test]
fn a_worker_that_keeps_crashing_backs_off_and_the_host_is_told() {
	let (ledger, recorder) = observed();
	let product = ProductId::for_test("a.dot");
	ledger.acquire(&product);
	let waits: Vec<_> = (0..12).map(|n| ledger.exited(&product, n * 10)).collect();
	let restarts_before = recorder.seen().len();
	ledger.restart(&product);
	ledger.restart(&product);
	let after_quiet = ledger.exited(&product, 11 * 10 + CRASH_RUN_WINDOW_MS + 1);

	assert_eq!(
		(
			waits,
			recorder.seen().len() - restarts_before,
			after_quiet,
			recorder.crashing.lock().expect("recorder mutex poisoned").clone()
		),
		(
			vec![
				None,
				Some(1_000),
				Some(2_000),
				Some(4_000),
				Some(8_000),
				Some(16_000),
				Some(32_000),
				Some(64_000),
				Some(128_000),
				Some(256_000),
				Some(RESTART_BACKOFF_LONGEST.as_millis() as u64),
				Some(RESTART_BACKOFF_LONGEST.as_millis() as u64),
			],
			// The restart once, not twice, then the new run's at once.
			2,
			None,
			vec![("a.dot".to_string(), CRASH_NOTICE_AFTER)]
		)
	);
}

// A restart waiting out its backoff does nothing once nothing holds the
// worker, or once it was started again meanwhile.
#[test]
fn a_pending_restart_is_dropped_when_it_is_no_longer_needed() {
	let (ledger, recorder) = observed();
	let product = ProductId::for_test("a.dot");
	ledger.acquire(&product);
	ledger.exited(&product, 0);
	assert!(ledger.exited(&product, 1).is_some());
	ledger.release(&product);
	ledger.restart(&product);
	ledger.acquire(&product);
	assert!(ledger.exited(&product, 2).is_some());
	ledger.release(&product);
	ledger.acquire(&product);
	ledger.restart(&product);

	assert_eq!(
		recorder.seen(),
		vec![
			("a.dot".to_string(), WorkerTransition::Start),
			("a.dot".to_string(), WorkerTransition::Start),
			("a.dot".to_string(), WorkerTransition::Stop),
			("a.dot".to_string(), WorkerTransition::Start),
			("a.dot".to_string(), WorkerTransition::Stop),
			("a.dot".to_string(), WorkerTransition::Start),
		]
	);
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

impl WorkerController for Reentrant {
	fn request_worker_transition(&self, product_id: &ProductId, transition: WorkerTransition) {
		self.seen.lock().expect("recorder mutex poisoned").push(transition);
		{
			let mut reenter_on = self.reenter_on.lock().expect("gate mutex poisoned");
			if *reenter_on != Some(transition) {
				return;
			}
			*reenter_on = None;
		}
		let ledger = self.ledger.upgrade().expect("ledger outlives its controller");
		ledger.acquire(product_id);
		ledger.release(product_id);
	}
}

#[test]
fn a_reentrant_call_from_the_controller_does_not_deadlock() {
	let ledger = Arc::new(WorkerLedger::default());
	let controller = Arc::new(Reentrant {
		ledger: Arc::downgrade(&ledger),
		seen: Mutex::new(Vec::new()),
		reenter_on: Mutex::new(Some(WorkerTransition::Stop)),
	});
	assert!(ledger.install_worker_controller(controller.clone()));

	ledger.acquire(&ProductId::for_test("a.dot"));
	ledger.release(&ProductId::for_test("a.dot"));

	// The nested pair runs while the outer `Stop` is still on the stack, so
	// its own `Start`/`Stop` are delivered after it, by the outer drain.
	assert_eq!(
		*controller.seen.lock().expect("recorder mutex poisoned"),
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

impl WorkerController for Gated {
	fn request_worker_transition(&self, _product_id: &ProductId, transition: WorkerTransition) {
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
	let controller = Arc::new(Gated {
		seen: Mutex::new(Vec::new()),
		entered: entered_tx,
		gate: Mutex::new(Some(gate_rx)),
	});
	assert!(ledger.install_worker_controller(controller.clone()));
	let seen = || controller.seen.lock().expect("recorder mutex poisoned").clone();

	let starting = std::thread::spawn({
		let ledger = ledger.clone();
		move || ledger.acquire(&ProductId::for_test("a.dot"))
	});
	entered_rx.recv().expect("controller never ran");

	// The reference is gone and the ledger knows it, but the controller is
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

	gate_tx.send(()).expect("controller gone");
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

impl WorkerController for Counter {
	fn request_worker_transition(&self, _product_id: &ProductId, _transition: WorkerTransition) {
		*self.delivered.lock().expect("counter mutex poisoned") += 1;
	}
}

#[test]
fn a_reference_taken_before_the_controller_is_reported_once_it_arrives() {
	let ledger = WorkerLedger::default();
	// A host that counts before it listens still owns the reference, so
	// the transition has to survive the wait rather than be dropped: the
	// count never crosses zero again to produce a second one.
	ledger.acquire(&ProductId::for_test("a.dot"));

	let recorder = Arc::new(Recorder::default());
	assert!(ledger.install_worker_controller(recorder.clone()));
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
	assert!(ledger.install_worker_controller(counter.clone()));

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

impl WorkerController for PanicsOnFirstCall {
	fn request_worker_transition(&self, _product_id: &ProductId, transition: WorkerTransition) {
		let mut called_before = self.called_before.lock().expect("gate mutex poisoned");
		if !*called_before {
			*called_before = true;
			drop(called_before);
			panic!("controller panics on its first call");
		}
		self.seen.lock().expect("recorder mutex poisoned").push(transition);
	}
}

#[test]
fn a_panicking_delivery_still_lets_a_later_transition_through() {
	let ledger = Arc::new(WorkerLedger::default());
	let controller = Arc::new(PanicsOnFirstCall {
		seen: Mutex::new(Vec::new()),
		called_before: Mutex::new(false),
	});
	assert!(ledger.install_worker_controller(controller.clone()));

	let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
		ledger.acquire(&ProductId::for_test("a.dot"));
	}));
	assert!(unwound.is_err(), "the panic should propagate to acquire");

	// A later transition still reaches the controller: the drain flag was
	// reset despite the earlier delivery unwinding.
	ledger.release(&ProductId::for_test("a.dot"));
	assert_eq!(
		*controller.seen.lock().expect("recorder mutex poisoned"),
		vec![WorkerTransition::Stop]
	);
	assert_eq!(ledger.count(&ProductId::for_test("a.dot")), 0);
}

// A launch's exit is reported only while it is the live one: a stopped
// launch, or one a later launch replaced, exits quietly.
#[test]
fn only_the_live_launch_reports_its_exit() {
	let table = WorkerLaunchTable::new();
	let ramp = || ProductId::for_test("ramp.dot");
	let first = table.begin(ramp()).expect("first launch");
	let while_live = table.begin(ramp());
	let stopped = table.stop(ramp());
	let first_exit = table.ended(ramp(), first);
	let second = table.begin(ramp()).expect("second launch");
	let stale_exit = table.ended(ramp(), first);
	let second_exit = table.ended(ramp(), second);
	let again = table.ended(ramp(), second);

	assert_eq!(
		(
			while_live,
			stopped,
			first_exit,
			second > first,
			stale_exit,
			second_exit,
			again,
			table.live(ramp())
		),
		(None, Some(first), false, true, false, true, false, None)
	);
}

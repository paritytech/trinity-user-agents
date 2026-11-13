use std::{
	collections::{HashMap, VecDeque},
	sync::{
		atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
		mpsc::{Receiver, RecvTimeoutError, Sender, channel},
	},
	time::Instant,
};

use truapi::latest::GenericError;

use super::*;
use crate::{
	stack::StackRefreshOutcome,
	test_support::{
		TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, published_test_descriptor, sign_test_descriptor,
	},
	time::ManualTimer,
};

const ANSWER_WAIT: Duration = Duration::from_secs(10);

/// The time the tests start at, in seconds since the Unix epoch.
const START: u64 = 1_800_000_000;

/// A clock that moves only when a test advances it.
struct ManualClock(AtomicU64);

impl Clock for Arc<ManualClock> {
	fn unix_secs(&self) -> u64 {
		self.0.load(Ordering::SeqCst)
	}
}

fn manual_clock() -> Arc<ManualClock> {
	Arc::new(ManualClock(AtomicU64::new(START)))
}

fn published(descriptor: &serde_json::Value) -> Result<StackFetchResponse, GenericError> {
	Ok(StackFetchResponse::Document { bytes: sign_test_descriptor(descriptor).1, etag: None })
}

fn unreachable() -> Result<StackFetchResponse, GenericError> {
	Err("connection refused".to_string().into())
}

fn unreachable_because(reason: &str) -> Option<StackRefreshOutcome> {
	Some(StackRefreshOutcome::Unreachable { reason: reason.to_string() })
}

#[derive(Default)]
struct MemoryRecords {
	records: Mutex<HashMap<String, Vec<u8>>>,
	/// Whether a write fails, as a full disk makes it.
	disk_full: AtomicBool,
	/// Whether a write never finishes.
	stuck: AtomicBool,
}

#[async_trait::async_trait]
impl StackRecords for Arc<MemoryRecords> {
	fn read_stack_record(&self, stack_id: &str) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.records.lock().expect("records mutex poisoned").get(stack_id).cloned())
	}

	async fn write_stack_record(
		&self,
		stack_id: &str,
		record: Vec<u8>,
	) -> Result<(), GenericError> {
		if self.stuck.load(Ordering::SeqCst) {
			return futures::future::pending().await;
		}
		if self.disk_full.load(Ordering::SeqCst) {
			return Err("disk full".to_string().into());
		}
		self.records
			.lock()
			.expect("records mutex poisoned")
			.insert(stack_id.to_string(), record);
		Ok(())
	}
}

/// A host that answers each fetch with the next queued response, never
/// answers once the queue is empty, and passes on every status it is told.
struct QueuedHost {
	fetches: Arc<AtomicUsize>,
	responses: Mutex<VecDeque<Result<StackFetchResponse, GenericError>>>,
	statuses: Mutex<Sender<StackStatus>>,
}

#[async_trait::async_trait]
impl StackHost for QueuedHost {
	async fn fetch_stack_descriptor(
		&self,
		_request: StackFetchRequest,
	) -> Result<StackFetchResponse, GenericError> {
		self.fetches.fetch_add(1, Ordering::SeqCst);
		let response = self.responses.lock().expect("responses mutex poisoned").pop_front();
		match response {
			Some(response) => response,
			None => futures::future::pending().await,
		}
	}

	fn stack_changed(&self, status: StackStatus) {
		let _ = self.statuses.lock().expect("statuses mutex poisoned").send(status);
	}
}

struct Started {
	refresh: StackRefresh,
	fetches: Arc<AtomicUsize>,
	timer: Arc<ManualTimer>,
	clock: Arc<ManualClock>,
	statuses: Receiver<StackStatus>,
}

impl Started {
	fn next_status(&self) -> StackStatus {
		self.statuses
			.recv_timeout(ANSWER_WAIT)
			.expect("the host is told the stack's status")
	}

	/// Lets `by` pass: the clock moves, then every wait that is over ends.
	fn advance(&self, by: Duration) {
		self.clock.0.fetch_add(by.as_secs(), Ordering::SeqCst);
		self.timer.advance(by);
	}

	/// Waits until the refresh loop has started its `sleeps`-th wait.
	fn wait_for_sleeps(&self, sleeps: usize) {
		let deadline = Instant::now() + ANSWER_WAIT;
		while self.timer.sleeps() < sleeps {
			assert!(
				Instant::now() < deadline,
				"the loop started {} waits, not {sleeps}",
				self.timer.sleeps()
			);
			std::thread::yield_now();
		}
	}
}

fn load(
	records: Arc<MemoryRecords>,
	timer: Arc<ManualTimer>,
	clock: Arc<ManualClock>,
) -> StackRefresh {
	let (link, bundled) =
		sign_test_descriptor(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE));
	StackRefresh::load(&link, &bundled, false, Box::new(records), timer, Box::new(clock))
		.expect("bundled copy is valid")
}

/// A core on the bundled copy whose host answers fetches with `responses`.
fn start(
	records: Arc<MemoryRecords>,
	responses: Vec<Result<StackFetchResponse, GenericError>>,
) -> Started {
	let timer = Arc::new(ManualTimer::default());
	let clock = manual_clock();
	let refresh = load(records, timer.clone(), clock.clone());
	let (told, statuses) = channel();
	let fetches = Arc::<AtomicUsize>::default();
	let host = QueuedHost {
		fetches: fetches.clone(),
		responses: Mutex::new(responses.into()),
		statuses: Mutex::new(told),
	};
	assert!(refresh.start(Box::new(host), &Spawner::for_test()));
	Started { refresh, fetches, timer, clock, statuses }
}

/// A host that is offline at launch still runs: the stack comes from the
/// bundled copy, and the failed fetch is only reported.
#[test]
fn an_offline_host_starts_from_the_bundled_copy() {
	let started = start(Arc::default(), vec![unreachable()]);

	let at_start = started.next_status();
	let after_fetch = started.next_status();

	assert_eq!(
		(started.refresh.stack().chains().people, at_start.last_refresh, after_fetch.last_refresh),
		(TEST_STACK_PEOPLE, None, unreachable_because("connection refused"))
	);
}

/// After a wipe the operator publishes the new genesis hashes. The running
/// core tells its host to restart, and the next start runs the new chains
/// without a new build.
#[test]
fn a_newer_sequence_is_reported_and_starts_the_next_runtime() {
	let records = Arc::<MemoryRecords>::default();
	let started = start(
		records.clone(),
		vec![published(&published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE))],
	);

	let _at_start = started.next_status();
	let after_fetch = started.next_status();
	let restarted = load(records, Arc::default(), manual_clock());

	assert_eq!(
		(
			started.refresh.stack().chains().people,
			after_fetch.restart_required,
			after_fetch.generation.as_str(),
			restarted.stack().chains().people,
			restarted.status().generation.as_str(),
		),
		(TEST_STACK_PEOPLE, true, "first", TEST_STACK_WIPED_PEOPLE, "second")
	);
}

/// A restart only helps once the new copy is on disk. While the store fails
/// the host is not told to restart, and the next fetch stores the copy again.
#[test]
fn a_copy_that_could_not_be_stored_asks_for_no_restart() {
	let records = Arc::<MemoryRecords>::default();
	records.disk_full.store(true, Ordering::SeqCst);
	let wiped = published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE);
	let started = start(records.clone(), vec![published(&wiped), published(&wiped)]);
	let _at_start = started.next_status();
	let while_full = started.next_status();

	records.disk_full.store(false, Ordering::SeqCst);
	started.refresh.refresh_now();
	let once_stored = started.next_status();

	assert_eq!(
		[
			(while_full.last_refresh, while_full.restart_required),
			(once_stored.last_refresh, once_stored.restart_required),
		],
		[(Some(StackRefreshOutcome::Current), false), (Some(StackRefreshOutcome::Current), true)]
	);
}

/// A browser host's `storeStackRecord` may return a promise that never
/// settles. The loop must not wait on it forever, or no later fetch could
/// start and a wipe would never reach the host.
#[test]
fn a_store_that_never_finishes_does_not_hold_the_loop() {
	let records = Arc::<MemoryRecords>::default();
	records.stuck.store(true, Ordering::SeqCst);
	let wiped = published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE);
	let started = start(records, vec![published(&wiped)]);
	let _at_start = started.next_status();

	started.wait_for_sleeps(2);
	started.advance(STORE_TIMEOUT);
	let after_fetch = started.next_status();

	assert_eq!(
		(after_fetch.last_refresh, after_fetch.restart_required),
		(Some(StackRefreshOutcome::Current), false)
	);
}

/// A server that is down only delays a change: the core keeps running, and
/// the host's next request for a refresh picks the new copy up.
#[test]
fn a_server_that_comes_back_is_followed() {
	let started = start(
		Arc::default(),
		vec![
			unreachable(),
			published(&published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE)),
		],
	);
	let _at_start = started.next_status();
	let while_down = started.next_status();

	started.refresh.refresh_now();
	let once_back = started.next_status();

	assert_eq!(
		[
			(while_down.last_refresh, while_down.restart_required),
			(once_back.last_refresh, once_back.restart_required),
		],
		[
			(unreachable_because("connection refused"), false),
			(Some(StackRefreshOutcome::Current), true),
		]
	);
}

/// A host whose HTTP client hangs must not hold the stack's one fetch
/// forever, or no later refresh could start.
#[test]
fn a_host_that_never_answers_counts_as_unreachable() {
	let started = start(Arc::default(), vec![]);
	let _at_start = started.next_status();

	started.wait_for_sleeps(1);
	started.advance(FETCH_TIMEOUT);

	assert_eq!(started.next_status().last_refresh, unreachable_because("no answer within 30s"));
}

/// A host may ask for a refresh on every return to the foreground. Asking
/// several times during one fetch must not queue a request to the
/// descriptor's server for each.
#[test]
fn requests_made_during_a_fetch_lead_to_one_more_fetch() {
	let started = start(Arc::default(), vec![]);
	let _at_start = started.next_status();
	started.wait_for_sleeps(1);

	for _ in 0..3 {
		started.refresh.refresh_now();
	}
	started.advance(FETCH_TIMEOUT);
	// The first fetch gave up; the loop waited, woke for the requests and
	// started the second.
	started.wait_for_sleeps(3);
	started.advance(FETCH_TIMEOUT);
	started.wait_for_sleeps(4);
	std::thread::sleep(Duration::from_millis(50));

	assert_eq!((started.fetches.load(Ordering::SeqCst), started.timer.sleeps()), (2, 4));
}

/// The core reads at most 1 MiB whatever the host sends, so a host that
/// ignores `max_bytes` cannot make the core parse an unbounded body.
#[test]
fn a_document_past_the_size_cap_is_not_read() {
	let oversized = Ok(StackFetchResponse::Document {
		bytes: vec![b' '; usize::try_from(MAX_DESCRIPTOR_BYTES).expect("fits usize") + 1],
		etag: None,
	});
	let started = start(Arc::default(), vec![oversized]);
	let _at_start = started.next_status();

	assert_eq!(
		started.next_status().last_refresh,
		unreachable_because("1048577 bytes is larger than 1048576")
	);
}

/// A core that woke every minute to ask whether a fetch is due would keep a
/// phone's process busy for nothing, so the loop sleeps until the hour is up.
#[test]
fn the_loop_sleeps_until_the_next_fetch_is_due() {
	let started = start(Arc::default(), vec![unreachable()]);
	let _at_start = started.next_status();
	let _after_fetch = started.next_status();
	started.wait_for_sleeps(2);

	assert_eq!(started.timer.remaining().last(), Some(&Duration::from_secs(60 * 60)));
}

/// A stack can be wiped while the app stays open. The host asks for nothing,
/// so only the loop's own fetch, an hour after the last, brings the change.
#[test]
fn a_copy_an_hour_old_is_fetched_again() {
	let started = start(
		Arc::default(),
		vec![
			published(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE)),
			published(&published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE)),
		],
	);
	let _at_start = started.next_status();
	let before_the_wipe = started.next_status();
	// The fetch's timeout, the store's timeout, then the wait for the hour.
	started.wait_for_sleeps(3);

	started.advance(Duration::from_secs(60 * 60));
	let after_the_wipe = started.next_status();

	assert_eq!(
		(
			before_the_wipe.restart_required,
			after_the_wipe.restart_required,
			started.fetches.load(Ordering::SeqCst),
		),
		(false, true, 2)
	);
}

/// A runtime the host tears down takes its refresh with it. A loop left
/// running would keep calling into a host that is gone.
#[test]
fn dropping_the_refresh_stops_the_loop() {
	let Started { refresh, statuses, .. } = start(Arc::default(), vec![]);
	let _at_start = statuses.recv_timeout(ANSWER_WAIT);

	drop(refresh);

	// The loop owns the host, and the host the only sender of statuses.
	assert_eq!(statuses.recv_timeout(ANSWER_WAIT), Err(RecvTimeoutError::Disconnected));
}

/// Two refresh loops would fetch twice and report every change twice.
#[test]
fn the_refresh_starts_once() {
	let started = start(Arc::default(), vec![]);
	let (told, _statuses) = channel();
	let second = QueuedHost {
		fetches: Arc::default(),
		responses: Mutex::default(),
		statuses: Mutex::new(told),
	};

	assert!(!started.refresh.start(Box::new(second), &Spawner::for_test()));
}

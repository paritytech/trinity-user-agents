use std::{
	collections::{HashMap, VecDeque},
	sync::{
		atomic::{AtomicUsize, Ordering},
		mpsc::{Receiver, Sender, channel},
	},
};

use truapi::latest::GenericError;

use super::*;
use crate::{
	stack::{
		StackRefreshOutcome,
		fixtures::{PEOPLE, WIPED_PEOPLE},
	},
	test_support::{published_test_descriptor, sign_test_descriptor},
	time::ManualTimer,
};

const ANSWER_WAIT: Duration = Duration::from_secs(10);

fn published(descriptor: &serde_json::Value) -> StackFetchResponse {
	StackFetchResponse::Document { bytes: sign_test_descriptor(descriptor).1, etag: None }
}

fn unreachable() -> StackFetchResponse {
	StackFetchResponse::Unreachable { reason: "connection refused".to_string() }
}

#[derive(Default)]
struct MemoryRecords(Mutex<HashMap<String, Vec<u8>>>);

#[async_trait::async_trait]
impl StackRecords for MemoryRecords {
	fn read_stack_record(&self, stack_id: &str) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.0.lock().expect("records mutex poisoned").get(stack_id).cloned())
	}

	async fn write_stack_record(
		&self,
		stack_id: &str,
		record: Vec<u8>,
	) -> Result<(), GenericError> {
		self.0
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
	responses: Mutex<VecDeque<StackFetchResponse>>,
	statuses: Mutex<Sender<StackStatus>>,
}

#[async_trait::async_trait]
impl StackHost for QueuedHost {
	async fn fetch_stack_descriptor(&self, _request: StackFetchRequest) -> StackFetchResponse {
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
	statuses: Receiver<StackStatus>,
}

impl Started {
	fn next_status(&self) -> StackStatus {
		self.statuses
			.recv_timeout(ANSWER_WAIT)
			.expect("the host is told the stack's status")
	}

	/// Waits until the refresh loop has started its `sleeps`-th wait.
	fn wait_for_sleeps(&self, sleeps: usize) {
		while self.timer.sleeps() < sleeps {
			std::thread::yield_now();
		}
	}
}

fn load(records: Arc<MemoryRecords>, timer: Arc<ManualTimer>) -> StackRefresh {
	let (link, bundled) = sign_test_descriptor(&published_test_descriptor(1, "first", PEOPLE));
	StackRefresh::load(&link, &bundled, false, records, timer).expect("bundled copy is valid")
}

/// A core on the bundled copy whose host answers fetches with `responses`.
fn start(records: Arc<MemoryRecords>, responses: Vec<StackFetchResponse>) -> Started {
	let timer = Arc::new(ManualTimer::default());
	let refresh = load(records, timer.clone());
	let (told, statuses) = channel();
	let fetches = Arc::<AtomicUsize>::default();
	let host = QueuedHost {
		fetches: fetches.clone(),
		responses: Mutex::new(responses.into()),
		statuses: Mutex::new(told),
	};
	assert!(refresh.start(Arc::new(host), &Spawner::for_test()));
	Started { refresh, fetches, timer, statuses }
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
		(PEOPLE, None, Some(StackRefreshOutcome::Unreachable))
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
		vec![published(&published_test_descriptor(2, "second", WIPED_PEOPLE))],
	);

	let _at_start = started.next_status();
	let after_fetch = started.next_status();

	assert_eq!(
		(
			started.refresh.stack().chains().people,
			after_fetch.restart_required,
			after_fetch.new_generation,
			load(records, Arc::default()).stack().chains().people,
		),
		(PEOPLE, true, true, WIPED_PEOPLE)
	);
}

/// A server that is down only delays a change: the core keeps running, and
/// the host's next request for a refresh picks the new copy up.
#[test]
fn a_server_that_comes_back_is_followed() {
	let started = start(
		Arc::default(),
		vec![unreachable(), published(&published_test_descriptor(2, "second", WIPED_PEOPLE))],
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
			(Some(StackRefreshOutcome::Unreachable), false),
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
	started.timer.advance(FETCH_TIMEOUT);

	assert_eq!(started.next_status().last_refresh, Some(StackRefreshOutcome::Unreachable));
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
	started.timer.advance(FETCH_TIMEOUT);
	// The first fetch gave up; the loop waited, woke for the requests and
	// started the second.
	started.wait_for_sleeps(3);
	started.timer.advance(FETCH_TIMEOUT);
	started.wait_for_sleeps(4);
	std::thread::sleep(Duration::from_millis(50));

	assert_eq!((started.fetches.load(Ordering::SeqCst), started.timer.sleeps()), (2, 4));
}

/// The core reads at most 1 MiB whatever the host sends, so a host that
/// ignores `max_bytes` cannot make the core parse an unbounded body.
#[test]
fn a_document_past_the_size_cap_is_not_read() {
	let oversized = StackFetchResponse::Document {
		bytes: vec![b' '; usize::try_from(MAX_DESCRIPTOR_BYTES).expect("fits usize") + 1],
		etag: None,
	};
	let started = start(Arc::default(), vec![oversized]);
	let _at_start = started.next_status();

	assert_eq!(started.next_status().last_refresh, Some(StackRefreshOutcome::Unreachable));
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

	assert!(!started.refresh.start(Arc::new(second), &Spawner::for_test()));
}

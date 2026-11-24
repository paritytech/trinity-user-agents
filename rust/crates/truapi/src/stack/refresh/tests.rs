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
	stack::{ReplaceCheck, StackRefreshOutcome},
	test_support::{
		TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, on_testnet, published_test_descriptor,
		published_test_descriptor_at, redeployed_reserved_test_descriptor, refused_for_its_tld,
		reserved_test_descriptor, sign_test_descriptor, stack_status, stored_test_record,
		without_bulletin,
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

/// `descriptor` served with an `ETag`, which the core keeps with the copy.
fn published_with_etag(descriptor: &serde_json::Value) -> Result<StackFetchResponse, GenericError> {
	Ok(StackFetchResponse::Document {
		bytes: sign_test_descriptor(descriptor).1,
		etag: Some("\"held\"".to_string()),
	})
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
	/// Whether a read never finishes.
	read_stuck: AtomicBool,
}

#[async_trait::async_trait]
impl StackRecords for Arc<MemoryRecords> {
	async fn read_stack_record(&self, stack_id: &str) -> Result<Option<Vec<u8>>, GenericError> {
		if self.read_stuck.load(Ordering::SeqCst) {
			return futures::future::pending().await;
		}
		Ok(self.records.lock().expect("records mutex poisoned").get(stack_id).cloned())
	}

	async fn write_stack_record(
		&self,
		stack_id: &str,
		record: Vec<u8>,
		check: ReplaceCheck,
	) -> Result<(), GenericError> {
		if self.stuck.load(Ordering::SeqCst) {
			return futures::future::pending().await;
		}
		if self.disk_full.load(Ordering::SeqCst) {
			return Err("disk full".to_string().into());
		}
		let mut records = self.records.lock().expect("records mutex poisoned");
		if records.get(stack_id).is_none_or(|existing| check.replaces(existing)) {
			records.insert(stack_id.to_string(), record);
		}
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

/// The test stack's descriptor as bundled.
fn bundled() -> serde_json::Value {
	published_test_descriptor_at(1)
}

/// The test stack's `stack_id`, which its record is stored under.
fn stack_id() -> String {
	let (link, _) = sign_test_descriptor(&bundled());
	signed_pointer(&link).expect("test link carries a key").stack_id()
}

/// A store that holds `record` for the test stack.
fn holding(record: Vec<u8>) -> Arc<MemoryRecords> {
	let records = Arc::<MemoryRecords>::default();
	records
		.records
		.lock()
		.expect("records mutex poisoned")
		.insert(stack_id(), record);
	records
}

/// The record `records` holds for the test stack.
fn stored(records: &MemoryRecords) -> Option<Vec<u8>> {
	records
		.records
		.lock()
		.expect("records mutex poisoned")
		.get(&stack_id())
		.cloned()
}

fn load(
	records: Arc<MemoryRecords>,
	timer: Arc<ManualTimer>,
	clock: Arc<ManualClock>,
) -> StackRefresh {
	load_stack(&bundled(), ReservedValuesPolicy::default(), records, timer, clock)
}

/// The refresh of a build that bundles `bundled` under `reserved_values`.
fn load_stack(
	bundled: &serde_json::Value,
	reserved_values: ReservedValuesPolicy,
	records: Arc<MemoryRecords>,
	timer: Arc<ManualTimer>,
	clock: Arc<ManualClock>,
) -> StackRefresh {
	let (link, bundled) = sign_test_descriptor(bundled);
	futures::executor::block_on(StackRefresh::load(
		&link,
		&bundled,
		reserved_values,
		Box::new(records),
		timer,
		Box::new(clock),
	))
	.expect("bundled copy is valid")
}

/// A core on the bundled copy whose host answers fetches with `responses`.
fn start(
	records: Arc<MemoryRecords>,
	responses: Vec<Result<StackFetchResponse, GenericError>>,
) -> Started {
	start_stack(&bundled(), ReservedValuesPolicy::default(), records, responses)
}

/// A core of a build that bundles `bundled` under `reserved_values`, whose
/// host answers fetches with `responses`.
fn start_stack(
	bundled: &serde_json::Value,
	reserved_values: ReservedValuesPolicy,
	records: Arc<MemoryRecords>,
	responses: Vec<Result<StackFetchResponse, GenericError>>,
) -> Started {
	let timer = Arc::new(ManualTimer::default());
	let clock = manual_clock();
	let refresh = load_stack(bundled, reserved_values, records, timer.clone(), clock.clone());
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

/// The policy is the build's own and holds for every copy it will run, so a
/// fault in it is reported before the link or the bundled copy is read.
#[test]
fn a_policy_fault_is_reported_before_the_link() {
	let policy = ReservedValuesPolicy {
		extra_tlds: vec!["kusama".to_string()],
		..ReservedValuesPolicy::default()
	};

	let refused = futures::executor::block_on(StackRefresh::load(
		"not a stack link",
		b"not a descriptor",
		policy,
		Box::new(Arc::<MemoryRecords>::default()),
		Arc::new(ManualTimer::default()),
		Box::new(manual_clock()),
	));

	assert_eq!(
		refused.err(),
		Some(StackError::UnsupportedReservedTld { tld: "kusama".to_string() })
	);
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
		[
			(Some(StackRefreshOutcome::NotStored { reason: "disk full".to_string() }), false),
			(Some(StackRefreshOutcome::Current), true),
		]
	);
}

/// A stolen key must not move a stack to another TLD. The host sees the copy
/// refused, nothing is stored, and the next start runs the same stack.
#[test]
fn a_fetched_copy_on_another_tld_is_refused_and_not_stored() {
	let records = Arc::<MemoryRecords>::default();
	let started =
		start(records.clone(), vec![published(&on_testnet(published_test_descriptor_at(2)))]);

	let at_start = started.next_status();
	let after_fetch = started.next_status();
	let restarted = load(records.clone(), Arc::default(), manual_clock());

	assert_eq!(
		(
			[at_start, after_fetch, restarted.status()],
			[started.refresh.stack().tld(), restarted.stack().tld()],
			stored(&records),
		),
		(
			[
				stack_status(&bundled(), None),
				stack_status(&bundled(), refused_for_its_tld()),
				stack_status(&bundled(), None)
			],
			["paseo", "paseo"],
			None
		)
	);
}

/// A refused or failed fetch rewrites the record without its `ETag`, still
/// holding the copy the core runs. Whether that write works or not, the host
/// sees the fetch's outcome and the next start runs the same copy.
#[test]
fn a_refused_or_failed_fetch_is_reported_as_such_whatever_the_store_does() {
	let held = published_test_descriptor_at(2);
	let moved = on_testnet(published_test_descriptor_at(3));
	let fetches = [(published(&moved), true), (published(&moved), false), (unreachable(), true)];

	let told_and_stored = fetches.map(|(answer, disk_full)| {
		let records = Arc::<MemoryRecords>::default();
		let started = start(records.clone(), vec![published_with_etag(&held), answer]);
		let _at_start = started.next_status();
		let _held = started.next_status();
		records.disk_full.store(disk_full, Ordering::SeqCst);
		started.refresh.refresh_now();
		let after_fetch = started.next_status();
		records.disk_full.store(false, Ordering::SeqCst);
		(after_fetch, load(records, Arc::default(), manual_clock()).status())
	});

	let refused = (stack_status(&held, refused_for_its_tld()), stack_status(&held, None));
	let unreachable =
		(stack_status(&held, unreachable_because("connection refused")), stack_status(&held, None));
	assert_eq!(told_and_stored, [refused.clone(), refused, unreachable]);
}

/// A newer copy whose store failed is stored by the record the next failed
/// fetch writes. The host is told to restart, the fetch is reported as
/// failed, and the next start runs the newer copy.
#[test]
fn a_copy_whose_store_failed_is_stored_when_the_next_fetch_fails() {
	let wiped = published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE);
	let records = Arc::<MemoryRecords>::default();
	records.disk_full.store(true, Ordering::SeqCst);
	let started = start(records.clone(), vec![published_with_etag(&wiped), unreachable()]);
	let _at_start = started.next_status();
	let while_full = started.next_status();

	records.disk_full.store(false, Ordering::SeqCst);
	started.refresh.refresh_now();
	let once_stored = started.next_status();
	let restarted = load(records, Arc::default(), manual_clock());

	assert_eq!(
		[while_full, once_stored, restarted.status()],
		[
			stack_status(
				&bundled(),
				Some(StackRefreshOutcome::NotStored { reason: "disk full".to_string() })
			),
			StackStatus {
				restart_required: true,
				..stack_status(&bundled(), unreachable_because("connection refused"))
			},
			stack_status(&wiped, None),
		]
	);
}

/// A record from another build of the app may hold a copy on another TLD.
/// The host starts on the bundled copy and refuses the published one. The
/// record stays until the core takes a copy, which the next start runs.
#[test]
fn a_stored_copy_on_another_tld_is_ignored_until_a_copy_the_core_takes_replaces_it() {
	let moved = on_testnet(published_test_descriptor_at(2));
	let corrected = published_test_descriptor_at(3);
	let records = holding(stored_test_record(&moved, START.cast_signed()));
	let started = start(records.clone(), vec![published(&moved), published(&corrected)]);

	let at_start = started.next_status();
	let after_fetch = started.next_status();
	let left_in_place = stored(&records);
	started.refresh.refresh_now();
	let once_corrected = started.next_status();
	let restarted = load(records, Arc::default(), manual_clock());

	assert_eq!(
		([at_start, after_fetch, once_corrected, restarted.status()], left_in_place),
		(
			[
				stack_status(&bundled(), None),
				stack_status(&bundled(), refused_for_its_tld()),
				stack_status(&corrected, Some(StackRefreshOutcome::Current)),
				stack_status(&corrected, None)
			],
			Some(stored_test_record(&moved, START.cast_signed()))
		)
	);
}

/// A record the core ignores, on another TLD or one it cannot run, may hold a
/// higher `sequence` than the published copy. It must not keep that copy out
/// of the store, or every start would ask for another restart.
#[test]
fn an_ignored_record_with_a_higher_sequence_does_not_keep_a_newer_copy_out() {
	let wiped = published_test_descriptor(3, "second", TEST_STACK_WIPED_PEOPLE);
	let ignored = [
		on_testnet(published_test_descriptor_at(5)),
		without_bulletin(published_test_descriptor_at(5)),
	];

	let told = ignored.map(|ignored| {
		let records = holding(stored_test_record(&ignored, START.cast_signed()));
		let started = start(records.clone(), vec![published(&wiped)]);
		let at_start = started.next_status();
		let after_fetch = started.next_status();
		let rebuilt = start(records, vec![published(&wiped)]);
		let rebuilt_at_start = rebuilt.next_status();
		rebuilt.refresh.refresh_now();
		[at_start, after_fetch, rebuilt_at_start, rebuilt.next_status()]
	});

	let follows_the_wipe = [
		stack_status(&bundled(), None),
		StackStatus {
			restart_required: true,
			..stack_status(&bundled(), Some(StackRefreshOutcome::Current))
		},
		stack_status(&wiped, None),
		stack_status(&wiped, Some(StackRefreshOutcome::Current)),
	];
	assert_eq!(told, [follows_the_wipe.clone(), follows_the_wipe]);
}

/// On a stack allowed reserved values the core also ignores a record that
/// changes a field the bundled copy pins. With a higher `sequence` than the
/// published copy it must not keep that copy out of the store either.
#[test]
fn a_record_that_changes_a_reserved_pin_does_not_keep_a_newer_copy_out() {
	let bundled = reserved_test_descriptor(1);
	let mut regenerated = reserved_test_descriptor(3);
	regenerated["generation"] = "second".into();
	let records = holding(stored_test_record(&redeployed_reserved_test_descriptor(5), 0));
	let start = || {
		let published = vec![published(&regenerated)];
		start_stack(&bundled, ReservedValuesPolicy::ALLOWED, records.clone(), published)
	};

	let started = start();
	let at_start = started.next_status();
	let after_fetch = started.next_status();
	let rebuilt = start();
	let rebuilt_at_start = rebuilt.next_status();
	rebuilt.refresh.refresh_now();

	assert_eq!(
		[at_start, after_fetch, rebuilt_at_start, rebuilt.next_status()],
		[
			stack_status(&bundled, None),
			StackStatus {
				restart_required: true,
				..stack_status(&bundled, Some(StackRefreshOutcome::Current))
			},
			stack_status(&regenerated, None),
			stack_status(&regenerated, Some(StackRefreshOutcome::Current)),
		]
	);
}

/// A browser host's storage may never answer. Building the runtime must not
/// wait on it for ever: the core starts from the bundled copy.
#[test]
fn a_record_read_that_never_finishes_starts_from_the_bundled_copy() {
	let records = Arc::<MemoryRecords>::default();
	records.read_stuck.store(true, Ordering::SeqCst);
	let timer = Arc::new(ManualTimer::default());
	let (link, bundled) =
		sign_test_descriptor(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE));
	let mut load = Box::pin(StackRefresh::load(
		&link,
		&bundled,
		ReservedValuesPolicy::default(),
		Box::new(records),
		timer.clone(),
		Box::new(manual_clock()),
	));

	assert!(load.as_mut().now_or_never().is_none(), "the load waits on the read");
	timer.advance(RECORD_READ_TIMEOUT);
	let refresh = load
		.now_or_never()
		.expect("the load finishes once the wait is over")
		.expect("the bundled copy is valid");

	assert_eq!(refresh.stack().chains().people, TEST_STACK_PEOPLE);
}

/// A browser core stores the record through the host, whose promise may
/// never settle. The loop must not wait on it forever, or no later fetch could
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
		(
			Some(StackRefreshOutcome::NotStored { reason: "not stored within 30s".to_string() }),
			false
		)
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

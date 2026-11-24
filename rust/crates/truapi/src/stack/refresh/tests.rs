use std::{
	collections::{HashMap, VecDeque},
	sync::{
		atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
		mpsc::{Receiver, RecvTimeoutError, Sender, channel},
	},
	time::Instant,
};

use parity_scale_codec::Encode;
use sp_crypto_hashing::twox_128;
use truapi::{CallContext, latest::GenericError};

use super::*;
use crate::{
	host_logic::root_entropy::RootEntropy,
	platform::{HostInfo, ProductContext, ProductId},
	runtime::{
		HostRole, LocalActivation, SigningHost, StatementRenewalTarget,
		services::RuntimeServices,
		statement_allowance::{StatementAllowanceError, slot::SlotError},
	},
	stack::{ReplaceCheck, StackMismatch, StackRefreshOutcome},
	test_support::{
		StubPlatform, TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, on_testnet,
		published_test_descriptor, published_test_descriptor_at, recorded_rpc_method_count,
		refused_for_its_tld, sign_test_descriptor, stack_status, stored_test_record, wait_until,
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

	/// Waits until the host was asked for its `fetches`-th fetch.
	fn wait_for_fetches(&self, fetches: usize) {
		let asked = || self.fetches.load(Ordering::SeqCst) >= fetches;
		wait_until(asked, "the host was not asked for the fetch");
	}

	/// A connection for People reaches a chain that restarted. Returns the
	/// status the host is told, once the loop has gone on to its next wait.
	fn people_restarted(&self) -> StackStatus {
		people_answers(&self.refresh, TEST_STACK_WIPED_PEOPLE);
		let told = self.next_status();
		self.wait_for_sleeps(4);
		told
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
	let (link, bundled) = sign_test_descriptor(&bundled());
	futures::executor::block_on(StackRefresh::load(
		&link,
		&bundled,
		false,
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
	let timer = Arc::new(ManualTimer::default());
	let clock = manual_clock();
	let refresh = load(records, timer.clone(), clock.clone());
	start_loaded(refresh, timer, clock, responses)
}

/// A core that has made its first fetch, answered with the first of
/// `responses`. The host was told the status at start and after the fetch,
/// and the loop waits for the copy to be an hour old.
fn start_fetched(
	records: Arc<MemoryRecords>,
	responses: Vec<Result<StackFetchResponse, GenericError>>,
) -> Started {
	let started = start(records, responses);
	let _at_start = started.next_status();
	let _after_the_fetch = started.next_status();
	// The fetch's timeout, the store's timeout, then the wait for the hour.
	started.wait_for_sleeps(3);
	started
}

/// Starts `refresh` with a host that answers fetches with `responses`.
fn start_loaded(
	refresh: StackRefresh,
	timer: Arc<ManualTimer>,
	clock: Arc<ManualClock>,
	responses: Vec<Result<StackFetchResponse, GenericError>>,
) -> Started {
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

/// A connection opened for People reached a chain with the genesis hash
/// `answered`.
fn people_answers(refresh: &StackRefresh, answered: [u8; 32]) {
	let stack = refresh.stack();
	let checks = crate::stack::checks(&stack).expect("a refreshed stack has its chains checked");
	checks
		.genesis_check()
		.chain_answered(GenesisHashes { named: TEST_STACK_PEOPLE, answered });
}

fn people_gave(answered: [u8; 32]) -> Vec<StackMismatch> {
	vec![StackMismatch::GenesisHash {
		role: "People".to_string(),
		named: TEST_STACK_PEOPLE,
		answered,
	}]
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
		false,
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

const FIVE_MINUTES: Duration = Duration::from_secs(5 * 60);

/// A stack can be wiped while the app stays open, and the hour until the next
/// fetch is a long time to keep a chain out of use. The host hears of the
/// wrong chain at once, before any fetch, and the descriptor is fetched once
/// the copy is five minutes old.
#[test]
fn a_chain_that_restarted_is_reported_at_once_and_brings_the_fetch_forward() {
	let started = start_fetched(
		Arc::default(),
		vec![
			published(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE)),
			published(&published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE)),
		],
	);
	let before_the_wipe = started.refresh.status();

	let once_told = started.people_restarted();
	let wait_for_the_fetch = started.timer.remaining().last().copied();
	started.advance(FIVE_MINUTES);
	let after_the_fetch = started.next_status();

	assert_eq!(
		(
			before_the_wipe.mismatches,
			(once_told.mismatches, once_told.restart_required),
			wait_for_the_fetch,
			(after_the_fetch.mismatches, after_the_fetch.restart_required),
			started.fetches.load(Ordering::SeqCst),
		),
		(
			Vec::new(),
			(people_gave(TEST_STACK_WIPED_PEOPLE), false),
			Some(FIVE_MINUTES),
			(people_gave(TEST_STACK_WIPED_PEOPLE), true),
			2,
		)
	);
}

/// A host's HTTP client may take half a minute to give up, and calls to the
/// chain fail meanwhile. The host is told why before the fetch the wrong
/// answer brings forward, not after it.
#[test]
fn a_chain_that_restarted_is_reported_before_the_fetch_it_brings_forward() {
	let unchanged = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let started = start_fetched(Arc::default(), vec![published(&unchanged)]);
	started.clock.0.fetch_add(FIVE_MINUTES.as_secs(), Ordering::SeqCst);

	people_answers(&started.refresh, TEST_STACK_WIPED_PEOPLE);
	let once_told = started.next_status();
	// The host never answers this fetch, so its timeout is the next wait.
	started.wait_for_fetches(2);
	started.wait_for_sleeps(4);

	assert_eq!(
		(once_told.mismatches, started.timer.remaining().last()),
		(people_gave(TEST_STACK_WIPED_PEOPLE), Some(&FETCH_TIMEOUT))
	);
}

/// At start the loop is fetching while the core's services open their first
/// connections. A chain found wrong then is reported while the fetch is still
/// out, not once a hung fetch has timed out.
#[test]
fn a_chain_found_wrong_during_a_fetch_is_reported_while_it_is_out() {
	let started = start(Arc::default(), vec![]);
	let _at_start = started.next_status();
	started.wait_for_fetches(1);

	people_answers(&started.refresh, TEST_STACK_WIPED_PEOPLE);
	let during_the_fetch = started.next_status();

	assert_eq!(
		(during_the_fetch.mismatches, during_the_fetch.last_refresh),
		(people_gave(TEST_STACK_WIPED_PEOPLE), None)
	);
}

/// An operator may publish the new descriptor some time after the wipe. The
/// first fetch then brings nothing new. The loop keeps asking every five
/// minutes, or the chain would stay out of use until the hourly fetch.
#[test]
fn a_restarted_chain_the_descriptor_does_not_follow_is_asked_about_every_five_minutes() {
	let unchanged = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let responses = vec![published(&unchanged), published(&unchanged)];
	let started = start_fetched(Arc::default(), responses);
	started.people_restarted();

	started.advance(FIVE_MINUTES);
	// The second fetch's timeout, its store's timeout, then the next wait.
	started.wait_for_sleeps(7);

	assert_eq!(
		(started.fetches.load(Ordering::SeqCst), started.timer.remaining().last()),
		(2, Some(&FIVE_MINUTES))
	);
}

/// One node behind an endpoint may lag a wipe while the others do not. The
/// host stops showing the chain as wrong once a connection reaches the one
/// the descriptor names. That costs the descriptor's server no request, and
/// the next one is an hour after the last again.
#[test]
fn the_host_is_told_when_a_chain_answers_as_named_again() {
	let unchanged = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let started = start_fetched(Arc::default(), vec![published(&unchanged)]);
	started.people_restarted();

	people_answers(&started.refresh, TEST_STACK_PEOPLE);
	let once_right = started.next_status();
	started.wait_for_sleeps(5);

	assert_eq!(
		(
			once_right.mismatches,
			started.fetches.load(Ordering::SeqCst),
			started.timer.remaining().last(),
		),
		(Vec::new(), 1, Some(&Duration::from_secs(60 * 60)))
	);
}

/// A host installs its callbacks after the runtime is built, and by then a
/// service of the core may have reached a wiped chain. The stored copy is
/// well under an hour old, so only the wrong answer makes the refresh fetch
/// as it starts.
#[test]
fn a_wrong_answer_before_the_refresh_starts_brings_its_first_fetch_forward() {
	let records = Arc::<MemoryRecords>::default();
	let unchanged = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let first_run = start_fetched(records.clone(), vec![published(&unchanged)]);
	first_run.advance(FIVE_MINUTES);
	let Started { clock, .. } = first_run;
	let timer = Arc::new(ManualTimer::default());
	let refresh = load(records, timer.clone(), clock.clone());

	people_answers(&refresh, TEST_STACK_WIPED_PEOPLE);
	let second_run = start_loaded(
		refresh,
		timer,
		clock,
		vec![published(&published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE))],
	);
	let at_start = second_run.next_status();
	let after_fetch = second_run.next_status();

	assert_eq!(
		(
			at_start.mismatches,
			at_start.restart_required,
			after_fetch.restart_required,
			second_run.fetches.load(Ordering::SeqCst),
		),
		(people_gave(TEST_STACK_WIPED_PEOPLE), false, true, 1)
	);
}

/// A host that does not fetch the descriptor still has to keep its own
/// features off a chain that restarted. It is told the status at start, and
/// again when a chain's answer changes it.
#[test]
fn a_host_that_does_not_fetch_is_told_when_a_chain_restarted() {
	let refresh = load(Arc::default(), Arc::default(), manual_clock());
	let (told, statuses) = channel();
	let tell = move |status: &StackStatus| {
		let _ = told.send(status.clone());
	};

	let started = refresh.start_reporting(Box::new(tell), &Spawner::for_test());
	let at_start = statuses.try_recv().expect("the host is told before the call returns");
	people_answers(&refresh, TEST_STACK_WIPED_PEOPLE);
	let once_answered = statuses
		.recv_timeout(ANSWER_WAIT)
		.expect("the host is told what the chain answered");

	assert_eq!(
		(started, at_start.mismatches, once_answered.mismatches),
		(true, vec![], people_gave(TEST_STACK_WIPED_PEOPLE))
	);
}

/// One loop tells the host. A refresh started beside the reports, or the
/// reports beside a refresh, would tell it every change twice.
#[test]
fn a_stack_reports_or_refreshes_but_not_both() {
	let reporting = load(Arc::default(), Arc::default(), manual_clock());
	let reports = reporting.start_reporting(Box::new(|_| ()), &Spawner::for_test());
	let (told, _statuses) = channel();
	let host = QueuedHost {
		fetches: Arc::default(),
		responses: Mutex::default(),
		statuses: Mutex::new(told),
	};
	let refreshing = start(Arc::default(), vec![]);

	let refresh_too = reporting.start(Box::new(host), &Spawner::for_test());
	let reports_too = refreshing.refresh.start_reporting(Box::new(|_| ()), &Spawner::for_test());

	assert_eq!((reports, refresh_too, reports_too), (true, false, false));
}

/// A runtime the host tears down takes its reports with it. A loop left
/// running would keep calling into a host that is gone.
#[test]
fn dropping_the_refresh_stops_the_reports() {
	let refresh = load(Arc::default(), Arc::default(), manual_clock());
	let (told, statuses) = channel();
	let tell = move |status: &StackStatus| {
		let _ = told.send(status.clone());
	};
	assert!(refresh.start_reporting(Box::new(tell), &Spawner::for_test()));
	let _at_start = statuses.try_recv();

	drop(refresh);

	// The loop owns `tell`, the only sender of statuses.
	assert_eq!(statuses.recv_timeout(ANSWER_WAIT), Err(RecvTimeoutError::Disconnected));
}

/// Runs `work` to the end, failing the test instead of hanging when it has not
/// finished within [`ANSWER_WAIT`].
fn within_answer_wait<T>(work: impl Future<Output = T>) -> T {
	let work = core::pin::pin!(work);
	match futures::executor::block_on(select(work, futures_timer::Delay::new(ANSWER_WAIT))) {
		Either::Left((done, _)) => done,
		Either::Right(_) => panic!("not finished within {}s", ANSWER_WAIT.as_secs()),
	}
}

/// The People metadata an allowance reads before People's suffix.
const PEOPLE_METADATA: &[u8] =
	include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

/// The Asset Hub of [`published_test_descriptor`].
const TEST_STACK_ASSET_HUB: [u8; 32] = [0xcc; 32];

/// A runtime on the stack `refresh` hands out, over nodes that each name the
/// chain they were dialled for, apart from People's, which names `genesis`.
/// People gives `suffix` as its `NetworkSuffix`, and each node serves the
/// runtime version and People metadata an allowance reads first. Any other
/// storage is empty, and any other request gets a JSON-RPC error. Asset Hub is
/// out of reach, so activating a wallet looks up no username.
fn runtime_on(
	refresh: &StackRefresh,
	genesis: [u8; 32],
	suffix: &'static str,
) -> (Arc<RuntimeServices>, Arc<StubPlatform>) {
	let suffix_key = [twox_128(b"NetworkSuffix"), twox_128(b"NetworkSuffix")].concat();
	let suffix_key = format!("0x{}", hex::encode(suffix_key));
	let node = Arc::new(StubPlatform {
		chain_rpc_responder: Some(Arc::new(move |chain, request| {
			let request: serde_json::Value =
				serde_json::from_str(request).expect("the core sends JSON-RPC");
			let people = chain == TEST_STACK_PEOPLE;
			let named = if people { genesis } else { chain };
			let as_hex = |bytes: &[u8]| serde_json::json!(format!("0x{}", hex::encode(bytes)));
			let answer = match (request["method"].as_str(), request["params"][0].as_str()) {
				(Some("chainSpec_v1_genesisHash" | "chain_getBlockHash"), _) => Ok(as_hex(&named)),
				(Some("state_getRuntimeVersion"), _) => {
					Ok(serde_json::json!({"specVersion": 1, "transactionVersion": 1}))
				},
				(Some("state_call"), Some("Metadata_metadata_at_version")) => {
					Ok(as_hex(&Some(PEOPLE_METADATA.to_vec()).encode()))
				},
				(Some("state_getStorage"), Some(key)) if people && key == suffix_key => {
					Ok(as_hex(&suffix.as_bytes().to_vec().encode()))
				},
				(Some("state_getStorage"), _) => Ok(serde_json::Value::Null),
				(method, _) => Err(format!("the node serves no {method:?}")),
			};
			// An error, not a panic: a panic would end the node's answers, and
			// the call waiting on one would never finish.
			let frame = match answer {
				Ok(answer) => serde_json::json!({
					"jsonrpc": "2.0",
					"id": request["id"],
					"result": answer,
				}),
				Err(message) => serde_json::json!({
					"jsonrpc": "2.0",
					"id": request["id"],
					"error": {"code": -32601, "message": message},
				}),
			};
			vec![frame.to_string()]
		})),
		unreachable_genesis: Some(TEST_STACK_ASSET_HUB),
		..Default::default()
	});
	let services = RuntimeServices::new(
		node.clone(),
		HostInfo {
			name: "Stack test".into(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		refresh.stack(),
		Spawner::for_test(),
	);
	(services, node)
}

/// The checks reach the chains only through the stack the refresh hands out.
/// A runtime built on that stack has to put the connections it opens through
/// them, or a wiped chain would be read as the one the descriptor names.
#[test]
fn a_runtime_on_the_refreshed_stack_refuses_a_chain_that_restarted() {
	let refresh = load(Arc::default(), Arc::default(), manual_clock());
	let (services, _node) = runtime_on(&refresh, TEST_STACK_WIPED_PEOPLE, "paseo");

	let people = within_answer_wait(services.chain.rpc_client("test", &TEST_STACK_PEOPLE));

	assert!(people.is_err(), "the restarted chain is not handed out");
	assert_eq!(refresh.status().mismatches, people_gave(TEST_STACK_WIPED_PEOPLE));
}

/// A signing host's allowance work builds its proof contexts with People's
/// suffix. The suffix that work reads is what the host is told about, at no
/// extra request to People, and the work goes on with the suffix People gave.
#[test]
fn a_runtime_on_the_refreshed_stack_reports_a_people_suffix_that_is_not_the_tld() {
	let refresh = load(Arc::default(), Arc::default(), manual_clock());
	let (services, node) = runtime_on(&refresh, TEST_STACK_PEOPLE, "testnet");

	let read = within_answer_wait(async {
		let people = services.statement_store.chain_client("test").await.ok()?;
		services.people_network_suffix(people.rpc()).await.ok()
	});

	assert_eq!(
		(
			read,
			refresh.status().mismatches,
			recorded_rpc_method_count(&node.sent_rpc, "state_getStorage"),
		),
		(
			Some(b"testnet".to_vec()),
			vec![StackMismatch::NetworkSuffix {
				named: "paseo".to_string(),
				answered: "testnet".to_string(),
			}],
			1,
		)
	);
}

/// What the host is told once People has given `testnet` as its suffix.
fn testnet_suffix_listed() -> Vec<StackMismatch> {
	vec![StackMismatch::NetworkSuffix {
		named: "paseo".to_string(),
		answered: "testnet".to_string(),
	}]
}

/// A People chain that holds no suffix of one to sixteen bytes fails the
/// allowance work with that error. The failed read says nothing about the
/// suffix People holds, so a suffix listed before stays listed.
#[test]
fn a_people_suffix_that_cannot_be_read_leaves_the_listing_as_it_was() {
	let refresh = load(Arc::default(), Arc::default(), manual_clock());
	let (services, _node) = runtime_on(&refresh, TEST_STACK_PEOPLE, "");
	let stack = refresh.stack();
	let checks = crate::stack::checks(&stack).expect("a refreshed stack has its chains checked");
	checks.people_suffix_answered(b"testnet");

	let read = within_answer_wait(async {
		let people = services.statement_store.chain_client("test").await.ok()?;
		Some(services.people_network_suffix(people.rpc()).await)
	});

	assert!(
		matches!(
			read,
			Some(Err(StatementAllowanceError::Slot(SlotError::InvalidNetworkSuffixLength {
				len: 0
			})))
		),
		"the empty suffix fails the read: {read:?}"
	);
	assert_eq!(refresh.status().mismatches, testnet_suffix_listed());
}

/// A signing host with an active wallet, on a runtime over [`runtime_on`]'s
/// nodes where People gives `testnet` as its suffix, and the refresh whose
/// stack it runs. Past the suffix the nodes give only empty storage and
/// errors, so any allowance work the host starts fails after reading it.
fn signing_host_over_a_testnet_suffix() -> (StackRefresh, Arc<SigningHost>) {
	let refresh = load(Arc::default(), Arc::default(), manual_clock());
	let (services, _node) = runtime_on(&refresh, TEST_STACK_PEOPLE, "testnet");
	let signing_host = SigningHost::new(services);
	within_answer_wait(signing_host.activate_local_session(RootEntropy::new([0xab; 16])))
		.expect("activation succeeds");
	(refresh, signing_host)
}

fn test_product() -> ProductContext {
	ProductContext::new(ProductId::for_test("myapp.paseo"))
}

/// The allowance work is where a signing host reads People's suffix, so each
/// allowance has to report it. One that read the suffix past the checks would
/// build proofs with a suffix the host never hears of.
#[test]
fn a_statement_store_allowance_reports_a_people_suffix_that_is_not_the_tld() {
	let (refresh, signing_host) = signing_host_over_a_testnet_suffix();
	let accounts = signing_host.accounts();
	let session = accounts.current_session().expect("a session is active");

	let allocated = within_answer_wait(accounts.statement_store_allowance_key(
		&CallContext::default(),
		&session,
		&test_product(),
	));

	assert_eq!((allocated.is_ok(), refresh.status().mismatches), (false, testnet_suffix_listed()));
}

/// A Bulletin allowance proves with People's suffix too, read after Bulletin's
/// own authorization.
#[test]
fn a_bulletin_allowance_reports_a_people_suffix_that_is_not_the_tld() {
	let (refresh, signing_host) = signing_host_over_a_testnet_suffix();
	let accounts = signing_host.accounts();
	let session = accounts.current_session().expect("a session is active");

	let allocated = within_answer_wait(accounts.bulletin_allowance_key(
		&CallContext::default(),
		&session,
		&test_product(),
	));

	assert_eq!((allocated.is_ok(), refresh.status().mismatches), (false, testnet_suffix_listed()));
}

/// A renewal pass proves with People's suffix for each statement account it
/// keeps allowed, with no product asking.
#[test]
fn a_statement_store_renewal_reports_a_people_suffix_that_is_not_the_tld() {
	let (refresh, signing_host) = signing_host_over_a_testnet_suffix();
	within_answer_wait(
		signing_host.track_statement_renewal_targets(vec![StatementRenewalTarget::WalletSso]),
	)
	.expect("the wallet's own statement account is tracked");

	let renewed = within_answer_wait(signing_host.renew_statement_allowances());

	assert_eq!((renewed.is_ok(), refresh.status().mismatches), (false, testnet_suffix_listed()));
}

/// The host is told of a wrong suffix the way it is told of a wrong chain:
/// through the status its refresh reports, even while a fetch is out.
#[test]
fn the_host_is_told_of_a_people_suffix_that_is_not_the_tld() {
	let started = start(Arc::default(), vec![]);
	let _at_start = started.next_status();
	started.wait_for_fetches(1);
	let stack = started.refresh.stack();
	let checks = crate::stack::checks(&stack).expect("a refreshed stack has its chains checked");

	checks.people_suffix_answered(b"testnet");

	assert_eq!(
		started.next_status().mismatches,
		vec![StackMismatch::NetworkSuffix {
			named: "paseo".to_string(),
			answered: "testnet".to_string(),
		}]
	);
}

/// Which stack a host runs is its chains, network and TLD. Whether a refresh
/// checks its chains is no part of that, so a host config built on the
/// refreshed stack still equals one built on the bundled copy.
#[test]
fn the_refreshed_stack_equals_the_bundled_one() {
	let (link, bundled) =
		sign_test_descriptor(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE));
	let refresh = load(Arc::default(), Arc::default(), manual_clock());

	assert_eq!(
		refresh.stack(),
		Stack::bundled(&link, &bundled, false).expect("bundled copy is valid")
	);
}

use core::{num::NonZeroU32, ops::Range};
use std::sync::{
	Mutex,
	atomic::{AtomicUsize, Ordering},
};

use futures::{FutureExt, executor::block_on};
use tempfile::TempDir;

use super::*;
use crate::{
	host_logic::root_entropy::RootEntropy,
	nft::{
		ItemSlot, NftLedger, NftLedgerDeps, PurseId, PurseRecord, PurseTag,
		keys::ItemIndex,
		test_ledger,
		testing::{FixedInstallation, TEST_ENTROPY, TEST_INSTALLATION, test_ledger_deps},
	},
	runtime::AuthorityError,
	store::open_temporary_core_db,
	test_support::session_info,
};

/// An installation the seed ran as before the reinstall.
const EARLIER: InstallationId = InstallationId([0xe1; 32]);

/// Another earlier installation.
const EARLIER_STILL: InstallationId = InstallationId([0xe0; 32]);

/// The key at `index` of `purse` under `installation`, from the test seed.
fn item_key(purse: u32, installation: InstallationId, index: u32) -> ItemKey {
	let slot = ItemSlot { purse: PurseId(purse), installation, index: ItemIndex(index) };
	RootEntropy::new(TEST_ENTROPY).item_keys(&[slot]).expect("test entropy derives")[0]
}

/// Derives from the test seed, records each installation it derives a range
/// under, and fails the range derivation numbered `failing_derivation`,
/// counting from 1.
#[derive(Default)]
struct TestSeed {
	failing_derivation: Option<usize>,
	derivations: AtomicUsize,
	installations_scanned: Mutex<HashSet<InstallationId>>,
}

impl ItemKeyDeriver for TestSeed {
	fn derive_item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError> {
		Ok(RootEntropy::new(TEST_ENTROPY).item_keys(slots).unwrap())
	}

	fn derive_item_key_range(
		&self,
		purse: PurseId,
		installation: InstallationId,
		indices: Range<u32>,
	) -> Result<Vec<ItemKey>, AuthorityError> {
		let derivation = self.derivations.fetch_add(1, Ordering::Relaxed) + 1;
		if self.failing_derivation == Some(derivation) {
			return Err(AuthorityError::Unavailable { reason: "session locked".to_string() });
		}
		self.installations_scanned.lock().unwrap().insert(installation);
		Ok(RootEntropy::new(TEST_ENTROPY)
			.item_key_range(purse, installation, indices)
			.unwrap())
	}
}

/// A chain on which the seed registered `registered` and the keys in `held`
/// hold an NFT.
struct TestChain {
	registered: Mutex<HashSet<InstallationId>>,
	held: HashSet<ItemKey>,
}

impl TestChain {
	fn new(registered: &[InstallationId], held: impl IntoIterator<Item = ItemKey>) -> Self {
		let registered = registered.iter().copied().collect();
		Self { registered: Mutex::new(registered), held: held.into_iter().collect() }
	}
}

#[async_trait::async_trait]
impl RecoveryChain for TestChain {
	async fn finalized_block(&self) -> Result<H256, NftRecoveryError> {
		Ok(H256::repeat_byte(0xf1))
	}

	async fn registered_installations(
		&self,
		_at: H256,
	) -> Result<HashSet<InstallationId>, NftRecoveryError> {
		Ok(self.registered.lock().unwrap().clone())
	}

	async fn holdings_at(
		&self,
		_at: H256,
		keys: &[ItemKey],
	) -> anyhow::Result<Vec<Option<HeldItem>>> {
		let held = HeldItem { collection: 1, item: 2, instance: 3 };
		Ok(keys.iter().map(|key| self.held.contains(key).then_some(held)).collect())
	}
}

/// Ends each purse after ten empty keys, read five at a time. The gap
/// arithmetic is tested in `gap_scan`, so these tests only need a scan that
/// is cheap to derive.
fn shallow_scan_config() -> GapScanConfig {
	GapScanConfig { gap_size: 10, batch_size: NonZeroU32::new(5).unwrap() }
}

/// Recovery over a fresh database, with the test seed and installation.
struct Harness {
	_dir: TempDir,
	deps: NftRecoveryDeps,
	chain: Arc<TestChain>,
	seed: Arc<TestSeed>,
}

impl Harness {
	fn new(chain: TestChain) -> Self {
		Self::with_seed(chain, TestSeed::default())
	}

	fn with_seed(chain: TestChain, seed: TestSeed) -> Self {
		let (dir, db) = open_temporary_core_db();
		let (chain, seed) = (Arc::new(chain), Arc::new(seed));
		let deps = NftRecoveryDeps {
			db,
			chain: chain.clone(),
			keys: seed.clone(),
			installation: Arc::new(FixedInstallation(TEST_INSTALLATION)),
			session_state: SessionState::new(),
			spawner: Spawner::for_test(),
			scan_config: shallow_scan_config(),
		};
		Self { _dir: dir, deps, chain, seed }
	}

	/// Recovery started over the harness's deps, which runs once a session
	/// connects.
	fn start(&self) -> NftRecovery {
		NftRecovery::start(NftRecoveryDeps {
			db: self.deps.db.clone(),
			chain: self.deps.chain.clone(),
			keys: self.deps.keys.clone(),
			installation: self.deps.installation.clone(),
			session_state: self.deps.session_state.clone(),
			spawner: self.deps.spawner.clone(),
			scan_config: self.deps.scan_config,
		})
	}

	/// A ledger whose new purses wait for recovery over the harness's deps.
	fn ledger(&self) -> NftLedger {
		let deps = test_ledger_deps(self.deps.db.clone());
		NftLedger::new(NftLedgerDeps { recovery: Arc::new(self.start()), ..deps })
	}

	fn recover(&self) -> Result<(), NftRecoveryError> {
		block_on(run_recovery_once(&self.deps))
	}

	fn purse_of(&self, key: ItemKey) -> Option<PurseId> {
		block_on(self.deps.db.read(move |conn| dao::purse_of(conn, &key))).unwrap()
	}

	fn purse(&self, id: u32) -> Option<PurseRecord> {
		block_on(self.deps.db.read(move |conn| dao::purse(conn, PurseId(id)))).unwrap()
	}
}

/// The end-to-end promise of the RFC's backup recovery: what the earlier
/// installation's purses held comes back under the purse it was in, and a
/// purse with no metadata is recreated, owned by no product.
#[test]
fn recovery_records_what_earlier_purses_held_and_recreates_their_purses() {
	let in_main = item_key(PurseId::MAIN.0, EARLIER, 4);
	let in_purse_3 = item_key(3, EARLIER, 0);
	let harness = Harness::new(TestChain::new(&[EARLIER], [in_main, in_purse_3]));

	harness.recover().unwrap();

	assert_eq!(harness.purse_of(in_main), Some(PurseId::MAIN));
	assert_eq!(harness.purse_of(in_purse_3), Some(PurseId(3)));
	let recreated = harness.purse(3).unwrap();
	assert_eq!(recreated.name, "Recovered purse 3");
	assert_eq!(recreated.tag.0, Vec::<u8>::new());
}

/// The walk over purse ids stops after ten empty purses in a row, and the
/// main purse, scanned first, never counts toward them.
#[test]
fn the_purse_walk_stops_after_ten_empty_purses() {
	let in_purse_9 = item_key(9, EARLIER, 0);
	let in_purse_10 = item_key(10, EARLIER, 0);
	let ninth = Harness::new(TestChain::new(&[EARLIER], [in_purse_9]));
	let tenth = Harness::new(TestChain::new(&[EARLIER], [in_purse_10]));

	ninth.recover().unwrap();
	tenth.recover().unwrap();

	assert_eq!(ninth.purse_of(in_purse_9), Some(PurseId(9)));
	assert_eq!(tenth.purse_of(in_purse_10), None);
}

/// A failed run must lose nothing and must not record anything twice: the
/// installation stays pending, and the retry records again what the failed
/// run already recorded.
#[test]
fn a_failed_run_leaves_the_installation_pending_and_the_retry_finds_its_items() {
	let in_main = item_key(PurseId::MAIN.0, EARLIER, 0);
	let in_purse_0 = item_key(0, EARLIER, 2);
	// The main purse takes three reads of 5 keys, so the fourth derivation
	// is purse 0's first, after the main purse's key is recorded.
	let seed = TestSeed { failing_derivation: Some(4), ..TestSeed::default() };
	let harness = Harness::with_seed(TestChain::new(&[EARLIER], [in_main, in_purse_0]), seed);

	assert!(matches!(
		harness.recover(),
		Err(NftRecoveryError::Scan(PurseScanError::Scan { .. }))
	));
	let pending = block_on(harness.deps.db.read(dao::pending_recoveries)).unwrap();
	let recorded_before_the_failure = harness.purse_of(in_main);
	harness.recover().unwrap();

	assert_eq!(pending, [EARLIER]);
	assert_eq!(recorded_before_the_failure, Some(PurseId::MAIN));
	assert_eq!(harness.purse_of(in_purse_0), Some(PurseId(0)));
}

/// Purse ids are shared by every installation, so an installation can hold
/// a purse only past ids it never used itself, such as a purse created after
/// an earlier recovery. The walk counts a purse as empty only when it is
/// empty under every installation.
#[test]
fn a_purse_past_another_installations_purses_is_found() {
	let in_earlier_purses = (0..=10).map(|purse| item_key(purse, EARLIER_STILL, 0));
	let in_purse_12 = item_key(12, EARLIER, 0);
	let held = in_earlier_purses.chain([in_purse_12]);
	let harness = Harness::new(TestChain::new(&[EARLIER_STILL, EARLIER], held));

	harness.recover().unwrap();

	assert_eq!(harness.purse_of(in_purse_12), Some(PurseId(12)));
}

/// Recovery must not clobber local state: a purse already at a recovered id
/// keeps its name and owner, and the current installation, whose keys the
/// ledger already holds, is never scanned.
#[test]
fn recovery_keeps_existing_purses_and_never_scans_the_current_installation() {
	let in_purse_0 = item_key(0, EARLIER, 0);
	let current = item_key(0, TEST_INSTALLATION, 0);
	let harness =
		Harness::new(TestChain::new(&[TEST_INSTALLATION, EARLIER], [in_purse_0, current]));
	let without_recovery = test_ledger(harness.deps.db.clone());
	block_on(without_recovery.create_purse("tickets".into(), PurseTag(b"tickets.dot".to_vec())))
		.unwrap();

	harness.recover().unwrap();

	let kept = harness.purse(0).unwrap();
	assert_eq!(kept.name, "tickets");
	assert_eq!(kept.tag.0, b"tickets.dot");
	assert_eq!(harness.purse_of(current), None);
}

/// Each session reads the registry again, so a newly registered installation
/// is scanned, while a finished one is not scanned twice.
#[test]
fn a_later_run_scans_only_installations_not_finished_yet() {
	let harness = Harness::new(TestChain::new(&[EARLIER], []));
	harness.recover().unwrap();
	harness.seed.installations_scanned.lock().unwrap().clear();
	harness.chain.registered.lock().unwrap().insert(EARLIER_STILL);

	harness.recover().unwrap();

	let scanned = harness.seed.installations_scanned.lock().unwrap().clone();
	assert_eq!(scanned, HashSet::from([EARLIER_STILL]));
}

/// A product that creates a purse during recovery must not take an id an
/// earlier purse is recreated at, or it would take over that purse's NFTs.
#[test]
fn creating_a_purse_waits_for_recovery_and_takes_an_id_past_the_recovered_ones() {
	let harness = Harness::new(TestChain::new(&[EARLIER], [item_key(3, EARLIER, 0)]));
	let ledger = harness.ledger();
	let mut created = core::pin::pin!(ledger.create_purse("new".into(), PurseTag::default()));
	assert!(created.as_mut().now_or_never().is_none());

	harness.recover().unwrap();

	assert_eq!(block_on(created).unwrap(), PurseId(4));
}

/// Recovery is finished only while no registered installation is pending,
/// so an installation registered after a finished run holds new purses back
/// again until a run scans it.
#[test]
fn a_newly_registered_installation_holds_new_purses_back_until_scanned() {
	let harness = Harness::new(TestChain::new(&[EARLIER], [item_key(5, EARLIER_STILL, 0)]));
	harness.recover().unwrap();
	harness.chain.registered.lock().unwrap().insert(EARLIER_STILL);
	let pending = record_registered_installations(
		&harness.deps.db,
		TEST_INSTALLATION,
		HashSet::from([EARLIER_STILL]),
	);
	block_on(pending).unwrap();
	let ledger = harness.ledger();
	let mut created = core::pin::pin!(ledger.create_purse("new".into(), PurseTag::default()));
	assert!(created.as_mut().now_or_never().is_none());

	harness.recover().unwrap();

	assert_eq!(block_on(created).unwrap(), PurseId(6));
}

/// A device that finished recovery before must not hold new purses back
/// until the next session.
#[tokio::test]
async fn new_purses_do_not_wait_once_every_installation_is_finished() {
	let harness = Harness::new(TestChain::new(&[EARLIER], []));
	harness.recover().unwrap();

	let recovery = harness.start();
	let finished =
		tokio::time::timeout(Duration::from_secs(5), recovery.wait_until_finished()).await;

	assert!(finished.is_ok(), "recovery did not read as finished");
}

/// Keys come from the session's seed, so recovery runs once a session
/// connects and not before.
#[tokio::test]
async fn recovery_runs_once_a_session_connects() {
	let in_purse_0 = item_key(0, EARLIER, 0);
	let harness = Harness::new(TestChain::new(&[EARLIER], [in_purse_0]));
	let recovery = harness.start();
	let before_session =
		tokio::time::timeout(Duration::from_millis(100), recovery.wait_until_finished()).await;

	harness.deps.session_state.set_session(session_info());
	let after_session =
		tokio::time::timeout(Duration::from_secs(5), recovery.wait_until_finished()).await;

	assert!(before_session.is_err(), "recovery ran before a session connected");
	assert!(after_session.is_ok(), "recovery did not finish once a session connected");
	assert_eq!(harness.purse_of(in_purse_0), Some(PurseId(0)));
}

/// A stack with no installation registry has nothing to recover, so new
/// purses never wait.
#[test]
fn recovery_that_is_off_is_always_finished() {
	let recovery = NftRecovery::off();

	assert!(recovery.wait_until_finished().now_or_never().is_some());
}

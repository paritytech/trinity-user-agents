use futures::executor::block_on;

use super::*;
use crate::coinage::{
	CoinSecret, PrivacyPolicy,
	chain::{ChainCoin, RingPlace},
	classification::{HeldBack, Selectability, classify},
	ledger::{BlockRef, Facts, Subject},
	namespace::datastore_key,
	policy::{MemberThreshold, ReadyRule},
	testing::{
		ENTROPY, FakeChain, FixedSeed, NOW, TestWallet, chain_params, entry_read,
		scans::{FINALIZED, ListedRegistry, RecordingChain},
	},
};

const EARLIER: InstallationId = InstallationId([4; 32]);

struct Registry(Option<Vec<Vec<u8>>>);

#[async_trait::async_trait]
impl InstallationRegistry for Registry {
	async fn records(&self) -> Option<Vec<Vec<u8>>> {
		self.0.clone()
	}
}

fn datastore() -> CoinSecret {
	datastore_key(&ENTROPY).unwrap().0
}

fn key() -> RegistryKey {
	RegistryKey::of(&datastore()).unwrap()
}

#[test]
fn a_reconstruction_adopts_what_an_installation_from_the_registry_holds() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let chain = Arc::new(FakeChain::default());
	let (_, account) = coin_key(&ENTROPY, PurseId::MAIN, EARLIER, 3).unwrap();
	chain.coins.lock().insert(account, ChainCoin { instance: 0, value: 5, age: 1, locked_until: None });
	let deps = ReconstructionDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		chain,
		registry: Arc::new(Registry(Some(vec![key().seal(EARLIER).to_vec()]))),
	};
	let config = ScanConfig { batch: 4, gap: 2, deep_batches: 4, purse_gap: 1 };
	let adopted = block_on(reconstruct(&deps, Purses::MainOnly, false, &config, &chain_params())).unwrap();
	assert_eq!(adopted, 1);
	let view = wallet.view();
	let found = Asset::Coin { purse: PurseId::MAIN, installation: EARLIER, index: 3 };
	assert_eq!(view.asset(&found).map(|record| record.exponent.get()), Some(5));
	assert!(view.installations().iter().all(|(_, progress)| progress.first_scan_done));
	assert!(view.assets(PurseId::MAIN).any(|record| record.asset == found), "it counts toward the main purse");
}

#[test]
fn a_failed_registry_read_still_scans_the_installations_already_known() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let deps = ReconstructionDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		chain: Arc::new(FakeChain::default()),
		registry: Arc::new(Registry(None)),
	};
	let config = ScanConfig { batch: 4, gap: 1, deep_batches: 2, purse_gap: 1 };
	assert_eq!(block_on(reconstruct(&deps, Purses::MainOnly, false, &config, &chain_params())).unwrap(), 0);
	assert!(wallet.view().installations().iter().all(|(_, progress)| progress.first_scan_done));
}

const GAP_OF_TWO: ScanConfig = ScanConfig { batch: 4, gap: 2, deep_batches: 4, purse_gap: 1 };

/// The account of the coin at `index` of the main purse under `EARLIER`.
fn earlier_coin(index: u32) -> [u8; 32] {
	coin_key(&ENTROPY, PurseId::MAIN, EARLIER, index).unwrap().1
}

/// A wallet whose registry lists `EARLIER`, which holds a coin at each of
/// `indices` of the main purse, read through a recording chain.
fn wallet_with_earlier_coins(indices: &[u32]) -> (TestWallet, Arc<RecordingChain>, ReconstructionDeps) {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let chain = Arc::new(RecordingChain::default());
	for index in indices {
		let coin = ChainCoin { instance: 0, value: 5, age: 1, locked_until: None };
		chain.chain.coins.lock().insert(earlier_coin(*index), coin);
	}
	let deps = ReconstructionDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		chain: chain.clone(),
		registry: Arc::new(ListedRegistry(vec![EARLIER])),
	};
	(wallet, chain, deps)
}

fn earlier_coins_held(wallet: &TestWallet) -> Vec<u32> {
	wallet
		.view()
		.assets(PurseId::MAIN)
		.filter_map(|record| match record.asset {
			Asset::Coin { installation: EARLIER, index, .. } => Some(index),
			_ => None,
		})
		.collect()
}

#[test]
fn a_scan_stops_after_consecutive_empty_batches_not_after_as_many_in_total() {
	let (wallet, _, deps) = wallet_with_earlier_coins(&[0, 8, 16]);
	block_on(reconstruct(&deps, Purses::MainOnly, false, &GAP_OF_TWO, &chain_params())).unwrap();
	assert_eq!(earlier_coins_held(&wallet), [0, 8, 16], "each hit starts the count of empty batches again");
}

#[test]
fn coins_are_recovered_from_the_finalized_head() {
	let (wallet, chain, deps) = wallet_with_earlier_coins(&[3]);
	block_on(reconstruct(&deps, Purses::MainOnly, false, &GAP_OF_TWO, &chain_params())).unwrap();
	assert_eq!(earlier_coins_held(&wallet), [3]);
	assert!(chain.coin_reads.lock().iter().all(|(at, _)| *at == FINALIZED.hash), "never the best head");
}

#[test]
fn a_scan_that_cannot_read_the_heads_recovers_nothing() {
	let (wallet, chain, deps) = wallet_with_earlier_coins(&[3]);
	chain.heads_failing.store(true, core::sync::atomic::Ordering::SeqCst);
	let failed = block_on(reconstruct(&deps, Purses::MainOnly, false, &GAP_OF_TWO, &chain_params()));
	assert!(matches!(failed, Err(ReconstructError::Unavailable)));
	assert_eq!(earlier_coins_held(&wallet), [0u32; 0]);
	let progress: Vec<_> = wallet.view().installations().to_vec();
	assert_eq!(progress, [(InstallationId([9; 32]), ScanProgress::default()), (EARLIER, ScanProgress::default())]);
}

#[test]
fn an_interrupted_scan_resumes_where_it_stopped() {
	let (wallet, chain, deps) = wallet_with_earlier_coins(&[0, 4]);
	*chain.coin_reads_left.lock() = Some(4);
	let interrupted = block_on(reconstruct(&deps, Purses::MainOnly, false, &GAP_OF_TWO, &chain_params()));
	assert!(matches!(interrupted, Err(ReconstructError::Unavailable)), "the third read of `EARLIER` fails");
	assert_eq!(earlier_coins_held(&wallet), [0, 4], "what the batches before the failure found is kept");

	*chain.coin_reads_left.lock() = None;
	chain.coin_reads.lock().clear();
	block_on(reconstruct(&deps, Purses::MainOnly, false, &GAP_OF_TWO, &chain_params())).unwrap();
	let earlier: Vec<[u8; 32]> = (0..32).map(earlier_coin).collect();
	let indices_read: Vec<usize> = chain
		.accounts_read()
		.iter()
		.filter_map(|account| earlier.iter().position(|candidate| candidate == account))
		.collect();
	assert_eq!(indices_read, (8..16).collect::<Vec<_>>(), "the scan of `EARLIER` resumes at index 8");
	assert!(wallet.view().installations().iter().all(|(_, progress)| progress.first_scan_done));
}

#[test]
fn a_reconstructed_entry_waits_out_its_dwell_from_its_first_observation() {
	let dwell = ReadyRule { members: MemberThreshold::Count(1), dwell: core::time::Duration::from_secs(600) };
	let wallet = TestWallet::new(PrivacyPolicy { ready_when: vec![dwell], ..PrivacyPolicy::fastest() });
	let entry = Asset::Entry { purse: PurseId::MAIN, installation: EARLIER, index: 5 };
	let found = ledger::AssetRecord { asset: entry, exponent: Exponent::new(2).unwrap() };
	block_on(wallet.db.write(move |tx| ledger::adopt(tx, &[found]))).unwrap();
	let observe_at = |number: u32, now: u64| {
		let facts = Facts { at: BlockRef { number, hash: [number as u8; 32] }, reads: vec![(Subject::Asset(entry), entry_read(0))] };
		block_on(wallet.db.write(move |tx| ledger::observe(tx, &facts, now, 255))).unwrap();
	};
	observe_at(1, NOW);
	observe_at(2, NOW + 60);
	let view = wallet.view();
	assert_eq!(view.observation(&Subject::Asset(entry)).and_then(|observation| observation.included_at), Some(NOW));
	let at = |now| classify(&entry, &view, now, &chain_params()).selectability;
	assert_eq!((at(NOW + 599), at(NOW + 600)), (Selectability::HeldBack(HeldBack::Dwell), Selectability::Now));
}

#[test]
fn a_tampered_record_does_not_open() {
	let mut sealed = key().seal(EARLIER);
	sealed[20] ^= 1;
	assert_eq!(key().open(&sealed), None);
}

/// An unload names only the alias, so an entry the wallet already unloaded is
/// still a ring member; adopting it would count value that is gone.
#[test]
fn a_reconstruction_skips_entries_whose_alias_is_unloaded() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let chain = Arc::new(FakeChain::default());
	let vrf = block_on(load_vrf()).unwrap();
	let seed = |index| entry_seed(&ENTROPY, PurseId::MAIN, EARLIER, index).0;
	let place = RingPlace { ring: 3, page: 0, position: 0, members: 8, included: 8, immutable_since: None, revision: 1 };
	for index in [0, 1] {
		chain.places.lock().insert(vrf.member(&seed(index)).unwrap(), (2, place));
	}
	chain.unplaced.lock().insert(vrf.member(&seed(2)).unwrap(), (2, RingPosition::Onboarding));
	let unloaded = vrf.alias(&seed(0), &UNLOADING_RECYCLER_CONTEXT).unwrap();
	chain.aliases.lock().insert(unloaded, AliasState::Unloaded);
	let deps = ReconstructionDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		chain,
		registry: Arc::new(Registry(Some(vec![key().seal(EARLIER).to_vec()]))),
	};
	let config = ScanConfig { batch: 4, gap: 2, deep_batches: 4, purse_gap: 1 };
	assert_eq!(block_on(reconstruct(&deps, Purses::MainOnly, false, &config, &chain_params())).unwrap(), 2);
	let mut adopted: Vec<Asset> = wallet.view().all_assets().map(|record| record.asset).collect();
	adopted.sort();
	let entry = |index| Asset::Entry { purse: PurseId::MAIN, installation: EARLIER, index };
	assert_eq!(adopted, vec![entry(1), entry(2)]);
}

/// The installation a shipped app allocated under, handed over.
const APP: InstallationId = InstallationId([5; 32]);

fn app_coin(index: u32) -> [u8; 32] {
	coin_key(&ENTROPY, PurseId::MAIN, APP, index).unwrap().1
}

/// A wallet that recorded a hand-over of `APP` up to `next_coin`, listing
/// `outstanding`, over a chain holding a coin at each of `indices` of `APP`.
fn handed_over(
	next_coin: u32,
	outstanding: Vec<handover::OutstandingPayment>,
	indices: &[u32],
) -> (TestWallet, ReconstructionDeps) {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let handed = handover::Handover {
		installations: vec![handover::AppInstallation { id: APP, next_coin, next_entry: 0 }],
		outstanding,
		incoming: Vec::new(),
	};
	block_on(wallet.db.write(move |tx| handover::record(tx, &handed))).unwrap();
	let chain = Arc::new(FakeChain::default());
	for index in indices {
		chain.coins.lock().insert(app_coin(*index), ChainCoin { instance: 0, value: 5, age: 1, locked_until: None });
	}
	let deps = ReconstructionDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		chain,
		registry: Arc::new(Registry(None)),
	};
	(wallet, deps)
}

fn app_coins_held(wallet: &TestWallet) -> Vec<u32> {
	let mut held: Vec<u32> = wallet
		.view()
		.assets(PurseId::MAIN)
		.filter_map(|record| match record.asset {
			Asset::Coin { installation: APP, index, .. } => Some(index),
			_ => None,
		})
		.collect();
	held.sort();
	held
}

/// The app knows how far it allocated, so the scan reads that far whatever
/// lies spent below, and applies the gap rule only past the mark.
#[test]
fn a_scan_reads_a_handed_over_installation_up_to_its_high_water_mark() {
	let short = ScanConfig { batch: 4, gap: 1, deep_batches: 2, purse_gap: 1 };
	let (wallet, deps) = handed_over(40, Vec::new(), &[30, 50]);
	block_on(reconstruct(&deps, Purses::MainOnly, false, &short, &chain_params())).unwrap();
	assert_eq!(app_coins_held(&wallet), vec![30]);
}

/// The payee holds the secrets of a sent payment's coins, so the wallet must
/// never spend them, and the user can still take them back.
#[test]
fn a_coin_of_a_sent_payment_is_adopted_as_handed_out() {
	let short = ScanConfig { batch: 4, gap: 1, deep_batches: 2, purse_gap: 1 };
	let payment = handover::OutstandingPayment { id: [8; 32], accounts: vec![app_coin(3)] };
	let (wallet, deps) = handed_over(8, vec![payment], &[2, 3]);
	block_on(reconstruct(&deps, Purses::MainOnly, false, &short, &chain_params())).unwrap();
	let view = wallet.view();
	let coin = |index| Asset::Coin { purse: PurseId::MAIN, installation: APP, index };
	assert_eq!(
		(app_coins_held(&wallet), view.mark(&coin(3)), view.mark(&coin(2))),
		(vec![2, 3], Some(ledger::Mark::Committed), None)
	);
	assert_eq!(classify(&coin(3), &view, NOW, &chain_params()).selectability, Selectability::Never);
}

const OTHER: InstallationId = InstallationId([6; 32]);

fn coin_at(installation: InstallationId, index: u32) -> [u8; 32] {
	coin_key(&ENTROPY, PurseId::MAIN, installation, index).unwrap().1
}

fn coins_held(wallet: &TestWallet, of: InstallationId) -> Vec<u32> {
	let mut held: Vec<u32> = wallet
		.view()
		.assets(PurseId::MAIN)
		.filter_map(|record| match record.asset {
			Asset::Coin { installation, index, .. } if installation == of => Some(index),
			_ => None,
		})
		.collect();
	held.sort();
	held
}

/// One installation whose reads fail must not keep the others from being
/// recovered; it stays unscanned, so the next reconstruction tries it again.
#[test]
fn a_failed_read_stops_only_its_installation() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let chain = Arc::new(FakeChain::default());
	chain.coins.lock().insert(coin_at(OTHER, 1), ChainCoin { instance: 0, value: 5, age: 1, locked_until: None });
	chain.failing_accounts.lock().push(coin_at(EARLIER, 0));
	let records = vec![key().seal(EARLIER).to_vec(), key().seal(OTHER).to_vec()];
	let deps = ReconstructionDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		chain,
		registry: Arc::new(Registry(Some(records))),
	};
	let config = ScanConfig { batch: 4, gap: 1, deep_batches: 2, purse_gap: 1 };
	assert!(block_on(reconstruct(&deps, Purses::MainOnly, false, &config, &chain_params())).is_err());
	let view = wallet.view();
	let scanned = |of: InstallationId| view.installations().iter().any(|(id, progress)| *id == of && progress.first_scan_done);
	assert_eq!((coins_held(&wallet, OTHER), scanned(OTHER), scanned(EARLIER)), (vec![1], true, false));
}

/// A deep scan looks further than the last scan stopped, which is where a
/// wallet whose spent indices left a long empty run keeps the rest.
#[test]
fn a_deep_scan_reads_past_where_the_last_scan_stopped() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let chain = Arc::new(FakeChain::default());
	for index in [6, 13] {
		chain.coins.lock().insert(coin_at(EARLIER, index), ChainCoin { instance: 0, value: 5, age: 1, locked_until: None });
	}
	let deps = ReconstructionDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		chain,
		registry: Arc::new(Registry(Some(vec![key().seal(EARLIER).to_vec()]))),
	};
	let config = ScanConfig { batch: 4, gap: 1, deep_batches: 2, purse_gap: 1 };
	block_on(reconstruct(&deps, Purses::MainOnly, false, &config, &chain_params())).unwrap();
	assert_eq!(coins_held(&wallet, EARLIER), Vec::<u32>::new(), "the default scan stops after one empty batch");
	block_on(reconstruct(&deps, Purses::MainOnly, true, &config, &chain_params())).unwrap();
	assert_eq!(coins_held(&wallet, EARLIER), vec![6], "the deep scan reads two batches past index 4");
	block_on(reconstruct(&deps, Purses::MainOnly, true, &config, &chain_params())).unwrap();
	assert_eq!(coins_held(&wallet, EARLIER), vec![6, 13], "the next deep scan continues past index 12");
}

use futures::executor::block_on;

use super::*;
use crate::coinage::{
	PrivacyPolicy,
	chain::ChainCoin,
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

fn bytes(hex: &str) -> Vec<u8> {
	hex::decode(hex).unwrap()
}

/// The shipped wallets' vectors (iOS `InstallationRecordCipherTests`,
/// Android `InstallationRecordCipherTest`): the secret is the bytes 0 to 63,
/// the installation `[0x5a; 32]`.
#[test]
fn a_record_seals_and_opens_as_the_shipped_wallets_do() {
	let secret: [u8; 64] = core::array::from_fn(|byte| byte as u8);
	let key = RegistryKey::from_ed25519_secret(&secret);
	assert_eq!(key.0.to_vec(), bytes("fc7bd73006ab990f9649eebec3aab66023af1f064d33a694d4d7a4f3bef86748"));
	let installation = InstallationId([0x5a; 32]);
	let record = bytes(
		"2cb9ac950a3a69cfa00480e7e221d461069884d050d21b2ba1c05e0e9bd08b3fe0edf24784976d3bd6e5652bb755ca90d48906c10e655eb8f3acc4c6",
	);
	assert_eq!(key.seal(installation).to_vec(), record);
	let with_another_nonce = bytes(
		"000102030405060708090a0bcdc58e6066449725c690c637add5e75d5fae9ea0e7c5df58e99ac4e0923ba664ba2d2aa75adc69e254a3e31eb2df5595",
	);
	assert_eq!(key.open(&with_another_nonce), Some(installation), "a record sealed with a random nonce opens");
	assert_eq!(key.open(&record[..59]), None);
}

/// The key of the `bottom drive obey` phrase, as polkadot-js, Android and
/// iOS compute it.
#[test]
fn a_seeds_registry_key_matches_the_shipped_wallets() {
	let phrase = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
	let entropy = bip39::Mnemonic::parse(phrase).unwrap().to_entropy();
	let (datastore, account) = datastore_key(&entropy).unwrap();
	assert_eq!(account.to_vec(), bytes("82fde43d07766255566803c486f9427cfb7026328a3d87cddd34b362acbdb54d"));
	let expanded = schnorrkel::SecretKey::from_bytes(&datastore.0).unwrap().to_ed25519_bytes();
	let expected_secret = concat!(
		"f88810d0da504a86f719c0c85cac296eb9d5d0e3e51d54b53cf15d96f6084849",
		"2dddcd798787957c4b5d3b960b4b78d0c925adeb662079c192c01ba13c4f9abe",
	);
	assert_eq!(expanded.to_vec(), bytes(expected_secret));
	let key = RegistryKey::of(&datastore).unwrap();
	assert_eq!(key.0.to_vec(), bytes("a217d0089ea2bc908772a8df8ab8f4aeb122eb1f41bbd26b6aff9c9386143f2a"));
}

#[test]
fn another_seeds_key_does_not_open_a_record() {
	let sealed = key().seal(EARLIER);
	assert_eq!(sealed, key().seal(EARLIER), "sealing is deterministic");
	assert_eq!(key().open(&sealed), Some(EARLIER));
	let other = RegistryKey::of(&datastore_key(&[8; 16]).unwrap().0).unwrap();
	assert_eq!(other.open(&sealed), None);
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
	let config = ScanConfig { batch: 4, gap: 2, deep_gap: 4, purse_gap: 1 };
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
	let config = ScanConfig { batch: 4, gap: 1, deep_gap: 2, purse_gap: 1 };
	assert_eq!(block_on(reconstruct(&deps, Purses::MainOnly, false, &config, &chain_params())).unwrap(), 0);
	assert!(wallet.view().installations().iter().all(|(_, progress)| progress.first_scan_done));
}

const GAP_OF_TWO: ScanConfig = ScanConfig { batch: 4, gap: 2, deep_gap: 4, purse_gap: 1 };

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

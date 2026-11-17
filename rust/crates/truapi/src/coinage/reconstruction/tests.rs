use futures::executor::block_on;

use super::*;
use crate::coinage::{
	CoinSecret, PrivacyPolicy,
	chain::{ChainCoin, RingPlace},
	classification::{Selectability, classify},
	namespace::datastore_key,
	testing::{ENTROPY, FakeChain, FixedSeed, NOW, TestWallet, chain_params},
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
	let config = ScanConfig { batch: 4, gap: 2, deep_gap: 4, purse_gap: 1 };
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
	let short = ScanConfig { batch: 4, gap: 1, deep_gap: 2, purse_gap: 1 };
	let (wallet, deps) = handed_over(40, Vec::new(), &[30, 50]);
	block_on(reconstruct(&deps, Purses::MainOnly, false, &short, &chain_params())).unwrap();
	assert_eq!(app_coins_held(&wallet), vec![30]);
}

/// The payee holds the secrets of a sent payment's coins, so the wallet must
/// never spend them, and the user can still take them back.
#[test]
fn a_coin_of_a_sent_payment_is_adopted_as_handed_out() {
	let short = ScanConfig { batch: 4, gap: 1, deep_gap: 2, purse_gap: 1 };
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

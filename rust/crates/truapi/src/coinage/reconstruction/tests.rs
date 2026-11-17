use futures::executor::block_on;

use super::*;
use crate::coinage::{
	PrivacyPolicy,
	chain::{ChainCoin, RingPlace},
	namespace::datastore_key,
	testing::{ENTROPY, FakeChain, FixedSeed, TestWallet, chain_params},
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

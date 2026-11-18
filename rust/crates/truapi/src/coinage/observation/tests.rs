use core::sync::atomic::Ordering;

use futures::executor::block_on;
use subxt::utils::H256;

use super::*;
use crate::{
	coinage::{
		PrivacyPolicy, PurseId,
		chain::{ChainCoin, FreeTokenLimits, RingPlace},
		ledger::{AssetKind, Observed},
		namespace::entry_key,
		testing::{ENTROPY, FakeChain, FixedSeed, NOW, NoPerson, TestWallet, chain_params, coin_read, entry_read},
	},
	installation::InstallationId,
	runtime::statement_allowance::{PersonhoodSigner, StatementAllowanceError, ring::RingParams},
};

fn deps(wallet: &TestWallet, chain: Arc<FakeChain>) -> ObserverDeps {
	ObserverDeps { store: wallet.db.clone(), seed: Arc::new(FixedSeed), chain, personhood: Arc::new(NoPerson) }
}

fn block(number: u64) -> HashAndNumber {
	HashAndNumber { hash: H256([number as u8; 32]), number }
}

fn account(index: u32) -> [u8; 32] {
	coin_key(&ENTROPY, PurseId::MAIN, InstallationId([9; 32]), index).unwrap().1
}

#[test]
fn a_coin_is_observed_at_the_block_with_its_exponent_age_and_lock() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let coin = wallet.coin(3);
	let chain = Arc::new(FakeChain::default());
	chain.coins.lock().insert(account(0), ChainCoin { instance: 0, value: 3, age: 2, locked_until: Some(NOW + 5) });
	block_on(on_block(&deps(&wallet, chain), block(20), NOW, &chain_params())).unwrap();
	let observation = wallet.view().observation(&Subject::Asset(coin)).cloned().unwrap();
	assert_eq!(observation.observed, Observed::At(BlockRef { number: 20, hash: [20; 32] }));
	let expected = CoinOnChain { exponent: Exponent::new(3).unwrap(), age: 2, locked_until: Some(NOW + 5) };
	assert_eq!(observation.value, Some(Read::Coin { now: Some(expected), emptied_by: None }));
}

#[test]
fn a_coin_of_another_instance_reads_as_an_empty_account() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let coin = wallet.coin(3);
	let chain = Arc::new(FakeChain::default());
	chain.coins.lock().insert(account(0), ChainCoin { instance: 7, value: 3, age: 0, locked_until: None });
	block_on(on_block(&deps(&wallet, chain), block(20), NOW, &chain_params())).unwrap();
	let observation = wallet.view().observation(&Subject::Asset(coin)).cloned().unwrap();
	assert_eq!(observation.value, Some(Read::Coin { now: None, emptied_by: None }));
}

#[test]
fn a_failed_read_never_reaches_the_ledger_as_absent() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let coin = wallet.coin(3);
	let chain = Arc::new(FakeChain::default());
	chain.failing.store(true, Ordering::SeqCst);
	block_on(on_block(&deps(&wallet, chain), block(20), NOW, &chain_params())).unwrap();
	let observation = wallet.view().observation(&Subject::Asset(coin)).cloned().unwrap();
	assert!(matches!(observation.observed, Observed::FailedSince(_)), "{observation:?}");
	assert_eq!(observation.value, Some(coin_read(3)), "the last stored value stands");
}

#[test]
fn an_entry_is_observed_in_its_ring_with_its_expiry() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let entry = wallet.hold(PurseId::MAIN, AssetKind::Entry, 4, entry_read(0));
	let vrf = block_on(load_vrf()).unwrap();
	let (_, member) = entry_key(&ENTROPY, PurseId::MAIN, InstallationId([9; 32]), 0, &vrf).unwrap();
	let place = RingPlace { ring: 3, page: 1, position: 2, members: 300, included: 290, immutable_since: Some(NOW), revision: 9 };
	let chain = Arc::new(FakeChain::default());
	chain.places.lock().insert(member, (4, place));
	block_on(on_block(&deps(&wallet, chain), block(21), NOW, &chain_params())).unwrap();
	let observation = wallet.view().observation(&Subject::Asset(entry)).cloned().unwrap();
	let Some(Read::Entry(Some(on_chain))) = observation.value else { panic!("{observation:?}") };
	assert_eq!((on_chain.ring, on_chain.revision, on_chain.page, on_chain.position), (3, 9, 1, 2));
	assert!(on_chain.closed);
	assert_eq!(on_chain.expires_at, Some(NOW + 90 * 24 * 3600));
}

/// A person of one collection, whose every alias is the same.
struct Person(PersonhoodCollection);

impl PersonhoodSigner for Person {
	fn member(&self, _: PersonhoodCollection) -> Result<[u8; 32], StatementAllowanceError> {
		Ok([7; 32])
	}

	fn alias(&self, _: PersonhoodCollection, _: &[u8]) -> Result<[u8; 32], StatementAllowanceError> {
		Ok([8; 32])
	}

	fn prove(&self, _: &RingParams, _: &[u8], _: &[u8]) -> Result<Vec<u8>, StatementAllowanceError> {
		Ok(Vec::new())
	}
}

#[async_trait::async_trait]
impl PersonhoodSource for Person {
	async fn person(&self) -> Option<(Arc<dyn PersonhoodSigner>, RingParams, u32)> {
		let ring = RingParams { collection: self.0, members: Vec::new(), exponent: 9, ring_index: 0, block_hash: String::new() };
		Some((Arc::new(Person(self.0)), ring, 0))
	}
}

/// Each collection has its own allowance, and it follows the unload fee, so
/// every block reads the one of the user's collection.
#[test]
fn every_block_reads_the_allowance_of_the_users_collection() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let chain = Arc::new(FakeChain::default());
	*chain.free_tokens.lock() = Some(FreeTokenLimits { people: 30, lite_people: 5 });
	let observed = |collection| {
		let deps = ObserverDeps { personhood: Arc::new(Person(collection)), ..deps(&wallet, chain.clone()) };
		block_on(on_block(&deps, block(20), NOW, &chain_params())).unwrap();
		wallet.view().unload_allowance()
	};
	assert_eq!([observed(PersonhoodCollection::LitePeople), observed(PersonhoodCollection::People)], [Some(5), Some(30)]);
}

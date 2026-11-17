mod assets;
mod evidence;
mod fuzz;
mod reorgs;
mod rules;
mod submission;
mod system;
mod vouchers;
mod watches;

use std::collections::HashMap;

use futures::executor::block_on;

use super::*;
use crate::{
	chain::{EncodedExtrinsic, HashAndNumber, MortalExtrinsic, Mortality, extrinsic_service_serving},
	coinage::{
		CoinSecret, PrivacyPolicy, PurseId,
		ledger::AssetKind,
		registration::{Admission, Rows},
		testing::{
			ENTROPY, FakeChain, FixedSeed, NOW, NoPerson, NoRegistryWriter, PEOPLE_METADATA_BYTES, TestWallet, chain_params, coin_read,
			register_rows, unbuilt_engine,
		},
	},
	durable::{DurableTxStatus, SubmissionPlan, Window},
};

fn deps(wallet: &TestWallet, chain: Arc<FakeChain>, running: Running) -> CoinageDeps {
	CoinageDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		clock: Arc::new(|| NOW),
		people: chain,
		people_genesis: H256([0xab; 32]),
		asset_hub_genesis: H256([0xcd; 32]),
		extrinsics: Arc::new(extrinsic_service_serving(PEOPLE_METADATA_BYTES, 0)),
		personhood: Arc::new(NoPerson),
		running,
		registry: Some(Arc::new(NoRegistryWriter)),
	}
}

fn account(index: u32) -> [u8; 32] {
	coin_key(&ENTROPY, PurseId::MAIN, crate::installation::InstallationId([9; 32]), index).unwrap().1
}

fn entry_for(record: DurableTxId) -> DurableTxEntry {
	DurableTxEntry {
		id: record,
		domain: coinage_domain(),
		group: None,
		tx_hash: H256::zero(),
		window: Window { birth: 1, period: 64 },
		status: DurableTxStatus::Pending,
		scanned_to: None,
		plan: None,
	}
}

fn chain_coin(value: i8, locked_until: Option<u64>) -> ChainCoin {
	ChainCoin { instance: 0, value, age: 0, locked_until }
}

/// A transfer of the coin at index 0 into the coin at index 1, registered
/// and pending.
fn transfer(wallet: &TestWallet) -> DurableTxId {
	let input = wallet.coin(2);
	let output = wallet.view().assets(PurseId::MAIN).count() as u32;
	let outputs = [crate::coinage::Exponent::new(2).unwrap()];
	let output_asset = block_on(wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, AssetKind::Coin, &outputs))).unwrap()[0];
	assert_eq!(output_asset, Asset::Coin { purse: PurseId::MAIN, installation: crate::installation::InstallationId([9; 32]), index: output });
	let engine = unbuilt_engine(wallet.db.clone());
	let operation = OpKey { scope: b"p".to_vec(), id: [1; 32] };
	let rows = Rows { inputs: vec![input], outputs: vec![output_asset], tokens: Vec::new() };
	register_rows(&engine, &operation, Admission::Ordinary, vec![rows]).unwrap()[0]
}

fn judged(wallet: &TestWallet, chain: Arc<FakeChain>, record: DurableTxId) -> (bool, bool) {
	let oracle = CoinageOracle(Arc::new(deps(wallet, chain.clone(), Running::new())));
	let heads = block_on(chain.heads()).unwrap();
	let pass = block_on(oracle.open_pass(&[entry_for(record)], &heads)).ok().unwrap();
	let entry = entry_for(record);
	(pass.proven_completed(&entry, HeadKind::Finalized), pass.proven_not_completed(&entry, HeadKind::Finalized))
}

#[test]
fn a_present_output_proves_completion() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let record = transfer(&wallet);
	let chain = Arc::new(FakeChain::default());
	chain.coins.lock().insert(account(1), chain_coin(2, None));
	assert_eq!(judged(&wallet, chain, record), (true, false));
}

#[test]
fn an_available_input_proves_the_transfer_has_not_completed() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let record = transfer(&wallet);
	let chain = Arc::new(FakeChain::default());
	chain.coins.lock().insert(account(0), chain_coin(2, None));
	assert_eq!(judged(&wallet, chain, record), (false, true));
}

#[test]
fn a_locked_input_shows_a_failed_dispatch_so_nothing_is_claimed() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let record = transfer(&wallet);
	let chain = Arc::new(FakeChain::default());
	chain.coins.lock().insert(account(0), chain_coin(2, Some(NOW + 60)));
	assert_eq!(judged(&wallet, chain, record), (false, false));
}

#[test]
fn a_coin_of_another_value_at_an_output_account_proves_nothing() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let record = transfer(&wallet);
	let chain = Arc::new(FakeChain::default());
	chain.coins.lock().insert(account(1), chain_coin(5, None));
	assert_eq!(judged(&wallet, chain, record), (false, false));
}

#[test]
fn a_token_paid_unload_is_not_retried_after_its_dispatch_failed() {
	let unload = RecordParams { call: Call::UnloadToCoins, retry: Retry::Until(u64::MAX) };
	assert!(!retries(unload, VerdictFailureKind::DispatchFailed));
	assert!(retries(unload, VerdictFailureKind::Expired));
	let transfer = RecordParams { call: Call::Transfer, retry: Retry::ExpiryOnly };
	assert!(retries(transfer, VerdictFailureKind::Expired));
	assert!(!retries(transfer, VerdictFailureKind::Rejected));
	assert!(!retries(RecordParams { retry: Retry::Never, ..transfer }, VerdictFailureKind::Expired));
}

#[test]
fn an_until_deadline_shortens_the_window() {
	let params = chain_params();
	assert_eq!(window_period(&params, Retry::ExpiryOnly, NOW), 64);
	assert_eq!(window_period(&params, Retry::Until(NOW + 6 * 20), NOW), 16);
	assert_eq!(window_period(&params, Retry::Until(NOW), NOW), 4, "never below the shortest era");
}

#[test]
fn nothing_is_built_before_running_opens() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let record = transfer(&wallet);
	wallet.hold(PurseId::MAIN, AssetKind::Coin, 0, coin_read(0));
	let running = Running::new();
	let policy = CoinagePolicy(Arc::new(deps(&wallet, Arc::new(FakeChain::default()), running.clone())));
	let params = RecordParams { call: Call::Transfer, retry: Retry::ExpiryOnly }.to_params();
	let waiting = DeferredDurableTx {
		id: record,
		domain: coinage_domain(),
		group: None,
		plan: SubmissionPlan { params },
		attempts: 0,
	};
	assert!(block_on(policy.prepare(core::slice::from_ref(&waiting))).unwrap().is_empty());
	running.open(RunningState { params: chain_params(), check_failed: true, funding: HashMap::new() });
	assert!(block_on(policy.prepare(&[waiting])).unwrap().is_empty(), "nor while the runtime check fails");
}

/// Until the installation is in the registry, value placed under it could
/// be found only through this store, so nothing is built (G3 invariant 4).
#[test]
fn nothing_is_built_before_the_installation_is_registered() {
	let wallet = TestWallet::unregistered(PrivacyPolicy::fastest());
	let record = transfer(&wallet);
	let running = Running::new();
	running.open(RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() });
	let policy = CoinagePolicy(Arc::new(deps(&wallet, Arc::new(FakeChain::default()), running)));
	let params = RecordParams { call: Call::Transfer, retry: Retry::ExpiryOnly }.to_params();
	let waiting =
		DeferredDurableTx { id: record, domain: coinage_domain(), group: None, plan: SubmissionPlan { params }, attempts: 0 };
	let answered = || block_on(policy.prepare(core::slice::from_ref(&waiting))).unwrap().contains_key(&record);
	assert!(!answered());
	wallet.register_installation();
	assert!(answered(), "once the registration landed, the record is decided");
}

/// A host that runs no registry has nothing to wait for, so its wallet is
/// built at once, as NFT purses are, and stays unrecoverable from the seed.
#[test]
fn without_a_registry_nothing_waits_for_a_registration() {
	let wallet = TestWallet::unregistered(PrivacyPolicy::fastest());
	let record = transfer(&wallet);
	let running = Running::new();
	running.open(RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() });
	let policy = CoinagePolicy(Arc::new(CoinageDeps { registry: None, ..deps(&wallet, Arc::new(FakeChain::default()), running) }));
	let params = RecordParams { call: Call::Transfer, retry: Retry::ExpiryOnly }.to_params();
	let waiting =
		DeferredDurableTx { id: record, domain: coinage_domain(), group: None, plan: SubmissionPlan { params }, attempts: 0 };
	assert!(block_on(policy.prepare(&[waiting])).unwrap().contains_key(&record));
}

/// Captures what the registry policy asks the writer to append.
#[derive(Default)]
struct RecordingWriter(parking_lot::Mutex<Vec<(CoinSecret, Vec<u8>)>>);

#[async_trait::async_trait]
impl RegistryWriter for RecordingWriter {
	async fn append(&self, datastore: &CoinSecret, record: &[u8]) -> Result<MortalExtrinsic, String> {
		self.0.lock().push((datastore.clone(), record.to_vec()));
		let birth = HashAndNumber { hash: H256([1; 32]), number: 1 };
		Ok(MortalExtrinsic { extrinsic: EncodedExtrinsic::new(b"append".to_vec()), mortality: Mortality::new(birth, 64).unwrap() })
	}
}

/// The append the shipped wallets read back: the installation sealed under
/// the seed's registry key, sent by the `//datastore` account.
#[test]
fn the_registration_appends_the_installation_sealed_for_the_registry() {
	let wallet = TestWallet::unregistered(PrivacyPolicy::fastest());
	let writer = Arc::new(RecordingWriter::default());
	let running = Running::new();
	let policy = RegistryPolicy(Arc::new(CoinageDeps {
		registry: Some(writer.clone()),
		..deps(&wallet, Arc::new(FakeChain::default()), running.clone())
	}));
	let installation = crate::installation::InstallationId([9; 32]);
	let waiting = |id, params| DeferredDurableTx {
		id: DurableTxId(id),
		domain: registry_domain(),
		group: None,
		plan: SubmissionPlan { params },
		attempts: 0,
	};
	let append = RecordParams { call: Call::AppendRegistryRecord(installation), retry: Retry::ExpiryOnly }.to_params();
	let batch = [waiting(1, append), waiting(2, PolicyParams(vec![0xff]))];
	assert!(block_on(policy.prepare(&batch)).unwrap().is_empty(), "nothing before the start sequence");

	running.open(RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() });
	let answers = block_on(policy.prepare(&batch)).unwrap();
	assert!(matches!(answers.get(&DurableTxId(1)), Some(Preparation::Ready { params: None, .. })));
	assert!(matches!(answers.get(&DurableTxId(2)), Some(Preparation::GiveUp)));
	let (datastore, _) = datastore_key(&ENTROPY).unwrap();
	let sealed = RegistryKey::of(&datastore).unwrap().seal(installation).to_vec();
	assert_eq!(*writer.0.lock(), vec![(datastore, sealed)]);
}

/// The chain checks an unload's aliases only at dispatch, where a failure
/// would still spend the free token, so the policy checks them first.
#[test]
fn an_unload_is_given_up_once_an_alias_is_unloaded_and_waits_while_one_is_locked() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let entry = wallet.entry(3, 7);
	let engine = unbuilt_engine(wallet.db.clone());
	let key = OpKey::own([1; 32]);
	let rows = vec![Rows { inputs: vec![entry], outputs: Vec::new(), tokens: Vec::new() }];
	let record = register_rows(&engine, &key, Admission::Ordinary, rows).unwrap()[0];
	let chain = Arc::new(FakeChain::default());
	let deps = deps(&wallet, chain.clone(), Running::new());
	let view = wallet.view();
	let facts = view.registrations().record(record).unwrap().clone();
	let round = BuildRound {
		view,
		heads: block_on(chain.heads()).unwrap(),
		vrf: block_on(load_vrf()).unwrap(),
		running: RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() },
		entropy: ENTROPY.to_vec(),
		now: NOW,
	};
	let Asset::Entry { purse, installation, index } = entry else { unreachable!("held as an entry") };
	let alias = round.vrf.alias(&entry_seed(&ENTROPY, purse, installation, index).0, &assembly::UNLOADING_RECYCLER_CONTEXT).unwrap();
	let check = || block_on(alias_check(&deps, &facts, (3, 7), &round));

	assert!(matches!(check(), Some(AliasCheck::Clear)));
	chain.aliases.lock().insert(alias, AliasState::Locked(NOW + 10));
	assert!(check().is_none(), "a locked alias makes the record wait");
	chain.aliases.lock().insert(alias, AliasState::Unloaded);
	assert!(matches!(check(), Some(AliasCheck::Consumed)));
}

#[test]
fn the_installation_registry_domain_lives_on_asset_hub() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let registry = with_coinage(DurableRegistry::new(), deps(&wallet, Arc::new(FakeChain::default()), Running::new()));
	assert_eq!(
		(registry.chain(&registry_domain()), registry.chain(&coinage_domain())),
		(Some(H256([0xcd; 32])), Some(H256([0xab; 32])))
	);
}

/// An onboarding entry was never in a ring, so no unload can have consumed
/// it; a suspended one holds no ring index to read its alias under, and may
/// have been unloaded before it left its ring (invariant 2).
#[test]
fn only_an_onboarding_entry_proves_its_alias_is_not_unloaded() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let entry = wallet.entry(3, 7);
	let engine = unbuilt_engine(wallet.db.clone());
	let rows = vec![Rows { inputs: vec![entry], outputs: Vec::new(), tokens: Vec::new() }];
	let record = register_rows(&engine, &OpKey::own([1; 32]), Admission::Ordinary, rows).unwrap()[0];
	let vrf = block_on(load_vrf()).unwrap();
	let Asset::Entry { purse, installation, index } = entry else { unreachable!("held as an entry") };
	let member = vrf.member(&entry_seed(&ENTROPY, purse, installation, index).0).unwrap();
	let judged_at = |position| {
		let chain = Arc::new(FakeChain::default());
		chain.unplaced.lock().insert(member, (3, position));
		judged(&wallet, chain, record)
	};
	assert_eq!([judged_at(RingPosition::Onboarding), judged_at(RingPosition::Suspended)], [(false, true), (false, false)]);
}

mod use_cases;

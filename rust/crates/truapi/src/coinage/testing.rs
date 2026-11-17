//! Fixtures shared by the coinage layer's tests.

pub mod funding;
pub mod planning;
pub mod scans;
pub mod world;

use core::time::Duration;
use std::{collections::HashMap, sync::Arc};

use futures::executor::block_on;
use parity_scale_codec::Decode;
use subxt::{Metadata, utils::H256};

use super::{
	Asset, ChainConfig, ChainParams, Exponent, PrivacyPolicy, PurseId, Snapshot,
	chain::{AliasState, ChainCoin, CoinageChain, Emptying, RingPlace, RingPosition},
	layer::{Interfaces, Layer, Randomness, RuntimeFacts, RuntimeSource, start},
	plugins::{PersonhoodSource, Running, Seed},
	reconstruction::{InstallationRegistry, ScanConfig},
	ledger::{
		AssetKind, BlockRef, CoinOnChain, EntryOnChain, Facts, Read, Subject, allocate, mint_installation, observe,
		snapshot,
	},
	operation::OpKey,
	params::{InstanceRecord, validate},
	registration::{Admission, RegisterError, Rows, coinage_domain, register, registry_domain},
};
use crate::{
	chain::{HashAndNumber, Heads, StorageWatchError},
	chain_runtime::RuntimeFailure,
	runtime::statement_allowance::{PersonhoodSigner, ring::RingParams},
	durable::{
		DeferredDurableTx, DurableRegistry, DurableRequest, DurableTxEngine, DurableTxEntry, DurableTxId,
		PolicyError, PolicyParams, Preparation, RegistrationError, SubmissionPolicy, Unobservable,
		VerdictFailureKind,
	},
	installation::InstallationId,
	store::Db,
};

/// Metadata of the Paseo Next People chain, which runs the coinage pallet.
pub const PEOPLE_METADATA_BYTES: &[u8] =
	include_bytes!("../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

/// [`PEOPLE_METADATA_BYTES`], decoded.
pub fn people_metadata() -> Metadata {
	let prefixed =
		frame_metadata::RuntimeMetadataPrefixed::decode(&mut &PEOPLE_METADATA_BYTES[..])
			.expect("the People fixture decodes");
	Metadata::try_from(prefixed).expect("the People fixture is subxt metadata")
}

/// A configuration with the shipped values for the Paseo Next People chain.
pub fn chain_config() -> ChainConfig {
	ChainConfig {
		instance: 0,
		extension_version: 0,
		ring_lifetime: Duration::from_secs(90 * 24 * 3600),
		ring_page_size: 255,
		token_grace: Duration::from_secs(3600),
		unload_allowance: 20,
		external_minimum_balance: 1,
		block_time: Duration::from_secs(6),
		rescue_margin: Duration::from_secs(7 * 24 * 3600),
		reserve_share: 20,
		fallback_age_cutoff: 14,
		longest_window: 64,
		max_claim_window: 256,
	}
}

/// The record of the instance [`chain_config`] names.
pub fn instance_record() -> InstanceRecord {
	InstanceRecord { asset_unit: 10_000 }
}

/// Parameters validated from the People fixture with [`chain_config`] and a
/// maximum coin age of 16.
pub fn chain_params() -> ChainParams {
	validate(&people_metadata(), Some(&instance_record()), Some(16), chain_config())
		.expect("the People fixture serves the layer")
}

/// A durability engine over `db` that serves both coinage domains and never
/// builds or decides anything, so records keep the status a test writes.
pub fn unbuilt_engine(db: Db) -> Arc<DurableTxEngine> {
	let registry = DurableRegistry::new()
		.with_domain(coinage_domain(), H256::zero(), Arc::new(Unobservable), Arc::new(NeverBuilds))
		.with_domain(registry_domain(), H256::zero(), Arc::new(Unobservable), Arc::new(NeverBuilds));
	crate::durable::engine_on(db, registry)
}

/// A policy that leaves every record waiting to be built.
struct NeverBuilds;

#[async_trait::async_trait]
impl SubmissionPolicy for NeverBuilds {
	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, _: VerdictFailureKind) -> bool {
		false
	}

	async fn prepare(&self, _: &[DeferredDurableTx]) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		Ok(HashMap::new())
	}
}

/// Registers one record per entry of `rows` under `operation`, writing C1's
/// rows in the registration callback, and returns their identities.
pub fn register_rows(
	engine: &Arc<DurableTxEngine>,
	operation: &OpKey,
	admission: Admission,
	rows: Vec<Rows>,
) -> Result<Vec<DurableTxId>, RegisterError> {
	let count = rows.len();
	let request = DurableRequest::deferred_in(coinage_domain(), Some(operation.group()), move |_| {
		Ok(vec![PolicyParams(Vec::new()); count])
	});
	let operation = operation.clone();
	let registered = block_on(engine.execute_with(request, move |tx, ids| {
		let records: Vec<_> = ids.iter().copied().zip(rows).collect();
		register(tx, &coinage_domain(), &operation, admission, &records).expect("the rows are readable")
	}));
	match registered {
		Ok((ids, ())) => Ok(ids),
		Err(RegistrationError::Hook(error)) => Err(error),
		Err(other) => panic!("the registration did not run: {other}"),
	}
}

/// The time tests classify at, in Unix seconds.
pub const NOW: u64 = 1_000_000;

/// The recovery phrase entropy the plug-in and observation tests derive
/// keys from.
pub const ENTROPY: [u8; 16] = [3; 16];

/// A seed that always gives [`ENTROPY`].
pub struct FixedSeed;

impl Seed for FixedSeed {
	fn entropy(&self) -> Option<Vec<u8>> {
		Some(ENTROPY.to_vec())
	}
}

/// No person, so no unload is built and no token checked.
pub struct NoPerson;

#[async_trait::async_trait]
impl PersonhoodSource for NoPerson {
	async fn person(&self) -> Option<(Arc<dyn PersonhoodSigner>, RingParams, u32)> {
		None
	}
}

/// A chain whose coins and ring places a test sets, the same at both heads;
/// every read fails while `failing` is set.
#[derive(Default)]
pub struct FakeChain {
	/// The coin at each account.
	pub coins: parking_lot::Mutex<HashMap<[u8; 32], ChainCoin>>,
	/// The exponent and ring place of each member key in a ring.
	pub places: parking_lot::Mutex<HashMap<[u8; 32], (i8, RingPlace)>>,
	/// The exponent and position of each member key in no ring: onboarding
	/// or suspended.
	pub unplaced: parking_lot::Mutex<HashMap<[u8; 32], (i8, RingPosition)>>,
	/// The state of each alias that has one.
	pub aliases: parking_lot::Mutex<HashMap<[u8; 32], AliasState>>,
	/// Every read fails.
	pub failing: core::sync::atomic::AtomicBool,
	/// What each account holds of the external asset; none for the rest.
	pub funds: parking_lot::Mutex<HashMap<[u8; 32], u128>>,
	/// The transaction that emptied each account, and the block it is in.
	pub emptied: parking_lot::Mutex<HashMap<[u8; 32], Emptying>>,
	/// The finalized height, when a test moves it from block 10.
	pub finalized: parking_lot::Mutex<Option<u64>>,
}

impl FakeChain {
	fn check(&self) -> Result<(), StorageWatchError> {
		if self.failing.load(core::sync::atomic::Ordering::SeqCst) {
			let failure = RuntimeFailure::host_failure("fake_chain", "the test fails every read");
			return Err(StorageWatchError::Chain(failure));
		}
		Ok(())
	}
}

#[async_trait::async_trait]
impl CoinageChain for FakeChain {
	async fn heads(&self) -> Result<Heads, StorageWatchError> {
		let number = self.finalized.lock().unwrap_or(10);
		let block = HashAndNumber { hash: H256([1; 32]), number };
		Ok(Heads { finalized: block, best: block })
	}

	async fn coins_at(&self, _: H256, accounts: &[[u8; 32]]) -> Result<Vec<Option<ChainCoin>>, StorageWatchError> {
		self.check()?;
		let coins = self.coins.lock();
		Ok(accounts.iter().map(|account| coins.get(account).copied()).collect())
	}

	async fn members_at(&self, _: H256, members: &[[u8; 32]]) -> Result<Vec<Option<(u32, i8)>>, StorageWatchError> {
		self.check()?;
		let (places, unplaced) = (self.places.lock(), self.unplaced.lock());
		Ok(members
			.iter()
			.map(|member| {
				let exponent = places.get(member).map(|(exponent, _)| *exponent);
				exponent.or_else(|| unplaced.get(member).map(|(exponent, _)| *exponent)).map(|exponent| (0, exponent))
			})
			.collect())
	}

	async fn places_at(&self, _: H256, _: i8, members: &[[u8; 32]]) -> Result<Vec<Option<RingPosition>>, StorageWatchError> {
		self.check()?;
		let (places, unplaced) = (self.places.lock(), self.unplaced.lock());
		Ok(members
			.iter()
			.map(|member| {
				let included = places.get(member).map(|(_, place)| RingPosition::Included(*place));
				included.or_else(|| unplaced.get(member).map(|(_, position)| *position))
			})
			.collect())
	}

	async fn aliases_at(&self, _: H256, _: i8, _: u32, aliases: &[[u8; 32]]) -> Result<Vec<Option<AliasState>>, StorageWatchError> {
		self.check()?;
		let states = self.aliases.lock();
		Ok(aliases.iter().map(|alias| states.get(alias).copied()).collect())
	}

	async fn ring_members_at(&self, _: H256, _: i8, _: u32) -> Result<Vec<[u8; 32]>, StorageWatchError> {
		Ok(Vec::new())
	}

	async fn tokens_consumed_at(&self, _: H256, tokens: &[(u32, [u8; 32])]) -> Result<Vec<bool>, StorageWatchError> {
		Ok(vec![false; tokens.len()])
	}

	async fn nonce_at(&self, _: H256, _: [u8; 32]) -> Result<u32, StorageWatchError> {
		Ok(0)
	}

	async fn funds_at(&self, _: H256, account: [u8; 32]) -> Result<u128, StorageWatchError> {
		self.check()?;
		Ok(self.funds.lock().get(&account).copied().unwrap_or(0))
	}

	async fn emptied_by(
		&self,
		account: [u8; 32],
		since: u64,
		at: HashAndNumber,
	) -> Result<Option<Emptying>, StorageWatchError> {
		self.check()?;
		let emptied = self.emptied.lock().get(&account).copied();
		Ok(emptied.filter(|emptying| since < emptying.block && emptying.block <= at.number))
	}
}

/// A store with a minted installation, and helpers to hold assets in it.
pub struct TestWallet {
	/// The store.
	pub db: Db,
}

impl TestWallet {
	/// A wallet with the given policy.
	pub fn new(policy: PrivacyPolicy) -> Self {
		let db = crate::store::open_memory_core_db();
		block_on(db.write(move |tx| {
			mint_installation(tx, InstallationId([9; 32]), &policy, None, 0)?.expect("a fresh store");
			Ok(())
		}))
		.expect("the installation is minted");
		Self { db }
	}

	/// An asset allocated in `purse`, observed as `read` a little before
	/// [`NOW`].
	pub fn hold(&self, purse: PurseId, kind: AssetKind, exponent: u8, read: Read) -> Asset {
		let outputs = [Exponent::new(exponent).expect("a legal exponent")];
		let asset = block_on(self.db.write(move |tx| allocate(tx, purse, kind, &outputs))).expect("allocated")[0];
		let facts = Facts { at: BlockRef { number: 1, hash: [1; 32] }, reads: vec![(Subject::Asset(asset), read)] };
		block_on(self.db.write(move |tx| observe(tx, &facts, NOW - 100, 255))).expect("observed");
		asset
	}

	/// A spendable coin of the main purse.
	pub fn coin(&self, exponent: u8) -> Asset {
		self.hold(PurseId::MAIN, AssetKind::Coin, exponent, coin_read(exponent))
	}

	/// A spendable entry of the main purse, in `ring`.
	pub fn entry(&self, exponent: u8, ring: u32) -> Asset {
		self.hold(PurseId::MAIN, AssetKind::Entry, exponent, entry_read(ring))
	}

	/// The store as committed now.
	pub fn view(&self) -> Snapshot {
		block_on(self.db.read(snapshot)).expect("a snapshot")
	}
}

/// A young, unlocked coin as F1 reads it.
pub fn coin_read(exponent: u8) -> Read {
	Read::Coin {
		now: Some(CoinOnChain { exponent: Exponent::new(exponent).expect("a legal exponent"), age: 0, locked_until: None }),
		emptied_by: None,
	}
}

/// An entry whose key the root of `ring` includes, as F1 reads it.
pub fn entry_read(ring: u32) -> Read {
	Read::Entry(Some(EntryOnChain {
		ring,
		revision: 1,
		page: 0,
		position: 0,
		members: 10,
		included: 10,
		closed: false,
		expires_at: None,
		locked_until: None,
	}))
}

/// Writes the durability layer's status of `record`; a success is recorded
/// at block 1.
pub fn set_status(db: &Db, record: DurableTxId, status: &'static str, failure: Option<&'static str>) {
	let success = status.ends_with("SUCCESS");
	block_on(db.write(move |tx| {
		tx.execute(
			"UPDATE durable_tx SET status = ?1, failure_kind = ?2, success_number = ?3, success_hash = ?4,
			 tx_hash = ?5 WHERE id = ?6",
			rusqlite::params![
				status,
				failure,
				success.then_some(1),
				success.then_some([1u8; 32].to_vec()),
				[record.0 as u8; 32].to_vec(),
				record.0,
			],
		)?;
		Ok(())
	}))
	.expect("the status is written");
}

/// A runtime that passes A3's check until `failing` is set.
#[derive(Default)]
pub struct TestRuntime {
	/// Every read fails.
	pub failing: core::sync::atomic::AtomicBool,
	/// The spec version, which stays readable while the rest fails.
	pub version: core::sync::atomic::AtomicU32,
}

#[async_trait::async_trait]
impl RuntimeSource for TestRuntime {
	async fn runtime(&self, _: Option<HashAndNumber>) -> Option<RuntimeFacts> {
		if self.failing.load(core::sync::atomic::Ordering::SeqCst) {
			return None;
		}
		let version = self.version.load(core::sync::atomic::Ordering::SeqCst);
		Some(RuntimeFacts { metadata: Arc::new(people_metadata()), instance: Some(instance_record()), maximum_age: Some(16), version })
	}

	async fn version(&self, _: HashAndNumber) -> Option<u32> {
		Some(self.version.load(core::sync::atomic::Ordering::SeqCst))
	}
}

/// Draws that mint the installation `[7; 32]`.
pub struct TestDraws;

impl Randomness for TestDraws {
	fn installation(&self) -> InstallationId {
		InstallationId([7; 32])
	}
}

/// A registry whose read always fails.
pub struct NoRegistry;

#[async_trait::async_trait]
impl InstallationRegistry for NoRegistry {
	async fn records(&self) -> Option<Vec<Vec<u8>>> {
		None
	}
}

/// The block tests revalidate at.
pub const BLOCK: HashAndNumber = HashAndNumber { hash: H256([2; 32]), number: 20 };

/// A small reconstruction scan.
pub const SCAN: ScanConfig = ScanConfig { batch: 4, gap: 1, deep_gap: 2, purse_gap: 1 };

/// The interfaces a test layer starts with, over `store`, at [`NOW`].
pub fn layer_interfaces(store: Db, runtime: Arc<TestRuntime>) -> Interfaces {
	Interfaces {
		engine: unbuilt_engine(store.clone()),
		store,
		chain: Arc::new(FakeChain::default()),
		runtime,
		seed: Arc::new(FixedSeed),
		clock: Arc::new(|| NOW),
		randomness: Arc::new(TestDraws),
		personhood: Arc::new(NoPerson),
		registry: Arc::new(NoRegistry),
		spawner: crate::Spawner::new(tokio::runtime::Handle::current()),
		policy: PrivacyPolicy::fastest(),
	}
}

/// A layer started over `store`, with its running handle and its runtime.
pub async fn started_layer(store: Db) -> (Layer, Running, Arc<TestRuntime>) {
	let runtime = Arc::new(TestRuntime::default());
	let running = Running::new();
	let layer = start(layer_interfaces(store, runtime.clone()), chain_config(), SCAN, None, running.clone())
		.await
		.expect("the layer starts");
	(layer, running, runtime)
}

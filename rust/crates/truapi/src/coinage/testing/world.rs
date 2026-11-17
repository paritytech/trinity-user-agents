//! [`World`]: the durability engine with the coinage plug-ins over a test
//! wallet, on a [`WorldChain`], and the verbs a scenario drives them with.
//!
//! Every record is registered through C1 with real record parameters. By
//! default its first attempt is a scripted extrinsic born at the finalized
//! head, so a scenario controls what the chain includes; every later attempt,
//! and every record registered with [`World::register_unbuilt`], is built by
//! the real coinage policy. Passes run only when the scenario runs one.

mod chain;

pub use chain::{CoinageState, Faults, Tree, WorldChain};

use core::{
	sync::atomic::{AtomicU64, Ordering},
	time::Duration,
};
use std::{
	collections::HashMap,
	sync::{Arc, Weak},
};

use futures::executor::block_on;
use parking_lot::Mutex;
use subxt::utils::H256;

use super::{
	ENTROPY, FixedSeed, NOW, NoPerson, NoRegistry, PEOPLE_METADATA_BYTES, SCAN, TestDraws, TestRuntime, TestWallet,
	chain_config, chain_params,
};
use crate::{
	Spawner,
	chain::{DispatchOutcome, EncodedExtrinsic, HashAndNumber, MortalExtrinsic, Mortality, extrinsic_service_serving},
	coinage::{
		Asset, Exponent, PrivacyPolicy, PurseId,
		chain::{AliasState, ChainCoin, RingPlace, RingPosition},
		layer::{Interfaces, start},
		ledger::{self, AssetKind, Mark, Snapshot},
		namespace::{CoinSecret, coin_key, entry_seed},
		observation::{ObserverDeps, on_block},
		operation::{OpKey, Retry},
		planning::Call,
		plugins::{CoinageDeps, Running, RunningState, with_coinage},
		registration::{Admission, RegisterError, Rows, coinage_domain, register, registry_domain},
		start::RecordParams,
	},
	durable::{
		self, CompletionOracle, DeferredDurableTx, DurableDeps, DurableRegistry, DurableRequest, DurableTxEngine,
		DurableTxEntry, DurableTxFailureKind, DurableTxId, DurableTxStatus, HeadKind, PolicyError, PolicyParams, Preparation,
		RegistrationError, SubmissionPolicy, VerdictFailureKind,
	},
	installation::InstallationId,
	runtime::{Vrf, load_vrf},
	test_support::wait_until,
	time::ManualTimer,
};

/// The People chain's genesis in a world.
const PEOPLE_GENESIS: H256 = H256([0xab; 32]);

/// Asset Hub's genesis in a world.
const ASSET_HUB_GENESIS: H256 = H256([0xcd; 32]);

/// The installation every world asset belongs to, as [`TestWallet`] mints it.
const INSTALLATION: InstallationId = InstallationId([9; 32]);

/// The exponent of a coin a scenario names without one.
pub const COIN_EXPONENT: u8 = 2;

/// The exponent of an entry a scenario names without one.
pub const ENTRY_EXPONENT: u8 = 3;

/// The window period of a scripted first attempt, in blocks, unless a
/// scenario sets another with [`World::set_period`].
pub const PERIOD: u64 = 128;

/// The seconds a block takes, by which each new block moves the clock.
const BLOCK_SECONDS: u64 = 6;

/// The recovery phrase entropy the peers who pay a world derive their coins
/// from.
const PEER_ENTROPY: [u8; 16] = [5; 16];

/// Whether an action's block is finalized or only on the best chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finality {
	/// The block is finalized along with everything below it.
	Finalized,
	/// The block is only on the best chain.
	InBest,
}

/// Where a record stands, without the block a success names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
	/// Waiting to be built.
	Waiting,
	/// Submitted, with no evidence either way.
	Pending,
	/// Took effect at a block that is not finalized yet.
	PendingSuccess,
	/// Took effect at a finalized block.
	FinalizedSuccess,
	/// Can no longer take effect.
	Failed(DurableTxFailureKind),
}

impl From<DurableTxStatus> for Standing {
	fn from(status: DurableTxStatus) -> Self {
		match status {
			DurableTxStatus::PendingSubmission => Self::Waiting,
			DurableTxStatus::Pending => Self::Pending,
			DurableTxStatus::PendingSuccess { .. } => Self::PendingSuccess,
			DurableTxStatus::FinalizedSuccess { .. } => Self::FinalizedSuccess,
			DurableTxStatus::Failure(kind) => Self::Failed(kind),
		}
	}
}

/// What the oracle proves of one record at one head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proof {
	/// `proven_completed`.
	pub completed: bool,
	/// `proven_not_completed`.
	pub not_completed: bool,
}

/// The durability engine with the coinage plug-ins over a test wallet, on a
/// [`WorldChain`].
pub struct World {
	chain: Arc<WorldChain>,
	wallet: TestWallet,
	timer: Arc<ManualTimer>,
	running: Running,
	clock: Arc<AtomicU64>,
	scripted: Arc<Mutex<HashMap<DurableTxId, MortalExtrinsic>>>,
	asked: Arc<Mutex<Vec<Vec<DurableTxId>>>>,
	engine: Arc<DurableTxEngine>,
	oracle: Arc<dyn CompletionOracle>,
	recovery_requests: futures::channel::mpsc::Receiver<()>,
	vrf: Vrf,
	/// Each coin's account, derived once: a derivation is slow.
	accounts: Mutex<HashMap<Asset, [u8; 32]>>,
	/// Each entry's member key and alias, derived once.
	entry_keys: Mutex<HashMap<Asset, [[u8; 32]; 2]>>,
	coins: HashMap<u32, Asset>,
	entries: HashMap<u32, Asset>,
	operations: u32,
	extrinsics: u32,
	period: u64,
}

impl Default for World {
	fn default() -> Self {
		Self::new()
	}
}

impl World {
	/// A world over an empty chain at genesis, with the running handle open.
	pub fn new() -> Self {
		let chain = WorldChain::new();
		let wallet = TestWallet::new(PrivacyPolicy::fastest());
		let running = Running::new();
		running.open(RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() });
		let clock = Arc::new(AtomicU64::new(NOW));
		let scripted = Arc::default();
		let asked = Arc::default();
		let timer = Arc::new(ManualTimer::default());
		let (registry, oracle) = registry(&chain, &wallet, &running, &clock, &scripted, &asked);
		let engine = engine(&chain, &wallet, &timer, registry);
		let recovery_requests = engine.recovery_requests();
		Self {
			chain,
			wallet,
			timer,
			running,
			clock,
			scripted,
			asked,
			engine,
			oracle,
			recovery_requests,
			vrf: block_on(load_vrf()).expect("the ring-VRF operations load"),
			accounts: Mutex::default(),
			entry_keys: Mutex::default(),
			coins: HashMap::new(),
			entries: HashMap::new(),
			operations: 0,
			extrinsics: 0,
			period: PERIOD,
		}
	}

	/// The chain.
	pub fn chain(&self) -> &Arc<WorldChain> {
		&self.chain
	}

	/// The engine.
	pub fn engine(&self) -> &Arc<DurableTxEngine> {
		&self.engine
	}

	/// The engine's clock, which paces the submission executor.
	pub fn timer(&self) -> &ManualTimer {
		&self.timer
	}

	/// The store.
	pub fn db(&self) -> &crate::store::Db {
		&self.wallet.db
	}

	/// The store as committed now.
	pub fn view(&self) -> Snapshot {
		self.wallet.view()
	}

	/// The time the plug-ins read, in Unix seconds.
	pub fn now(&self) -> u64 {
		self.clock.load(Ordering::SeqCst)
	}

	/// Moves the plug-ins' clock forward by `seconds`.
	pub fn advance_clock(&self, seconds: u64) {
		self.clock.fetch_add(seconds, Ordering::SeqCst);
	}

	// ---- assets ------------------------------------------------------------

	/// Own coin `n` of [`COIN_EXPONENT`], allocated in the main purse at its
	/// first mention.
	pub fn coin(&mut self, n: u32) -> Asset {
		self.coin_of(n, COIN_EXPONENT)
	}

	/// Own coin `n`, allocated in the main purse with `exponent` at its first
	/// mention.
	pub fn coin_of(&mut self, n: u32, exponent: u8) -> Asset {
		if let Some(asset) = self.coins.get(&n) {
			return *asset;
		}
		let asset = self.allocate(AssetKind::Coin, exponent);
		self.coins.insert(n, asset);
		asset
	}

	/// Own entry `n` of [`ENTRY_EXPONENT`], allocated in the main purse at
	/// its first mention.
	pub fn entry(&mut self, n: u32) -> Asset {
		self.entry_of(n, ENTRY_EXPONENT)
	}

	/// Own entry `n`, allocated in the main purse with `exponent` at its
	/// first mention.
	pub fn entry_of(&mut self, n: u32, exponent: u8) -> Asset {
		if let Some(asset) = self.entries.get(&n) {
			return *asset;
		}
		let asset = self.allocate(AssetKind::Entry, exponent);
		self.entries.insert(n, asset);
		asset
	}

	/// An own asset's exponent, as the chain stores it.
	pub fn exponent_of(&self, asset: Asset) -> i8 {
		let exponent = self.view().asset(&asset).expect("an own asset").exponent.get();
		i8::try_from(exponent).expect("a small exponent")
	}

	/// Coin `n` of a peer, whose secret the wallet keeps for an import.
	pub fn peer_coin(&self, n: u32) -> Asset {
		let (secret, account) = peer_key(n);
		block_on(self.wallet.db.write(move |tx| ledger::keep_secrets(tx, &[(account, secret)]))).expect("the secret is kept");
		Asset::PeerCoin(account)
	}

	fn allocate(&self, kind: AssetKind, exponent: u8) -> Asset {
		let exponents = [Exponent::new(exponent).expect("a legal exponent")];
		let write = self.wallet.db.write(move |tx| ledger::allocate(tx, PurseId::MAIN, kind, &exponents));
		block_on(write).expect("allocated")[0]
	}

	/// The account a coin's key controls on chain.
	pub fn account(&self, coin: Asset) -> [u8; 32] {
		*self.accounts.lock().entry(coin).or_insert_with(|| match coin {
			Asset::Coin { purse, installation, index } => {
				coin_key(&ENTROPY, purse, installation, index).expect("the entropy derives keys").1
			},
			Asset::PeerCoin(account) => account,
			Asset::Entry { .. } => panic!("an entry has no account"),
		})
	}

	/// `coin` as the chain stores it once minted: a peer's coin has
	/// [`COIN_EXPONENT`].
	pub fn chain_coin(&self, coin: Asset) -> ChainCoin {
		let exponent = self.view().asset(&coin).map_or(COIN_EXPONENT, |record| record.exponent.get());
		ChainCoin { instance: 0, value: i8::try_from(exponent).expect("a small exponent"), age: 0, locked_until: None }
	}

	/// An entry's member key and its alias for unloading.
	pub fn entry_keys(&self, entry: Asset) -> ([u8; 32], [u8; 32]) {
		let [member, alias] = *self.entry_keys.lock().entry(entry).or_insert_with(|| {
			let Asset::Entry { purse, installation, index } = entry else { panic!("not an entry") };
			let seed = entry_seed(&ENTROPY, purse, installation, index);
			let member = self.vrf.member(&seed.0).expect("a member key");
			let alias = self.vrf.alias(&seed.0, &crate::coinage::assembly::UNLOADING_RECYCLER_CONTEXT).expect("an alias");
			[member, alias]
		});
		(member, alias)
	}

	// ---- moving the chain --------------------------------------------------

	/// Produces one block that changes the state as `change` does.
	pub fn produce(&self, finality: Finality, change: impl FnOnce(&mut CoinageState)) -> HashAndNumber {
		self.produce_with(&[], finality, change)
	}

	fn produce_with(
		&self,
		body: &[(H256, DispatchOutcome)],
		finality: Finality,
		change: impl FnOnce(&mut CoinageState),
	) -> HashAndNumber {
		let block = self.chain.produce(body, change);
		self.advance_clock(BLOCK_SECONDS);
		if finality == Finality::Finalized {
			self.chain.finalize_to_best();
		}
		block
	}

	/// Produces `count` empty blocks.
	pub fn advance(&self, count: u64, finality: Finality) {
		for _ in 0..count {
			self.produce(Finality::InBest, |_| {});
		}
		if finality == Finality::Finalized {
			self.finalize();
		}
	}

	/// Finalizes every block up to the best head.
	pub fn finalize(&self) {
		self.chain.finalize_to_best();
	}

	/// Drops the `depth` blocks at the top of the best chain.
	pub fn reorg(&self, depth: u64) {
		self.chain.reorg(depth);
	}

	/// Mints each of `coins` in one block.
	pub fn mint(&self, coins: &[Asset], finality: Finality) -> HashAndNumber {
		let minted: Vec<([u8; 32], ChainCoin)> = coins.iter().map(|coin| (self.account(*coin), self.chain_coin(*coin))).collect();
		self.produce(finality, |state| state.coins.extend(minted))
	}

	/// Empties `coin`'s account, as a spend by anyone does.
	pub fn consume(&self, coin: Asset, finality: Finality) -> HashAndNumber {
		let account = self.account(coin);
		self.produce(finality, |state| {
			state.coins.remove(&account);
		})
	}

	/// Places `entry` in `ring` of its collection.
	pub fn place(&self, entry: Asset, ring: u32, finality: Finality) {
		self.position(entry, RingPosition::Included(ring_place(ring)), finality);
	}

	/// Puts `entry` at `position` in its collection.
	pub fn position(&self, entry: Asset, position: RingPosition, finality: Finality) {
		let (member, _) = self.entry_keys(entry);
		self.produce(finality, |state| {
			state.members.insert(member, (self.exponent_of(entry), position));
		});
	}

	/// Removes `entry` from its collection, as archiving its ring does.
	pub fn archive(&self, entry: Asset, finality: Finality) {
		let (member, _) = self.entry_keys(entry);
		self.produce(finality, |state| {
			state.members.remove(&member);
		});
	}

	/// Marks `entry`'s alias unloaded in the ring it is in at the best head.
	pub fn unload_alias(&self, entry: Asset, finality: Finality) {
		let ring = self.ring_of(entry).expect("the entry is in a ring");
		self.unload_alias_in(entry, ring, finality);
	}

	/// Marks `entry`'s alias unloaded in `ring`, whether or not it is in it.
	pub fn unload_alias_in(&self, entry: Asset, ring: u32, finality: Finality) {
		let (_, alias) = self.entry_keys(entry);
		self.produce(finality, |state| {
			state.aliases.insert((self.exponent_of(entry), ring, alias), AliasState::Unloaded);
		});
	}

	fn ring_of(&self, entry: Asset) -> Option<u32> {
		let (member, _) = self.entry_keys(entry);
		match self.chain.tree().best_state().members.get(&member)? {
			(_, RingPosition::Included(place)) => Some(place.ring),
			_ => None,
		}
	}

	/// Puts `id`'s current attempt in a new block with `outcome`, changing
	/// nothing else.
	pub fn include(&self, id: DurableTxId, outcome: DispatchOutcome, finality: Finality) -> HashAndNumber {
		let tx = self.attempt(id).tx_hash;
		self.produce_with(&[(tx, outcome)], finality, |_| {})
	}

	/// Includes `id`'s current attempt with a successful dispatch and applies
	/// its effects: its input coins are spent, its input entries' aliases
	/// unloaded, its output coins minted and its output entries onboarded.
	pub fn execute(&self, id: DurableTxId, finality: Finality) -> HashAndNumber {
		let tx = self.attempt(id).tx_hash;
		let rows = self.rows(id);
		let inputs: Vec<(Asset, Option<u32>)> = rows.inputs.iter().map(|input| (*input, self.ring_or_none(*input))).collect();
		let effects = |state: &mut CoinageState| {
			for (input, ring) in &inputs {
				match input {
					Asset::Entry { .. } => {
						let (_, alias) = self.entry_keys(*input);
						let ring = ring.expect("an unloaded entry is in a ring");
						state.aliases.insert((self.exponent_of(*input), ring, alias), AliasState::Unloaded);
					},
					_ => {
						state.coins.remove(&self.account(*input));
					},
				}
			}
			for output in &rows.outputs {
				match output {
					Asset::Entry { .. } => {
						let (member, _) = self.entry_keys(*output);
						state.members.insert(member, (self.exponent_of(*output), RingPosition::Onboarding));
					},
					_ => {
						state.coins.insert(self.account(*output), self.chain_coin(*output));
					},
				}
			}
		};
		self.produce_with(&[(tx, DispatchOutcome::Succeeded)], finality, effects)
	}

	fn ring_or_none(&self, asset: Asset) -> Option<u32> {
		matches!(asset, Asset::Entry { .. }).then(|| self.ring_of(asset)).flatten()
	}

	/// Produces blocks until the best head is one past the death of `id`'s
	/// current attempt, so a finalized head there closes its window.
	pub fn reach_mortality_of(&self, id: DurableTxId, finality: Finality) {
		let target = self.attempt(id).window.death() + 1;
		while self.chain.tree().best().number < target {
			self.produce(Finality::InBest, |_| {});
		}
		if finality == Finality::Finalized {
			self.finalize();
		}
	}

	/// Has F1 observe the finalized head, as it does at each new one.
	pub fn observe(&self) {
		let deps = ObserverDeps {
			store: self.wallet.db.clone(),
			seed: Arc::new(FixedSeed),
			chain: self.chain.clone(),
			personhood: Arc::new(NoPerson),
		};
		let finalized = self.chain.tree().finalized();
		block_on(on_block(&deps, finalized, self.now(), &chain_params())).expect("the block is observed");
	}

	// ---- registering -------------------------------------------------------

	/// Gives every later scripted attempt a window of `period` blocks, a
	/// power of two from 4.
	pub fn set_period(&mut self, period: u64) {
		self.period = period;
	}

	/// Registers one record taking `inputs` and creating `outputs`, with the
	/// call their shape implies and no retries, and waits for its scripted
	/// first attempt to be watched.
	pub fn register(&mut self, inputs: &[Asset], outputs: &[Asset]) -> Result<DurableTxId, RegisterError> {
		let params = RecordParams { call: call_for(inputs, outputs), retry: Retry::Never };
		self.register_with(Admission::Ordinary, vec![rows(inputs, outputs)], params, true).map(|ids| ids[0])
	}

	/// [`Self::register`], retried after an expiry: the shape Android gives a
	/// policy.
	pub fn register_retriable(&mut self, inputs: &[Asset], outputs: &[Asset]) -> Result<DurableTxId, RegisterError> {
		let params = RecordParams { call: call_for(inputs, outputs), retry: Retry::ExpiryOnly };
		self.register_with(Admission::Ordinary, vec![rows(inputs, outputs)], params, true).map(|ids| ids[0])
	}

	/// Registers one record left for the real policy to build, as E3 does.
	pub fn register_unbuilt(&mut self, inputs: &[Asset], outputs: &[Asset], retry: Retry) -> Result<DurableTxId, RegisterError> {
		let params = RecordParams { call: call_for(inputs, outputs), retry };
		self.register_with(Admission::Ordinary, vec![rows(inputs, outputs)], params, false).map(|ids| ids[0])
	}

	/// Registers one record per entry of `records` under one fresh operation,
	/// all with `params`, admitted as `admission`. With `scripted`, each gets
	/// a scripted first attempt and this waits until each is watched.
	pub fn register_with(
		&mut self,
		admission: Admission,
		records: Vec<Rows>,
		params: RecordParams,
		scripted: bool,
	) -> Result<Vec<DurableTxId>, RegisterError> {
		self.operations += 1;
		let mut id = [0u8; 32];
		id[..4].copy_from_slice(&self.operations.to_be_bytes());
		let operation = OpKey { scope: b"world".to_vec(), id };
		let attempts: Vec<MortalExtrinsic> = if scripted { (0..records.len()).map(|_| self.next_extrinsic()).collect() } else { Vec::new() };
		let hashes: Vec<H256> = attempts.iter().map(|attempt| attempt.extrinsic.hash()).collect();
		let count = records.len();
		let request = DurableRequest::deferred_in(coinage_domain(), Some(operation.group()), move |_| {
			Ok(vec![params.to_params(); count])
		});
		let queue = self.scripted.clone();
		let registered = block_on(self.engine.execute_with(request, move |tx, ids| {
			let paired: Vec<(DurableTxId, Rows)> = ids.iter().copied().zip(records).collect();
			register(tx, &coinage_domain(), &operation, admission, &paired).expect("the rows are readable")?;
			queue.lock().extend(ids.iter().copied().zip(attempts));
			Ok(())
		}));
		let ids = match registered {
			Ok((ids, ())) => ids,
			Err(RegistrationError::Hook(refusal)) => return Err(refusal),
			Err(other) => panic!("the registration did not run: {other}"),
		};
		for (id, tx) in ids.iter().zip(hashes) {
			wait_until(
				|| self.status(*id) != DurableTxStatus::PendingSubmission && self.submitted(tx),
				"the scripted attempt is submitted",
			);
		}
		Ok(ids)
	}

	fn submitted(&self, tx: H256) -> bool {
		self.chain.tree().submitted.iter().any(|extrinsic| extrinsic.hash() == tx)
	}

	fn next_extrinsic(&mut self) -> MortalExtrinsic {
		self.extrinsics += 1;
		let mut bytes = b"world".to_vec();
		bytes.extend(self.extrinsics.to_be_bytes());
		let birth = self.chain.tree().finalized();
		MortalExtrinsic {
			extrinsic: EncodedExtrinsic::new(bytes),
			mortality: Mortality::new(birth, self.period).expect("a legal era"),
		}
	}

	/// Ends every open watch, again while one of `ids` is still watched, so a
	/// watch that starts after the first ending is ended too, and waits until
	/// no watch owns `ids`, so a pass decides them.
	pub fn release(&self, ids: &[DurableTxId]) {
		for id in ids {
			wait_until(
				|| {
					self.chain.end_watches();
					!self.engine.is_watched(*id)
				},
				"the watch releases the record",
			);
		}
	}

	/// Registers a transfer of `input` into `output` and releases its watch.
	pub fn unwatched(&mut self, input: Asset, output: Asset) -> DurableTxId {
		let id = self.register(&[input], &[output]).expect("registered");
		self.release(&[id]);
		id
	}

	/// [`Self::unwatched`], with its transaction executed on chain in a block
	/// of `finality`; the ledger has not been told.
	pub fn executed(&mut self, input: Asset, output: Asset, finality: Finality) -> DurableTxId {
		let id = self.unwatched(input, output);
		self.execute(id, finality);
		id
	}

	/// [`Self::executed`], followed by the pass that decides it.
	pub fn decided(&mut self, input: Asset, output: Asset, finality: Finality) -> DurableTxId {
		let id = self.executed(input, output, finality);
		self.run_pass();
		id
	}

	/// Writes an export's disclosure mark on `asset`, committed when
	/// `committed`.
	pub fn disclose(&self, asset: Asset, committed: bool) -> Result<(), Asset> {
		block_on(self.wallet.db.write(move |tx| {
			if let Err(refused) = ledger::mark_disclosed(tx, &[asset])? {
				return Ok(Err(refused));
			}
			if committed {
				ledger::commit_disclosed(tx, &[asset])?;
			}
			Ok(Ok(()))
		}))
		.expect("the store writes")
	}

	// ---- recovery ----------------------------------------------------------

	/// Runs one recovery pass.
	pub fn run_pass(&self) {
		block_on(self.engine.run_pass());
	}

	/// Whether the engine asked the host for recovery since the last call.
	pub fn recovery_requested(&mut self) -> bool {
		self.settle();
		let mut requested = false;
		while self.recovery_requests.try_recv().is_ok() {
			requested = true;
		}
		requested
	}

	/// Moves the engine's clock past the longest rebuild cooldown once a
	/// submission worker sleeps out a cooldown, so a record handed back to
	/// its policy is built again. Every open watch falls silent with it.
	///
	/// A watch's silence timeout is 30 seconds and stays listed after its
	/// watch ends; the first cooldowns are shorter, which is how the worker's
	/// sleep is told apart.
	pub fn pass_rebuild_cooldown(&self) {
		let cooling = || self.timer.remaining().iter().any(|left| *left < Duration::from_secs(30));
		wait_until(cooling, "a submission worker sleeps out a cooldown");
		self.timer.advance(durable::MAX_REBUILD_COOLDOWN);
	}

	/// Waits until `count` extrinsics were submitted in all.
	pub fn wait_for_submissions(&self, count: usize) {
		wait_until(|| self.chain.tree().submitted.len() >= count, "the attempts are submitted");
	}

	/// The batches the real policy was asked to build, in order.
	pub fn asked(&self) -> Vec<Vec<DurableTxId>> {
		self.asked.lock().clone()
	}

	/// Lets queued background work run, so a test can check that something
	/// did not happen.
	pub fn settle(&self) {
		std::thread::sleep(Duration::from_millis(50));
	}

	/// Drops the engine as a crash does, with every watch and the ownership
	/// it held, and starts a new one over the same store, the running handle
	/// open as G3 leaves it but without G3's other steps.
	pub fn crash(&mut self) {
		self.stop_engine();
		self.running = Running::new();
		self.running.open(RunningState { params: chain_params(), check_failed: false, funding: HashMap::new() });
		self.launch();
	}

	/// [`Self::crash`], then G3's start sequence, which releases every
	/// uncommitted export and opens the running handle.
	pub fn relaunch(&mut self) {
		self.stop_engine();
		self.running = Running::new();
		self.launch();
		let interfaces = Interfaces {
			engine: self.engine.clone(),
			store: self.wallet.db.clone(),
			chain: self.chain.clone(),
			runtime: Arc::new(TestRuntime::default()),
			seed: Arc::new(FixedSeed),
			clock: clock_of(&self.clock),
			randomness: Arc::new(TestDraws),
			personhood: Arc::new(NoPerson),
			registry: Arc::new(NoRegistry),
			spawner: Spawner::for_test(),
			policy: PrivacyPolicy::fastest(),
		};
		block_on(start(interfaces, chain_config(), SCAN, None, self.running.clone())).expect("the layer starts");
	}

	fn stop_engine(&mut self) {
		self.chain.end_watches();
		self.scripted.lock().clear();
		let stopped: Weak<DurableTxEngine> = Arc::downgrade(&self.engine);
		let placeholder = engine(&self.chain, &self.wallet, &self.timer, DurableRegistry::new());
		drop(core::mem::replace(&mut self.engine, placeholder));
		wait_until(|| stopped.upgrade().is_none(), "the crashed engine stops");
	}

	fn launch(&mut self) {
		let (registry, oracle) = registry(&self.chain, &self.wallet, &self.running, &self.clock, &self.scripted, &self.asked);
		self.oracle = oracle;
		self.engine = engine(&self.chain, &self.wallet, &self.timer, registry);
		self.recovery_requests = self.engine.recovery_requests();
	}

	// ---- reading -----------------------------------------------------------

	/// Where `id` stands.
	pub fn status(&self, id: DurableTxId) -> DurableTxStatus {
		block_on(self.engine.status(id)).expect("the ledger reads").expect("the record exists")
	}

	/// Where `id` stands, without the block a success names.
	pub fn standing(&self, id: DurableTxId) -> Standing {
		self.status(id).into()
	}

	/// Waits until `id` stands at `status`.
	pub fn wait_for(&self, id: DurableTxId, status: DurableTxStatus) {
		wait_until(|| self.status(id) == status, "the record reaches its status");
	}

	/// `id`'s current attempt.
	pub fn attempt(&self, id: DurableTxId) -> DurableTxEntry {
		self.try_attempt(id).expect("the record has an attempt")
	}

	/// `id`'s current attempt, or none before its first.
	pub fn try_attempt(&self, id: DurableTxId) -> Option<DurableTxEntry> {
		let read = self.wallet.db.read(move |conn| durable::entry(conn, id));
		block_on(read).expect("the ledger reads")
	}

	/// What `id` consumes, creates and presents.
	pub fn rows(&self, id: DurableTxId) -> Rows {
		self.view().registrations().record(id).expect("a coinage record").rows.clone()
	}

	/// The status of the record that holds `asset` as an input: the one that
	/// has not failed, else the latest.
	pub fn consumer(&self, asset: Asset) -> Option<Standing> {
		let view = self.view();
		let standings: Vec<Standing> = view.registrations().consumers(&asset).map(|facts| facts.state.status.into()).collect();
		let live = standings.iter().find(|standing| !matches!(standing, Standing::Failed(_)));
		live.or(standings.last()).copied()
	}

	/// How many records that have not failed hold `asset` as an input.
	pub fn holders(&self, asset: Asset) -> usize {
		let view = self.view();
		let holders = view.registrations().consumers(&asset).filter(|facts| !matches!(facts.state.status, DurableTxStatus::Failure(_)));
		holders.count()
	}

	/// The status of the record that creates `asset`.
	pub fn minter(&self, asset: Asset) -> Option<Standing> {
		self.view().registrations().creator(&asset).map(|facts| facts.state.status.into())
	}

	/// `asset`'s disclosure mark.
	pub fn mark(&self, asset: Asset) -> Option<Mark> {
		self.view().mark(&asset)
	}

	/// What the coinage oracle proves of `id` at the finalized head and at
	/// the best head, read now.
	pub fn proofs(&self, id: DurableTxId) -> [Proof; 2] {
		let entry = self.attempt(id);
		let heads = block_on(crate::chain::ChainHeads::heads(self.chain.as_ref(), PEOPLE_GENESIS)).expect("the heads read");
		let pass = block_on(self.oracle.open_pass(core::slice::from_ref(&entry), &heads)).expect("the pass opens");
		let at = |head: fn() -> HeadKind| Proof {
			completed: pass.proven_completed(&entry, head()),
			not_completed: pass.proven_not_completed(&entry, head()),
		};
		[at(|| HeadKind::Finalized), at(|| HeadKind::Best)]
	}

	/// What the coinage oracle proves of `id` at the finalized head.
	pub fn proof(&self, id: DurableTxId) -> Proof {
		self.proofs(id)[0]
	}
}

/// Answers each scripted first attempt itself and hands every other record to
/// the real coinage policy.
struct ScriptedFirstAttempts {
	real: Arc<dyn SubmissionPolicy>,
	scripted: Arc<Mutex<HashMap<DurableTxId, MortalExtrinsic>>>,
	asked: Arc<Mutex<Vec<Vec<DurableTxId>>>>,
}

#[async_trait::async_trait]
impl SubmissionPolicy for ScriptedFirstAttempts {
	fn can_retry(&self, entry: &DurableTxEntry, params: &PolicyParams, failure: VerdictFailureKind) -> bool {
		self.real.can_retry(entry, params, failure)
	}

	async fn prepare(&self, batch: &[DeferredDurableTx]) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let mut answers = HashMap::new();
		let mut rest = Vec::new();
		{
			let mut scripted = self.scripted.lock();
			for waiting in batch {
				match scripted.remove(&waiting.id) {
					Some(extrinsic) => {
						answers.insert(waiting.id, Preparation::Ready { extrinsic, params: None });
					},
					None => rest.push(waiting.clone()),
				}
			}
		}
		if !rest.is_empty() {
			self.asked.lock().push(rest.iter().map(|waiting| waiting.id).collect());
			answers.extend(self.real.prepare(&rest).await?);
		}
		Ok(answers)
	}
}

/// Both coinage domains, with their real oracles, the `coinage` policy behind
/// [`ScriptedFirstAttempts`], and the `coinage` oracle.
fn registry(
	chain: &Arc<WorldChain>,
	wallet: &TestWallet,
	running: &Running,
	clock: &Arc<AtomicU64>,
	scripted: &Arc<Mutex<HashMap<DurableTxId, MortalExtrinsic>>>,
	asked: &Arc<Mutex<Vec<Vec<DurableTxId>>>>,
) -> (DurableRegistry, Arc<dyn CompletionOracle>) {
	let deps = CoinageDeps {
		store: wallet.db.clone(),
		seed: Arc::new(FixedSeed),
		clock: clock_of(clock),
		people: chain.clone(),
		people_genesis: PEOPLE_GENESIS,
		asset_hub_genesis: ASSET_HUB_GENESIS,
		extrinsics: Arc::new(extrinsic_service_serving(PEOPLE_METADATA_BYTES, 0)),
		personhood: Arc::new(NoPerson),
		running: running.clone(),
	};
	let real = with_coinage(DurableRegistry::new(), deps);
	let served = |domain| {
		let oracle = real.oracle(&domain).expect("served").clone();
		let policy = real.submission_policy(&domain).expect("served").clone();
		(oracle, policy)
	};
	let (oracle, policy) = served(coinage_domain());
	let (registry_oracle, registry_policy) = served(registry_domain());
	let policy = Arc::new(ScriptedFirstAttempts { real: policy, scripted: scripted.clone(), asked: asked.clone() });
	let registry = DurableRegistry::new()
		.with_domain(coinage_domain(), PEOPLE_GENESIS, oracle.clone(), policy)
		.with_domain(registry_domain(), ASSET_HUB_GENESIS, registry_oracle, registry_policy);
	(registry, oracle)
}

fn engine(chain: &Arc<WorldChain>, wallet: &TestWallet, timer: &Arc<ManualTimer>, registry: DurableRegistry) -> Arc<DurableTxEngine> {
	DurableTxEngine::new(DurableDeps {
		db: wallet.db.clone(),
		registry,
		heads: chain.clone(),
		blocks: chain.clone(),
		validator: chain.clone(),
		submitter: chain.clone(),
		timer: timer.clone(),
		spawner: Spawner::for_test(),
	})
}

fn clock_of(clock: &Arc<AtomicU64>) -> Arc<dyn Fn() -> u64 + Send + Sync> {
	let clock = clock.clone();
	Arc::new(move || clock.load(Ordering::SeqCst))
}

/// The call a record of this shape makes.
fn call_for(inputs: &[Asset], outputs: &[Asset]) -> Call {
	let entry = |asset: &Asset| matches!(asset, Asset::Entry { .. });
	if inputs.iter().any(entry) {
		Call::UnloadToCoins
	} else if inputs.is_empty() {
		Call::Load([0; 32])
	} else if outputs.iter().any(entry) {
		Call::Recycle
	} else if outputs.len() > 1 {
		Call::Split
	} else {
		Call::Transfer
	}
}

fn rows(inputs: &[Asset], outputs: &[Asset]) -> Rows {
	Rows { inputs: inputs.to_vec(), outputs: outputs.to_vec(), tokens: Vec::new() }
}


/// A ring place that includes its member.
pub fn ring_place(ring: u32) -> RingPlace {
	RingPlace { ring, page: 0, position: 0, members: 10, included: 10, immutable_since: None, revision: 1 }
}

fn peer_key(n: u32) -> (CoinSecret, [u8; 32]) {
	coin_key(&PEER_ENTROPY, PurseId::MAIN, INSTALLATION, n).expect("the peer's entropy derives keys")
}

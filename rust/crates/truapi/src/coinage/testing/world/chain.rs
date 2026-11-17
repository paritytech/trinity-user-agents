//! [`WorldChain`]: one chain as a tree of blocks, each carrying the coinage
//! state at it, behind every chain trait the durability engine and the
//! coinage plug-ins read through, with the read faults a scenario switches
//! on.

use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};

use futures::{
	channel::mpsc,
	stream::{BoxStream, StreamExt},
};
use parking_lot::{Mutex, MutexGuard};
use subxt::{
	tx::{TransactionValid, ValidationResult},
	utils::H256,
};

use crate::{
	chain::{
		BlockBackend, ChainHeads, DispatchOutcome, EncodedExtrinsic, HashAndNumber, HeadEvent, Heads,
		StorageWatchError, TxSubmitter, TxValidator, WatchEvent,
	},
	chain_runtime::RuntimeFailure,
	coinage::chain::{AliasState, ChainCoin, CoinageChain, Emptying, RingPosition},
};

/// The coinage state at one block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoinageState {
	/// The coin at each account.
	pub coins: HashMap<[u8; 32], ChainCoin>,
	/// The exponent and position of each recycler member key.
	pub members: HashMap<[u8; 32], (i8, RingPosition)>,
	/// The state of each alias, by exponent, ring and alias.
	pub aliases: HashMap<(i8, u32, [u8; 32]), AliasState>,
}

/// One block: its place in the tree, the state after it, and what it
/// dispatched.
struct Block {
	at: HashAndNumber,
	state: CoinageState,
	body: Vec<H256>,
	outcomes: HashMap<H256, DispatchOutcome>,
}

/// Which reads fail. A batched read fails as a whole when any key it names
/// is unreadable, as a storage read does.
#[derive(Debug, Default)]
pub struct Faults {
	/// Coin accounts whose reads fail at every block.
	pub coins: HashSet<[u8; 32]>,
	/// Blocks at which every coin read fails.
	pub coins_at: HashSet<H256>,
	/// Aliases whose reads fail.
	pub aliases: HashSet<[u8; 32]>,
	/// Every recycler membership read fails.
	pub memberships: bool,
	/// Every ring position read fails.
	pub positions: bool,
	/// Heights whose canonical hash cannot be read.
	pub heights: HashSet<u64>,
	/// No canonical hash can be read.
	pub every_height: bool,
	/// No block's number can be read from its hash.
	pub numbers: bool,
	/// Transactions whose dispatch outcome cannot be read.
	pub outcomes: HashSet<H256>,
	/// The heads cannot be read, so a pass reads nothing.
	pub heads: bool,
	/// No block body can be read, so the durability layer's search never
	/// decides anything while the heads and hashes still read.
	pub bodies: bool,
	/// Every submission fails before the node watches it.
	pub submissions: bool,
}

/// The tree, its heads, its faults and what was submitted to it.
pub struct Tree {
	blocks: Vec<Block>,
	/// The best chain, as indexes into `blocks`, from genesis.
	best: Vec<usize>,
	finalized: u64,
	/// The reads that fail.
	pub faults: Faults,
	/// What validation answers for every extrinsic.
	pub validation: ValidationResult,
	/// Every extrinsic submitted, in order.
	pub submitted: Vec<EncodedExtrinsic>,
	/// The open watch of each submitted extrinsic, by its hash.
	watches: HashMap<H256, mpsc::UnboundedSender<WatchEvent>>,
	/// How often the heads were read, which a pass does before anything else.
	pub heads_reads: usize,
}

impl Tree {
	fn block(&self, hash: H256) -> Option<&Block> {
		self.blocks.iter().find(|block| block.at.hash == hash)
	}

	fn best_block(&self) -> &Block {
		&self.blocks[*self.best.last().expect("the genesis block is always best or below")]
	}

	/// The best head.
	pub fn best(&self) -> HashAndNumber {
		self.best_block().at
	}

	/// The finalized head.
	pub fn finalized(&self) -> HashAndNumber {
		self.blocks[self.best[usize::try_from(self.finalized).expect("a small chain")]].at
	}

	/// The canonical block at `number`.
	pub fn canonical(&self, number: u64) -> Option<HashAndNumber> {
		let index = self.best.get(usize::try_from(number).ok()?)?;
		Some(self.blocks[*index].at)
	}

	/// The coinage state at the best head.
	pub fn best_state(&self) -> &CoinageState {
		&self.best_block().state
	}

	/// The coinage state at the finalized head.
	pub fn finalized_state(&self) -> &CoinageState {
		&self.blocks[self.best[usize::try_from(self.finalized).expect("a small chain")]].state
	}

	/// The extrinsics `block` includes, in order.
	pub fn body(&self, block: H256) -> Vec<H256> {
		self.block(block).map(|block| block.body.clone()).unwrap_or_default()
	}

	/// The outcome `block` recorded for `tx`, reorged out or not.
	pub fn outcome_in(&self, block: H256, tx: H256) -> Option<DispatchOutcome> {
		self.block(block)?.outcomes.get(&tx).cloned()
	}

	fn state_at(&self, at: H256) -> Result<&CoinageState, StorageWatchError> {
		self.block(at).map(|block| &block.state).ok_or_else(|| unreadable("an unknown block"))
	}
}

/// One chain, as [`Tree`] describes it.
pub struct WorldChain(Mutex<Tree>);

impl WorldChain {
	/// A chain with only its genesis block, which is final.
	pub fn new() -> Arc<Self> {
		let genesis = Block {
			at: HashAndNumber { hash: hash_of(0), number: 0 },
			state: CoinageState::default(),
			body: Vec::new(),
			outcomes: HashMap::new(),
		};
		Arc::new(Self(Mutex::new(Tree {
			blocks: vec![genesis],
			best: vec![0],
			finalized: 0,
			faults: Faults::default(),
			validation: ValidationResult::Valid(TransactionValid {
				priority: 0,
				requires: Vec::new(),
				provides: Vec::new(),
				longevity: 64,
				propagate: true,
			}),
			submitted: Vec::new(),
			watches: HashMap::new(),
			heads_reads: 0,
		})))
	}

	/// The tree, to change or inspect.
	pub fn tree(&self) -> MutexGuard<'_, Tree> {
		self.0.lock()
	}

	/// A new best block on top of the best head, dispatching `body` and
	/// holding the best head's state as `change` leaves it.
	pub fn produce(&self, body: &[(H256, DispatchOutcome)], change: impl FnOnce(&mut CoinageState)) -> HashAndNumber {
		let mut tree = self.0.lock();
		let mut state = tree.best_state().clone();
		change(&mut state);
		let index = tree.blocks.len();
		let at = HashAndNumber { hash: hash_of(index), number: tree.best().number + 1 };
		tree.blocks.push(Block {
			at,
			state,
			body: body.iter().map(|(tx, _)| *tx).collect(),
			outcomes: body.iter().cloned().collect(),
		});
		tree.best.push(index);
		at
	}

	/// Finalizes every block up to the best head.
	pub fn finalize_to_best(&self) {
		let mut tree = self.0.lock();
		tree.finalized = tree.best().number;
	}

	/// Drops the `depth` blocks at the top of the best chain, which must all
	/// be above the finalized head. They stay readable by hash.
	pub fn reorg(&self, depth: u64) {
		let mut tree = self.0.lock();
		let keep = tree.best().number - depth;
		assert!(keep >= tree.finalized, "a reorg never reaches below the finalized head");
		tree.best.truncate(usize::try_from(keep + 1).expect("a small chain"));
	}

	/// The events the watch of `tx` reports, sent by the test, or none when
	/// it was never submitted or its watch ended.
	pub fn watch(&self, tx: H256) -> Option<mpsc::UnboundedSender<WatchEvent>> {
		self.0.lock().watches.get(&tx).cloned()
	}

	/// Ends every open watch without a verdict, as a dropped connection does.
	pub fn end_watches(&self) {
		self.0.lock().watches.clear();
	}
}

/// The hash of the block at `index` of the tree, unique whatever its number.
fn hash_of(index: usize) -> H256 {
	let mut hash = [0xb1; 32];
	hash[..8].copy_from_slice(&u64::try_from(index).expect("a small chain").to_be_bytes());
	H256(hash)
}

fn failure(method: &'static str) -> RuntimeFailure {
	RuntimeFailure::host_failure(method, "the scenario fails this read")
}

fn unreadable(method: &'static str) -> StorageWatchError {
	StorageWatchError::Chain(failure(method))
}

#[async_trait::async_trait]
impl ChainHeads for WorldChain {
	async fn heads(&self, _: H256) -> Result<Heads, RuntimeFailure> {
		let mut tree = self.0.lock();
		tree.heads_reads += 1;
		if tree.faults.heads {
			return Err(failure("heads"));
		}
		Ok(Heads { finalized: tree.finalized(), best: tree.best() })
	}

	async fn head_events(&self, _: H256) -> Result<BoxStream<'static, Result<HeadEvent, RuntimeFailure>>, RuntimeFailure> {
		Err(failure("head_events"))
	}
}

#[async_trait::async_trait]
impl BlockBackend for WorldChain {
	async fn block_hash(&self, _: H256, number: u64) -> Result<Option<H256>, RuntimeFailure> {
		let tree = self.0.lock();
		if tree.faults.every_height || tree.faults.heights.contains(&number) {
			return Err(failure("block_hash"));
		}
		Ok(tree.canonical(number).map(|block| block.hash))
	}

	async fn block_number(&self, _: H256, hash: H256) -> Result<Option<u64>, RuntimeFailure> {
		let tree = self.0.lock();
		if tree.faults.numbers {
			return Err(failure("block_number"));
		}
		Ok(tree.block(hash).map(|block| block.at.number))
	}

	async fn extrinsic_hashes(&self, _: H256, at: H256) -> Result<Option<Vec<H256>>, RuntimeFailure> {
		let tree = self.0.lock();
		if tree.faults.bodies {
			return Err(failure("extrinsic_hashes"));
		}
		Ok(tree.block(at).map(|block| block.body.clone()))
	}

	async fn dispatch_outcome(
		&self,
		_: H256,
		at: HashAndNumber,
		tx: H256,
	) -> Result<Option<DispatchOutcome>, RuntimeFailure> {
		let tree = self.0.lock();
		if tree.faults.outcomes.contains(&tx) {
			return Err(failure("dispatch_outcome"));
		}
		Ok(tree.outcome_in(at.hash, tx))
	}
}

#[async_trait::async_trait]
impl TxValidator for WorldChain {
	async fn validate(&self, _: H256, _: &EncodedExtrinsic) -> Result<ValidationResult, RuntimeFailure> {
		Ok(self.0.lock().validation.clone())
	}
}

#[async_trait::async_trait]
impl TxSubmitter for WorldChain {
	async fn submit_and_watch(
		&self,
		_: H256,
		extrinsic: &EncodedExtrinsic,
	) -> Result<BoxStream<'static, WatchEvent>, RuntimeFailure> {
		let mut tree = self.0.lock();
		if tree.faults.submissions {
			return Err(failure("submit_and_watch"));
		}
		tree.submitted.push(extrinsic.clone());
		let (sender, receiver) = mpsc::unbounded();
		tree.watches.insert(extrinsic.hash(), sender);
		Ok(receiver.boxed())
	}
}

#[async_trait::async_trait]
impl CoinageChain for WorldChain {
	async fn heads(&self) -> Result<Heads, StorageWatchError> {
		let tree = self.0.lock();
		if tree.faults.heads {
			return Err(unreadable("heads"));
		}
		Ok(Heads { finalized: tree.finalized(), best: tree.best() })
	}

	async fn coins_at(&self, at: H256, accounts: &[[u8; 32]]) -> Result<Vec<Option<ChainCoin>>, StorageWatchError> {
		let tree = self.0.lock();
		let faults = &tree.faults;
		if faults.coins_at.contains(&at) || accounts.iter().any(|account| faults.coins.contains(account)) {
			return Err(unreadable("coins_at"));
		}
		let state = tree.state_at(at)?;
		Ok(accounts.iter().map(|account| state.coins.get(account).copied()).collect())
	}

	async fn members_at(&self, at: H256, members: &[[u8; 32]]) -> Result<Vec<Option<(u32, i8)>>, StorageWatchError> {
		let tree = self.0.lock();
		if tree.faults.memberships {
			return Err(unreadable("members_at"));
		}
		let state = tree.state_at(at)?;
		Ok(members.iter().map(|member| state.members.get(member).map(|(exponent, _)| (0, *exponent))).collect())
	}

	async fn places_at(
		&self,
		at: H256,
		exponent: i8,
		members: &[[u8; 32]],
	) -> Result<Vec<Option<RingPosition>>, StorageWatchError> {
		let tree = self.0.lock();
		if tree.faults.positions {
			return Err(unreadable("places_at"));
		}
		let state = tree.state_at(at)?;
		let position = |member| state.members.get(member).filter(|(loaded, _)| *loaded == exponent).map(|(_, at)| *at);
		Ok(members.iter().map(position).collect())
	}

	async fn aliases_at(
		&self,
		at: H256,
		exponent: i8,
		ring: u32,
		aliases: &[[u8; 32]],
	) -> Result<Vec<Option<AliasState>>, StorageWatchError> {
		let tree = self.0.lock();
		if aliases.iter().any(|alias| tree.faults.aliases.contains(alias)) {
			return Err(unreadable("aliases_at"));
		}
		let state = tree.state_at(at)?;
		Ok(aliases.iter().map(|alias| state.aliases.get(&(exponent, ring, *alias)).copied()).collect())
	}

	async fn ring_members_at(&self, at: H256, exponent: i8, ring: u32) -> Result<Vec<[u8; 32]>, StorageWatchError> {
		let tree = self.0.lock();
		let state = tree.state_at(at)?;
		let mut members: Vec<(u32, [u8; 32])> = state
			.members
			.iter()
			.filter_map(|(member, (loaded, position))| match position {
				RingPosition::Included(place) if *loaded == exponent && place.ring == ring => Some((place.position, *member)),
				_ => None,
			})
			.collect();
		members.sort();
		Ok(members.into_iter().map(|(_, member)| member).collect())
	}

	async fn unloaded_at(&self, at: H256, exponent: i8, ring: u32) -> Result<Option<u32>, StorageWatchError> {
		let tree = self.0.lock();
		let state = tree.state_at(at)?;
		let unloaded = state.aliases.iter().filter(|((loaded, in_ring, _), alias_state)| {
			*loaded == exponent && *in_ring == ring && **alias_state == AliasState::Unloaded
		});
		Ok(Some(u32::try_from(unloaded.count()).unwrap_or(u32::MAX)))
	}

	async fn tokens_consumed_at(&self, _: H256, tokens: &[(u32, [u8; 32])]) -> Result<Vec<bool>, StorageWatchError> {
		Ok(vec![false; tokens.len()])
	}

	async fn nonce_at(&self, _: H256, _: [u8; 32]) -> Result<u32, StorageWatchError> {
		Ok(0)
	}

	async fn funds_at(&self, _: H256, _: [u8; 32]) -> Result<u128, StorageWatchError> {
		Ok(0)
	}

	async fn emptied_by(&self, _: [u8; 32], _: u64, _: HashAndNumber) -> Result<Option<Emptying>, StorageWatchError> {
		Ok(None)
	}
}

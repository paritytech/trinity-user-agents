//! The pallet's storage entries, the values they store, and reading them at
//! one block.

use std::sync::Arc;

use subxt::{
	ext::scale_decode::DecodeAsType,
	storage::{DynamicAddress, dynamic},
	utils::{AccountId32, H256},
};

use crate::{
	chain::{ChainHeads, HashAndNumber, StorageWatchError, WatchedStorage},
	chain_runtime::RuntimeFailure,
	nft::{HeldItem, ItemKey},
};

/// The `Scarcity::NftsByOwner` entry, naming what each account holds.
pub fn nfts_by_owner() -> DynamicAddress<(AccountId32,), ScarcityNft> {
	dynamic("Scarcity", "NftsByOwner")
}

/// The `Scarcity::Instances` entry, naming who holds each instance.
fn instances_entry() -> DynamicAddress<(u64,), AccountId32> {
	dynamic("Scarcity", "Instances")
}

/// The `Scarcity::Locked` entry, naming the keys a failed transfer locked.
fn locked_entry() -> DynamicAddress<(AccountId32,), ScarcityLock> {
	dynamic("Scarcity", "Locked")
}

/// The `Timestamp::Now` entry, the chain time a lock compares against.
fn timestamp_now_entry() -> DynamicAddress<(), u64> {
	dynamic("Timestamp", "Now")
}

/// What one account holds, decoded from `Scarcity::NftsByOwner` by field
/// name. Every field of the pallet's `Nft` is listed, because a value only
/// decodes whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq, DecodeAsType)]
pub struct ScarcityNft {
	/// Global serial of the instance.
	pub instance: u64,
	/// Collection the instance belongs to.
	pub collection: u32,
	/// Item definition within the collection.
	pub item: u32,
	/// Unix seconds at mint.
	pub minted_at: u64,
	/// Unix seconds of the last move.
	pub last_moved: u64,
	/// Revision a transfer must sign. Read fresh when a transfer is built, never
	/// cached.
	pub state_nonce: u64,
	/// Fee-less moves already used.
	pub moves: u16,
}

impl From<ScarcityNft> for HeldItem {
	fn from(nft: ScarcityNft) -> Self {
		HeldItem { collection: nft.collection, item: nft.item, instance: nft.instance }
	}
}

/// A lock a failed transfer put on a key, decoded from `Scarcity::Locked`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, DecodeAsType)]
struct ScarcityLock {
	/// Failed transfers so far, which double the lock each time.
	retries: u8,
	/// Chain time, in milliseconds, at which the lock ends.
	until: u64,
}

/// The locks on some keys, with the chain time they compare against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyLocks {
	/// `Timestamp::Now` at the block, in milliseconds.
	pub now: u64,
	/// For each key, the chain time its lock ends at, if it has one.
	pub until: Vec<Option<u64>>,
}

/// Scarcity storage at one block, read fresh from the node.
#[async_trait::async_trait]
pub trait ScarcityChain: Send + Sync {
	/// What each of `keys` holds at `at`.
	async fn nfts_at(
		&self,
		at: H256,
		keys: &[ItemKey],
	) -> Result<Vec<Option<ScarcityNft>>, StorageWatchError>;

	/// Who holds each of `instances` at `at`.
	async fn holders_at(
		&self,
		at: H256,
		instances: &[u64],
	) -> Result<Vec<Option<ItemKey>>, StorageWatchError>;

	/// The lock on each of `keys` at `at`, and the chain time of `at`.
	async fn locks_at(&self, at: H256, keys: &[ItemKey]) -> Result<KeyLocks, StorageWatchError>;

	/// Where transfers built now stand: the finalized head they are born at and
	/// the best head their state is read at.
	async fn transfer_context(&self) -> Result<TransferContext, StorageWatchError>;
}

/// What a batch of transfers is built against. State is read at the best head,
/// where the tracker sees holdings, so a transfer is never judged on a finalized
/// block that does not hold its NFT yet. Its era is born at the finalized
/// head, which no reorg removes.
#[derive(Clone, Copy, Debug)]
pub struct TransferContext {
	/// The finalized head, where each transfer's era is born.
	pub birth: HashAndNumber,
	/// The best head, where the state each transfer depends on is read.
	pub best: HashAndNumber,
}

/// [`ScarcityChain`] over the storage and heads of the chain that runs the
/// pallet.
pub struct ScarcityStorage {
	storage: Arc<dyn WatchedStorage>,
	heads: Arc<dyn ChainHeads>,
	genesis: H256,
}

impl ScarcityStorage {
	/// Reads of the pallet on the chain with `genesis`.
	pub fn new(
		storage: Arc<dyn WatchedStorage>,
		heads: Arc<dyn ChainHeads>,
		genesis: H256,
	) -> Self {
		Self { storage, heads, genesis }
	}
}

#[async_trait::async_trait]
impl ScarcityChain for ScarcityStorage {
	async fn nfts_at(
		&self,
		at: H256,
		keys: &[ItemKey],
	) -> Result<Vec<Option<ScarcityNft>>, StorageWatchError> {
		let owners: Vec<_> = keys.iter().map(|key| (AccountId32(key.0),)).collect();
		self.storage.read_entries_at(self.genesis, at, &nfts_by_owner(), &owners).await
	}

	async fn holders_at(
		&self,
		at: H256,
		instances: &[u64],
	) -> Result<Vec<Option<ItemKey>>, StorageWatchError> {
		let instances: Vec<_> = instances.iter().map(|instance| (*instance,)).collect();
		let holders = self
			.storage
			.read_entries_at(self.genesis, at, &instances_entry(), &instances)
			.await?;
		Ok(holders
			.into_iter()
			.map(|holder| holder.map(|holder| ItemKey(holder.0)))
			.collect())
	}

	async fn locks_at(&self, at: H256, keys: &[ItemKey]) -> Result<KeyLocks, StorageWatchError> {
		let owners: Vec<_> = keys.iter().map(|key| (AccountId32(key.0),)).collect();
		let (locks_entry, time_entry) = (locked_entry(), timestamp_now_entry());
		let read_locks = self.storage.read_entries_at(self.genesis, at, &locks_entry, &owners);
		let read_now = self.storage.read_entries_at(self.genesis, at, &time_entry, &[()]);
		let (locks, now) = futures::future::try_join(read_locks, read_now).await?;
		let now = now.into_iter().flatten().next().ok_or_else(|| chain_time_missing(at))?;
		let until = locks.into_iter().map(|lock| lock.map(|lock| lock.until)).collect();
		Ok(KeyLocks { now, until })
	}

	async fn transfer_context(&self) -> Result<TransferContext, StorageWatchError> {
		let heads = self.heads.heads(self.genesis).await?;
		Ok(TransferContext { birth: heads.finalized, best: heads.best })
	}
}

/// The error for a block whose `Timestamp::Now` read came back empty, which
/// locks cannot be compared against.
fn chain_time_missing(at: H256) -> StorageWatchError {
	let reason = format!("Timestamp::Now is absent at {at:?}, expected the block's chain time");
	StorageWatchError::Chain(RuntimeFailure::host_failure("scarcity_locks_at", reason))
}

#[cfg(test)]
mod tests {
	use std::{collections::HashMap, sync::Arc};

	use futures::{executor::block_on, stream::BoxStream};
	use parity_scale_codec::Encode;
	use sp_crypto_hashing::{blake2_128, twox_64, twox_128};
	use subxt::{client::OnlineClientAtBlock, config::SubstrateConfig, utils::H256};

	use super::*;
	use crate::{
		chain::{
			DecodableAtBlock, HashAndNumber, Node, RawStorageChanges, StorageSnapshot, WatchedHead,
			serve,
		},
		chain_runtime::{ChainRuntime, RuntimeFailure},
		nft::scarcity::testing::{ASSET_HUB_METADATA_BYTES, asset_hub_metadata},
		time::{ManualTimer, Timer},
	};

	const GENESIS: H256 = H256([0xab; 32]);
	const AT: H256 = H256([0x05; 32]);

	/// What `HOLDER` holds on Paseo Asset Hub Next: instance 330 of item 13 in
	/// collection 27.
	const HOLDER_NFT: &[u8] =
		include_bytes!("../../../tests/fixtures/paseo-next-asset-hub-nfts-by-owner.scale");

	const HOLDER: ItemKey = ItemKey([0xfa; 32]);
	const EMPTY: ItemKey = ItemKey([0x01; 32]);

	fn map_key(entry: (&str, &str), hashed_key: Vec<u8>) -> Vec<u8> {
		let (pallet, name) = entry;
		[twox_128(pallet.as_bytes()).to_vec(), twox_128(name.as_bytes()).to_vec(), hashed_key]
			.concat()
	}

	/// A key under `Blake2_128Concat`, as `NftsByOwner` and `Locked` hash theirs.
	fn account_key(entry: (&str, &str), key: ItemKey) -> Vec<u8> {
		map_key(entry, [blake2_128(&key.0).to_vec(), key.0.to_vec()].concat())
	}

	/// An instance under `Twox64Concat`, as `Instances` hashes it.
	fn instance_key(instance: u64) -> Vec<u8> {
		let encoded = instance.encode();
		map_key(("Scarcity", "Instances"), [twox_64(&encoded).to_vec(), encoded].concat())
	}

	/// Storage served at [`AT`] by raw key. Keys are encoded with the Asset Hub
	/// runtime a scripted node serves, so a key encoded wrongly finds nothing.
	struct StoredAtBlock {
		chain: ChainRuntime,
		values: HashMap<Vec<u8>, Vec<u8>>,
	}

	impl StoredAtBlock {
		fn new(values: Vec<(Vec<u8>, Vec<u8>)>) -> Arc<Self> {
			let mut node = Node::new(3, 5);
			node.metadata = ASSET_HUB_METADATA_BYTES;
			let (_node, _provider, chain) = serve(node);
			Arc::new(Self { chain, values: values.into_iter().collect() })
		}
	}

	#[async_trait::async_trait]
	impl WatchedStorage for StoredAtBlock {
		async fn client_at_finalized_head(
			&self,
			genesis: H256,
		) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure> {
			self.chain.client_at_finalized_head(genesis).await
		}

		async fn client_at(
			&self,
			genesis: H256,
			at: HashAndNumber,
		) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure> {
			self.chain.client_at(genesis, at).await
		}

		async fn watch_raw_key_changes(
			&self,
			_genesis: H256,
			_head: WatchedHead,
			_keys: Vec<Vec<u8>>,
		) -> Result<BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>, RuntimeFailure>
		{
			Err(RuntimeFailure::host_failure("test", "this fake only reads at a block"))
		}

		async fn read_raw_keys_at(
			&self,
			_genesis: H256,
			at: H256,
			keys: Vec<Vec<u8>>,
		) -> Result<StorageSnapshot, RuntimeFailure> {
			assert_eq!(at, AT);
			let value = keys.iter().map(|key| self.values.get(key).cloned()).collect();
			let at = HashAndNumber { hash: at, number: 5 };
			Ok(DecodableAtBlock { at, metadata: asset_hub_metadata(), value })
		}

		fn timer(&self) -> Arc<dyn Timer> {
			Arc::new(ManualTimer::default())
		}
	}

	fn scarcity(values: Vec<(Vec<u8>, Vec<u8>)>) -> ScarcityStorage {
		let stored = StoredAtBlock::new(values);
		let heads = Arc::new(stored.chain.clone());
		ScarcityStorage::new(stored, heads, GENESIS)
	}

	#[test]
	fn reads_what_each_key_holds() {
		let holding = (account_key(("Scarcity", "NftsByOwner"), HOLDER), HOLDER_NFT.to_vec());
		let reads = scarcity(vec![holding]);

		let nfts = block_on(reads.nfts_at(AT, &[HOLDER, EMPTY])).unwrap();

		let held = nfts[0].map(HeldItem::from);
		assert_eq!(held, Some(HeldItem { collection: 27, item: 13, instance: 330 }));
		assert_eq!(nfts[1], None);
	}

	#[test]
	fn reads_who_holds_each_instance() {
		let reads = scarcity(vec![(instance_key(330), HOLDER.0.to_vec())]);

		let holders = block_on(reads.holders_at(AT, &[330, 331])).unwrap();

		assert_eq!(holders, vec![Some(HOLDER), None]);
	}

	#[test]
	fn reads_each_lock_with_the_chain_time() {
		let lock = ScarcityLockFixture { retries: 2, until: 1_700_000_120_000 };
		let reads = scarcity(vec![
			(account_key(("Scarcity", "Locked"), HOLDER), lock.encode()),
			(map_key(("Timestamp", "Now"), Vec::new()), 1_700_000_000_000_u64.encode()),
		]);

		let locks = block_on(reads.locks_at(AT, &[HOLDER, EMPTY])).unwrap();

		let expected =
			KeyLocks { now: 1_700_000_000_000, until: vec![Some(1_700_000_120_000), None] };
		assert_eq!(locks, expected);
	}

	/// `LockInfo` as the pallet encodes it.
	#[derive(Encode)]
	struct ScarcityLockFixture {
		retries: u8,
		until: u64,
	}
}

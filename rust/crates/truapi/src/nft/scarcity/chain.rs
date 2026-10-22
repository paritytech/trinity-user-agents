//! The pallet's storage entries, the values they store, and reading them at
//! one block.

use std::sync::Arc;

use subxt::{
	ext::scale_decode::DecodeAsType,
	storage::{DynamicAddress, dynamic},
	utils::{AccountId32, H256},
};

use super::transfer::MoveRuntime;
use crate::{
	chain::{HashAndNumber, StorageWatchError, WatchedStorage},
	nft::{HeldItem, ItemKey},
};

/// The `Scarcity::NftsByOwner` entry, naming what each account holds.
pub fn nfts_by_owner() -> DynamicAddress<(AccountId32,), ScarcityNft> {
	dynamic("Scarcity", "NftsByOwner")
}

/// The `Scarcity::Instances` entry, naming who holds each instance.
fn instance_holders() -> DynamicAddress<(u64,), AccountId32> {
	dynamic("Scarcity", "Instances")
}

/// The `Scarcity::Locked` entry, naming the keys a failed move locked.
fn key_locks() -> DynamicAddress<(AccountId32,), ScarcityLock> {
	dynamic("Scarcity", "Locked")
}

/// The `Timestamp::Now` entry, the chain time a lock compares against.
fn chain_time() -> DynamicAddress<(), u64> {
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
	/// Revision a move must sign. Read fresh when a move is built, never
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

/// A lock a failed move put on a key, decoded from `Scarcity::Locked`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, DecodeAsType)]
struct ScarcityLock {
	/// Failed moves so far, which double the lock each time.
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

	/// The finalized head, and the runtime a move born there is signed for.
	async fn finalized_runtime(&self) -> Result<(HashAndNumber, MoveRuntime), StorageWatchError>;
}

/// [`ScarcityChain`] over the storage of the chain that runs the pallet.
pub struct ScarcityStorage {
	storage: Arc<dyn WatchedStorage>,
	genesis: H256,
}

impl ScarcityStorage {
	/// Reads of the pallet on the chain with `genesis`.
	pub fn new(storage: Arc<dyn WatchedStorage>, genesis: H256) -> Self {
		Self { storage, genesis }
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
			.read_entries_at(self.genesis, at, &instance_holders(), &instances)
			.await?;
		Ok(holders
			.into_iter()
			.map(|holder| holder.map(|holder| ItemKey(holder.0)))
			.collect())
	}

	async fn locks_at(&self, at: H256, keys: &[ItemKey]) -> Result<KeyLocks, StorageWatchError> {
		let owners: Vec<_> = keys.iter().map(|key| (AccountId32(key.0),)).collect();
		let (locks_entry, time_entry) = (key_locks(), chain_time());
		let read_locks = self.storage.read_entries_at(self.genesis, at, &locks_entry, &owners);
		let read_now = self.storage.read_entries_at(self.genesis, at, &time_entry, &[()]);
		let (locks, now) = futures::future::try_join(read_locks, read_now).await?;
		let now = now.into_iter().flatten().next().unwrap_or_default();
		let until = locks.into_iter().map(|lock| lock.map(|lock| lock.until)).collect();
		Ok(KeyLocks { now, until })
	}

	async fn finalized_runtime(&self) -> Result<(HashAndNumber, MoveRuntime), StorageWatchError> {
		let client = self.storage.client_at_finalized_head(self.genesis).await?;
		let head = HashAndNumber { hash: client.block_hash(), number: client.block_number() };
		let runtime = MoveRuntime {
			genesis: self.genesis,
			spec_version: client.spec_version(),
			transaction_version: client.transaction_version(),
			metadata: client.metadata(),
		};
		Ok((head, runtime))
	}
}

#[cfg(test)]
mod tests;

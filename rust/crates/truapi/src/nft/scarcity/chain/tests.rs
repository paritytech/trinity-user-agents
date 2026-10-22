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
	include_bytes!("../../../../tests/fixtures/paseo-next-asset-hub-nfts-by-owner.scale");

const HOLDER: ItemKey = ItemKey([0xfa; 32]);
const EMPTY: ItemKey = ItemKey([0x01; 32]);

fn map_key(entry: (&str, &str), hashed_key: Vec<u8>) -> Vec<u8> {
	let (pallet, name) = entry;
	[twox_128(pallet.as_bytes()).to_vec(), twox_128(name.as_bytes()).to_vec(), hashed_key].concat()
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

	async fn watch_raw_key_changes(
		&self,
		_genesis: H256,
		_head: WatchedHead,
		_keys: Vec<Vec<u8>>,
	) -> Result<BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>, RuntimeFailure> {
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
	ScarcityStorage::new(StoredAtBlock::new(values), GENESIS)
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

	let expected = KeyLocks { now: 1_700_000_000_000, until: vec![Some(1_700_000_120_000), None] };
	assert_eq!(locks, expected);
}

/// `LockInfo` as the pallet encodes it.
#[derive(Encode)]
struct ScarcityLockFixture {
	retries: u8,
	until: u64,
}

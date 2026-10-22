//! Reading the watched keys: all of them at a block, or their changes from a
//! subscription, with the metadata of the runtime they belong to.

use core::future::ready;
use std::{collections::HashMap, sync::Arc};

use futures::stream::{StreamExt, TryStreamExt};
use parking_lot::Mutex;
use subxt::{ArcMetadata, ext::frame_decode::storage::encode_storage_key_prefix, utils::H256};
use subxt_rpcs::{client::RpcSubscription, methods::legacy::StorageChangeSet, rpc_params};

use super::{RawChangeStream, STORAGE_WATCH, all_keys_answered_or_err, group_values_by_key};
use crate::{
	chain::{
		DecodableAtBlock, HashAndNumber, RawStorageChanges,
		storage_watch::{StorageSnapshot, value_hash},
		subxt_chain::{failure, numbered},
	},
	chain_runtime::{ChainRuntime, LegacyConnection, RuntimeFailure},
};

/// The keys one subscription notification changed, by position among the
/// reader's keys, and the block it is for.
struct ChangedKeysAtBlock {
	block: H256,
	changed: Vec<(usize, Option<Vec<u8>>)>,
}

/// Reads the watched keys, keeping the metadata of the runtime it last saw.
///
/// Each subscription and each snapshot read asks the chain for its
/// connection, so a watch carries on over a new one after the old one closes.
pub struct WatchedKeysReader {
	chain: ChainRuntime,
	genesis: H256,
	/// `System.LastRuntimeUpgrade`, then the watched keys.
	keys: Vec<Vec<u8>>,
	/// The position of each of `keys`, to place the keys a notification names.
	position_by_key: HashMap<Vec<u8>, usize>,
	/// The hash of each key's last reported value, `None` before the first
	/// notification. Kept across subscriptions, so one opened again reports
	/// only what changed meanwhile.
	held_hashes: Mutex<Option<Vec<Option<[u8; 32]>>>>,
	last_seen_runtime: Mutex<Option<LastSeenRuntime>>,
}

/// The runtime a reader last saw, identified by its `System.LastRuntimeUpgrade`.
struct LastSeenRuntime {
	last_upgrade: Vec<u8>,
	metadata: ArcMetadata,
}

impl WatchedKeysReader {
	/// A reader of `watched` on the chain with `genesis`.
	pub fn new(chain: ChainRuntime, genesis: H256, watched: Vec<Vec<u8>>) -> Self {
		let upgrade_key = encode_storage_key_prefix("System", "LastRuntimeUpgrade").to_vec();
		let keys = [vec![upgrade_key], watched].concat();
		let position_by_key =
			keys.iter().cloned().enumerate().map(|(index, key)| (key, index)).collect();
		Self {
			chain,
			genesis,
			keys,
			position_by_key,
			held_hashes: Mutex::new(None),
			last_seen_runtime: Mutex::new(None),
		}
	}

	/// Every watched key at `at`, read in one `state_queryStorageAt` call.
	pub async fn read_snapshot_at(
		self: Arc<Self>,
		at: HashAndNumber,
	) -> Result<StorageSnapshot, RuntimeFailure> {
		let legacy = self.chain.legacy(self.genesis).await?;
		let queried_snapshot = legacy
			.methods
			.state_query_storage_at(self.keys.iter().map(Vec::as_slice), Some(at.hash))
			.await
			.map_err(|error| failure(STORAGE_WATCH, error))?;
		let values_by_key = group_values_by_key(queried_snapshot);
		let mut values = all_keys_answered_or_err(&self.keys, &values_by_key, at.hash)?;
		let last_upgrade = values.remove(0);
		let metadata = self.metadata_for_upgrade(&legacy, last_upgrade, at).await?;
		Ok(DecodableAtBlock { at, metadata, value: values })
	}

	/// A new `state_subscribeStorage` of the keys, as changes. The watch's
	/// first notification must answer every key. A node also names keys a
	/// block wrote with the value they already had; those are dropped here,
	/// and so is a notification left with nothing.
	pub async fn subscribe_to_changes(self: Arc<Self>) -> Result<RawChangeStream, RuntimeFailure> {
		let legacy = self.chain.legacy(self.genesis).await?;
		let subscription = self.open_storage_subscription(&legacy).await?;
		let tracker = self.clone();
		let changes = subscription
			.map_err(|error| failure(STORAGE_WATCH, error))
			.and_then(move |change_set| ready(tracker.track_changed_keys(change_set)))
			.try_filter_map(|changed| ready(Ok(changed)))
			.and_then(move |changed| {
				self.clone().enrich_raw_changes_with_metadata(legacy.clone(), changed)
			})
			.boxed();
		Ok(changes)
	}

	/// A `state_subscribeStorage` of every one of the reader's keys.
	async fn open_storage_subscription(
		&self,
		legacy: &LegacyConnection,
	) -> Result<RpcSubscription<StorageChangeSet<H256>>, RuntimeFailure> {
		let keys: Vec<String> =
			self.keys.iter().map(|key| format!("0x{}", hex::encode(key))).collect();
		legacy
			.raw_rpc_client
			.subscribe("state_subscribeStorage", rpc_params![keys], "state_unsubscribeStorage")
			.await
			.map_err(|error| failure(STORAGE_WATCH, error))
	}

	/// The keys `change_set` changed, whose values are held from then on.
	fn track_changed_keys(
		&self,
		change_set: StorageChangeSet<H256>,
	) -> Result<Option<ChangedKeysAtBlock>, RuntimeFailure> {
		let mut held_hashes = self.held_hashes.lock();
		let changed = self.find_changed_keys(held_hashes.as_deref(), change_set)?;
		if let Some(changed) = &changed {
			record_hashes(&mut held_hashes, self.keys.len(), &changed.changed);
		}
		Ok(changed)
	}

	/// `change_set` as changes to the watched keys, with the block number and
	/// the metadata to decode them with.
	async fn enrich_raw_changes_with_metadata(
		self: Arc<Self>,
		legacy: LegacyConnection,
		change_set: ChangedKeysAtBlock,
	) -> Result<RawStorageChanges, RuntimeFailure> {
		let at = numbered(&legacy, change_set.block, STORAGE_WATCH).await?;
		let UpgradeAndWatched { last_upgrade, watched } =
			separate_runtime_upgrade_and_watched(change_set.changed);
		let metadata = match last_upgrade {
			Some(last_upgrade) => self.metadata_for_upgrade(&legacy, last_upgrade, at).await?,
			None => self.cached_or_fetched_metadata(&legacy, at).await?,
		};
		Ok(DecodableAtBlock { at, metadata, value: watched })
	}

	/// The keys in `change_set` whose value changed since the values with
	/// `held_hashes`, by position, `None` when none did. With nothing held
	/// yet, `change_set` must name every key, and all of them changed.
	fn find_changed_keys(
		&self,
		held_hashes: Option<&[Option<[u8; 32]>]>,
		change_set: StorageChangeSet<H256>,
	) -> Result<Option<ChangedKeysAtBlock>, RuntimeFailure> {
		let block = change_set.block;
		let values_by_key = group_values_by_key([change_set]);
		let Some(held_hashes) = held_hashes else {
			let values = all_keys_answered_or_err(&self.keys, &values_by_key, block)?;
			let changed = values.into_iter().enumerate().collect();
			return Ok(Some(ChangedKeysAtBlock { block, changed }));
		};
		let changed = self.find_changed_values(held_hashes, values_by_key);
		Ok((!changed.is_empty()).then_some(ChangedKeysAtBlock { block, changed }))
	}

	/// Each of `values_by_key`, with its position, whose hash differs from
	/// the one in `held_hashes`.
	fn find_changed_values(
		&self,
		held_hashes: &[Option<[u8; 32]>],
		values_by_key: HashMap<Vec<u8>, Option<Vec<u8>>>,
	) -> Vec<(usize, Option<Vec<u8>>)> {
		values_by_key
			.into_iter()
			.filter_map(|(key, value)| {
				let position = *self.position_by_key.get(&key)?;
				(held_hashes[position] != value_hash(&value)).then_some((position, value))
			})
			.collect()
	}

	/// The metadata of the runtime whose `System.LastRuntimeUpgrade` is
	/// `last_upgrade`, fetched at `at` unless the reader already has it.
	async fn metadata_for_upgrade(
		&self,
		legacy: &LegacyConnection,
		last_upgrade: Option<Vec<u8>>,
		at: HashAndNumber,
	) -> Result<ArcMetadata, RuntimeFailure> {
		if let Some(metadata) = self.cached_metadata_for(last_upgrade.as_deref()) {
			return Ok(metadata);
		}
		let metadata = fetch_metadata_at_block(legacy, at, STORAGE_WATCH).await?;
		if let Some(last_upgrade) = last_upgrade {
			self.cache_runtime(last_upgrade, metadata.clone());
		}
		Ok(metadata)
	}

	/// Remembers `metadata` as the runtime whose `System.LastRuntimeUpgrade`
	/// is `last_upgrade`.
	fn cache_runtime(&self, last_upgrade: Vec<u8>, metadata: ArcMetadata) {
		*self.last_seen_runtime.lock() = Some(LastSeenRuntime { last_upgrade, metadata });
	}

	/// The metadata seen last, when `System.LastRuntimeUpgrade` did not change.
	/// Fetched at `at` on a chain without it.
	async fn cached_or_fetched_metadata(
		&self,
		legacy: &LegacyConnection,
		at: HashAndNumber,
	) -> Result<ArcMetadata, RuntimeFailure> {
		let known = self.last_seen_runtime.lock().as_ref().map(|known| known.metadata.clone());
		match known {
			Some(metadata) => Ok(metadata),
			None => fetch_metadata_at_block(legacy, at, STORAGE_WATCH).await,
		}
	}

	/// The metadata seen last, if `last_upgrade` still identifies its runtime.
	/// A chain without `System.LastRuntimeUpgrade` never matches.
	fn cached_metadata_for(&self, last_upgrade: Option<&[u8]>) -> Option<ArcMetadata> {
		let runtime = self.last_seen_runtime.lock();
		let known = runtime.as_ref()?;
		(Some(known.last_upgrade.as_slice()) == last_upgrade).then(|| known.metadata.clone())
	}
}

/// Puts the hash of each `changed` value into `held_hashes`, which start as
/// `key_count` absent entries.
fn record_hashes(
	held_hashes: &mut Option<Vec<Option<[u8; 32]>>>,
	key_count: usize,
	changed: &[(usize, Option<Vec<u8>>)],
) {
	let hashes = held_hashes.get_or_insert_with(|| vec![None; key_count]);
	for (position, value) in changed {
		hashes[*position] = value_hash(value);
	}
}

/// One notification's changes, with `System.LastRuntimeUpgrade` taken out.
struct UpgradeAndWatched {
	/// Its new value, if the notification changed it.
	last_upgrade: Option<Option<Vec<u8>>>,
	/// The other changes, by position among the watched keys alone.
	watched: Vec<(usize, Option<Vec<u8>>)>,
}

/// `changed` split into the runtime upgrade and the watched keys, in order.
fn separate_runtime_upgrade_and_watched(
	changed: Vec<(usize, Option<Vec<u8>>)>,
) -> UpgradeAndWatched {
	let mut last_upgrade = None;
	let mut watched = Vec::with_capacity(changed.len());
	for (index, value) in changed {
		match index.checked_sub(1) {
			Some(position) => watched.push((position, value)),
			None => last_upgrade = Some(value),
		}
	}
	watched.sort_by_key(|(position, _)| *position);
	UpgradeAndWatched { last_upgrade, watched }
}

/// Metadata of the runtime that produced `at`.
pub async fn fetch_metadata_at_block(
	legacy: &LegacyConnection,
	at: HashAndNumber,
	method: &'static str,
) -> Result<ArcMetadata, RuntimeFailure> {
	let client = legacy
		.client
		.at_block_hash_and_number(at.hash, at.number)
		.await
		.map_err(|error| failure(method, error))?;
	Ok(client.metadata())
}

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
		storage_watch::{StorageSnapshot, hash_of_storage_value},
		subxt_chain::{failure, numbered},
	},
	chain_runtime::{LegacyConnection, RuntimeFailure},
};

/// The keys one subscription notification changed, by position among the
/// reader's keys, and the block it is for.
struct ChangedKeysAtBlock {
	block: H256,
	changed: Vec<(usize, Option<Vec<u8>>)>,
}

/// Reads the watched keys, keeping the metadata of the runtime it last saw.
pub struct WatchedKeysReader {
	legacy: LegacyConnection,
	/// `System.LastRuntimeUpgrade`, then the watched keys.
	keys: Vec<Vec<u8>>,
	/// The position of each of `keys`, to place the keys a notification names.
	position_by_key: HashMap<Vec<u8>, usize>,
	last_seen_runtime: Mutex<Option<LastSeenRuntime>>,
}

/// The runtime a reader last saw, identified by its `System.LastRuntimeUpgrade`.
struct LastSeenRuntime {
	last_upgrade: Vec<u8>,
	metadata: ArcMetadata,
}

impl WatchedKeysReader {
	/// A reader of `watched` over `legacy`.
	pub fn new(legacy: LegacyConnection, watched: Vec<Vec<u8>>) -> Self {
		let upgrade_key = encode_storage_key_prefix("System", "LastRuntimeUpgrade").to_vec();
		let keys = [vec![upgrade_key], watched].concat();
		let position_by_key =
			keys.iter().cloned().enumerate().map(|(index, key)| (key, index)).collect();
		Self { legacy, keys, position_by_key, last_seen_runtime: Mutex::new(None) }
	}

	/// Every watched key at `at`, read in one `state_queryStorageAt` call.
	pub async fn read_snapshot_at(
		self: Arc<Self>,
		at: HashAndNumber,
	) -> Result<StorageSnapshot, RuntimeFailure> {
		let queried_snapshot = self
			.legacy
			.methods
			.state_query_storage_at(self.keys.iter().map(Vec::as_slice), Some(at.hash))
			.await
			.map_err(|error| failure(STORAGE_WATCH, error))?;
		let values_by_key = group_values_by_key(queried_snapshot);
		let mut values = all_keys_answered_or_err(&self.keys, &values_by_key, at.hash)?;
		let last_upgrade = values.remove(0);
		let metadata = self.metadata_for_runtime_upgrade(last_upgrade, at).await?;
		Ok(DecodableAtBlock { at, metadata, value: values })
	}

	/// A new `state_subscribeStorage` of the keys, as changes. Its first
	/// notification must answer every key. A node also names keys a block
	/// wrote with the value they already had; those are dropped here, and so
	/// is a notification left with nothing.
	pub async fn subscribe_to_changes(self: Arc<Self>) -> Result<RawChangeStream, RuntimeFailure> {
		let subscription = self.open_storage_subscription().await?;
		let computing = self.clone();
		let changed_keys = subscription.scan(None, move |held_value_hashes, change_set| {
			let change_set = change_set.map_err(|error| failure(STORAGE_WATCH, error));
			let changed = change_set.and_then(|change_set| {
				computing.compute_changeset_updating_held(held_value_hashes, change_set)
			});
			ready(Some(changed))
		});
		let nonempty_changed_keys = changed_keys.try_filter_map(|changed| ready(Ok(changed)));
		let enriched = nonempty_changed_keys
			.and_then(move |changed| self.clone().enrich_raw_changes_with_metadata(changed));
		Ok(enriched.boxed())
	}

	/// A `state_subscribeStorage` of every one of the reader's keys.
	async fn open_storage_subscription(
		&self,
	) -> Result<RpcSubscription<StorageChangeSet<H256>>, RuntimeFailure> {
		let keys: Vec<String> =
			self.keys.iter().map(|key| format!("0x{}", hex::encode(key))).collect();
		self.legacy
			.raw_rpc_client
			.subscribe("state_subscribeStorage", rpc_params![keys], "state_unsubscribeStorage")
			.await
			.map_err(|error| failure(STORAGE_WATCH, error))
	}

	/// `change_set` as changes to the watched keys, with the block number and
	/// the metadata to decode them with.
	async fn enrich_raw_changes_with_metadata(
		self: Arc<Self>,
		change_set: ChangedKeysAtBlock,
	) -> Result<RawStorageChanges, RuntimeFailure> {
		let at = numbered(&self.legacy, change_set.block, STORAGE_WATCH).await?;
		let RuntimeUpgradeAndWatchedChanges { last_upgrade, watched } =
			separate_runtime_upgrade_and_watched(change_set.changed);
		let metadata = match last_upgrade {
			Some(last_upgrade) => self.metadata_for_runtime_upgrade(last_upgrade, at).await?,
			None => self.last_seen_metadata_or_fetch_at(at).await?,
		};
		Ok(DecodableAtBlock { at, metadata, value: watched })
	}

	/// The keys in `change_set` whose value hash differs from the one in
	/// `held_value_hashes`, by position, which then hold the new hashes.
	/// `None` when no value changed. With nothing held yet, `change_set` must
	/// name every key, and all of them are reported.
	fn compute_changeset_updating_held(
		&self,
		held_value_hashes: &mut Option<Vec<Option<[u8; 32]>>>,
		change_set: StorageChangeSet<H256>,
	) -> Result<Option<ChangedKeysAtBlock>, RuntimeFailure> {
		let block = change_set.block;
		let values_by_key = group_values_by_key([change_set]);
		let Some(hashes) = held_value_hashes.as_mut() else {
			let values = all_keys_answered_or_err(&self.keys, &values_by_key, block)?;
			*held_value_hashes = Some(values.iter().map(hash_of_storage_value).collect());
			let changed = values.into_iter().enumerate().collect();
			return Ok(Some(ChangedKeysAtBlock { block, changed }));
		};
		let changed = self.values_whose_hash_changed_updating_hashes(hashes, values_by_key);
		Ok((!changed.is_empty()).then_some(ChangedKeysAtBlock { block, changed }))
	}

	/// Each of `values_by_key`, with its position, whose hash differs from
	/// the one in `held_value_hashes`, which takes the new hash.
	fn values_whose_hash_changed_updating_hashes(
		&self,
		held_value_hashes: &mut [Option<[u8; 32]>],
		values_by_key: HashMap<Vec<u8>, Option<Vec<u8>>>,
	) -> Vec<(usize, Option<Vec<u8>>)> {
		values_by_key
			.into_iter()
			.filter_map(|(key, value)| {
				let position = *self.position_by_key.get(&key)?;
				let hash = hash_of_storage_value(&value);
				let previous = core::mem::replace(&mut held_value_hashes[position], hash);
				(previous != hash).then_some((position, value))
			})
			.collect()
	}

	/// The metadata of the runtime whose `System.LastRuntimeUpgrade` is
	/// `last_upgrade`, fetched at `at` unless the reader already has it.
	async fn metadata_for_runtime_upgrade(
		&self,
		last_upgrade: Option<Vec<u8>>,
		at: HashAndNumber,
	) -> Result<ArcMetadata, RuntimeFailure> {
		if let Some(metadata) =
			self.last_seen_metadata_if_runtime_unchanged(last_upgrade.as_deref())
		{
			return Ok(metadata);
		}
		let metadata = fetch_metadata_at_block(&self.legacy, at, STORAGE_WATCH).await?;
		if let Some(last_upgrade) = last_upgrade {
			let known = LastSeenRuntime { last_upgrade, metadata: metadata.clone() };
			*self.last_seen_runtime.lock() = Some(known);
		}
		Ok(metadata)
	}

	/// The metadata seen last, when `System.LastRuntimeUpgrade` did not change.
	/// Fetched at `at` on a chain without it.
	async fn last_seen_metadata_or_fetch_at(
		&self,
		at: HashAndNumber,
	) -> Result<ArcMetadata, RuntimeFailure> {
		let known = self.last_seen_runtime.lock().as_ref().map(|known| known.metadata.clone());
		match known {
			Some(metadata) => Ok(metadata),
			None => fetch_metadata_at_block(&self.legacy, at, STORAGE_WATCH).await,
		}
	}

	/// The metadata seen last, if `last_upgrade` still identifies its runtime.
	/// A chain without `System.LastRuntimeUpgrade` never matches.
	fn last_seen_metadata_if_runtime_unchanged(
		&self,
		last_upgrade: Option<&[u8]>,
	) -> Option<ArcMetadata> {
		let runtime = self.last_seen_runtime.lock();
		let known = runtime.as_ref()?;
		(Some(known.last_upgrade.as_slice()) == last_upgrade).then(|| known.metadata.clone())
	}
}

/// One notification's changes, with `System.LastRuntimeUpgrade` taken out.
struct RuntimeUpgradeAndWatchedChanges {
	/// Its new value, if the notification changed it.
	last_upgrade: Option<Option<Vec<u8>>>,
	/// The other changes, by position among the watched keys alone.
	watched: Vec<(usize, Option<Vec<u8>>)>,
}

/// `changed` split into the runtime upgrade and the watched keys, in order.
fn separate_runtime_upgrade_and_watched(
	changed: Vec<(usize, Option<Vec<u8>>)>,
) -> RuntimeUpgradeAndWatchedChanges {
	let mut last_upgrade = None;
	let mut watched = Vec::with_capacity(changed.len());
	for (index, value) in changed {
		match index.checked_sub(1) {
			Some(position) => watched.push((position, value)),
			None => last_upgrade = Some(value),
		}
	}
	watched.sort_by_key(|(position, _)| *position);
	RuntimeUpgradeAndWatchedChanges { last_upgrade, watched }
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

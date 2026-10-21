//! Reading the watched keys: all of them at a block, or their changes from a
//! subscription, with the metadata of the runtime they belong to.

use core::future::ready;
use std::{collections::HashMap, sync::Arc};

use futures::stream::{BoxStream, StreamExt, TryStreamExt};
use parking_lot::Mutex;
use subxt::{ArcMetadata, ext::frame_decode::storage::encode_storage_key_prefix, utils::H256};
use subxt_rpcs::{methods::legacy::StorageChangeSet, rpc_params};

use super::{STORAGE_WATCH, answered_by_key, answered_values};
use crate::{
	chain::{
		DecodableAtBlock, HashAndNumber, RawStorageChanges,
		storage_watch::StorageSnapshot,
		subxt_chain::{failure, numbered},
	},
	chain_runtime::{LegacyConnection, RuntimeFailure},
};

/// The keys one subscription notification changed, by position among the
/// reader's keys, and the block it is for.
struct ChangeSetAt {
	block: H256,
	changed: Vec<(usize, Option<Vec<u8>>)>,
}

type Changes = BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>;

/// Reads the watched keys, keeping the metadata of the runtime it last saw.
pub struct StorageReader {
	legacy: LegacyConnection,
	/// `System.LastRuntimeUpgrade`, then the watched keys.
	keys: Vec<Vec<u8>>,
	/// Where each of `keys` is, to place the keys a notification names.
	positions: HashMap<Vec<u8>, usize>,
	runtime: Mutex<Option<KnownRuntime>>,
}

/// The runtime a reader last saw, identified by its `System.LastRuntimeUpgrade`.
struct KnownRuntime {
	last_upgrade: Vec<u8>,
	metadata: ArcMetadata,
}

impl StorageReader {
	/// A reader of `watched` over `legacy`.
	pub fn new(legacy: LegacyConnection, watched: Vec<Vec<u8>>) -> Self {
		let upgrade_key = encode_storage_key_prefix("System", "LastRuntimeUpgrade").to_vec();
		let keys = [vec![upgrade_key], watched].concat();
		let positions = keys.iter().cloned().enumerate().map(|(index, key)| (key, index)).collect();
		Self { legacy, keys, positions, runtime: Mutex::new(None) }
	}

	/// Every watched key at `at`, read in one `state_queryStorageAt` call.
	pub async fn read_at(
		self: Arc<Self>,
		at: HashAndNumber,
	) -> Result<StorageSnapshot, RuntimeFailure> {
		let change_sets = self
			.legacy
			.methods
			.state_query_storage_at(self.keys.iter().map(Vec::as_slice), Some(at.hash))
			.await
			.map_err(|error| failure(STORAGE_WATCH, error))?;
		let answered = answered_by_key(change_sets);
		let mut values = answered_values(&self.keys, &answered, at.hash)?;
		let last_upgrade = values.remove(0);
		let metadata = self.metadata_for(last_upgrade, at).await?;
		Ok(DecodableAtBlock { at, metadata, value: values })
	}

	/// A new `state_subscribeStorage` of the keys, as changes. Its first
	/// notification must answer every key. A node also names keys a block
	/// wrote with the value they already had; those are dropped here, and so
	/// is a notification left with nothing.
	pub async fn subscribe(self: Arc<Self>) -> Result<Changes, RuntimeFailure> {
		let keys: Vec<String> =
			self.keys.iter().map(|key| format!("0x{}", hex::encode(key))).collect();
		let subscription = self
			.legacy
			.rpc
			.subscribe::<StorageChangeSet<H256>>(
				"state_subscribeStorage",
				rpc_params![keys],
				"state_unsubscribeStorage",
			)
			.await
			.map_err(|error| failure(STORAGE_WATCH, error))?;
		let placing = self.clone();
		let placed = subscription.scan(None, move |held, change_set| {
			let change_set = change_set.map_err(|error| failure(STORAGE_WATCH, error));
			ready(Some(change_set.and_then(|change_set| placing.placed(held, change_set))))
		});
		let changed = placed.try_filter_map(|change_set| ready(Ok(change_set)));
		Ok(changed.and_then(move |change_set| self.clone().changes_in(change_set)).boxed())
	}

	/// `change_set` as changes to the watched keys, with the block number and
	/// the metadata to decode them with.
	async fn changes_in(
		self: Arc<Self>,
		change_set: ChangeSetAt,
	) -> Result<RawStorageChanges, RuntimeFailure> {
		let at = numbered(&self.legacy, change_set.block, STORAGE_WATCH).await?;
		let SplitChanges { last_upgrade, watched } = split_upgrade(change_set.changed);
		let metadata = match last_upgrade {
			Some(last_upgrade) => self.metadata_for(last_upgrade, at).await?,
			None => self.current_runtime_metadata(at).await?,
		};
		Ok(DecodableAtBlock { at, metadata, value: watched })
	}

	/// The keys in `change_set` whose value differs from the one `held`, by
	/// position, `None` when there are none. With nothing held yet,
	/// `change_set` must name every key, and all of them are reported.
	fn placed(
		&self,
		held: &mut Option<Vec<Option<Vec<u8>>>>,
		change_set: StorageChangeSet<H256>,
	) -> Result<Option<ChangeSetAt>, RuntimeFailure> {
		let block = change_set.block;
		let answered = answered_by_key([change_set]);
		let Some(values) = held.as_mut() else {
			let values = answered_values(&self.keys, &answered, block)?;
			let changed = values.iter().cloned().enumerate().collect();
			*held = Some(values);
			return Ok(Some(ChangeSetAt { block, changed }));
		};
		let changed = self.changed_values(values, answered);
		Ok((!changed.is_empty()).then_some(ChangeSetAt { block, changed }))
	}

	/// Each of `answered` whose value differs from `held`, which takes the new
	/// value.
	fn changed_values(
		&self,
		held: &mut [Option<Vec<u8>>],
		answered: HashMap<Vec<u8>, Option<Vec<u8>>>,
	) -> Vec<(usize, Option<Vec<u8>>)> {
		answered
			.into_iter()
			.filter_map(|(key, value)| {
				let position = *self.positions.get(&key)?;
				let previous = core::mem::replace(&mut held[position], value.clone());
				(previous != value).then_some((position, value))
			})
			.collect()
	}

	/// The metadata of the runtime whose `System.LastRuntimeUpgrade` is
	/// `last_upgrade`, fetched at `at` unless the reader already has it.
	async fn metadata_for(
		&self,
		last_upgrade: Option<Vec<u8>>,
		at: HashAndNumber,
	) -> Result<ArcMetadata, RuntimeFailure> {
		if let Some(metadata) = self.known_metadata(last_upgrade.as_deref()) {
			return Ok(metadata);
		}
		let metadata = metadata_at(&self.legacy, at, STORAGE_WATCH).await?;
		if let Some(last_upgrade) = last_upgrade {
			let known = KnownRuntime { last_upgrade, metadata: metadata.clone() };
			*self.runtime.lock() = Some(known);
		}
		Ok(metadata)
	}

	/// The metadata seen last, when `System.LastRuntimeUpgrade` did not change.
	/// Fetched at `at` on a chain without it.
	async fn current_runtime_metadata(
		&self,
		at: HashAndNumber,
	) -> Result<ArcMetadata, RuntimeFailure> {
		let known = self.runtime.lock().as_ref().map(|known| known.metadata.clone());
		match known {
			Some(metadata) => Ok(metadata),
			None => metadata_at(&self.legacy, at, STORAGE_WATCH).await,
		}
	}

	/// The metadata seen last, if `last_upgrade` still identifies its runtime.
	/// A chain without `System.LastRuntimeUpgrade` never matches.
	fn known_metadata(&self, last_upgrade: Option<&[u8]>) -> Option<ArcMetadata> {
		let runtime = self.runtime.lock();
		let known = runtime.as_ref()?;
		(Some(known.last_upgrade.as_slice()) == last_upgrade).then(|| known.metadata.clone())
	}
}

/// One notification's changes, with `System.LastRuntimeUpgrade` taken out.
struct SplitChanges {
	/// Its new value, if the notification changed it.
	last_upgrade: Option<Option<Vec<u8>>>,
	/// The other changes, by position among the watched keys alone.
	watched: Vec<(usize, Option<Vec<u8>>)>,
}

/// `changed` split into the runtime upgrade and the watched keys, in order.
fn split_upgrade(changed: Vec<(usize, Option<Vec<u8>>)>) -> SplitChanges {
	let mut last_upgrade = None;
	let mut watched = Vec::with_capacity(changed.len());
	for (index, value) in changed {
		match index.checked_sub(1) {
			Some(position) => watched.push((position, value)),
			None => last_upgrade = Some(value),
		}
	}
	watched.sort_by_key(|(position, _)| *position);
	SplitChanges { last_upgrade, watched }
}

/// Metadata of the runtime that produced `at`.
pub async fn metadata_at(
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

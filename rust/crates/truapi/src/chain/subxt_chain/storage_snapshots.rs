//! [`StorageSnapshots`] for `ChainRuntime`.
//!
//! Heads come from [`ChainHeads::head_events`], opened again whenever the
//! node ends it. Each head is one `state_queryStorageAt` call, which answers
//! every key, absent ones included, so a key the node leaves out is an
//! error rather than an absent value. `System.LastRuntimeUpgrade` is read in
//! the same call, and metadata is fetched only when it changes.

use core::{future::ready, time::Duration};
use std::{collections::HashMap, sync::Arc};

use futures::stream::{self, BoxStream, StreamExt, TryStreamExt};
use parking_lot::Mutex;
use subxt::{ArcMetadata, ext::frame_decode::storage::encode_storage_key_prefix, utils::H256};

use super::{failure, finalized_block};
use crate::{
	chain::{
		ChainHeads, DecodableAtBlock, HashAndNumber, HeadEvent, StorageSnapshots, WatchedHead,
	},
	chain_runtime::{ChainRuntime, LegacyConnection, RuntimeFailure},
};

const CURRENT_METADATA: &str = "storage_current_metadata";
const STORAGE_SNAPSHOT: &str = "storage_snapshot";

/// How long to wait before opening the head subscriptions again after
/// opening them failed.
const RESUBSCRIBE_DELAY: Duration = Duration::from_secs(1);

/// Most heads collected while one read runs. Only the newest is read.
const MAX_SKIPPED_HEADS: usize = 64;

type Blocks = BoxStream<'static, Result<HashAndNumber, RuntimeFailure>>;

/// Every watched key's value at one block, `None` when absent.
type Snapshot = DecodableAtBlock<Vec<Option<Vec<u8>>>>;

#[async_trait::async_trait]
impl StorageSnapshots for ChainRuntime {
	async fn current_metadata(&self, genesis: H256) -> Result<ArcMetadata, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		let finalized = finalized_block(&legacy, CURRENT_METADATA).await?;
		metadata_at(&legacy, finalized, CURRENT_METADATA).await
	}

	async fn snapshots(
		&self,
		genesis: H256,
		head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<BoxStream<'static, Result<Snapshot, RuntimeFailure>>, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		let blocks = self.watched_blocks(genesis, head).await?;
		let reader = Arc::new(SnapshotReader::new(legacy, keys));
		let newest = newest_only(blocks);
		Ok(newest.and_then(move |at| reader.clone().read(at)).boxed())
	}
}

impl ChainRuntime {
	/// Blocks of the `head` kind. The subscription is opened once here, so a
	/// chain that cannot be reached fails the call, and again whenever the
	/// node ends it.
	async fn watched_blocks(
		&self,
		genesis: H256,
		head: WatchedHead,
	) -> Result<Blocks, RuntimeFailure> {
		let first = self.head_events(genesis).await?;
		let chain = self.clone();
		let reopened = stream::unfold(chain, move |chain| async move {
			let events = chain.reopened_head_events(genesis).await;
			Some((events, chain))
		});
		let events = first.chain(reopened.flatten());
		let blocks = events.try_filter_map(move |event| ready(Ok(watched_block(head, event))));
		Ok(blocks.boxed())
	}

	/// The head events, or after waiting [`RESUBSCRIBE_DELAY`] the failure to
	/// open them.
	async fn reopened_head_events(
		&self,
		genesis: H256,
	) -> BoxStream<'static, Result<HeadEvent, RuntimeFailure>> {
		match self.head_events(genesis).await {
			Ok(events) => events,
			Err(error) => {
				futures_timer::Delay::new(RESUBSCRIBE_DELAY).await;
				stream::once(ready(Err(error))).boxed()
			},
		}
	}
}

/// The block `event` announces when it is of the `head` kind.
fn watched_block(head: WatchedHead, event: HeadEvent) -> Option<HashAndNumber> {
	match (head, event) {
		(WatchedHead::Best, HeadEvent::Best(block)) |
		(WatchedHead::Finalized, HeadEvent::Finalized(block)) => Some(block),
		_ => None,
	}
}

/// `items`, except that items already waiting when the next one is asked for
/// collapse into the newest of them.
fn newest_only<T: Send + 'static>(items: BoxStream<'static, T>) -> BoxStream<'static, T> {
	items
		.ready_chunks(MAX_SKIPPED_HEADS)
		.filter_map(|waiting| ready(waiting.into_iter().last()))
		.boxed()
}

/// Reads the watched keys at one block after another, keeping the metadata
/// of the runtime it last saw.
struct SnapshotReader {
	legacy: LegacyConnection,
	/// `System.LastRuntimeUpgrade`, then the watched keys.
	keys: Vec<Vec<u8>>,
	runtime: Mutex<Option<KnownRuntime>>,
}

/// The runtime a reader last saw, identified by its `System.LastRuntimeUpgrade`.
struct KnownRuntime {
	last_upgrade: Vec<u8>,
	metadata: ArcMetadata,
}

impl SnapshotReader {
	fn new(legacy: LegacyConnection, watched: Vec<Vec<u8>>) -> Self {
		let upgrade_key = encode_storage_key_prefix("System", "LastRuntimeUpgrade").to_vec();
		let keys = [vec![upgrade_key], watched].concat();
		Self { legacy, keys, runtime: Mutex::new(None) }
	}

	async fn read(self: Arc<Self>, at: HashAndNumber) -> Result<Snapshot, RuntimeFailure> {
		let mut values = query_storage_at(&self.legacy, &self.keys, at.hash).await?;
		let last_upgrade = values.remove(0);
		let metadata = self.metadata_for(last_upgrade, at).await?;
		Ok(DecodableAtBlock { at, metadata, value: values })
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
		let metadata = metadata_at(&self.legacy, at, STORAGE_SNAPSHOT).await?;
		if let Some(last_upgrade) = last_upgrade {
			let known = KnownRuntime { last_upgrade, metadata: metadata.clone() };
			*self.runtime.lock() = Some(known);
		}
		Ok(metadata)
	}

	/// The metadata seen last, if `last_upgrade` still identifies its runtime.
	/// A chain without `System.LastRuntimeUpgrade` never matches.
	fn known_metadata(&self, last_upgrade: Option<&[u8]>) -> Option<ArcMetadata> {
		let runtime = self.runtime.lock();
		let known = runtime.as_ref()?;
		(Some(known.last_upgrade.as_slice()) == last_upgrade).then(|| known.metadata.clone())
	}
}

/// Metadata of the runtime that produced `at`.
async fn metadata_at(
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

/// The value of each of `keys` at `at`, in one `state_queryStorageAt` call.
async fn query_storage_at(
	legacy: &LegacyConnection,
	keys: &[Vec<u8>],
	at: H256,
) -> Result<Vec<Option<Vec<u8>>>, RuntimeFailure> {
	let change_sets = legacy
		.methods
		.state_query_storage_at(keys.iter().map(Vec::as_slice), Some(at))
		.await
		.map_err(|error| failure(STORAGE_SNAPSHOT, error))?;
	let answered: HashMap<Vec<u8>, Option<Vec<u8>>> = change_sets
		.into_iter()
		.flat_map(|change_set| change_set.changes)
		.map(|(key, value)| (key.0, value.map(|value| value.0)))
		.collect();
	keys.iter().map(|key| answered_value(&answered, key, at)).collect()
}

/// The answer for `key`. A key the node left out is an error, so that it is
/// never mistaken for an absent entry.
fn answered_value(
	answered: &HashMap<Vec<u8>, Option<Vec<u8>>>,
	key: &[u8],
	at: H256,
) -> Result<Option<Vec<u8>>, RuntimeFailure> {
	answered.get(key).cloned().ok_or_else(|| {
		let key = hex::encode(key);
		RuntimeFailure::host_failure(
			STORAGE_SNAPSHOT,
			format!("expected a value or null for key 0x{key} at {at:?}, the node left it out"),
		)
	})
}

#[cfg(test)]
mod tests {
	use futures::{StreamExt, executor::block_on, stream};

	use super::newest_only;

	#[test]
	fn heads_waiting_together_collapse_into_the_newest() {
		// A slow read must not leave the watch reading one stale head after
		// another: whatever queued up meanwhile is read once, at its newest.
		let waiting = stream::iter([1, 2, 3]).boxed();

		let read: Vec<u32> = block_on(newest_only(waiting).collect());

		assert_eq!(read, vec![3]);
	}
}

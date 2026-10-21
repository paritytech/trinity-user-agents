//! [`WatchedStorage`] for `ChainRuntime`, over the legacy connection.
//!
//! A finalized watch follows [`ChainHeads::head_events`], reads every key at
//! each finalized head with one `state_queryStorageAt` call, and turns the
//! snapshots into changes. A best watch holds one `state_subscribeStorage`:
//! its first notification names every key, and later ones the keys each new
//! best block wrote. A node names a key even when the block wrote the value it
//! already had, and smoldot currently names every key in every notification
//! (paritytech/smoldot#3413), so the reader keeps each key's last value and
//! drops those that did not change. A full answer gives an absent entry as an
//! explicit null, and a key it leaves out is an error rather than an absent
//! value. `System.LastRuntimeUpgrade` is watched with the keys, and metadata
//! is fetched only when it changes. A subscription the node ends is opened
//! again.

use core::{
	future::{Future, ready},
	time::Duration,
};
use std::{collections::HashMap, sync::Arc};

use futures::stream::{self, BoxStream, StreamExt, TryStreamExt};
use subxt::{ArcMetadata, utils::H256};
use subxt_rpcs::methods::legacy::StorageChangeSet;

use super::finalized_block;
use crate::{
	chain::{
		ChainHeads, HashAndNumber, HeadEvent, RawStorageChanges, WatchedHead, WatchedStorage,
		storage_watch::snapshot_changes,
	},
	chain_runtime::{ChainRuntime, RuntimeFailure},
};

mod reader;

use reader::StorageReader;

const CURRENT_METADATA: &str = "storage_current_metadata";
const STORAGE_WATCH: &str = "storage_watch";

/// How long to wait before opening a subscription again after opening it
/// failed.
const RESUBSCRIBE_DELAY: Duration = Duration::from_secs(1);

/// Most heads collected while one read runs. Only the newest is read.
const MAX_SKIPPED_HEADS: usize = 64;

type Changes = BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>;

#[async_trait::async_trait]
impl WatchedStorage for ChainRuntime {
	async fn current_metadata(&self, genesis: H256) -> Result<ArcMetadata, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		let finalized = finalized_block(&legacy, CURRENT_METADATA).await?;
		reader::metadata_at(&legacy, finalized, CURRENT_METADATA).await
	}

	async fn changes(
		&self,
		genesis: H256,
		head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<Changes, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		let reader = Arc::new(StorageReader::new(legacy, keys));
		match head {
			WatchedHead::Best => subscribed_changes(reader).await,
			WatchedHead::Finalized => self.queried_changes(genesis, reader).await,
		}
	}
}

impl ChainRuntime {
	/// Changes between the snapshots read at each finalized head. The head
	/// subscription is opened here, so a chain that cannot be reached fails
	/// the call.
	async fn queried_changes(
		&self,
		genesis: H256,
		reader: Arc<StorageReader>,
	) -> Result<Changes, RuntimeFailure> {
		let first = self.head_events(genesis).await?;
		let chain = self.clone();
		let events = reopening(first, move || {
			let chain = chain.clone();
			async move { chain.head_events(genesis).await }
		});
		let blocks = events.try_filter_map(|event| ready(Ok(finalized(event))));
		let newest = newest_only(blocks.boxed());
		let snapshots = newest.and_then(move |at| reader.clone().read_at(at));
		Ok(snapshot_changes(snapshots.boxed()))
	}
}

/// The notifications of the storage subscription, as changes. It is opened
/// here, so a chain that cannot be reached fails the call.
async fn subscribed_changes(reader: Arc<StorageReader>) -> Result<Changes, RuntimeFailure> {
	let first = reader.clone().subscribe().await?;
	let reopened_reader = reader.clone();
	let change_sets = reopening(first, move || reopened_reader.clone().subscribe());
	Ok(change_sets
		.and_then(move |change_set| reader.clone().changes_in(change_set))
		.boxed())
}

/// The block `event` announces when it is finalized.
fn finalized(event: HeadEvent) -> Option<HashAndNumber> {
	match event {
		HeadEvent::Finalized(block) => Some(block),
		HeadEvent::Best(_) => None,
	}
}

/// `first`, then the stream `open` gives each time the one before it ends.
/// A failure to open is reported as an item after [`RESUBSCRIBE_DELAY`].
fn reopening<T, Open, Opening>(
	first: BoxStream<'static, Result<T, RuntimeFailure>>,
	open: Open,
) -> BoxStream<'static, Result<T, RuntimeFailure>>
where
	T: Send + 'static,
	Open: Fn() -> Opening + Send + 'static,
	Opening: Future<Output = Result<BoxStream<'static, Result<T, RuntimeFailure>>, RuntimeFailure>>
		+ Send
		+ 'static,
{
	let reopened = stream::unfold(open, |open| async move {
		let items = reopened_items(open()).await;
		Some((items, open))
	});
	first.chain(reopened.flatten()).boxed()
}

/// What `opening` opens, or after waiting [`RESUBSCRIBE_DELAY`] the failure
/// to open it.
async fn reopened_items<T: Send + 'static>(
	opening: impl Future<Output = Result<BoxStream<'static, Result<T, RuntimeFailure>>, RuntimeFailure>>,
) -> BoxStream<'static, Result<T, RuntimeFailure>> {
	match opening.await {
		Ok(items) => items,
		Err(error) => {
			futures_timer::Delay::new(RESUBSCRIBE_DELAY).await;
			stream::once(ready(Err(error))).boxed()
		},
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

/// The value the node gave for each key it answered, `None` when absent.
fn answered_by_key(
	change_sets: impl IntoIterator<Item = StorageChangeSet<H256>>,
) -> HashMap<Vec<u8>, Option<Vec<u8>>> {
	change_sets
		.into_iter()
		.flat_map(|change_set| change_set.changes)
		.map(|(key, value)| (key.0, value.map(|value| value.0)))
		.collect()
}

/// The answer for each of `keys`. A key the node left out is an error, so
/// that it is never mistaken for an absent entry.
fn answered_values(
	keys: &[Vec<u8>],
	answered: &HashMap<Vec<u8>, Option<Vec<u8>>>,
	at: H256,
) -> Result<Vec<Option<Vec<u8>>>, RuntimeFailure> {
	keys.iter().map(|key| answered_value(answered, key, at)).collect()
}

fn answered_value(
	answered: &HashMap<Vec<u8>, Option<Vec<u8>>>,
	key: &[u8],
	at: H256,
) -> Result<Option<Vec<u8>>, RuntimeFailure> {
	answered.get(key).cloned().ok_or_else(|| {
		let key = hex::encode(key);
		RuntimeFailure::host_failure(
			STORAGE_WATCH,
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

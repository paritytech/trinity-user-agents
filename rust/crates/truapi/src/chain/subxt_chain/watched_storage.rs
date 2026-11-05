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
//! is fetched only when it changes. When the node ends a subscription, or a
//! notification cannot be read, the watch opens a new one, on a new connection
//! if the old one was closed. After that it reports only the keys whose values
//! differ from the last ones it reported.

use core::{
	future::{Future, ready},
	time::Duration,
};
use std::{collections::HashMap, sync::Arc};

use futures::stream::{self, BoxStream, StreamExt, TryStreamExt};
use subxt::{client::OnlineClientAtBlock, config::SubstrateConfig, utils::H256};
use subxt_rpcs::methods::legacy::StorageChangeSet;

use super::{finalized_block, numbered};
use crate::{
	chain::{
		ChainHeads, HashAndNumber, HeadEvent, RawStorageChanges, WatchedHead, WatchedStorage,
		storage_watch::{StorageSnapshot, diffed},
	},
	chain_runtime::{ChainRuntime, RuntimeFailure},
	time::{RealTimer, Timer},
};

mod reader;

use reader::WatchedKeysReader;

const CLIENT_AT_FINALIZED_HEAD: &str = "storage_client_at_finalized_head";
const CLIENT_AT: &str = "storage_client_at";
const STORAGE_WATCH: &str = "storage_watch";
const READ_KEYS_AT: &str = "storage_read_keys_at";

/// How long to wait before opening a subscription again after opening it
/// failed.
const RESUBSCRIBE_DELAY: Duration = Duration::from_secs(1);

/// Most heads collected while one read runs. Only the newest is read.
const MAX_SKIPPED_HEADS: usize = 64;

type RawChangeStream = BoxStream<'static, Result<RawStorageChanges, RuntimeFailure>>;

#[async_trait::async_trait]
impl WatchedStorage for ChainRuntime {
	async fn client_at_finalized_head(
		&self,
		genesis: H256,
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		let finalized = finalized_block(&legacy, CLIENT_AT_FINALIZED_HEAD).await?;
		reader::client_at_block(&legacy, finalized, CLIENT_AT_FINALIZED_HEAD).await
	}

	async fn client_at(
		&self,
		genesis: H256,
		at: HashAndNumber,
	) -> Result<OnlineClientAtBlock<SubstrateConfig>, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		reader::client_at_block(&legacy, at, CLIENT_AT).await
	}

	async fn watch_raw_key_changes(
		&self,
		genesis: H256,
		head: WatchedHead,
		keys: Vec<Vec<u8>>,
	) -> Result<RawChangeStream, RuntimeFailure> {
		let reader = Arc::new(WatchedKeysReader::new(self.clone(), genesis, keys));
		match head {
			WatchedHead::Best => best_changes(reader).await,
			WatchedHead::Finalized => self.finalized_changes(genesis, reader).await,
		}
	}

	async fn read_raw_keys_at(
		&self,
		genesis: H256,
		at: H256,
		keys: Vec<Vec<u8>>,
	) -> Result<StorageSnapshot, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		let at = numbered(&legacy, at, READ_KEYS_AT).await?;
		Arc::new(WatchedKeysReader::new(self.clone(), genesis, keys))
			.read_snapshot_at(at)
			.await
	}

	fn timer(&self) -> Arc<dyn Timer> {
		Arc::new(RealTimer)
	}
}

impl ChainRuntime {
	/// Changes between the snapshots read at each finalized head. The head
	/// subscription is opened here, so a chain that cannot be reached fails
	/// the call.
	async fn finalized_changes(
		&self,
		genesis: H256,
		reader: Arc<WatchedKeysReader>,
	) -> Result<RawChangeStream, RuntimeFailure> {
		let chain = self.clone();
		let open_head_events = move || {
			let chain = chain.clone();
			async move { chain.head_events(genesis).await }
		};
		let events = retriable_stream(open_head_events).await?;
		let blocks = events.try_filter_map(|event| ready(Ok(block_if_finalized(event))));
		let newest = consume_latest(blocks.boxed());
		let snapshots = newest.and_then(move |at| reader.clone().read_snapshot_at(at));
		Ok(diffed(snapshots.boxed()))
	}
}

/// The notifications of the storage subscription, as changes. It is opened
/// here, so a chain that cannot be reached fails the call.
async fn best_changes(reader: Arc<WatchedKeysReader>) -> Result<RawChangeStream, RuntimeFailure> {
	let open_subscription = move || reader.clone().subscribe_to_changes();
	retriable_stream(open_subscription).await
}

/// The block `event` announces when it is finalized.
fn block_if_finalized(event: HeadEvent) -> Option<HashAndNumber> {
	match event {
		HeadEvent::Finalized(block) => Some(block),
		HeadEvent::Best(_) => None,
	}
}

/// The stream `open` gives, then the one it gives again each time the one
/// before ends. A failure to open the first one fails the call; a later one
/// is reported as an item after [`RESUBSCRIBE_DELAY`].
async fn retriable_stream<T, Open, Opening>(
	open: Open,
) -> Result<BoxStream<'static, Result<T, RuntimeFailure>>, RuntimeFailure>
where
	T: Send + 'static,
	Open: Fn() -> Opening + Send + 'static,
	Opening: Future<Output = Result<BoxStream<'static, Result<T, RuntimeFailure>>, RuntimeFailure>>
		+ Send
		+ 'static,
{
	let first = open().await?;
	let reopened = stream::unfold(open, |open| async move {
		let items = open_or_delay_retry(open()).await;
		Some((items, open))
	});
	Ok(first.chain(reopened.flatten()).boxed())
}

/// What `opening` opens, or after waiting [`RESUBSCRIBE_DELAY`] the failure
/// to open it.
async fn open_or_delay_retry<T: Send + 'static>(
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
fn consume_latest<T: Send + 'static>(items: BoxStream<'static, T>) -> BoxStream<'static, T> {
	items
		.ready_chunks(MAX_SKIPPED_HEADS)
		.filter_map(|waiting| ready(waiting.into_iter().last()))
		.boxed()
}

/// The value the node gave for each key it answered, `None` when absent.
fn group_values_by_key(
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
fn all_keys_answered_or_err(
	keys: &[Vec<u8>],
	answered: &HashMap<Vec<u8>, Option<Vec<u8>>>,
	at: H256,
) -> Result<Vec<Option<Vec<u8>>>, RuntimeFailure> {
	keys.iter().map(|key| key_answered_or_err(answered, key, at)).collect()
}

fn key_answered_or_err(
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

	use super::consume_latest;

	#[test]
	fn heads_waiting_together_collapse_into_the_newest() {
		// A slow read must not leave the watch reading one stale head after
		// another: whatever queued up meanwhile is read once, at its newest.
		let waiting = stream::iter([1, 2, 3]).boxed();

		let read: Vec<u32> = block_on(consume_latest(waiting).collect());

		assert_eq!(read, vec![3]);
	}
}

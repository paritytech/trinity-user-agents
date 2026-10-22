//! Keeps the ledger's holdings equal to `Scarcity::NftsByOwner` at the best
//! block, for every item key.

use core::fmt::Display;
use std::{collections::HashSet, future::Future, sync::Arc, time::Duration};

use futures::{
	FutureExt, StreamExt,
	future::{AbortHandle, Abortable, ready},
	stream::{self, BoxStream},
};
use parking_lot::Mutex;
use subxt::utils::H256;

use super::chain::{ScarcityNft, nfts_by_owner};
use crate::{
	chain::{DecodedEntryChanges, StorageWatchError, WatchedHead, WatchedStorage},
	durable::Timer,
	nft::{HeldItem, ItemKey, NftLedger},
	subscription::Spawner,
};

/// How long a watch that failed to open waits before trying again.
const REOPEN_DELAY: Duration = Duration::from_secs(5);

/// How long a failed holdings write waits before trying again.
const REWRITE_DELAY: Duration = Duration::from_secs(1);

/// What one watch reports for the keys it covers.
type HoldingChanges = DecodedEntryChanges<([u8; 32],), ScarcityNft>;

/// One read of a watch: its changes, or why they could not be read.
type WatchedRead = Result<HoldingChanges, StorageWatchError>;

/// What the holding tracker runs on.
pub struct HoldingTrackerDeps {
	/// Item keys in, holdings out.
	pub ledger: NftLedger,
	/// Storage of the chain that runs the pallet.
	pub storage: Arc<dyn WatchedStorage>,
	/// Genesis of that chain.
	pub genesis: H256,
	/// Runs the tracking task.
	pub spawner: Spawner,
	/// Waits between attempts.
	pub timer: Arc<dyn Timer>,
}

/// Keeps the ledger's holdings equal to `NftsByOwner` at the best block, for
/// every item key. Every key stays watched, because keys are never reused
/// and anyone can still send an NFT to one that was emptied.
pub struct HoldingTracker {
	tracking: Tracking,
	spawner: Spawner,
	task: Mutex<Option<AbortHandle>>,
}

impl HoldingTracker {
	/// A tracker that is not running yet.
	pub fn new(deps: HoldingTrackerDeps) -> Arc<Self> {
		let HoldingTrackerDeps { ledger, storage, genesis, spawner, timer } = deps;
		let tracking = Tracking { ledger, storage, genesis, timer };
		Arc::new(Self { tracking, spawner, task: Mutex::new(None) })
	}

	/// Starts tracking until the tracker is dropped. Calling it again while
	/// it runs does nothing.
	pub fn ensure_started(&self) {
		let mut task = self.task.lock();
		if task.is_some() {
			return;
		}
		let (abort, registration) = AbortHandle::new_pair();
		*task = Some(abort);
		let tracking = Abortable::new(self.tracking.clone().run(), registration).map(|_| ());
		(self.spawner)(Box::pin(tracking));
	}
}

impl Drop for HoldingTracker {
	fn drop(&mut self) {
		if let Some(task) = self.task.lock().take() {
			task.abort();
		}
	}
}

/// The ledger and the chain the tracking task joins.
#[derive(Clone)]
struct Tracking {
	ledger: NftLedger,
	storage: Arc<dyn WatchedStorage>,
	genesis: H256,
	timer: Arc<dyn Timer>,
}

impl Tracking {
	/// Watches each batch of keys as the ledger gains it and records every
	/// change the watches report, until the ledger can no longer be observed.
	async fn run(self) {
		let watch_batch = |batch| self.clone().watch_batch(batch);
		let changes = self.new_key_batches().map(watch_batch).flatten_unordered(None);
		changes.for_each(|changes| self.record(changes)).await;
	}

	/// The keys the ledger gained since the last read, once per change.
	fn new_key_batches(&self) -> BoxStream<'static, Vec<ItemKey>> {
		self.ledger
			.observe_item_keys()
			.filter_map(|read| ready(ok_or_warn(read, "item keys unreadable")))
			.scan(HashSet::new(), |watched, keys| {
				let new = find_new_keys(watched, keys);
				watched.extend(new.iter().copied());
				ready(Some(new))
			})
			.filter(|new| ready(!new.is_empty()))
			.boxed()
	}

	/// What `batch` holds at each best block, first every key and then the
	/// keys that changed. A read that failed is logged and skipped, because
	/// the watch reports the same keys again.
	fn watch_batch(self, batch: Vec<ItemKey>) -> BoxStream<'static, HoldingChanges> {
		stream::once(self.open_until_watching(batch))
			.flatten()
			.filter_map(|read| ready(ok_or_warn(read, "holdings unread")))
			.boxed()
	}

	/// A watch over `batch`, trying again after [`REOPEN_DELAY`] while the
	/// chain cannot open one. Nothing, when the runtime has no such entry or
	/// a key does not fit it, since trying again would fail the same way.
	async fn open_until_watching(self, batch: Vec<ItemKey>) -> BoxStream<'static, WatchedRead> {
		let open = || self.open_watch(&batch);
		let opened = self.retry_while(REOPEN_DELAY, "holdings unwatched", open, can_open_later);
		match opened.await {
			Ok(changes) => changes,
			Err(error) => {
				tracing::error!(%error, "holdings cannot be watched on this runtime");
				stream::empty().boxed()
			},
		}
	}

	async fn open_watch(
		&self,
		batch: &[ItemKey],
	) -> Result<BoxStream<'static, WatchedRead>, StorageWatchError> {
		let owners = batch.iter().map(|key| (key.0,)).collect();
		let storage = self.storage.clone();
		storage
			.watch_entry_changes(self.genesis, WatchedHead::Best, nfts_by_owner(), owners)
			.await
	}

	/// Stores what `changes` reports, trying again after [`REWRITE_DELAY`]
	/// while the write fails: the watch reports a key only when its value
	/// changes, so a lost write would leave it stale.
	async fn record(&self, changes: HoldingChanges) {
		let holdings: Vec<_> = changes
			.changed
			.into_iter()
			.map(|((owner,), nft)| holding_of(owner, nft))
			.collect();
		let write = || self.ledger.record_holdings(holdings.clone());
		let always = |_: &_| true;
		let _ = self.retry_while(REWRITE_DELAY, "holdings unrecorded", write, always).await;
	}

	/// What `attempt` gives, trying again after `delay` while it fails with
	/// an error `retriable` accepts. Each failure is logged as `failed`.
	async fn retry_while<T, E: Display, Attempt: Future<Output = Result<T, E>>>(
		&self,
		delay: Duration,
		failed: &'static str,
		attempt: impl Fn() -> Attempt,
		retriable: impl Fn(&E) -> bool,
	) -> Result<T, E> {
		loop {
			match attempt().await {
				Err(error) if retriable(&error) => tracing::warn!(%error, "{failed}"),
				outcome => return outcome,
			}
			self.timer.sleep(delay).await;
		}
	}
}

/// Whether opening a watch that failed with `error` can succeed later.
fn can_open_later(error: &StorageWatchError) -> bool {
	!matches!(error, StorageWatchError::Incompatible { .. } | StorageWatchError::KeyEncoding { .. })
}

/// `read`'s value, or `None` after logging its error as `failed`.
fn ok_or_warn<T, E: Display>(read: Result<T, E>, failed: &'static str) -> Option<T> {
	read.inspect_err(|error| tracing::warn!(%error, "{failed}")).ok()
}

/// The keys of `keys` not in `watched`.
fn find_new_keys(watched: &HashSet<ItemKey>, keys: Vec<ItemKey>) -> Vec<ItemKey> {
	keys.into_iter().filter(|key| !watched.contains(key)).collect()
}

fn holding_of(owner: [u8; 32], nft: Option<ScarcityNft>) -> (ItemKey, Option<HeldItem>) {
	(ItemKey(owner), nft.map(HeldItem::from))
}

#[cfg(test)]
mod tests;

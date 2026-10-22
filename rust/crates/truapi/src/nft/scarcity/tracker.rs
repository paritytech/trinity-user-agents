//! Keeps the ledger's holdings equal to `Scarcity::NftsByOwner` at the best
//! block, for every item key.

use core::fmt::Display;
use std::{collections::HashSet, sync::Arc};

use futures::{StreamExt, future::ready, stream::BoxStream};
use subxt::utils::{AccountId32, H256};

use super::chain::{ScarcityNft, nfts_by_owner};
use crate::{
	background_task::BackgroundTask,
	chain::{DecodedEntryChanges, WatchedHead, WatchedStorage},
	nft::{HeldItem, ItemKey, NftLedger},
	subscription::Spawner,
	time::Timer,
};

/// What one watch reports for the keys it covers.
type HoldingChanges = DecodedEntryChanges<(AccountId32,), ScarcityNft>;

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
/// every item key, until it is dropped. Every key stays watched, because
/// keys are never reused and anyone can still send an NFT to one that was
/// emptied.
pub struct HoldingTracker {
	_tracking: BackgroundTask,
}

impl HoldingTracker {
	/// Starts tracking on the deps' spawner.
	pub fn start(deps: HoldingTrackerDeps) -> Self {
		let HoldingTrackerDeps { ledger, storage, genesis, spawner, timer } = deps;
		let tracking = Tracking { ledger, storage, genesis, timer };
		Self { _tracking: BackgroundTask::spawn(&spawner, tracking.run()) }
	}
}

/// The ledger and the chain the tracking task joins.
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
		let watch_batch = |batch| self.watch_batch(batch);
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
	/// keys that changed. A failure is logged and skipped, because the watch
	/// opens again or reports the same keys again.
	fn watch_batch(&self, batch: Vec<ItemKey>) -> BoxStream<'static, HoldingChanges> {
		let owners = batch.iter().map(|key| (AccountId32(key.0),)).collect();
		let storage = self.storage.clone();
		let timer = self.timer.clone();
		storage
			.keep_watching_entry(timer, self.genesis, WatchedHead::Best, nfts_by_owner(), owners)
			.filter_map(|read| ready(ok_or_warn(read, "holdings unread")))
			.boxed()
	}

	/// Stores what `changes` reports. A failed write is logged.
	async fn record(&self, changes: HoldingChanges) {
		let holdings: Vec<_> = changes
			.changed
			.into_iter()
			.map(|((owner,), nft)| holding_of(owner, nft))
			.collect();
		let recorded = self.ledger.record_holdings(holdings).await;
		ok_or_warn(recorded, "holdings unrecorded");
	}
}

/// `read`'s value, or `None` after logging its error as `failed`.
fn ok_or_warn<T, E: Display>(read: Result<T, E>, failed: &'static str) -> Option<T> {
	read.inspect_err(|error| tracing::warn!(%error, "{failed}")).ok()
}

/// The keys of `keys` not in `watched`.
fn find_new_keys(watched: &HashSet<ItemKey>, keys: Vec<ItemKey>) -> Vec<ItemKey> {
	keys.into_iter().filter(|key| !watched.contains(key)).collect()
}

fn holding_of(owner: AccountId32, nft: Option<ScarcityNft>) -> (ItemKey, Option<HeldItem>) {
	(ItemKey(owner.0), nft.map(HeldItem::from))
}

#[cfg(test)]
mod tests;

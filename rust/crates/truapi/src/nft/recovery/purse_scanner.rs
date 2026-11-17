//! Scanning one purse of an earlier installation and recording what it
//! held.

use core::num::NonZeroU32;
use std::sync::{
	Arc, Weak,
	atomic::{AtomicUsize, Ordering},
};

use super::{ItemHoldings, RecoveryError};
use crate::{
	gap_scan::{
		FoundKey, GapLimit, HardKeyDerivation, HardPathKeys, ListedPurseIds, PurseScanOutcome,
		PurseScanPass, PurseScanner, SequentialPurseIds, scan_until_gap,
	},
	installation::InstallationId,
	nft::{
		HeldItem, ItemKey, ItemSlot, PurseId, dao,
		keys::{ItemIndex, item_key_prefix},
	},
	store::Db,
};

/// The scan of a purse's keys ends after more than this many empty keys in a
/// row, so one unfilled `create_items(1000)` call never hides what follows.
const MAX_EMPTY_KEYS: u32 = 1000;

/// The keys read in one request.
const KEYS_PER_READ: NonZeroU32 = NonZeroU32::new(500).expect("500 is not zero");

/// The walk over purse ids ends after this many empty purses in a row.
const MAX_EMPTY_PURSES: NonZeroU32 = NonZeroU32::new(10).expect("10 is not zero");

/// The purses scanned under each installation: the main purse first, then
/// purse ids from 0 until [`MAX_EMPTY_PURSES`] in a row hold nothing.
pub fn recovery_passes() -> [PurseScanPass<PurseId>; 2] {
	[
		PurseScanPass {
			purse_ids: Arc::new(ListedPurseIds(vec![PurseId::MAIN])),
			stop_after_gap: None,
		},
		PurseScanPass {
			purse_ids: Arc::new(SequentialPurseIds(0..PurseId::MAIN.0)),
			stop_after_gap: Some(MAX_EMPTY_PURSES),
		},
	]
}

/// Scans the purses of one earlier installation and records the keys that
/// hold an NFT.
pub struct NftPurseScanner {
	db: Db,
	keys: Weak<dyn HardKeyDerivation>,
	holdings: Arc<ItemHoldings>,
	installation: InstallationId,
	keys_found: AtomicUsize,
}

impl NftPurseScanner {
	/// Scans under `installation`, deriving keys with `keys` and reading them
	/// through `holdings`, and records what it finds in `db`.
	pub fn new(
		db: Db,
		keys: Weak<dyn HardKeyDerivation>,
		holdings: Arc<ItemHoldings>,
		installation: InstallationId,
	) -> Self {
		Self { db, keys, holdings, installation, keys_found: AtomicUsize::new(0) }
	}

	/// The number of keys found so far that hold an NFT.
	pub fn keys_found(&self) -> usize {
		self.keys_found.load(Ordering::Relaxed)
	}

	/// The slot and key of each of `found` in `purse`.
	fn items_of(
		&self,
		purse: PurseId,
		found: &[FoundKey<ItemKey, HeldItem>],
	) -> Vec<(ItemSlot, ItemKey)> {
		let slot = |index| ItemSlot { purse, installation: self.installation, index: ItemIndex(index) };
		found.iter().map(|hit| (slot(hit.index), hit.key)).collect()
	}
}

#[async_trait::async_trait]
impl PurseScanner<PurseId> for NftPurseScanner {
	type Error = RecoveryError;

	/// Records the purse and its keys in one write when any key holds an
	/// NFT, keeping a purse or key already recorded as it is.
	async fn scan_purse(&self, purse: PurseId) -> Result<PurseScanOutcome, RecoveryError> {
		let prefix = item_key_prefix(purse, self.installation).to_vec();
		let keys = HardPathKeys::<ItemKey>::new(self.keys.clone(), prefix);
		let limit = GapLimit { max_empty_run: MAX_EMPTY_KEYS, batch_size: KEYS_PER_READ };
		let found = scan_until_gap(&keys, &*self.holdings, limit)
			.await
			.map_err(|error| RecoveryError::Scan { purse: purse.0, reason: error.to_string() })?;
		if found.is_empty() {
			return Ok(PurseScanOutcome::Empty);
		}
		log_found(purse, &found);
		let items = self.items_of(purse, &found);
		self.keys_found.fetch_add(items.len(), Ordering::Relaxed);
		self.db
			.write(move |tx| {
				dao::insert_recovered_purse(tx, purse)?;
				dao::insert_recovered_items(tx, &items)
			})
			.await?;
		Ok(PurseScanOutcome::Held)
	}
}

/// Logs each NFT found in `purse` on the device.
fn log_found(purse: PurseId, found: &[FoundKey<ItemKey, HeldItem>]) {
	for hit in found {
		tracing::debug!(
			purse = purse.0,
			index = hit.index,
			instance = hit.holding.instance,
			"NFT backup recovery found an item"
		);
	}
}

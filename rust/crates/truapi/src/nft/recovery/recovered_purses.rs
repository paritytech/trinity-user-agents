//! Which purses recovery scans, how far, and how it records what they held.

use core::num::NonZeroU32;
use std::sync::Arc;

use crate::{
	gap_scan::{
		FoundKey, GapScanConfig, ListedPurseIds, PurseScanPass, ScannedPursesStore,
		SequentialPurseIds,
	},
	installation::InstallationId,
	nft::{HeldItem, ItemKey, ItemSlot, PurseId, dao, keys::ItemIndex},
	store::{Db, DbError},
};

/// The scan of a purse's keys ends after more than this many empty keys in a
/// row, so one unfilled `create_items(1000)` call never hides what follows.
const MAX_EMPTY_KEYS: u32 = 1000;

/// The keys read in one request.
const KEYS_PER_READ: NonZeroU32 = NonZeroU32::new(500).expect("500 is not zero");

/// The walk over purse ids ends after this many empty purses in a row.
const MAX_EMPTY_PURSES: NonZeroU32 = NonZeroU32::new(10).expect("10 is not zero");

/// The purses scanned: the main purse first, then purse ids from 0 until
/// [`MAX_EMPTY_PURSES`] in a row hold nothing.
pub fn recovery_passes() -> [PurseScanPass<PurseId>; 2] {
	[
		PurseScanPass {
			purse_ids: Arc::new(ListedPurseIds::new(vec![PurseId::MAIN])),
			stop_after_gap: None,
		},
		PurseScanPass {
			purse_ids: Arc::new(SequentialPurseIds::new(0..PurseId::MAIN.0)),
			stop_after_gap: Some(MAX_EMPTY_PURSES),
		},
	]
}

/// Each purse is read [`KEYS_PER_READ`] keys at a time from index 0, until
/// more than [`MAX_EMPTY_KEYS`] in a row hold nothing.
pub fn recovery_scan_config() -> GapScanConfig {
	GapScanConfig { gap_size: MAX_EMPTY_KEYS, batch_size: KEYS_PER_READ }
}

/// Records each recovered purse and its item keys in the core database, in
/// one write per purse and installation.
pub struct NftScannedPurses(Db);

impl NftScannedPurses {
	/// Records into `db`.
	pub fn new(db: Db) -> Self {
		Self(db)
	}
}

#[async_trait::async_trait]
impl ScannedPursesStore<PurseId, ItemKey, HeldItem> for NftScannedPurses {
	async fn record_recovered_keys(
		&self,
		purse: &PurseId,
		installation: InstallationId,
		found: &[FoundKey<ItemKey, HeldItem>],
	) -> Result<(), DbError> {
		let purse = *purse;
		let items = found_item_slots(purse, installation, found);
		self.0
			.write(move |tx| {
				dao::insert_recovered_purse(tx, purse)?;
				dao::insert_recovered_items(tx, &items)
			})
			.await
	}
}

/// The slot of each key in `found`, a key of `purse` under `installation`,
/// logging each NFT found.
fn found_item_slots(
	purse: PurseId,
	installation: InstallationId,
	found: &[FoundKey<ItemKey, HeldItem>],
) -> Vec<(ItemSlot, ItemKey)> {
	found
		.iter()
		.inspect(|hit| {
			tracing::debug!(
				purse = purse.0,
				index = hit.index,
				instance = hit.holding.instance,
				"NFT backup recovery found an item"
			)
		})
		.map(|hit| (ItemSlot { purse, installation, index: ItemIndex(hit.index) }, hit.key))
		.collect()
}

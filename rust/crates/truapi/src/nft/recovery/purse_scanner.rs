//! Scanning one purse of an earlier installation and recording what it
//! held.

use core::num::NonZeroU32;
use std::sync::{Arc, Weak};

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
	store::{Db, DbError},
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

/// Scans each purse under every earlier installation of the seed and records
/// the keys that hold an NFT.
pub struct NftPurseScanner {
	db: Db,
	keys: Weak<dyn HardKeyDerivation>,
	holdings: Arc<ItemHoldings>,
	installations: Vec<InstallationId>,
}

impl NftPurseScanner {
	/// Scans under each of `installations`, deriving keys with `keys` and
	/// reading them through `holdings`, and records what it finds in `db`.
	pub fn new(
		db: Db,
		keys: Weak<dyn HardKeyDerivation>,
		holdings: Arc<ItemHoldings>,
		installations: Vec<InstallationId>,
	) -> Self {
		Self { db, keys, holdings, installations }
	}

	/// The keys of `purse` under `installation` that hold an NFT, read from
	/// index 0 until more than [`MAX_EMPTY_KEYS`] in a row hold nothing.
	async fn find_held_keys(
		&self,
		purse: PurseId,
		installation: InstallationId,
	) -> Result<Vec<FoundKey<ItemKey, HeldItem>>, RecoveryError> {
		let prefix = item_key_prefix(purse, installation).to_vec();
		let keys = HardPathKeys::<ItemKey>::new(self.keys.clone(), prefix);
		let limit = GapLimit { max_empty_run: MAX_EMPTY_KEYS, batch_size: KEYS_PER_READ };
		scan_until_gap(&keys, &*self.holdings, limit)
			.await
			.map_err(|error| RecoveryError::Scan { purse: purse.0, reason: error.to_string() })
	}

	/// Records `purse` and the keys in `found` under `installation` in one
	/// write, keeping a purse or key already recorded as it is.
	async fn record_recovered_keys(
		&self,
		purse: PurseId,
		installation: InstallationId,
		found: &[FoundKey<ItemKey, HeldItem>],
	) -> Result<(), DbError> {
		log_recovered_items(purse, found);
		let items = found_item_slots(purse, installation, found);
		self.db
			.write(move |tx| {
				dao::insert_recovered_purse(tx, purse)?;
				dao::insert_recovered_items(tx, &items)
			})
			.await
	}
}

#[async_trait::async_trait]
impl PurseScanner<PurseId> for NftPurseScanner {
	type Error = RecoveryError;

	async fn scan_purse(&self, purse: PurseId) -> Result<PurseScanOutcome, RecoveryError> {
		let mut outcome = PurseScanOutcome::Empty;
		for installation in &self.installations {
			let found = self.find_held_keys(purse, *installation).await?;
			if found.is_empty() {
				continue;
			}
			self.record_recovered_keys(purse, *installation, &found).await?;
			outcome = PurseScanOutcome::Held;
		}
		Ok(outcome)
	}
}

/// The slot of each key in `found`, a key of `purse` under `installation`.
fn found_item_slots(
	purse: PurseId,
	installation: InstallationId,
	found: &[FoundKey<ItemKey, HeldItem>],
) -> Vec<(ItemSlot, ItemKey)> {
	found
		.iter()
		.map(|hit| (ItemSlot { purse, installation, index: ItemIndex(hit.index) }, hit.key))
		.collect()
}

/// Logs each NFT found in `purse` on the device.
fn log_recovered_items(purse: PurseId, found: &[FoundKey<ItemKey, HeldItem>]) {
	for hit in found {
		tracing::debug!(
			purse = purse.0,
			index = hit.index,
			instance = hit.holding.instance,
			"NFT backup recovery found an item"
		);
	}
}

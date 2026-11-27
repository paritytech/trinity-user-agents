//! What each item key holds on chain, as the tracker last saw it.

use rusqlite::Transaction;

use super::{dao, keys::ItemKey};
use crate::store::DbError;

/// The NFT an item key holds, with what a purse needs to know about it. It
/// says which NFT this is and where its metadata, such as its image, is
/// looked up. The instance's own metadata comes first, then its item's, then
/// its collection's. Any NFT pallet names these three levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldItem {
	/// Collection the NFT belongs to.
	pub collection: u32,
	/// Item definition within the collection.
	pub item: u32,
	/// Which instance of the item this is.
	pub instance: u64,
}

/// What a key holds as the tracker records it: the NFT, and the fee-less
/// moves it used since its last paid move. The count only changes when the
/// NFT moves, so it stays current while the NFT sits at the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackedHolding {
	/// The NFT the key holds.
	pub item: HeldItem,
	/// Fee-less moves the NFT used.
	pub moves: u16,
}

/// Stores what each key holds in `tx`: a row for a key that holds
/// something, none for a key that holds nothing.
pub fn record_holdings(
	tx: &Transaction<'_>,
	holdings: &[(ItemKey, Option<TrackedHolding>)],
) -> Result<(), DbError> {
	for (key, held) in holdings {
		match held {
			Some(held) => dao::upsert_holding(tx, key, held)?,
			None => dao::delete_holding(tx, key)?,
		}
	}
	Ok(())
}

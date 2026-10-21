//! What each item key holds on chain, as the tracker last saw it.

use rusqlite::Transaction;

use super::{dao, keys::ItemKey};
use crate::{chain::HashAndNumber, store::DbError};

/// The NFT an item key holds, as far as a purse needs to know: which NFT it
/// is, and the three levels its metadata, such as its image, is looked up
/// at: the instance's own, then its item's, then its collection's. Any NFT
/// pallet names these three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldItem {
	/// Collection the NFT belongs to.
	pub collection: u32,
	/// Item definition within the collection.
	pub item: u32,
	/// Which instance of the item this is.
	pub instance: u64,
}

/// Stores what each key holds at `at` in `tx`: a row for a key that holds
/// something, none for a key that holds nothing.
pub fn record_in(
	tx: &Transaction<'_>,
	at: HashAndNumber,
	holdings: &[(ItemKey, Option<HeldItem>)],
) -> Result<(), DbError> {
	for (key, held) in holdings {
		match held {
			Some(held) => dao::upsert_holding(tx, key, held, &at)?,
			None => dao::delete_holding(tx, key)?,
		}
	}
	Ok(())
}
